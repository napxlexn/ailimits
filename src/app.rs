// app.rs — main application state and the event loop.
//
// Threads:
//   Thread 1: the tao event loop — window events, rendering, AppCommand.
//   Thread 2+: the tokio runtime — the scheduler and provider fetches.
//
// `run` builds everything once and hands each event to `App::handle`. The
// state lives in `App`; what happens to it is split by where it comes from:
//   events.rs        window, panel, timer and user events; provider updates
//   menu.rs          the context menu's actions
//   flow.rs          what several of them share: syncing the providers and
//                    the surfaces, the picker, persisting state
//   provider_set.rs  the provider instances the scheduler polls
//   cache.rs         the provider cache and the one writer per state file
//   model_job.rs     the model label reads
//   clipboard.rs     keys and tokens pasted from the menu
//   widget.rs        the overlay window: glass, showing, layout
//   indicator.rs     the taskbar panel's fallback to the tray icon (Windows)
//   config_watch.rs  config.toml edited by hand, applied while the app runs

mod cache;
mod clipboard;
mod config_watch;
mod events;
mod flow;
#[cfg(target_os = "windows")]
mod indicator;
mod menu;
#[cfg(target_os = "windows")]
mod menu_preview;
mod model_job;
mod provider_set;
mod sign_in;
mod widget;

use crate::{
    config::{
        schema::{Config, PendingNotice, Surface},
        storage,
    },
    models::Models,
    monitor::{
        scheduler::{CycleHook, Scheduler, SharedInterval},
        trend::Trend,
    },
    notifications::{alerts::AlertBook, toast::ToastNotifier},
    providers::{cursor::session::Approval, ProviderData, ProviderId, ProviderStatus, TokenCheck},
    rows::{self, RowView},
    ui::{
        context_menu::ContextMenu,
        layout,
        picker::{PickerInput, PickerMode, PickerWindow, ScreenRect},
        renderer::Renderer,
        taskbar_panel::TaskbarPanel,
        theme::ComputedTheme,
        tray::Tray,
        window::WindowState,
    },
    updater,
};
use anyhow::{Context as AnyhowContext, Result};
use cache::{load_provider_cache, CacheLedger, FileWriter};
use chrono::Utc;
use model_job::{spawn_model_read, ModelJob, SharedModelJob};
use provider_set::ProviderSet;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::EventLoopBuilder,
    window::WindowBuilder,
};
use tokio::sync::mpsc;
use tracing::{info, warn};
#[cfg(target_os = "windows")]
use widget::apply_windows_glass;

/// Commands for the event loop.
#[derive(Debug)]
pub enum AppCommand {
    /// Update a single provider's data, made by the provider instance built
    /// under `serial` (`monitor::scheduler::ProviderSlot`): an update from an
    /// instance that has since been replaced is dropped.
    UpdateProvider { data: ProviderData, serial: u64 },
    /// The Cursor sign-in started from the window stopped waiting for the
    /// browser. Its session is kept here, on the event loop, and only while
    /// `ticket` is still the sign-in awaited (`sign_in::Awaited`): one
    /// cancelled, signed out of or started over since keeps nothing.
    CursorSignIn { approval: Approval, ticket: u64 },
    /// A usage token pasted in the window was checked (sign_in.rs
    /// `paste_usage_token`). It is stored here, on the event loop, unless
    /// refused, and only while `ticket` is still the paste awaited: one
    /// replaced by a later paste or by a removal stores nothing. The token
    /// never shows in a debug print (`sign_in::PastedToken`).
    UsageToken {
        pid: String,
        check: TokenCheck,
        token: sign_in::PastedToken,
        ticket: u64,
    },
    /// The model labels were read (models/): each provider's model id and
    /// effort, nothing else of the files; with the generation of the
    /// request they answer.
    Models(u64, Models),
    /// config.toml changed on disk, as the polling cycle or the writer
    /// found: the event loop reads and applies it (app/config_watch.rs).
    ConfigFileChanged,
    /// Quit the application.
    Quit,
}

