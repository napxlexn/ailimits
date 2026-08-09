// app.rs — main application state and the event loop.
//
// Threads:
//   Thread 1: the tao event loop — window events, rendering, AppCommand.
//   Thread 2+: the tokio runtime — the scheduler and provider fetches.

use crate::{
    config::{
        schema::{AuthMethod, Config},
        storage,
    },
    hooks,
    monitor::{
        scheduler::{Scheduler, SharedInterval, SharedProviders},
        trend::Trend,
    },
    notifications::toast::ToastNotifier,
    providers::{
        antigravity::AntigravityProvider, claude::ClaudeProvider, codex::CodexProvider,
        copilot::CopilotProvider, key_label_for, usage_token_label_for, Metric, MetricWindow,
        Provider, ProviderData, ProviderId, ProviderStatus,
    },
    ui::{
        context_menu::{ContextMenu, MenuAction},
        layout,
        renderer::{format_duration, Renderer},
        taskbar_panel::TaskbarPanel,
        theme::ComputedTheme,
        tray::Tray,
        window::WindowState,
    },
    updater,
};
use anyhow::{Context as AnyhowContext, Result};
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Commands for the event loop.
#[derive(Debug)]
pub enum AppCommand {
    /// Update a single provider's data.
    UpdateProvider(ProviderData),
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
}

/// Tray interactions we care about.
#[derive(Debug)]
pub enum TrayKind {
    /// Left-click → toggle the overlay window's visibility.
    LeftClick,
}

/// Re-evaluate the Panel indicator's tray fallback: show a tray icon while the
/// overlay cannot be seen — the Start/Search scrim occludes it (a protected
/// z-band), the cursor-peek "rude topmost" taskbar covers it (a floating
/// overlay cannot beat either), OR a fullscreen app owns the screen (the bar
/// sits under it without moving, so geometry checks are blind; the topmost
/// overlay would float over the game). Fullscreen also hides the panel window
/// itself, and the next evaluation after the app is gone (alt-tab back — a
/// foreground event) re-presents it, exactly like the taskbar reappears. When
/// a non-fullscreen fallback clears, a redraw re-presents the still-positioned
/// panel. No-op outside the Panel modes (every panel method guards on the mode).
#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn eval_indicator_fallback(
    panel: &mut TaskbarPanel,
    tray: &mut Tray,
    menu: &muda::Menu,
    providers: &[ProviderData],
    theme: &ComputedTheme,
    fallback_was_active: &mut bool,
    fullscreen_was_active: &mut bool,
    target: crate::config::schema::PanelDisplay,
) {
    let scrim = crate::platform::foreground_scrim_active(target);
    let fullscreen = crate::platform::fullscreen_foreground_active(target);
    // While hidden for fullscreen the rect is None, so is_covered reads false —
    // ordering matters: check coverage before this pass may hide the panel.
    let covered = panel.is_covered();
    let fallback = crate::platform::taskbar_geom::should_fall_back(scrim, covered, fullscreen);
    tray.set_scrim_fallback(fallback, menu, providers, theme);
    if fullscreen {
        // Idempotent — also re-hides the panel if anything re-presented it
        // since the last evaluation.
        panel.suppress_for_fullscreen();
    }
    if *fullscreen_was_active && !fullscreen {
        panel.restore_from_fullscreen(providers);
    } else if *fallback_was_active && !fallback {
        panel.redraw(providers);
    }
    *fullscreen_was_active = fullscreen;
    *fallback_was_active = fallback;
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

// ─── Provider cache ──────────────────────────────────────────────
// Metrics with a still-future reset survive widget restarts: after a
// relaunch the row shows the last real value (greyed, with its age) even if
// the source is temporarily unavailable, instead of an error text.

fn provider_cache_path() -> std::path::PathBuf {
    storage::config_path().with_file_name("provider-cache.json")
}

fn cache_metric_is_valid(metric: &Metric) -> bool {
    // Any metric whose window has not reset yet is still true and worth
    // keeping: after a widget restart the row then shows the last real value
    // (greyed, with its age) instead of an error text while the first cycles
    // fight the flaky endpoints. Metrics past their reset are dropped — the
    // window rolled over, the number is dead.
    metric
        .reset_at
        .is_some_and(|reset| reset + Duration::minutes(1) > Utc::now())
        && metric.percentage().is_some()
}

fn prune_provider_cache_entry(data: &ProviderData) -> Option<ProviderData> {
    if !matches!(data.status, ProviderStatus::Ok) {
        return None;
    }

    let metrics: Vec<Metric> = data
        .metrics
        .iter()
        .filter(|metric| cache_metric_is_valid(metric))
        .cloned()
        .collect();

    (!metrics.is_empty()).then(|| ProviderData {
        metrics,
        ..data.clone()
    })
}

fn cacheable_provider_data(data: &ProviderData) -> Option<ProviderData> {
    prune_provider_cache_entry(data)
}

/// Whether a live metric already occupies the slot a cached metric would fill.
///
/// The label is authoritative when both sides have one. Beyond that, only
/// session-window metrics may collapse onto a shared slot by unit: providers
/// rename those rows between fetches (Antigravity follows Google's model
/// rotation), whereas long windows are distinct named pools — Claude reports
/// Weekly, Opus and Sonnet at once — so two differently-labelled long metrics
/// are always different slots.
fn metrics_match_slot(a: &Metric, b: &Metric) -> bool {
    if a.label.eq_ignore_ascii_case(&b.label) {
        return true;
    }
    if matches!(a.window, MetricWindow::Long) || matches!(b.window, MetricWindow::Long) {
        return false;
    }
    std::mem::discriminant(&a.unit) == std::mem::discriminant(&b.unit)
}

fn merge_cached_metrics(mut data: ProviderData, cached: Option<&ProviderData>) -> ProviderData {
    let Some(cached) = cached else {
        return data;
    };
    if data.id != cached.id || !matches!(cached.status, ProviderStatus::Ok) {
        return data;
    }

    for cached_metric in &cached.metrics {
        if !data
            .metrics
            .iter()
            .any(|metric| metrics_match_slot(metric, cached_metric))
        {
            data.metrics.push(cached_metric.clone());
        }
    }
    data
}

fn prune_provider_cache_map(cache: &mut HashMap<ProviderId, ProviderData>) -> bool {
    let before: HashMap<ProviderId, usize> = cache
        .iter()
        .map(|(id, data)| (id.clone(), data.metrics.len()))
        .collect();
    let pruned: HashMap<ProviderId, ProviderData> = cache
        .values()
        .filter_map(prune_provider_cache_entry)
        .map(|data| (data.id.clone(), data))
        .collect();
    let changed = before.len() != pruned.len()
        || pruned.iter().any(|(id, data)| {
            before
                .get(id)
                .is_none_or(|metric_count| *metric_count != data.metrics.len())
        });
    *cache = pruned;
    changed
}

async fn load_provider_cache() -> HashMap<ProviderId, ProviderData> {
    let path = provider_cache_path();
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(content) => content,
        Err(_) => return HashMap::new(),
    };

    match serde_json::from_str::<Vec<ProviderData>>(&content) {
        Ok(items) => items
            .into_iter()
            .filter_map(|data| prune_provider_cache_entry(&data))
            .map(|data| (data.id.clone(), data))
            .collect(),
        Err(e) => {
            warn!("provider cache parse failed: {e}");
            HashMap::new()
        }
    }
}

