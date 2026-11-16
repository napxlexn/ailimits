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

mod cache;
mod clipboard;
mod events;
mod flow;
#[cfg(target_os = "windows")]
mod indicator;
mod menu;
mod model_job;
mod provider_set;
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
    providers::{
        cursor::session::{self as cursor_session, SignInOutcome},
        ProviderData, ProviderId, ProviderStatus,
    },
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
    /// A Cursor sign-in from the menu has ended.
    CursorSignIn(SignInOutcome),
    /// The model labels were read (models/): each provider's model id and
    /// effort, nothing else of the files; with the generation of the
    /// request they answer.
    Models(u64, Models),
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

/// Providers visible in the widget: enabled, allowed and connected. Disabled
/// (enabled=false), not allowed to read their sign-in, and not-connected
/// (NotConfigured) ones are not rendered.
fn visible_data(config: &Config, display: &[ProviderData]) -> Vec<ProviderData> {
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
        // A window whose reset has provably passed is shown as ≈0%.
        .map(|d| d.aged_for_display())
        .collect()
}

/// The rows a surface shows (rows.rs), built from the providers the widget
/// shows. With the default config: one row per visible provider, untouched.
fn surface_rows(
    config: &Config,
    display: &[ProviderData],
    surface: Surface,
    models: &Models,
) -> Vec<RowView> {
    rows::visible_rows(config, &visible_data(config, display), surface, models)
}

/// What the widget's size depends on (layout.rs `Shape`): its rows, the
/// lines their models take, and whether nothing is chosen yet.
fn widget_shape(
    config: &Config,
    display: &[ProviderData],
    models: &Models,
    renderer: &Renderer,
) -> layout::Shape {
    layout::Shape::of(
        &config.ui,
        &surface_rows(config, display, Surface::Widget, models),
        rows::nothing_chosen(config),
        |text, size| renderer.text_width(text, size),
    )
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
fn open_picker_window<T: 'static>(
    target: &tao::event_loop::EventLoopWindowTarget<T>,
    renderer: &Rc<Renderer>,
    input: PickerInput,
    anchor: ScreenRect,
) -> Option<PickerWindow> {
    match PickerWindow::open(target, renderer.clone(), input, anchor) {
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
    cursor_sign_in: Option<tokio::task::JoinHandle<()>>,
    tray: Tray,
    panel: TaskbarPanel,
    #[cfg(target_os = "windows")]
    indicator: indicator::Indicator,

    // ── The picker: open only while in use ──
    found_tools: Vec<&'static str>,
    picker_mode: PickerMode,
    /// A provider the Providers menu asked about: once allowed, its own row
    /// comes back wherever rows were chosen.
    menu_asked: Option<String>,
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
    let mut config = runtime.block_on(storage::load_or_default())?;
    info!("Config loaded: {} providers", config.providers.len());

    // Toasts carry the app's own name and icon; the identity is registered
    // per user, before the first threshold alert can fire.
    crate::platform::register_toast_identity();

    // Rescue a saved position that now lands on no monitor (a display was
    // unplugged/resized since last run): a borderless, taskbar-skipped overlay
    // off every screen is invisible and un-draggable. Snap it back on-screen.
    #[cfg(target_os = "windows")]
    {
        let (cx, cy) = crate::platform::ensure_on_screen(config.window.pos_x, config.window.pos_y);
        config.window.pos_x = cx;
        config.window.pos_y = cy;
    }

    // 2. The event loop with user events.
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
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
    let model_job = ModelJob::new(&config);
    let cycle_hook: CycleHook = {
        let job = model_job.clone();
        let tx = cmd_tx.clone();
        let rt = runtime.handle().clone();
        Arc::new(move || spawn_model_read(&job, &tx, &rt))
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
    let win_layout = layout::compute(
        &config.ui.layout,
        &config.ui.detail,
        &config.ui.width_scale,
        &config.ui.column_flow,
        &shape,
    );
    let theme = ComputedTheme::compute(&config.ui);

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
        .with_position(PhysicalPosition::new(
            config.window.pos_x,
            config.window.pos_y,
        ));

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
    apply_windows_glass(&window);

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
    menu.set_cursor_signed_in(cursor_session::is_stored());
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
    #[cfg(target_os = "windows")]
    let indicator = indicator::Indicator::new();

    // 10. The window state.
    let win_state = WindowState {
        pos: (config.window.pos_x as f64, config.window.pos_y as f64),
        size: (win_layout.width as f64, win_layout.height as f64),
        pinned: config.window.pinned,
        ..Default::default()
    };

    let saver = storage::spawn_saver(runtime.handle(), storage::config_path());
    let config_saves = saver.handle();
    let picker_mode = picker_mode_for(&config);

    let mut app = App {
        runtime,
        cmd_tx,
        config,
        config_saves,
        saver_at_exit: Some(saver),
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
        cursor_sign_in: None,
        tray,
        panel,
        #[cfg(target_os = "windows")]
        indicator,
        found_tools: Vec::new(),
        picker_mode,
        menu_asked: None,
        picker: None,
        auto_open_waiting: false,
    };

    // On a fresh install the picker opens by itself as the welcome, after
    // the update that added [reads] as the one-time notice - unless a
    // fullscreen app is in front, which it waits for.
    match auto_open(
        app.config.reads.pending,
        false,
        crate::platform::foreground_fills_its_monitor,
    ) {
        AutoOpen::Now => app.open_picker(&event_loop, None),
        AutoOpen::Later => {
            info!("a fullscreen app is in front: the owed window waits for it");
            app.auto_open_waiting = true;
        }
        AutoOpen::No => {}
    }

    info!("AI Limits Widget ready");

    // 11. The event loop — never returns.
    event_loop.run(move |event, target, control_flow| app.handle(event, target, control_flow));
}

#[cfg(test)]
mod tests {
    use super::*;

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