/// tao user events: commands + menu clicks + tray clicks + taskbar moves.
#[derive(Debug)]
pub enum UserEvent {
    Command(AppCommand),
    Menu(muda::MenuId),
    Tray(TrayKind),
    /// The taskbar slid/moved (auto-hide, resolution change) or the
    /// notification area changed width (icon pinned/unpinned) — the embedded
    /// panel follows it. Sent by the SetWinEventHook watcher.
    TaskbarMoved,
    /// A window came to the foreground (e.g. the tray overflow flyout) and may
    /// have covered the panel overlay — re-assert its topmost z-order.
    PanelRaise,
    /// The shell re-stacked the taskbar's z-order (EVENT_OBJECT_REORDER) with no
    /// move/foreground event — e.g. the auto-hide bar peeking back fronts itself
    /// above the overlay. Re-check coverage and toggle the tray fallback.
    #[cfg(target_os = "windows")]
    PanelRecheck,
    /// A popup menu opened or closed. One that reaches over the panel takes
    /// the panel under it; when it goes, the panel comes back on top.
    #[cfg(target_os = "windows")]
    PanelMenu,
    /// A monitor was unplugged, added or re-arranged (WM_DISPLAYCHANGE). Both
    /// surfaces have to be found a place again: the overlay may be left on no
    /// screen at all, and the taskbar the panel follows may have been the one
    /// on the monitor that went.
    #[cfg(target_os = "windows")]
    DisplaysChanged,
    /// The app was started again while running (`main.rs`): show the
    /// widget, so the second start visibly does something.
    ShowWidget,
    /// The tray icon's menu (opened by tray-icon, with the event loop free)
    /// highlighted an item or closed: handled as any event, so a look shown
    /// resizes the widget with the loop's own events held back.
    #[cfg(target_os = "windows")]
    MenuHover(crate::platform::menu_hover::MenuHover),
}

/// Tray interactions we care about.
#[derive(Debug)]
pub enum TrayKind {
    /// Left-click → toggle the overlay window's visibility.
    LeftClick,
}

/// Loading placeholder before the first fetch.
fn loading_data(id: ProviderId) -> ProviderData {
    ProviderData {
        id,
        status: ProviderStatus::Loading,
        metrics: vec![],
        updated_at: Utc::now(),
        received_at: Some(std::time::Instant::now()),
    }
}

/// Quiet toast feedback for menu actions.
fn feedback(title: &str, body: &str) {
    if let Err(e) = ToastNotifier::show(title, body) {
        warn!("feedback toast failed: {e}");
    }
}

/// The providers the surfaces show, as they reported: enabled, allowed and
/// connected. Disabled (enabled=false), not allowed to read their sign-in,
/// and not-connected (NotConfigured) ones are left out. Not aged yet: the
/// rows age each row on its own (`rows::visible_rows`).
fn active_data(config: &Config, display: &[ProviderData]) -> Vec<ProviderData> {
    display
        .iter()
        .filter(|d| {
            let enabled = config
                .providers
                .iter()
                .find(|p| ProviderId::from_config_id(&p.id) == Some(d.id.clone()))
                .is_some_and(|p| config.provider_active(p));
            enabled && !matches!(d.status, ProviderStatus::NotConfigured)
        })
        .cloned()
        .collect()
}

/// `active_data` aged for display, for the surfaces that show one entry per
/// provider (the tray icon, the taskbar panel): a window whose reset has
/// provably passed is shown as ≈0%. Once the user chose which providers
/// those show (the picker's Taskbar column, `rows::taskbar_ticked`), only
/// those; untouched, every running one, as always.
fn visible_data(config: &Config, display: &[ProviderData]) -> Vec<ProviderData> {
    let chosen = config
        .ui
        .rows
        .taskbar
        .is_some()
        .then(|| rows::taskbar_ticked(config));
    active_data(config, display)
        .iter()
        .filter(|d| {
            chosen
                .as_ref()
                .is_none_or(|ids| ids.contains(&d.id.config_id()))
        })
        .map(ProviderData::aged_for_display)
        .collect()
}

/// The rows a surface shows (rows.rs), built from the providers the widget
/// shows as they reported, so that each row ages by its own windows (an
/// extra row by its one metric). With the default config: one row per
/// visible provider, aged for display.
fn surface_rows(
    config: &Config,
    display: &[ProviderData],
    surface: Surface,
    models: &Models,
) -> Vec<RowView> {
    rows::visible_rows(config, &active_data(config, display), surface, models)
}

/// What the widget's size depends on (layout.rs `Shape`): its rows, the
/// lines their models take, whether nothing is chosen yet, and whether the
/// rows chosen have nothing to show while providers run.
fn widget_shape(
    config: &Config,
    display: &[ProviderData],
    models: &Models,
    renderer: &Renderer,
) -> layout::Shape {
    layout::Shape::of_tuned(
        &config.ui,
        &layout::WidgetMetrics::from_tune(&config.tune),
        &surface_rows(config, display, Surface::Widget, models),
        rows::nothing_chosen(config),
        |text, size| renderer.text_width(text, size),
    )
    .with_unshown(!active_data(config, display).is_empty())
}

/// What the picker opens as: the welcome or the update notice while one is
/// owed, the welcome too while nothing is chosen, otherwise the plain picker.
fn picker_mode_for(config: &Config) -> PickerMode {
    match config.reads.pending {
        PendingNotice::Welcome => PickerMode::Welcome,
        PendingNotice::UpdateNotice => PickerMode::UpdateNotice,
        PendingNotice::None if rows::nothing_chosen(config) => PickerMode::Welcome,
        PendingNotice::None => PickerMode::Normal,
    }
}