async fn save_provider_cache(cache: HashMap<ProviderId, ProviderData>) -> Result<()> {
    let path = provider_cache_path();
    let mut items: Vec<ProviderData> = cache
        .into_values()
        .filter_map(|data| prune_provider_cache_entry(&data))
        .collect();

    if items.is_empty() {
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    items.sort_by_key(|d| match d.id {
        ProviderId::Claude => 0,
        ProviderId::Codex => 1,
        ProviderId::Copilot => 2,
        ProviderId::Antigravity => 3,
    });
    let content = serde_json::to_string_pretty(&items)?;
    storage::atomic_write(&path, content.as_bytes()).await?;
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────

/// Build provider instances + alert thresholds from the config.
fn build_providers(config: &Config) -> (Vec<Arc<dyn Provider>>, HashMap<ProviderId, u8>) {
    let mut providers: Vec<Arc<dyn Provider>> = Vec::new();
    let mut thresholds: HashMap<ProviderId, u8> = HashMap::new();
    for pc in &config.providers {
        let (provider, id): (Arc<dyn Provider>, ProviderId) = match pc.id.as_str() {
            "claude" => (
                Arc::new(ClaudeProvider::new(pc.clone())),
                ProviderId::Claude,
            ),
            "codex" => (Arc::new(CodexProvider::new(pc.clone())), ProviderId::Codex),
            "copilot" => (
                Arc::new(CopilotProvider::new(pc.clone())),
                ProviderId::Copilot,
            ),
            // "gemini" is the pre-rename config id; storage::load migrates
            // it, this arm just covers a config edited back by hand.
            "antigravity" | "gemini" => (
                Arc::new(AntigravityProvider::new(pc.clone())),
                ProviderId::Antigravity,
            ),
            other => {
                warn!("unknown provider '{other}' in config, skipping");
                continue;
            }
        };
        thresholds.insert(id, pc.alert_threshold);
        providers.push(provider);
    }
    (providers, thresholds)
}

/// Read a key/token from the clipboard, with validation.
fn read_clipboard_key() -> Result<String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| anyhow::anyhow!("clipboard unavailable: {e}"))?;
    let text = clipboard
        .get_text()
        .map_err(|_| anyhow::anyhow!("clipboard is empty or holds no text"))?;
    let key = text.trim().to_string();
    // A rough sanity check that this looks like a key.
    if key.len() < 10 || key.len() > 500 || key.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        anyhow::bail!("clipboard content does not look like a key");
    }
    Ok(key)
}

/// Quiet toast feedback for menu actions.
fn feedback(title: &str, body: &str) {
    if let Err(e) = ToastNotifier::show(title, body) {
        warn!("feedback toast failed: {e}");
    }
}

/// Providers visible in the widget: enabled and connected. Disabled
/// (enabled=false) and not-connected (NotConfigured) ones are not rendered.
fn visible_data(config: &Config, display: &[ProviderData]) -> Vec<ProviderData> {
    display
        .iter()
        .filter(|d| {
            let enabled = config
                .providers
                .iter()
                .find(|p| ProviderId::from_config_id(&p.id) == Some(d.id.clone()))
                .is_some_and(|p| p.enabled);
            enabled && !matches!(d.status, ProviderStatus::NotConfigured)
        })
        // Stale data is extrapolated: windows whose reset has passed → ≈0%.
        .map(|d| d.aged_for_display())
        .collect()
}

/// Rebuild the providers after a config change — the list is shared
/// with the scheduler.
fn refresh_providers(
    config: &Config,
    providers: &SharedProviders,
    thresholds: &mut HashMap<ProviderId, u8>,
    display: &mut [ProviderData],
    changed_id: &str,
) {
    let (built, new_thresholds) = build_providers(config);
    *thresholds = new_thresholds;
    match providers.write() {
        Ok(mut guard) => *guard = built,
        Err(poisoned) => *poisoned.into_inner() = built,
    }
    // The changed provider's row → Loading until a fresh fetch lands.
    if let Some(pid) = ProviderId::from_config_id(changed_id) {
        if let Some(slot) = display.iter_mut().find(|d| d.id == pid) {
            *slot = loading_data(pid);
        }
    }
}

/// Out-of-band fetch of a single provider.
fn fetch_one(
    providers: &SharedProviders,
    cmd_tx: &mpsc::Sender<AppCommand>,
    runtime: &tokio::runtime::Runtime,
    changed_id: &str,
) {
    let target = ProviderId::from_config_id(changed_id);
    let list: Vec<Arc<dyn Provider>> = match providers.read() {
        Ok(g) => g.clone(),
        Err(p) => p.into_inner().clone(),
    };
    for provider in list {
        if Some(provider.id()) != target {
            continue;
        }
        let tx = cmd_tx.clone();
        runtime.spawn(async move {
            if let Ok(data) = provider.fetch().await {
                let _ = tx.send(AppCommand::UpdateProvider(data)).await;
            }
        });
    }
}