/// Where the picker opens: beside the widget while it is on screen,
/// otherwise at the cursor (on the menu or the tray icon just clicked).
fn picker_anchor(window: &tao::window::Window, widget_visible: bool) -> ScreenRect {
    if widget_visible {
        if let Ok(p) = window.outer_position() {
            let s = window.outer_size();
            return ScreenRect {
                x: p.x,
                y: p.y,
                w: s.width as i32,
                h: s.height as i32,
            };
        }
    }
    let c = window
        .cursor_position()
        .unwrap_or(PhysicalPosition::new(0.0, 0.0));
    ScreenRect {
        x: c.x as i32,
        y: c.y as i32,
        w: 1,
        h: 1,
    }
}

/// Open the picker window; a failure is logged and leaves the app as it was.
/// `activate`: take the keyboard focus, for a window the user opened (the
/// menu, a click). One that opens by itself (the welcome, the update notice)
/// leaves the focus where it is (`PickerWindow::open_quietly`).
fn open_picker_window<T: 'static>(
    target: &tao::event_loop::EventLoopWindowTarget<T>,
    renderer: &Rc<Renderer>,
    input: PickerInput,
    anchor: ScreenRect,
    activate: bool,
) -> Option<PickerWindow> {
    let opened = if activate {
        PickerWindow::open(target, renderer.clone(), input, anchor)
    } else {
        PickerWindow::open_quietly(target, renderer.clone(), input, anchor)
    };
    match opened {
        Ok(p) => Some(p),
        Err(e) => {
            warn!("the picker did not open: {e}");
            None
        }
    }
}

/// Whether the welcome or the update notice opens by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoOpen {
    /// Nothing is owed, or the picker is already open.
    No,
    Now,
    /// A fullscreen app is in front - a game, a video, a slide show. The
    /// window would land on top of it and take its keyboard: it waits until
    /// the app is no longer in front.
    Later,
}

/// Whether to open the owed window now. `fullscreen` is asked only when
/// something is owed.
fn auto_open(
    pending: PendingNotice,
    picker_open: bool,
    fullscreen: impl FnOnce() -> bool,
) -> AutoOpen {
    if pending == PendingNotice::None || picker_open {
        AutoOpen::No
    } else if fullscreen() {
        AutoOpen::Later
    } else {
        AutoOpen::Now
    }
}

/// `alerts.json`: which threshold alerts were given, so a restart does not
/// repeat them (notifications/alerts.rs).
fn alerts_path() -> std::path::PathBuf {
    storage::config_path().with_file_name("alerts.json")
}

fn load_alert_book() -> AlertBook {
    std::fs::read_to_string(alerts_path())
        .map(|text| AlertBook::from_json(&text))
        .unwrap_or_default()
}

type Window = tao::window::Window;

/// Everything the event loop works on. `run` builds it once; the loop hands
/// it every event (`App::handle`, events.rs).
struct App {
    runtime: tokio::runtime::Runtime,
    cmd_tx: mpsc::Sender<AppCommand>,
    config: Config,
    /// Saves the config in the background through a single serialised
    /// writer (storage::spawn_saver): a burst coalesces to the latest config
    /// on a one-slot channel, so the newest setting always wins and a slow
    /// disk cannot grow the queue.
    config_saves: storage::ConfigSaverHandle,
    /// Kept for the shutdown handshake, so the final write cannot race an
    /// in-flight background one.
    saver_at_exit: Option<storage::ConfigSaver>,
    /// config.toml as the writer left it, and the looks for edits made
    /// elsewhere (app/config_watch.rs).
    config_watch: config_watch::ConfigWatch,

    // ── Providers and what they reported ──
    providers: ProviderSet,
    /// One entry per provider, in config order: what the surfaces show.
    display: Vec<ProviderData>,
    provider_cache: HashMap<ProviderId, ProviderData>,
    /// When the cache file is behind (cache.rs).
    cache_ledger: CacheLedger,
    cache_writer: Option<FileWriter>,
    /// The threshold alerts given, kept across restarts.
    alert_book: AlertBook,
    alert_writer: Option<FileWriter>,
    /// Previous primary % per provider, for reset-edge detection.
    last_pct: HashMap<ProviderId, f32>,
    /// Whether the on_startup hook has fired (once after the first success).
    startup_fired: bool,
    /// Burn-rate trend buffers + the latest forecast per provider, stored as
    /// the ABSOLUTE projected hit moment: the renderer derives "time left"
    /// from it live, so the label ticks down between updates and expires on
    /// its own instead of freezing at the duration computed on the last
    /// fetch.
    trends: HashMap<ProviderId, Trend>,
    forecasts: HashMap<ProviderId, chrono::DateTime<Utc>>,
    empty_forecasts: HashMap<ProviderId, chrono::DateTime<Utc>>,
    /// Last fetch error per provider, kept after retention replaces the slot
    /// with old data: hovering a greyed/estimated row shows this as the
    /// "why" (the slot itself only knows the retained values, not the cause).
    last_errors: HashMap<ProviderId, String>,
    models: Models,
    model_job: SharedModelJob,
    /// Shared with the scheduler, changed live from the menu.
    update_interval: SharedInterval,
    /// Shared with the updater, changed live from the menu.
    auto_update_enabled: Arc<AtomicBool>,

    // ── The widget ──
    /// Shared with the picker window, which draws with the same font.
    renderer: Rc<Renderer>,
    shape: layout::Shape,
    win_layout: layout::WindowLayout,
    theme: ComputedTheme,
    window: Rc<Window>,
    /// Kept alive for the surface.
    _sb_context: softbuffer::Context<Rc<Window>>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    pixmap: tiny_skia::Pixmap,
    win_state: WindowState,
    window_visible: bool,
    cursor_inside_window: bool,

    // ── Menu, tray icon, taskbar panel ──
    menu: ContextMenu,
    /// The Cursor sign-in waiting for the browser, if one is.
    cursor_sign_in: sign_in::Awaited<tokio::task::JoinHandle<()>>,
    /// A pasted usage token being checked, if one is: a second paste
    /// replaces it.
    usage_token_check: sign_in::Awaited<tokio::task::JoinHandle<()>>,
    /// What the picker's Sign-in blocks show besides the config
    /// (app/sign_in.rs).
    sign_in: crate::choices::SignInState,
    tray: Tray,
    panel: TaskbarPanel,
    #[cfg(target_os = "windows")]
    indicator: indicator::Indicator,
    /// The context menu asked for by a right-click on this window (the
    /// widget's or the panel's): opened once the event is handled, so the
    /// look items it highlights can be shown while it is open.
    #[cfg(target_os = "windows")]
    menu_request: Option<isize>,
    /// The open menu's look shown under the pointer (app/menu_preview.rs).
    #[cfg(target_os = "windows")]
    menu_preview: Option<menu_preview::MenuPreview>,
    /// The look from before the menu, while its click is on the way.
    #[cfg(target_os = "windows")]
    look_base: Option<menu_preview::Look>,

    // ── The picker: open only while in use ──
    found_tools: Vec<&'static str>,
    picker_mode: PickerMode,
    picker: Option<PickerWindow>,
    /// An owed window held back while a fullscreen app is in front; it opens
    /// when that app is no longer in front (a foreground change, the next
    /// provider update, or the app started again by hand).
    auto_open_waiting: bool,
}