#[cfg(target_os = "windows")]
fn apply_windows_glass(window: &tao::window::Window) {
    use std::ffi::c_void;
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::HWND as WinHwnd;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
    use windows_sys::Win32::Foundation::{BOOL, HWND};
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

    #[repr(C)]
    struct AccentPolicy {
        accent_state: u32,
        accent_flags: u32,
        gradient_color: u32,
        animation_id: u32,
    }

    #[repr(C)]
    struct WindowCompositionAttribData {
        attrib: u32,
        data: *mut c_void,
        size: usize,
    }

    type SetWindowCompositionAttribute =
        unsafe extern "system" fn(HWND, *mut WindowCompositionAttribData) -> BOOL;

    const WCA_ACCENT_POLICY: u32 = 0x13;
    const ACCENT_DISABLED: u32 = 0;
    // BLURBEHIND (3), NOT ACRYLICBLURBEHIND (4): acrylic has a known bug —
    // DWM drops the blur on drag/monitor change leaving a solid matte tint.
    const ACCENT_ENABLE_BLURBEHIND: u32 = 3;

    // Glass tint — the single dark theme.
    let (tint_r, tint_g, tint_b, tint_a): (u32, u32, u32, u32) = (18, 18, 18, 28);

    unsafe {
        let hwnd = window.hwnd() as HWND;
        let user32 = LoadLibraryA(c"user32.dll".as_ptr().cast());
        if user32.is_null() {
            warn!("acrylic accent unavailable: failed to load user32.dll");
        } else if let Some(proc) =
            GetProcAddress(user32, c"SetWindowCompositionAttribute".as_ptr().cast())
        {
            let set_window_composition_attribute: SetWindowCompositionAttribute =
                std::mem::transmute(proc);
            let apply_state = |state: u32, color: u32| {
                let mut accent = AccentPolicy {
                    accent_state: state,
                    accent_flags: 0,
                    // AABBGGRR format.
                    gradient_color: color,
                    animation_id: 0,
                };
                let mut data = WindowCompositionAttribData {
                    attrib: WCA_ACCENT_POLICY,
                    data: &mut accent as *mut _ as _,
                    size: std::mem::size_of_val(&accent),
                };
                set_window_composition_attribute(hwnd, &mut data as *mut _);
            };
            // The off→on cycle forces DWM to restart the blur if it got stuck.
            apply_state(ACCENT_DISABLED, 0);
            apply_state(
                ACCENT_ENABLE_BLURBEHIND,
                tint_r | (tint_g << 8) | (tint_b << 16) | (tint_a << 24),
            );
        } else {
            warn!("acrylic accent unavailable: SetWindowCompositionAttribute missing");
        }

        // Native Win11 rounded corners: DWMWA_WINDOW_CORNER_PREFERENCE=33, ROUND=2.
        let pref: u32 = 2;
        let _ = DwmSetWindowAttribute(
            WinHwnd(window.hwnd() as _),
            DWMWINDOWATTRIBUTE(33),
            &pref as *const u32 as _,
            std::mem::size_of::<u32>() as u32,
        );

        // Win11 strokes a 1px border around a rounded top-level window
        // (DWMWA_BORDER_COLOR=34) whose default dark-mode color is a faint light
        // grey — a visible hairline. DWMWA_COLOR_NONE (0xFFFFFFFE) removes it
        // entirely; with the undecorated shadow off (client == window, see the
        // builder) there is no 1px frame left to expose, so the edge is just the
        // widget's own translucent fill — seamless on any backdrop, no hairline.
        let border_none: u32 = 0xFFFF_FFFE;
        let _ = DwmSetWindowAttribute(
            WinHwnd(window.hwnd() as _),
            DWMWINDOWATTRIBUTE(34),
            &border_none as *const u32 as _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// Left-click on the tray icon or mini panel: if the overlay is already visible
/// and the front-most window at its own centre, hide it; otherwise (hidden, or
/// covered by another window) show it and raise it to the front. So a single
/// click always brings the widget forward, and a second one tucks it away.
fn toggle_or_raise_widget(
    window: &tao::window::Window,
    win_state: &WindowState,
    window_visible: &mut bool,
) {
    #[cfg(target_os = "windows")]
    use tao::platform::windows::WindowExtWindows;
    #[cfg(not(target_os = "windows"))]
    let _ = win_state; // coverage test is Windows-only
    #[cfg(target_os = "windows")]
    let already_front = {
        let hwnd = window.hwnd();
        let cx = (win_state.pos.0 + win_state.size.0 / 2.0) as i32;
        let cy = (win_state.pos.1 + win_state.size.1 / 2.0) as i32;
        *window_visible && crate::platform::point_owner(cx, cy) == hwnd
    };
    #[cfg(not(target_os = "windows"))]
    let already_front = *window_visible;

    if already_front {
        *window_visible = false;
        window.set_visible(false);
    } else {
        *window_visible = true;
        window.set_visible(true);
        #[cfg(target_os = "windows")]
        {
            apply_windows_glass(window);
            crate::platform::bring_to_front(window.hwnd());
        }
        window.request_redraw();
    }
}

/// Apply a live UI change: recompute the layout/theme, resize the window
/// and the buffers.
#[allow(clippy::too_many_arguments)]
fn apply_ui_change(
    config: &Config,
    win_layout: &mut layout::WindowLayout,
    theme: &mut ComputedTheme,
    pixmap: &mut tiny_skia::Pixmap,
    surface: &mut softbuffer::Surface<Rc<tao::window::Window>, Rc<tao::window::Window>>,
    window: &Rc<tao::window::Window>,
    win_state: &mut WindowState,
    menu: &ContextMenu,
    provider_count: usize,
) {
    *win_layout = layout::compute(
        &config.ui.layout,
        &config.ui.detail,
        &config.ui.width_scale,
        &config.ui.column_flow,
        provider_count,
    );
    *theme = ComputedTheme::compute(&config.ui);

    let (w, h) = (win_layout.width as u32, win_layout.height as u32);
    window.set_inner_size(PhysicalSize::new(w, h));
    if let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) {
        if let Err(e) = surface.resize(nw, nh) {
            warn!("surface resize failed: {e}");
        }
    }
    if let Some(pm) = tiny_skia::Pixmap::new(w, h) {
        *pixmap = pm;
    }
    win_state.size = (win_layout.width as f64, win_layout.height as f64);

    // Resizing keeps the top-left corner, so a widget parked against the right
    // or bottom edge would grow straight off the screen. Pull it back into the
    // work area of the monitor it sits on.
    if let Ok(wp) = window.outer_position() {
        let pos = (wp.x as f64, wp.y as f64);
        let centre = (
            (pos.0 + win_state.size.0 / 2.0) as i32,
            (pos.1 + win_state.size.1 / 2.0) as i32,
        );
        if let Some((l, t, r, b)) = crate::platform::work_area_at(centre.0, centre.1) {
            let work = (l as f64, t as f64, r as f64, b as f64);
            let (nx, ny) = crate::ui::window::clamp_to_work_area(pos, win_state.size, work);
            if (nx, ny) != pos {
                window.set_outer_position(PhysicalPosition::new(nx as i32, ny as i32));
                win_state.pos = (nx, ny);
            }
        }
    }

    menu.sync(config, win_state.pinned);
    window.request_redraw();
}

/// Entry point — run the application.
pub fn run() -> Result<()> {
    // 1. The tokio runtime + the config. 2 workers instead of one-per-core:
    // we make 3 light HTTP requests per cycle; the default 32 workers would
    // waste ~30 threads and RAM.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;
    let mut config = runtime.block_on(storage::load_or_default())?;
    info!("Config loaded: {} providers", config.providers.len());

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
    // The list is shared with the scheduler and rebuilt live from the menu.
    let (built, mut thresholds) = build_providers(&config);
    let mut provider_cache = runtime.block_on(load_provider_cache());
    let mut display: Vec<ProviderData> = built
        .iter()
        .map(|p| {
            let id = p.id();
            provider_cache
                .get(&id)
                .cloned()
                .unwrap_or_else(|| loading_data(id))
        })
        .collect();
    let providers: SharedProviders = Arc::new(RwLock::new(built));
    if display.is_empty() {
        anyhow::bail!("no providers in config");
    }

    // 4. The background scheduler; the interval is shared — changed live
    // from the menu.
    let update_interval: SharedInterval =
        Arc::new(AtomicU64::new(config.general.update_interval_secs));
    let scheduler = Scheduler::new(providers.clone(), update_interval.clone(), cmd_tx.clone());
    runtime.spawn(scheduler.run());

    // 4b. Background auto-updater — polls GitHub Releases and silently installs
    // newer builds. The enabled flag is shared so the menu toggle takes effect
    // without a restart.
    let auto_update_enabled = Arc::new(AtomicBool::new(config.general.auto_update));
    runtime.spawn(updater::run(auto_update_enabled.clone()));

    // 5. Layout, theme, renderer — mut: switched live from the menu.
    // The layout is computed from VISIBLE providers, not all of them.
    let mut visible_count = visible_data(&config, &display).len();
    let mut win_layout = layout::compute(
        &config.ui.layout,
        &config.ui.detail,
        &config.ui.width_scale,
        &config.ui.column_flow,
        visible_count,
    );
    let mut theme = ComputedTheme::compute(&config.ui);
    let renderer = Renderer::new()?;

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
        // Dropping it makes the client fill the whole window so the edge is the
        // widget's own translucent fill — truly borderless, seamless on any bg.
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

    let mut pixmap = tiny_skia::Pixmap::new(buf_w, buf_h).context("pixmap alloc failed")?;

    // Per-provider threshold-event cooldown (shared by toast and on_threshold).
    let mut last_toast: HashMap<ProviderId, std::time::Instant> = HashMap::new();
    // Previous primary % per provider, for reset-edge detection.
    let mut last_pct: HashMap<ProviderId, f32> = HashMap::new();
    // Whether the on_startup hook has fired (once after the first success).
    let mut startup_fired = false;
    // Burn-rate trend buffers + the latest forecast per provider, stored as
    // the ABSOLUTE projected hit moment: the renderer derives "time left" from
    // it live, so the label ticks down between updates and expires on its own
    // instead of freezing at the duration computed on the last fetch.
    let mut trends: HashMap<ProviderId, Trend> = HashMap::new();
    let mut forecasts: HashMap<ProviderId, chrono::DateTime<Utc>> = HashMap::new();
    let empty_forecasts: HashMap<ProviderId, chrono::DateTime<Utc>> = HashMap::new();
    // Last fetch error per provider, kept after retention replaces the slot
    // with old data: hovering a greyed/estimated row shows this as the
    // "why" (the slot itself only knows the retained values, not the cause).
    let mut last_errors: HashMap<ProviderId, String> = HashMap::new();

    // 9. The context menu: events → proxy.
    let menu = ContextMenu::new(&config, config.window.pinned)?;
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
    panel.set_mode(config.general.indicator, &visible_data(&config, &display));
    let mut window_visible = true;
    // The indicator falls back to a tray icon while a Panel overlay cannot be
    // seen — the Start/Search scrim is up, OR the cursor-peek "rude topmost"
    // taskbar covers it. `fallback_was_active` tracks the previous state so the
    // panel is re-presented exactly when the fallback clears. The shell can
    // cover the overlay with no move/foreground event, so a reorder cue arms a
    // coalesced re-check; at idle no reorders fire, so CPU stays 0%.
    #[cfg(target_os = "windows")]
    let mut fallback_was_active = false;
    // Whether the last evaluation saw a fullscreen app: the panel is hidden
    // while true; the falling edge re-presents it (alt-tab back to desktop).
    #[cfg(target_os = "windows")]
    let mut fullscreen_was_active = false;
    #[cfg(target_os = "windows")]
    let mut recheck_at: Option<std::time::Instant> = None;
    // The hover tooltip appears only after the cursor lingers for the system
    // mouse-hover time (the same delay tray-icon tooltips use), tracked by this
    // deadline; it is cancelled the moment the cursor leaves.
    #[cfg(target_os = "windows")]
    let mut tooltip_at: Option<std::time::Instant> = None;
    #[cfg(target_os = "windows")]
    let hover_delay = std::time::Duration::from_millis(crate::platform::mouse_hover_time_ms());

    // 10. The window state.
    let mut win_state = WindowState {
        pos: (config.window.pos_x as f64, config.window.pos_y as f64),
        size: (win_layout.width as f64, win_layout.height as f64),
        pinned: config.window.pinned,
        ..Default::default()
    };
    let mut cursor_inside_window = false;

    let rt_handle = runtime.handle().clone();
    let cache_rt_handle = rt_handle.clone();
    let save_cached_providers = move |cache: HashMap<ProviderId, ProviderData>| {
        cache_rt_handle.spawn(async move {
            if let Err(e) = save_provider_cache(cache).await {
                warn!("provider cache save failed: {e}");
            }
        });
    };

    // Save the config in the background through a single serialised writer
    // (storage::spawn_saver): a burst coalesces to the latest config on a
    // one-slot channel, so the newest setting always wins and a slow disk
    // cannot grow the queue. The saver itself is kept for the shutdown
    // handshake so the final write cannot race an in-flight background one.
    let saver = storage::spawn_saver(&rt_handle, storage::config_path());
    let save_config = {
        let handle = saver.handle();
        move |cfg: Config| handle.save(cfg)
    };
    let mut saver_at_exit = Some(saver);

    info!("AI Limits Widget ready");

    // 11. The event loop — never returns.
    event_loop.run(move |event, _target, control_flow| {
        #[cfg(target_os = "windows")]
        {
            let next = [recheck_at, tooltip_at].into_iter().flatten().min();
            *control_flow = match next {
                Some(t) => ControlFlow::WaitUntil(t),
                None => ControlFlow::Wait,
            };
        }
        #[cfg(not(target_os = "windows"))]
        {
            *control_flow = ControlFlow::Wait;
        }

        match event {
            // Scheduled deadlines came due: re-check overlay coverage (toggle the
            // tray fallback), and/or show the hover tooltip after its delay.
            // Idle returns to 0% afterwards (no pending timer).
            #[cfg(target_os = "windows")]
            Event::NewEvents(_) => {
                let now = std::time::Instant::now();
                if let Some(t) = recheck_at {
                    if now >= t {
                        recheck_at = None;
                        eval_indicator_fallback(
                            &mut panel,
                            &mut tray,
                            &menu.menu,
                            &visible_data(&config, &display),
                            &theme,
                            &mut fallback_was_active,
                            &mut fullscreen_was_active,
                            config.general.panel_display,
                        );
                    }
                }
                if let Some(t) = tooltip_at {
                    if now >= t {
                        tooltip_at = None;
                        panel.show_tooltip(&visible_data(&config, &display));
                    }
                }
            }
            // The embedded panel's own events: a left-click toggles the
            // overlay (like the tray icon), a right-click opens the menu.
            Event::WindowEvent {
                window_id, event, ..
            } if window_id == panel.window_id() => {
                match event {
                    WindowEvent::MouseInput {
                        state: ElementState::Released,
                        button: MouseButton::Left,
                        ..
                    } => {
                        toggle_or_raise_widget(&window, &win_state, &mut window_visible);
                    }
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Right,
                        ..
                    } => {
                        #[cfg(target_os = "windows")]
                        {
                            use muda::ContextMenu as _;
                            unsafe {
                                menu.menu.show_context_menu_for_hwnd(panel.hwnd(), None);
                            }
                        }
                    }
                    // Our own hover tooltip: arm the system hover delay on
                    // enter/hover (show only if the cursor lingers, like a
                    // tray-icon tooltip), hide and cancel on leave.
                    WindowEvent::CursorEntered { .. } | WindowEvent::CursorMoved { .. } => {
                        #[cfg(target_os = "windows")]
                        if !panel.tooltip_shown() && tooltip_at.is_none() {
                            tooltip_at = Some(std::time::Instant::now() + hover_delay);
                        }
                    }
                    WindowEvent::CursorLeft { .. } => {
                        panel.hide_tooltip();
                        #[cfg(target_os = "windows")]
                        {
                            tooltip_at = None;
                        }
                    }
                    _ => {}
                }
            }

            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,

                WindowEvent::Focused(_) => {
                    #[cfg(target_os = "windows")]
                    apply_windows_glass(&window);
                    window.request_redraw();
                }

                // The system switched light/dark theme — repaint the taskbar
                // panel and the tray icon so their monochrome ink keeps
                // contrasting with the bar (the tray follows the taskbar theme,
                // not the widget's).
                WindowEvent::ThemeChanged(_) => {
                    panel.update(&visible_data(&config, &display), true);
                    tray.update(&visible_data(&config, &display), &theme, true);
                }

                // A DPI change when crossing monitors: pin the physical size
                // and re-apply the accent — otherwise the second monitor
                // shows a white background.
                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    *new_inner_size =
                        PhysicalSize::new(win_layout.width as u32, win_layout.height as u32);
                    #[cfg(target_os = "windows")]
                    apply_windows_glass(&window);
                    window.request_redraw();
                }

                WindowEvent::CursorMoved { position, .. } => {
                    cursor_inside_window = true;
                    win_state.cursor_in_window = (position.x, position.y);
                    if let Ok(wp) = window.outer_position() {
                        // Screen cursor = window position + in-window position.
                        let screen = (wp.x as f64 + position.x, wp.y as f64 + position.y);
                        if win_state.drag.active {
                            // Hold Shift while dragging → soft-magnet the widget
                            // to the nearest work-area edge of the monitor it is
                            // currently over.
                            let magnet = if crate::platform::shift_held() {
                                let cx = (win_state.pos.0 + win_state.size.0 / 2.0) as i32;
                                let cy = (win_state.pos.1 + win_state.size.1 / 2.0) as i32;
                                crate::platform::work_area_at(cx, cy).map(|(l, t, r, b)| {
                                    (
                                        (l as f64, t as f64, r as f64, b as f64),
                                        crate::ui::window::MAGNET_RANGE,
                                    )
                                })
                            } else {
                                None
                            };
                            if let Some((nx, ny)) = win_state.update_drag(screen, magnet) {
                                window.set_outer_position(PhysicalPosition::new(nx, ny));
                            }
                        } else if win_state.drag_pending {
                            // The drag starts from the REAL current cursor
                            // position, not the one cached at click time —
                            // otherwise the window jumped to the click point
                            // after the context menu.
                            win_state.drag_pending = false;
                            win_state.pos = (wp.x as f64, wp.y as f64);
                            win_state.begin_drag(screen);
                        }
                    }
                    window.request_redraw();
                }

                WindowEvent::CursorLeft { .. } => {
                    cursor_inside_window = false;
                    window.request_redraw();
                }

                WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) => {
                        // Drag from anywhere, but it starts only on the first
                        // cursor move; a locked widget will not move.
                        if !config.window.locked {
                            win_state.drag_pending = true;
                        }
                    }
                    (ElementState::Released, MouseButton::Left) => {
                        if win_state.end_drag() {
                            // Persist the position to the config.
                            config.window.pos_x = win_state.pos.0.round() as i32;
                            config.window.pos_y = win_state.pos.1.round() as i32;
                            save_config(config.clone());
                            // The accent gets lost when dragging to another
                            // monitor — re-apply it.
                            #[cfg(target_os = "windows")]
                            apply_windows_glass(&window);
                            window.request_redraw();
                        }
                    }
                    (ElementState::Pressed, MouseButton::Right) => {
                        // The native context menu at the cursor.
                        #[cfg(target_os = "windows")]
                        {
                            use muda::ContextMenu as _;
                            use tao::platform::windows::WindowExtWindows;
                            unsafe {
                                menu.menu
                                    .show_context_menu_for_hwnd(window.hwnd() as _, None);
                            }
                        }
                    }
                    _ => {}
                },

                _ => {}
            },

            Event::RedrawRequested(id) if id == panel.window_id() => {
                panel.redraw(&visible_data(&config, &display));
            }

            Event::RedrawRequested(_) => {
                // Render only the visible providers.
                let visible = visible_data(&config, &display);
                renderer.draw(
                    &mut pixmap,
                    &win_layout,
                    &theme,
                    &visible,
                    config.window.opacity,
                    &config.ui.detail,
                    cursor_inside_window.then_some((
                        win_state.cursor_in_window.0 as f32,
                        win_state.cursor_in_window.1 as f32,
                    )),
                    if config.ui.show_forecast {
                        &forecasts
                    } else {
                        &empty_forecasts
                    },
                    &last_errors,
                );
                // Copy the pixmap into softbuffer: RGBA → 0xAARRGGBB.
                if let Ok(mut buffer) = surface.buffer_mut() {
                    for (dst, src) in buffer.iter_mut().zip(pixmap.pixels().iter()) {
                        let c = src.demultiply();
                        *dst = ((c.alpha() as u32) << 24)
                            | ((c.red() as u32) << 16)
                            | ((c.green() as u32) << 8)
                            | (c.blue() as u32);
                    }
                    if let Err(e) = buffer.present() {
                        warn!("present failed: {e}");
                    }
                }
            }

            Event::UserEvent(ue) => match ue {
                UserEvent::Command(cmd) => match cmd {
                    AppCommand::UpdateProvider(data) => {
                        // Threshold crossing drives BOTH the toast and the
                        // `on_threshold` hook, sharing one cooldown. The hook
                        // fires even if toasts are disabled (it's its own opt-in).
                        if let Some(pct) = data.primary_percentage() {
                            let threshold = thresholds.get(&data.id).copied().unwrap_or(80) as f32;
                            let cooldown = std::time::Duration::from_secs(
                                config.notifications.cooldown_minutes.max(1) as u64 * 60,
                            );
                            let cooled = last_toast
                                .get(&data.id)
                                .is_none_or(|t| t.elapsed() >= cooldown);
                            if pct >= threshold && cooled {
                                last_toast.insert(data.id.clone(), std::time::Instant::now());
                                if config.notifications.enabled {
                                    let body = match data.headline_reset() {
                                        Some(t) => {
                                            let secs = t
                                                .signed_duration_since(Utc::now())
                                                .num_seconds()
                                                .max(0);
                                            format!(
                                                "{}% used, resets in {}",
                                                pct.round() as u32,
                                                format_duration(secs)
                                            )
                                        }
                                        None => format!("{}% used", pct.round() as u32),
                                    };
                                    let title = format!("{} limit", data.id.display_name());
                                    if let Err(e) = ToastNotifier::show(&title, &body) {
                                        warn!("toast failed: {e}");
                                    }
                                }
                                hooks::run(
                                    &runtime,
                                    &config.hooks.on_threshold,
                                    hooks::HookEvent::Threshold,
                                    data.id.clone(),
                                    Some(pct),
                                    data.headline_reset(),
                                );
                            }

                            // Reset edge: a provider that was near its limit
                            // (>= threshold) dropping sharply means a window
                            // rolled over. Self-debouncing: after firing, the
                            // tracked % is low, so it won't refire until usage
                            // climbs back above the threshold and drops again.
                            if let Some(&prev) = last_pct.get(&data.id) {
                                if prev >= threshold && pct + 30.0 < prev {
                                    hooks::run(
                                        &runtime,
                                        &config.hooks.on_reset,
                                        hooks::HookEvent::Reset,
                                        data.id.clone(),
                                        Some(pct),
                                        data.headline_reset(),
                                    );
                                }
                            }
                            last_pct.insert(data.id.clone(), pct);

                            // Burn-rate trend: record the sample and recompute
                            // this provider's forecast (suppressed if the window
                            // resets before the projected hit).
                            let trend = trends.entry(data.id.clone()).or_default();
                            trend.push(Utc::now(), pct);
                            match trend.forecast(data.headline_reset()) {
                                Some(f) if f.hit_at > Utc::now() => {
                                    forecasts.insert(data.id.clone(), f.hit_at);
                                }
                                _ => {
                                    forecasts.remove(&data.id);
                                }
                            }
                        }

                        // Startup edge: once, after the first successful fetch.
                        if !startup_fired && matches!(data.status, ProviderStatus::Ok) {
                            startup_fired = true;
                            hooks::run(
                                &runtime,
                                &config.hooks.on_startup,
                                hooks::HookEvent::Startup,
                                data.id.clone(),
                                data.primary_percentage(),
                                data.headline_reset(),
                            );
                        }

                        // Remember the failure text (or clear it) for the
                        // hover explanation on greyed/estimated rows.
                        match &data.status {
                            ProviderStatus::NetworkError(msg) | ProviderStatus::AuthError(msg) => {
                                last_errors.insert(data.id.clone(), msg.clone());
                            }
                            ProviderStatus::Ok => {
                                last_errors.remove(&data.id);
                            }
                            _ => {}
                        }

                        // A transient error must NOT wipe the last real data —
                        // it stays on screen; the renderer greys it out with
                        // its age. Applies to every provider.
                        let is_error = matches!(
                            data.status,
                            ProviderStatus::NetworkError(_) | ProviderStatus::AuthError(_)
                        );
                        if prune_provider_cache_map(&mut provider_cache) {
                            save_cached_providers(provider_cache.clone());
                        }

                        let is_success = matches!(data.status, ProviderStatus::Ok);
                        let cached_before_update = provider_cache.get(&data.id).cloned();
                        let display_data = if is_success {
                            merge_cached_metrics(data.clone(), cached_before_update.as_ref())
                        } else {
                            data.clone()
                        };
                        if is_success {
                            provider_cache.remove(&data.id);
                        }
                        if let Some(cached_data) = cacheable_provider_data(&display_data) {
                            provider_cache.insert(cached_data.id.clone(), cached_data);
                        }
                        if is_success {
                            save_cached_providers(provider_cache.clone());
                        }
                        let had_real_data = display.iter().any(|d| {
                            d.id == data.id
                                && matches!(d.status, ProviderStatus::Ok)
                                && !d.metrics.is_empty()
                        }) || provider_cache.contains_key(&data.id);
                        if is_error && had_real_data {
                            if let Some(cached) = provider_cache.get(&data.id).cloned() {
                                if let Some(slot) = display.iter_mut().find(|d| d.id == data.id) {
                                    if !matches!(slot.status, ProviderStatus::Ok)
                                        || slot.metrics.is_empty()
                                    {
                                        *slot = cached;
                                    } else {
                                        *slot = merge_cached_metrics(slot.clone(), Some(&cached));
                                    }
                                }
                            }
                            warn!(
                                "provider {:?}: {:?} — keeping last data on screen",
                                data.id, data.status
                            );
                        } else if let Some(slot) = display.iter_mut().find(|d| d.id == data.id) {
                            *slot = display_data;
                        }
                        // A provider appeared/disappeared (e.g. became
                        // NotConfigured) → resize.
                        let new_count = visible_data(&config, &display).len();
                        if new_count != visible_count {
                            visible_count = new_count;
                            apply_ui_change(
                                &config,
                                &mut win_layout,
                                &mut theme,
                                &mut pixmap,
                                &mut surface,
                                &window,
                                &mut win_state,
                                &menu,
                                visible_count,
                            );
                            tray.sync_providers(
                                &menu.menu,
                                &visible_data(&config, &display),
                                &theme,
                            );
                            panel.update(&visible_data(&config, &display), true);
                        }
                        window.request_redraw();
                        tray.update(&visible_data(&config, &display), &theme, false);
                        panel.update(&visible_data(&config, &display), false);
                    }
                    AppCommand::Quit => *control_flow = ControlFlow::Exit,
                },

                UserEvent::TaskbarMoved => {
                    // While fullscreen-suppressed this is a no-op (the panel
                    // gates every presentation path itself).
                    panel.on_taskbar_moved(&visible_data(&config, &display));
                    // The bar settled into a new position; re-check coverage a
                    // beat later (the "rude topmost" front lands after the slide).
                    #[cfg(target_os = "windows")]
                    {
                        recheck_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_millis(150));
                    }
                }

                UserEvent::PanelRaise => {
                    // The foreground changed (tray overflow flyout, Start menu, a
                    // fullscreen game, an app). Evaluate the fallback FIRST: if a
                    // fullscreen app just took over, this hides the panel, and the
                    // raise below becomes a no-op (rect=None) — never re-asserting
                    // the overlay above a game. Arm a delayed re-check too — the
                    // auto-hide bar can peek back (or the shell can flip its
                    // fullscreen state) a moment later with no further event.
                    #[cfg(target_os = "windows")]
                    {
                        eval_indicator_fallback(
                            &mut panel,
                            &mut tray,
                            &menu.menu,
                            &visible_data(&config, &display),
                            &theme,
                            &mut fallback_was_active,
                            &mut fullscreen_was_active,
                            config.general.panel_display,
                        );
                        recheck_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_millis(150));
                    }
                    panel.raise();
                }

                // The shell re-stacked the taskbar (it may have fronted the bar
                // above our overlay with no move/foreground event). Arm the
                // coalesced re-check; a burst of reorders collapses to one.
                #[cfg(target_os = "windows")]
                UserEvent::PanelRecheck => {
                    recheck_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(150));
                }

                UserEvent::Tray(TrayKind::LeftClick) => {
                    // Bring the overlay to the front (or hide it if it is already
                    // there); the tray icon stays either way.
                    toggle_or_raise_widget(&window, &win_state, &mut window_visible);
                }

                UserEvent::Menu(id) => match menu.action_for(&id) {
                    Some(MenuAction::Quit) => *control_flow = ControlFlow::Exit,
                    Some(MenuAction::TogglePin) => {
                        win_state.pinned = !win_state.pinned;
                        window.set_always_on_top(win_state.pinned);
                        menu.sync(&config, win_state.pinned);
                        config.window.pinned = win_state.pinned;
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetIndicator(kind)) => {
                        config.general.indicator = kind;
                        // Tray and panel each ignore the other's modes.
                        tray.set_mode(kind, &menu.menu, &visible_data(&config, &display), &theme);
                        panel.set_mode(kind, &visible_data(&config, &display));
                        menu.sync(&config, win_state.pinned);
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetPanelDisplay(target)) => {
                        config.general.panel_display = target;
                        panel.set_display(target);
                        crate::platform::watch_taskbar(target);
                        panel.on_taskbar_moved(&visible_data(&config, &display));
                        menu.sync(&config, win_state.pinned);
                        save_config(config.clone());
                    }
                    Some(MenuAction::ToggleForecast) => {
                        config.ui.show_forecast = !config.ui.show_forecast;
                        menu.sync(&config, win_state.pinned);
                        save_config(config.clone());
                        window.request_redraw();
                    }
                    Some(MenuAction::ToggleProvider(pid)) => {
                        let mut now_enabled = None;
                        if let Some(pc) = config.providers.iter_mut().find(|p| p.id == pid) {
                            pc.enabled = !pc.enabled;
                            now_enabled = Some(pc.enabled);
                        }
                        if now_enabled == Some(false) {
                            if let Some(provider_id) = ProviderId::from_config_id(&pid) {
                                provider_cache.remove(&provider_id);
                                save_cached_providers(provider_cache.clone());
                            }
                        }
                        refresh_providers(&config, &providers, &mut thresholds, &mut display, &pid);
                        // A disabled provider disappears immediately → resize.
                        visible_count = visible_data(&config, &display).len();
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        tray.sync_providers(&menu.menu, &visible_data(&config, &display), &theme);
                        panel.update(&visible_data(&config, &display), true);
                        save_config(config.clone());
                        fetch_one(&providers, &cmd_tx, &runtime, &pid);
                    }
                    Some(MenuAction::SetAuthMethod(pid, method)) => {
                        if let Some(pc) = config.providers.iter_mut().find(|p| p.id == pid) {
                            pc.auth_method = method.clone();
                            match method {
                                // api_key needs a key label.
                                AuthMethod::ApiKey => {
                                    pc.credential_label =
                                        key_label_for(&pid).unwrap_or_default().to_string();
                                }
                                // The subscription reads local files, no key needed.
                                AuthMethod::Subscription => pc.credential_label.clear(),
                            }
                        }
                        refresh_providers(&config, &providers, &mut thresholds, &mut display, &pid);
                        menu.sync(&config, win_state.pinned);
                        save_config(config.clone());
                        fetch_one(&providers, &cmd_tx, &runtime, &pid);
                        window.request_redraw();
                    }
                    Some(MenuAction::PasteKey(pid)) => match read_clipboard_key() {
                        Ok(key) => {
                            let label = key_label_for(&pid).unwrap_or_default();
                            let stored = keyring::Entry::new("ailimits", label)
                                .and_then(|e| e.set_password(&key));
                            match stored {
                                Ok(()) => {
                                    if let Some(pc) =
                                        config.providers.iter_mut().find(|p| p.id == pid)
                                    {
                                        pc.enabled = true;
                                        pc.credential_label = label.to_string();
                                        // Claude: a key implies the api_key method.
                                        // Copilot: the PAT is just a token source
                                        // for the subscription.
                                        if pid == "claude" {
                                            pc.auth_method = AuthMethod::ApiKey;
                                        }
                                    }
                                    refresh_providers(
                                        &config,
                                        &providers,
                                        &mut thresholds,
                                        &mut display,
                                        &pid,
                                    );
                                    menu.sync(&config, win_state.pinned);
                                    save_config(config.clone());
                                    fetch_one(&providers, &cmd_tx, &runtime, &pid);
                                    window.request_redraw();
                                    feedback("AI Limits", &format!("Key for {pid} stored"));
                                }
                                Err(e) => {
                                    warn!("keyring store failed: {e}");
                                    feedback("AI Limits", "Failed to store the key");
                                }
                            }
                        }
                        Err(e) => feedback("AI Limits", &format!("{e}")),
                    },
                    Some(MenuAction::RemoveKey(pid)) => {
                        let label = key_label_for(&pid).unwrap_or_default();
                        match keyring::Entry::new("ailimits", label)
                            .and_then(|e| e.delete_credential())
                        {
                            Ok(()) | Err(keyring::Error::NoEntry) => {
                                if let Some(pc) = config.providers.iter_mut().find(|p| p.id == pid)
                                {
                                    // Without a key everything runs on the
                                    // subscription method (Claude Code files /
                                    // the gh CLI token).
                                    pc.credential_label.clear();
                                    pc.auth_method = AuthMethod::Subscription;
                                }
                                refresh_providers(
                                    &config,
                                    &providers,
                                    &mut thresholds,
                                    &mut display,
                                    &pid,
                                );
                                menu.sync(&config, win_state.pinned);
                                save_config(config.clone());
                                fetch_one(&providers, &cmd_tx, &runtime, &pid);
                                window.request_redraw();
                                feedback("AI Limits", &format!("Key for {pid} removed"));
                            }
                            Err(e) => {
                                warn!("keyring delete failed: {e}");
                                feedback("AI Limits", "Failed to remove the key");
                            }
                        }
                    }
                    Some(MenuAction::PasteUsageToken(pid)) => match read_clipboard_key() {
                        Ok(token) => {
                            let label = usage_token_label_for(&pid).unwrap_or_default();
                            match keyring::Entry::new("ailimits", label)
                                .and_then(|e| e.set_password(&token))
                            {
                                Ok(()) => {
                                    // No config change: the usage token is just an
                                    // extra source the provider tries first.
                                    fetch_one(&providers, &cmd_tx, &runtime, &pid);
                                    feedback("AI Limits", &format!("Usage token for {pid} stored"));
                                }
                                Err(e) => {
                                    warn!("keyring store failed: {e}");
                                    feedback("AI Limits", "Failed to store the usage token");
                                }
                            }
                        }
                        Err(e) => feedback("AI Limits", &format!("{e}")),
                    },
                    Some(MenuAction::RemoveUsageToken(pid)) => {
                        let label = usage_token_label_for(&pid).unwrap_or_default();
                        match keyring::Entry::new("ailimits", label)
                            .and_then(|e| e.delete_credential())
                        {
                            Ok(()) | Err(keyring::Error::NoEntry) => {
                                fetch_one(&providers, &cmd_tx, &runtime, &pid);
                                feedback("AI Limits", &format!("Usage token for {pid} removed"));
                            }
                            Err(e) => {
                                warn!("keyring delete failed: {e}");
                                feedback("AI Limits", "Failed to remove the usage token");
                            }
                        }
                    }
                    Some(MenuAction::SetDetail(d)) => {
                        config.ui.detail = d;
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetLayout(l)) => {
                        config.ui.layout = l;
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetWidthScale(w)) => {
                        config.ui.width_scale = w;
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetColumnFlow(f)) => {
                        config.ui.column_flow = f;
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetPalette(p)) => {
                        // Picking a palette disables monochrome.
                        config.ui.palette = p;
                        config.ui.monochrome = false;
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        tray.update(&visible_data(&config, &display), &theme, true);
                        panel.update(&visible_data(&config, &display), true);
                        save_config(config.clone());
                    }
                    Some(MenuAction::ToggleMonochrome) => {
                        config.ui.monochrome = !config.ui.monochrome;
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        tray.update(&visible_data(&config, &display), &theme, true);
                        panel.update(&visible_data(&config, &display), true);
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetOpacity(pct)) => {
                        config.window.opacity = (pct as f32 / 100.0).clamp(0.10, 0.85);
                        menu.sync(&config, win_state.pinned);
                        save_config(config.clone());
                        window.request_redraw();
                    }
                    Some(MenuAction::SetBrightness(v)) => {
                        config.ui.brightness = v;
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        tray.update(&visible_data(&config, &display), &theme, true);
                        panel.update(&visible_data(&config, &display), true);
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetSaturation(v)) => {
                        config.ui.saturation = v;
                        apply_ui_change(
                            &config,
                            &mut win_layout,
                            &mut theme,
                            &mut pixmap,
                            &mut surface,
                            &window,
                            &mut win_state,
                            &menu,
                            visible_count,
                        );
                        tray.update(&visible_data(&config, &display), &theme, true);
                        panel.update(&visible_data(&config, &display), true);
                        save_config(config.clone());
                    }
                    Some(MenuAction::SetUpdateInterval(secs)) => {
                        config.general.update_interval_secs = secs;
                        // The scheduler picks the new value up on its next cycle.
                        update_interval.store(secs, Ordering::Relaxed);
                        menu.sync(&config, win_state.pinned);
                        save_config(config.clone());
                    }
                    Some(MenuAction::ToggleAutoUpdate) => {
                        config.general.auto_update = !config.general.auto_update;
                        // The updater task reads this before each check.
                        auto_update_enabled.store(config.general.auto_update, Ordering::Relaxed);
                        menu.sync(&config, win_state.pinned);
                        save_config(config.clone());
                    }
                    Some(MenuAction::ToggleLock) => {
                        config.window.locked = !config.window.locked;
                        // Cancel any in-flight drag.
                        win_state.drag_pending = false;
                        win_state.end_drag();
                        menu.sync(&config, win_state.pinned);
                        save_config(config.clone());
                    }
                    None => {}
                },
            },

            // tao calls process::exit() right after this event, and that does
            // not wait for the background writer. Hand the writer the final
            // config and wait for it to finish, so a setting changed moments
            // before quitting survives — and no in-flight background write can
            // land after it. This one arm covers every exit path (window
            // close, tray Quit, menu Quit).
            Event::LoopDestroyed => {
                if let Some(saver) = saver_at_exit.take() {
                    saver.shutdown(config.clone(), &rt_handle);
                }
            }

            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MetricUnit;

    fn pct(label: &str, used: u64, window: MetricWindow) -> Metric {
        Metric {
            label: label.to_string(),
            used,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: None,
            window,
        }
    }

    fn data(metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: ProviderId::Claude,
            status: ProviderStatus::Ok,
            metrics,
            updated_at: Utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    #[test]
    fn a_session_metric_never_covers_a_long_windows_slot() {
        // The old label guess called neither of these "weekly" and then matched
        // them on their shared unit, so a cached Opus row was thrown away.
        let session = pct("Session", 20, MetricWindow::Session);
        let opus = pct("Opus", 100, MetricWindow::Long);
        assert!(!metrics_match_slot(&session, &opus));
    }

    #[test]
    fn two_different_long_pools_are_different_slots() {
        // Claude reports Weekly, Opus and Sonnet at the same time; one long
        // window no longer stands for all of them.
        let weekly = pct("Weekly", 50, MetricWindow::Long);
        let opus = pct("Opus", 100, MetricWindow::Long);
        assert!(!metrics_match_slot(&weekly, &opus));
    }

    #[test]
    fn the_same_label_is_always_the_same_slot() {
        let a = pct("Weekly", 50, MetricWindow::Long);
        let b = pct("Weekly", 90, MetricWindow::Long);
        assert!(metrics_match_slot(&a, &b));
        let c = pct("Session", 10, MetricWindow::Session);
        let d = pct("session", 80, MetricWindow::Session);
        assert!(metrics_match_slot(&c, &d));
    }

    #[test]
    fn session_metrics_with_different_labels_still_share_a_slot_by_unit() {
        // Antigravity renames its rows as Google rotates models, so two session
        // metrics of the same unit must keep collapsing onto one slot.
        let a = pct("2.5 Pro", 30, MetricWindow::Session);
        let b = pct("3.6 Flash", 40, MetricWindow::Session);
        assert!(metrics_match_slot(&a, &b));
    }

    #[test]
    fn merging_restores_long_pools_the_api_omitted_this_cycle() {
        // Claude answers "seven_day_opus": null on some cycles; the cached rows
        // must come back instead of being swallowed by the Session metric.
        let live = data(vec![
            pct("Session", 20, MetricWindow::Session),
            pct("Weekly", 60, MetricWindow::Long),
        ]);
        let cached = data(vec![
            pct("Session", 15, MetricWindow::Session),
            pct("Weekly", 55, MetricWindow::Long),
            pct("Opus", 100, MetricWindow::Long),
            pct("Sonnet", 40, MetricWindow::Long),
        ]);
        let merged = merge_cached_metrics(live, Some(&cached));
        let labels: Vec<&str> = merged.metrics.iter().map(|m| m.label.as_str()).collect();
        assert!(labels.contains(&"Opus"), "Opus was dropped: {labels:?}");
        assert!(labels.contains(&"Sonnet"), "Sonnet was dropped: {labels:?}");
        // The live values must win for the slots the API did return.
        let weekly = merged.metrics.iter().find(|m| m.label == "Weekly").unwrap();
        assert_eq!(weekly.used, 60);
        assert_eq!(merged.metrics.len(), 4, "no duplicate slots: {labels:?}");
    }
}