/// Entry point — run the application.
pub fn run() -> Result<()> {
    // 1. The tokio runtime + the config. 2 workers instead of one-per-core:
    // a cycle is one light HTTP request per provider that runs; the default
    // one-per-core workers would waste threads and RAM.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;
    // The proxy set in Windows, for every HTTP client built from here on:
    // the providers, the Cursor sign-in, the updater.
    crate::providers::follow_windows_proxy();
    // A settings file that could not be read or understood is kept as it
    // is (storage.rs); the app then runs on a config in memory and says so
    // once, below.
    let loaded = runtime.block_on(storage::load());
    let config = loaded.config;
    info!("Config loaded: {} providers", config.providers.len());
    // The file as it was found: the writer replaces only that, and the
    // watch takes every other change for an edit (app/config_watch.rs).
    let config_file = config_watch::config_file(loaded.on_disk);

    // Toasts carry the app's own name and icon; the identity is registered
    // per user, before the first threshold alert can fire.
    crate::platform::register_toast_identity();
    // (storage.rs has logged the cause; the message says whether changes
    // are saved this session.)
    if let Some(problem) = &loaded.problem {
        feedback("AI Limits", &problem.message());
    }
    // An update that did not finish is said once (updater.rs).
    if let Some(note) = updater::unfinished_update_note() {
        feedback("AI Limits", &note);
    }

    // A saved position on no monitor now (a display unplugged, or one not
    // awake yet after a hard restart): a borderless, taskbar-skipped overlay
    // off every screen is invisible and un-draggable, so it opens on screen.
    // The saved position stays the user's: it is not overwritten, and the
    // widget goes back to it when its display comes (`on_displays_changed`).
    #[cfg(target_os = "windows")]
    let start = crate::platform::ensure_on_screen(config.window.pos_x, config.window.pos_y);
    #[cfg(not(target_os = "windows"))]
    let start = (config.window.pos_x, config.window.pos_y);

    // 2. The event loop with user events.
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    // No raw keyboard and mouse input: the app uses no device events, and
    // tao registers every keyboard and mouse for them by default, which
    // costs a wake-up per mouse move while a window of ours is in front and
    // looks like a key logger to behaviour-watching antivirus. Keys typed
    // into the picker come as window messages either way.
    event_loop.set_device_event_filter(tao::event_loop::DeviceEventFilter::Always);
    let proxy = event_loop.create_proxy();

    // The scheduler → event loop channel: tokio mpsc → EventLoopProxy.
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<AppCommand>(32);
    {
        let proxy = proxy.clone();
        runtime.spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                if proxy.send_event(UserEvent::Command(cmd)).is_err() {
                    break; // The event loop is gone.
                }
            }
        });
    }

    // 3. Providers — ALL from the config, including disabled ones: a disabled
    // provider's fetch returns NotConfigured instantly, with no network call.
    // The list is shared with the scheduler and rebuilt live from the menu,
    // only where the config changed (provider_set.rs).
    let providers = ProviderSet::new(&config);
    let provider_cache = runtime.block_on(load_provider_cache());
    let display: Vec<ProviderData> = providers
        .ids()
        .into_iter()
        .map(|id| {
            provider_cache
                .get(&id)
                .cloned()
                .unwrap_or_else(|| loading_data(id))
        })
        .collect();
    if display.is_empty() {
        anyhow::bail!("no providers in config");
    }
    // The cache file is written by one writer, only when it is behind
    // (cache.rs `CacheLedger`); a cycle's answers settle into one write.
    let cache_ledger = CacheLedger::new(&provider_cache);
    let cache_writer = FileWriter::spawn(
        runtime.handle(),
        cache::provider_cache_path(),
        std::time::Duration::from_secs(2),
    );
    // The threshold alerts given, kept across restarts.
    let alert_book = load_alert_book();
    let alert_writer =
        FileWriter::spawn(runtime.handle(), alerts_path(), std::time::Duration::ZERO);

    // 4. The background scheduler; the interval is shared — changed live
    // from the menu.
    let update_interval: SharedInterval =
        Arc::new(AtomicU64::new(config.general.update_interval_secs));
    // The model labels are re-read on the polling cycle, never on a timer
    // of their own, and only as far as `[reads]` allows (`model_request`).
    // config.toml is looked at on the same cycle: one metadata call, and an
    // event only when it changed.
    let model_job = ModelJob::new(&config);
    {
        let tx = cmd_tx.clone();
        config_file.set_notify(move || {
            let _ = tx.try_send(AppCommand::ConfigFileChanged);
        });
    }
    let cycle_hook: CycleHook = {
        let job = model_job.clone();
        let tx = cmd_tx.clone();
        let rt = runtime.handle().clone();
        let file = config_file.clone();
        Arc::new(move || {
            spawn_model_read(&job, &tx, &rt);
            file.tell_if_changed();
        })
    };
    let scheduler = Scheduler::new(providers.shared(), update_interval.clone(), cmd_tx.clone())
        .with_cycle_hook(cycle_hook);
    runtime.spawn(scheduler.run());

    // 4b. Background auto-updater — polls GitHub Releases and silently installs
    // newer builds. The enabled flag is shared so the menu toggle takes effect
    // without a restart.
    let auto_update_enabled = Arc::new(AtomicBool::new(config.general.auto_update));
    runtime.spawn(updater::run(auto_update_enabled.clone()));

    // 5. Layout, theme, renderer — mut: switched live from the menu.
    // The layout is computed from VISIBLE providers, not all of them.
    // The rows measure their text for the layout, so the renderer comes
    // first. No model is read yet: the readers arrive with their consent.
    // Shared with the picker window, which draws with the same font.
    let renderer = Rc::new(Renderer::new()?);
    let models = Models::new();
    let shape = widget_shape(&config, &display, &models, &renderer);
    let win_layout = layout::compute_tuned(
        &config.ui.layout,
        &config.ui.detail,
        &config.ui.width_scale,
        &config.ui.column_flow,
        &shape,
        &layout::WidgetMetrics::from_tune(&config.tune),
    );
    let theme = ComputedTheme::tuned(&config.ui, &config.tune);

    // 6. The window: borderless, transparent, out of the taskbar.
    let mut builder = WindowBuilder::new()
        .with_title("AI Limits")
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(config.window.pinned)
        .with_resizable(false)
        // Lift the system min width (~160px) — a single-provider window
        // is narrower than that.
        .with_min_inner_size(PhysicalSize::new(40u32, 40u32))
        .with_inner_size(PhysicalSize::new(
            win_layout.width as u32,
            win_layout.height as u32,
        ))
        .with_position(PhysicalPosition::new(start.0, start.1));

    #[cfg(target_os = "windows")]
    {
        use tao::platform::windows::WindowBuilderExtWindows;
        // WS_EX_TOOLWINDOW: hidden from the taskbar and Alt+Tab.
        builder = builder.with_skip_taskbar(true);
        // No drop-shadow frame: the undecorated shadow keeps a 1px non-client
        // border (client offset 1px from the window rect) that surfaces as a
        // white seam on the bottom/right edges once the DWM border is removed.
        builder = builder.with_undecorated_shadow(false);
    }

    let window = builder
        .build(&event_loop)
        .context("failed to create window")?;

    // Windows clamps the size AT CREATION to the system minimum (~160px);
    // a second set_inner_size after creation bypasses the clamp for popups.
    window.set_inner_size(PhysicalSize::new(
        win_layout.width as u32,
        win_layout.height as u32,
    ));

    // 7. The glass effect + DWM rounded corners.
    #[cfg(target_os = "windows")]
    {
        widget::set_corners(config.tune.widget.corners());
        apply_windows_glass(&window);
    }

    // 8. The softbuffer surface.
    let window = Rc::new(window);
    let sb_context = softbuffer::Context::new(window.clone())
        .map_err(|e| anyhow::anyhow!("softbuffer context: {e}"))?;
    let mut surface = softbuffer::Surface::new(&sb_context, window.clone())
        .map_err(|e| anyhow::anyhow!("softbuffer surface: {e}"))?;
    let (buf_w, buf_h) = (win_layout.width as u32, win_layout.height as u32);
    surface
        .resize(
            NonZeroU32::new(buf_w).context("zero width")?,
            NonZeroU32::new(buf_h).context("zero height")?,
        )
        .map_err(|e| anyhow::anyhow!("surface resize: {e}"))?;

    let pixmap = tiny_skia::Pixmap::new(buf_w, buf_h).context("pixmap alloc failed")?;

    // 9. The context menu: events → proxy.
    let menu = ContextMenu::new(&config, config.window.pinned)?;
    // One Credential Manager read: is there a Cursor session to sign out of.
    // A Store, Scoop, portable or self-built copy is updated through its own
    // channel: the Auto-update item would do nothing there, so it goes.
    if !updater::self_update_supported() {
        menu.remove_auto_update();
    }
    {
        let proxy = proxy.clone();
        muda::MenuEvent::set_event_handler(Some(move |e: muda::MenuEvent| {
            let _ = proxy.send_event(UserEvent::Menu(e.id().clone()));
        }));
    }

    // 9b. The tray icon: a left-click toggles the overlay; a right-click shows
    // the SAME context menu. Tray menu clicks reuse the MenuEvent handler above.
    let mut tray = Tray::new();
    {
        let proxy = proxy.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |e: tray_icon::TrayIconEvent| {
            if let tray_icon::TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = e
            {
                if button == tray_icon::MouseButton::Left
                    && button_state == tray_icon::MouseButtonState::Up
                {
                    let _ = proxy.send_event(UserEvent::Tray(TrayKind::LeftClick));
                }
            }
        }));
    }
    tray.set_first_install(
        rows::nothing_chosen(&config),
        &visible_data(&config, &display),
    );
    tray.set_tune(&config.tune.tray, &visible_data(&config, &display), &theme);
    tray.set_mode(
        config.general.indicator,
        &menu.menu,
        &visible_data(&config, &display),
        &theme,
    );
    // 9c. The taskbar mini panel (the readable indicator) + the event-driven
    // watcher that keeps it glued to the (auto-hiding) taskbar.
    let mut panel = TaskbarPanel::new(&event_loop)?;
    panel.set_offset(config.general.panel_offset_x, config.general.panel_offset_y);
    panel.set_display(config.general.panel_display);
    #[cfg(target_os = "windows")]
    crate::platform::install_taskbar_watch(proxy.clone(), config.general.panel_display);
    // The shell's own word on fullscreen apps, for the panel's stand-down.
    #[cfg(target_os = "windows")]
    crate::platform::register_fullscreen_watch();
    panel.set_mode(config.general.indicator, &visible_data(&config, &display));
    panel.set_numbers(config.ui.panel, &visible_data(&config, &display));
    panel.set_tune(&config.tune.panel, &visible_data(&config, &display));
    #[cfg(target_os = "windows")]
    let indicator = indicator::Indicator::new();

    // 10. The window state.
    let win_state = WindowState {
        pos: (start.0 as f64, start.1 as f64),
        size: (win_layout.width as f64, win_layout.height as f64),
        pinned: config.window.pinned,
        ..Default::default()
    };

    let saver = storage::spawn_saver_for(runtime.handle(), config_file.clone());
    let config_saves = saver.handle();
    let picker_mode = picker_mode_for(&config);

    let mut app = App {
        runtime,
        cmd_tx,
        config,
        config_saves,
        saver_at_exit: Some(saver),
        config_watch: config_watch::ConfigWatch::new(config_file),
        providers,
        display,
        provider_cache,
        cache_ledger,
        cache_writer: Some(cache_writer),
        alert_book,
        alert_writer: Some(alert_writer),
        last_pct: HashMap::new(),
        startup_fired: false,
        trends: HashMap::new(),
        forecasts: HashMap::new(),
        empty_forecasts: HashMap::new(),
        last_errors: HashMap::new(),
        models,
        model_job,
        update_interval,
        auto_update_enabled,
        renderer,
        shape,
        win_layout,
        theme,
        window,
        _sb_context: sb_context,
        surface,
        pixmap,
        win_state,
        window_visible: true,
        cursor_inside_window: false,
        menu,
        cursor_sign_in: Default::default(),
        usage_token_check: Default::default(),
        sign_in: Default::default(),
        tray,
        panel,
        #[cfg(target_os = "windows")]
        indicator,
        #[cfg(target_os = "windows")]
        menu_request: None,
        #[cfg(target_os = "windows")]
        menu_preview: None,
        #[cfg(target_os = "windows")]
        look_base: None,
        found_tools: Vec::new(),
        picker_mode,
        picker: None,
        auto_open_waiting: false,
    };

    // Which keys and tokens of the app's own are stored, where Cursor's
    // sign-in stands; a source left to the old order settled.
    app.init_sign_in_state();

    // On a fresh install the picker opens by itself as the welcome, after
    // the update that added [reads] as the one-time notice - unless a
    // fullscreen app is in front, which it waits for. Opened by itself, it
    // leaves the keyboard focus where it is.
    match auto_open(
        app.config.reads.pending,
        false,
        crate::platform::foreground_fills_its_monitor,
    ) {
        AutoOpen::Now => app.open_picker(&event_loop, false),
        AutoOpen::Later => {
            info!("a fullscreen app is in front: the owed window waits for it");
            app.auto_open_waiting = true;
        }
        AutoOpen::No => {}
    }

    info!("AI Limits Widget ready");

    // 11. The event loop — never returns. The app is shared with the
    // context menu's watcher (platform::menu_hover), which shows a look item
    // while the menu's own loop runs; a right-click's menu is opened once the
    // event that asked for it is handled, with the app free.
    let app = Rc::new(std::cell::RefCell::new(app));
    #[cfg(target_os = "windows")]
    {
        use muda::ContextMenu as _;
        let root = app.borrow().menu.menu.hpopupmenu();
        let weak = Rc::downgrade(&app);
        let proxy = proxy.clone();
        crate::platform::menu_hover::set_handler(root, move |ev| {
            // The widget's or the panel's menu runs inside the loop's own
            // handler, which holds every window event back: the look is
            // painted at once, from here. The tray icon's menu runs with the
            // loop free, where a resize would call the handler again from
            // inside this one: it goes as an event.
            if !crate::platform::menu_hover::showing() {
                return proxy.send_event(UserEvent::MenuHover(ev)).is_ok();
            }
            let Some(app) = weak.upgrade() else {
                return true;
            };
            let Ok(mut app) = app.try_borrow_mut() else {
                return false;
            };
            app.on_menu_hover(ev);
            true
        });
    }
    event_loop.run(move |event, target, control_flow| {
        app.borrow_mut().handle(event, target, control_flow);
        #[cfg(target_os = "windows")]
        {
            let request = app.borrow_mut().menu_request.take();
            if let Some(hwnd) = request {
                let menu = app.borrow().menu.menu.clone();
                // SAFETY: the widget's or the panel's window, alive with the
                // app on this thread.
                unsafe { crate::platform::menu_hover::show(&menu, hwnd) };
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Metric, MetricUnit, MetricWindow};

    fn metric(label: &str, used: u64, reset_in_min: Option<i64>, extra_only: bool) -> Metric {
        Metric {
            label: label.to_string(),
            used,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: reset_in_min.map(|m| Utc::now() + chrono::Duration::minutes(m)),
            window: MetricWindow::Long,
            unified_pool: false,
            extra_only,
        }
    }

    fn claude(metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: ProviderId::Claude,
            status: ProviderStatus::Ok,
            metrics,
            updated_at: Utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    fn running_claude() -> Config {
        let mut config = Config::default();
        config.allow_sign_in("claude");
        config
    }

    /// The taskbar panel and the tray icon get every running provider until
    /// the user chooses in the Taskbar column; then those chosen that run,
    /// and the first two that run when none of those chosen does.
    #[test]
    fn the_taskbar_shows_the_providers_chosen_for_it() {
        let mut config = Config::default();
        for id in ["claude", "codex", "grok"] {
            config.allow_sign_in(id);
        }
        let one = |id: ProviderId| ProviderData {
            id,
            ..claude(vec![metric("Session", 40, Some(120), false)])
        };
        let display = [
            one(ProviderId::Claude),
            one(ProviderId::Codex),
            one(ProviderId::Grok),
        ];
        let ids = |config: &Config| -> Vec<&'static str> {
            visible_data(config, &display)
                .iter()
                .map(|d| d.id.config_id())
                .collect()
        };
        assert_eq!(ids(&config), ["claude", "codex", "grok"]);
        config.ui.rows.taskbar = Some(vec!["grok".into(), "claude".into()]);
        assert_eq!(ids(&config), ["claude", "grok"]);
        config.ui.rows.taskbar = Some(vec!["cursor".into()]);
        assert_eq!(ids(&config), ["claude", "codex"]);
    }

    /// The widget's and the tooltip's rows are built from the data as the
    /// providers sent it, each row aged by its own windows: Fable's passed
    /// reset leaves the Claude row live, and an Opus week Claude reports as
    /// unused (0, no reset) stays live while the session's reset passed.
    /// The tray and the panel still get the provider aged as one entry.
    #[test]
    fn the_rows_age_each_row_by_its_own_windows() {
        let mut config = running_claude();
        config.ui.rows.widget = Some(vec!["claude".into(), "claude.opus".into()]);
        let display = [claude(vec![
            metric("Session", 40, Some(-5), false),
            metric("Opus", 0, None, false),
            metric("Fable", 90, Some(-5), true),
        ])];
        let rows = surface_rows(&config, &display, Surface::Widget, &Models::new());
        let ids: Vec<&str> = rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, ["claude", "claude.opus"]);
        // The session rolled over: the Claude row is an estimate by it.
        assert!(matches!(rows[0].data.status, ProviderStatus::Estimated));
        // Opus did not: it stays as reported, not "≈0%" by the session.
        assert!(matches!(rows[1].data.status, ProviderStatus::Ok));

        // Only Fable's week rolled over: the Claude row stays live.
        let display = [claude(vec![
            metric("Session", 40, Some(120), false),
            metric("Opus", 10, Some(600), false),
            metric("Fable", 90, Some(-5), true),
        ])];
        for surface in [Surface::Widget, Surface::Tooltip] {
            let rows = surface_rows(&running_claude(), &display, surface, &Models::new());
            assert!(matches!(rows[0].data.status, ProviderStatus::Ok));
            assert_eq!(rows[0].data.primary_percentage(), Some(40.0));
        }
        // The one-entry surfaces: aged as a whole, by its own windows.
        let shown = visible_data(&running_claude(), &display);
        assert!(matches!(shown[0].status, ProviderStatus::Ok));
    }

    /// Rows chosen that have nothing to show while a provider runs: the
    /// widget says so (and a click opens the picker) instead of pointing at
    /// the providers; with no provider running it still does that.
    #[test]
    fn rows_chosen_without_data_are_said_so() {
        let renderer = Renderer::new().expect("a system font");
        let mut config = running_claude();
        config.ui.rows.widget = Some(vec!["claude.fable".into()]);
        let display = [claude(vec![metric("Session", 40, Some(120), false)])];
        let shape = widget_shape(&config, &display, &Models::new(), &renderer);
        assert!(shape.unshown && !shape.choose);
        // Nothing running: not this box.
        let none: [ProviderData; 0] = [];
        assert!(!widget_shape(&config, &none, &Models::new(), &renderer).unshown);
        // A row to show: not this box either.
        config.ui.rows.widget = Some(vec!["claude".into()]);
        assert!(!widget_shape(&config, &display, &Models::new(), &renderer).unshown);
    }

    #[test]
    fn an_owed_window_waits_for_a_fullscreen_app() {
        let asked = std::cell::Cell::new(false);
        let fullscreen = |answer: bool| {
            let asked = &asked;
            move || {
                asked.set(true);
                answer
            }
        };
        assert_eq!(
            auto_open(PendingNotice::Welcome, false, fullscreen(false)),
            AutoOpen::Now
        );
        assert_eq!(
            auto_open(PendingNotice::UpdateNotice, false, fullscreen(true)),
            AutoOpen::Later
        );
        // Already open, or nothing owed: nothing to do, and the screen is
        // not even looked at.
        asked.set(false);
        assert_eq!(
            auto_open(PendingNotice::Welcome, true, fullscreen(true)),
            AutoOpen::No
        );
        assert_eq!(
            auto_open(PendingNotice::None, false, fullscreen(true)),
            AutoOpen::No
        );
        assert!(!asked.get());
    }
}
