// ui/taskbar_panel.rs — the mini indicator painted OVER the taskbar.
//
// NOT a window with a background: a per-pixel-alpha layered overlay whose
// transparent pixels let the taskbar show through, so only the painted digits
// and bars are visible — exactly like a native tray glyph, with no pasted
// rectangle. (TrafficMonitor takes the other route — a real SetParent child of
// Shell_TrayWnd, guarded by a fallback flag for when the insert fails. We stay
// a free-floating overlay: no dependency on the shell accepting a foreign
// child window, and nothing to unwind when it does not.)
// Content is presented with UpdateLayeredWindow, not softbuffer.
//
// Design goal: look like a continuation of the taskbar. So it is MONOCHROME
// and follows the SYSTEM theme — near-black ink on a light taskbar, near-white
// on a dark one (read from the registry, repainted on WindowEvent::ThemeChanged)
// — and the digits are rendered at the system-clock size, not blown up. Usage
// is the bar's fill, never a hue. Layout (config `general.indicator`, both
// PanelRows and PanelGrid map here): two clock-sized rows for the first two
// providers in the WIDGET's order (not the busiest); each is "percent +
// progress bar". The remaining providers stay in the tooltip. Auto-hide is tracked event-driven via
// SetWinEventHook (UserEvent::TaskbarMoved) — slides away with the bar.

use crate::config::schema::IndicatorKind;
use crate::platform::taskbar_geom::Edge;
use crate::providers::ProviderData;
use crate::ui::tray::{color, draw_digits_fit, fill_round_rect, provider_pct};
use anyhow::{Context as AnyhowContext, Result};
use std::rc::Rc;
use tao::window::{Window, WindowId};
use tiny_skia::{Color, Pixmap};

/// Base layout metrics at 100% DPI; desired_size() scales them to the bar.
const PAD_X: f32 = 8.0;
const NUM_W: f32 = 34.0;
const NUM_GAP: f32 = 7.0;
const BAR_W: f32 = 62.0;
/// The stacked layout's length along a side bar at 100% DPI: two rows of a
/// clock-sized percent over a short bar, with the clock's own breathing room.
const STACK_H: f32 = 64.0;
/// Gap between the overlay and the notification area.
const TRAY_MARGIN: i32 = 10;
/// Initial (hidden) window size; reposition() sizes it to the real taskbar.
const INIT_W: u32 = 119;
const INIT_H: u32 = 40;
/// Background alpha (out of 255) of the otherwise-transparent overlay: just
/// enough that every pixel catches the mouse (so the whole window is the
/// right-click / hover target, not only the painted glyphs), yet visually
/// imperceptible over the taskbar.
const HIT_ALPHA: u8 = 1;

/// Ink + track colors for the current system theme. Near-black on a light
/// taskbar, near-white on a dark one; the track is the ink at a low alpha so
/// it reads as a faint groove rather than a filled box.
///
/// The light ink is 36, not the 20 it used to be. Measured against the shell's
/// own clock on the same bar: our digits had a median ink luma of 110 where the
/// clock's was 119, over a background of 228 — we were ~8% darker, and read as
/// heavier and higher-contrast than everything else in the tray. Solving
/// `median = ink*cov + bg*(1-cov)` at the measured coverage of 0.567 gives 36.
///
/// Note this is a CONTRAST correction, not a weight one: our glyphs are not
/// thicker, they are inked harder. fontdue rasterises with crisper edges than
/// DirectWrite (over the same string the shell lights 163 columns to our 118,
/// at the same mean), so matching the shell's darkness is the closest we get
/// without swapping the rasteriser.
fn theme_ink(light: bool) -> (Color, Color) {
    if light {
        (color(36, 36, 36, 255), color(36, 36, 36, 64))
    } else {
        (color(236, 236, 236, 255), color(236, 236, 236, 66))
    }
}

/// The first two providers in the widget's display order. The mini-panel mirrors
/// the order shown in the main widget — it does NOT re-sort by who is busiest
/// (`providers` already arrives in the widget's order, via `visible_data`).
fn first_two(providers: &[ProviderData]) -> Vec<&ProviderData> {
    providers.iter().take(2).collect()
}

/// Premultiplied RGBA (tiny-skia) → premultiplied BGRA (top-down) for
/// UpdateLayeredWindow.
#[cfg(target_os = "windows")]
fn pixmap_to_bgra(pm: &Pixmap) -> Vec<u8> {
    let data = pm.data();
    let mut bgra = vec![0u8; data.len()];
    for (s, d) in data.chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
        d[0] = s[2];
        d[1] = s[1];
        d[2] = s[0];
        d[3] = s[3];
    }
    bgra
}

/// Paint the rows into the panel's pixmap: the layout lying down (rows of
/// "percent + bar" across, digits sized from the height) or standing up
/// (a stack of percent-over-bar, digits sized from the width), in the given
/// ink over the transparent fill. Free of the window so a test can look.
fn paint_rows(pm: &mut Pixmap, vertical: bool, top: &[&ProviderData], fg: Color, track: Color) {
    let (fw, fh) = (pm.width() as f32, pm.height() as f32);
    let label_of = |data: &ProviderData| match provider_pct(data) {
        Some(p) => format!("{}%", p.round().clamp(0.0, 100.0) as u32),
        None => "—".to_string(),
    };
    // A bar with a sliver kept visible at low usage, so it never looks absent.
    let bar_fill =
        |pm: &mut Pixmap, data: &ProviderData, bx: f32, by: f32, bar_w: f32, bar_h: f32| {
            let rad = bar_h / 2.0;
            fill_round_rect(pm, bx, by, bar_w, bar_h, rad, track);
            if let Some(p) = provider_pct(data) {
                let frac = (p / 100.0).clamp(0.0, 1.0);
                let fwid = (bar_w * frac).max(bar_h);
                fill_round_rect(pm, bx, by, fwid, bar_h, rad, fg);
            }
        };
    if vertical {
        // Standing up: the digits are sized from the bar's WIDTH (the
        // thickness, as the height is lying down), each provider a percent
        // centred over its own short bar, the two stacked like the clock's
        // lines with the same breathing room between them.
        let n = top.len().max(1) as f32;
        let digit_h = fw * 0.225;
        let bar_h = digit_h * 0.50;
        let under = digit_h * 0.35;
        let row_h = digit_h + under + bar_h;
        let row_gap = digit_h * 0.6;
        let block_h = n * row_h + (n - 1.0) * row_gap;
        let top0 = (fh - block_h) / 2.0;
        let pad_x = fw * 0.10;
        let inner = fw - 2.0 * pad_x;
        for (i, data) in top.iter().enumerate() {
            let ry = top0 + i as f32 * (row_h + row_gap);
            draw_digits_fit(pm, &label_of(data), pad_x, ry, inner, digit_h, fg);
            bar_fill(pm, data, pad_x, ry + digit_h + under, inner, bar_h);
        }
    } else {
        // Digit ink height matched to the taskbar clock: the user tuned it
        // to Segoe UI 13px (== 9px ink), which is 0.225 of this taskbar's
        // height. draw_digits_fit fits the ink to digit_h, so digit_h IS
        // the ink height and stays constant regardless of the row count.
        // The tight centered block mirrors the clock's time-over-date stack.
        let n = top.len().max(1) as f32;
        let digit_h = fh * 0.225;
        let row_gap = digit_h * 0.40;
        let bar_h = digit_h * 0.50;
        let block_h = n * digit_h + (n - 1.0) * row_gap;
        let top0 = (fh - block_h) / 2.0;
        let pad_x = fw * 0.06;
        let num_w = fw * 0.30;
        let num_gap = fw * 0.05;
        let bx = pad_x + num_w + num_gap;
        let bar_w = fw - bx - pad_x;
        for (i, data) in top.iter().enumerate() {
            let cy = top0 + digit_h / 2.0 + i as f32 * (digit_h + row_gap);
            draw_digits_fit(
                pm,
                &label_of(data),
                pad_x,
                cy - digit_h / 2.0,
                num_w,
                digit_h,
                fg,
            );
            bar_fill(pm, data, bx, cy - bar_h / 2.0, bar_w, bar_h);
        }
    }
}

pub struct TaskbarPanel {
    window: Rc<Window>,
    pixmap: Pixmap,
    mode: IndicatorKind,
    /// Last drawn integer % per provider, to skip no-op redraws.
    last: Vec<Option<u8>>,
    size: (u32, u32),
    /// Current screen placement, or None while hidden (taskbar slid away).
    rect: Option<(i32, i32, u32, u32)>,
    /// True while a fullscreen app owns the screen: EVERY presentation path
    /// (update ticks, taskbar moves, raises) is gated on this, so nothing can
    /// resurrect the overlay over a game between fallback evaluations.
    suppressed: bool,
    /// The panel could not place itself: no taskbar resolved, or the bar is a
    /// shape we refuse to draw on. Distinct from "the bar is auto-hidden",
    /// which is normal and needs no substitute — the tray icon lives in that
    /// same bar and would be hidden with it.
    unavailable: bool,
    /// Our own hover-tooltip window (a raw layered top-level window we paint),
    /// and whether it is currently shown. Painted dark/rounded/borderless to
    /// match the shell's tooltips, which a native control cannot.
    #[cfg(target_os = "windows")]
    tip_hwnd: isize,
    #[cfg(target_os = "windows")]
    tip_shown: bool,
    /// Last UpdateLayeredWindow error code, so a failing present is logged
    /// once per distinct cause instead of on every provider tick. `None`
    /// means the last present succeeded (or none has run yet) — distinct
    /// from `Some(0)`, which is itself a legitimate cause (the early
    /// size/buffer guard in `present_layered` returns `Err(0)`).
    last_present_error: Option<u32>,
    /// Manual position nudge from the config, already clamped.
    offset: (i32, i32),
    /// Which taskbar the panel attaches to.
    display: crate::config::schema::PanelDisplay,
    /// The monitor edge of the bar the panel was last placed on: it decides
    /// the layout (rows lying down, a stack standing up) and which side the
    /// tooltip opens on.
    edge: Edge,
    /// When the bar was last read off the screen. The read places the panel
    /// on a bar whose notification area cannot be asked for its position; it
    /// happens when the panel comes onto a bar and then once a minute, never
    /// on the re-checks that follow every foreground change.
    room_read: Option<std::time::Instant>,
}

/// The tooltip window is ours, created with CreateWindowExW; nothing else owns
/// it. Harmless to leak while exactly one panel lives for the whole process,
/// but `restart()` already exists as the "rebuild the panel" path, and the day
/// that becomes "make a new TaskbarPanel" the old window would outlive it.
///
/// **This does not run on a normal exit.** `tao::EventLoop::run` is `-> !` and
/// terminates the process from inside, so the closure holding the panel is
/// never dropped. The window is reclaimed by the OS instead. This impl exists
/// for the case above — a panel dropped while the process keeps running — and
/// is deliberately a no-op today rather than a fix for a live leak.
impl Drop for TaskbarPanel {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        crate::platform::destroy_window(self.tip_hwnd);
    }
}

impl TaskbarPanel {
    /// Create the (hidden) overlay window up front; it is shown and embedded
    /// only when the indicator switches to a Panel mode.
    pub fn new(event_loop: &tao::event_loop::EventLoop<crate::app::UserEvent>) -> Result<Self> {
        let mut builder = tao::window::WindowBuilder::new()
            .with_title("AI Limits Panel")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_inner_size(tao::dpi::PhysicalSize::new(INIT_W, INIT_H));
        #[cfg(target_os = "windows")]
        {
            use tao::platform::windows::WindowBuilderExtWindows;
            builder = builder.with_skip_taskbar(true);
        }
        let window = Rc::new(
            builder
                .build(event_loop)
                .context("panel window creation failed")?,
        );
        let pixmap = Pixmap::new(INIT_W, INIT_H).context("panel pixmap")?;
        Ok(Self {
            window,
            pixmap,
            mode: IndicatorKind::Off,
            last: Vec::new(),
            size: (INIT_W, INIT_H),
            rect: None,
            suppressed: false,
            unavailable: false,
            #[cfg(target_os = "windows")]
            // No DWM blur here, deliberately. It tints and blurs the whole
            // WINDOW rectangle, not the rounded box we paint inside it, so once
            // the pixmap grew a margin for the drop shadow the blur showed up
            // as a hard-edged grey rectangle around the tooltip. Measurement
            // says we do not want it anyway: the shell's tooltip is alpha ~244,
            // i.e. all but opaque, and what separates it from the background is
            // the shadow.
            tip_hwnd: crate::platform::create_tooltip_window(),
            #[cfg(target_os = "windows")]
            tip_shown: false,
            last_present_error: None,
            offset: (0, 0),
            display: crate::config::schema::PanelDisplay::Primary,
            edge: Edge::Bottom,
            room_read: None,
        })
    }

    /// Apply the configured manual offset. Clamped here, so no caller can
    /// push the panel off the desktop by editing config.toml.
    pub fn set_offset(&mut self, x: i32, y: i32) {
        use crate::platform::taskbar_geom::clamp_offset;
        self.offset = (clamp_offset(x), clamp_offset(y));
    }

    /// Point the panel at a taskbar. The caller re-positions afterwards.
    pub fn set_display(&mut self, target: crate::config::schema::PanelDisplay) {
        self.display = target;
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    #[cfg(target_os = "windows")]
    pub fn hwnd(&self) -> isize {
        use tao::platform::windows::WindowExtWindows;
        self.window.hwnd()
    }

    fn is_panel_mode(mode: IndicatorKind) -> bool {
        matches!(mode, IndicatorKind::PanelRows | IndicatorKind::PanelGrid)
    }

    /// Apply an indicator mode change: embed + show, or hide.
    pub fn set_mode(&mut self, mode: IndicatorKind, providers: &[ProviderData]) {
        self.mode = mode;
        // A mode change is an explicit user action — start unsuppressed; the
        // next fallback evaluation re-hides if a fullscreen app is still up.
        // The placement verdict is cleared too: it belongs to the mode that
        // just ended, and keeping it would hand the tray a stale reason to
        // stay up after the user turned the panel back on.
        self.suppressed = false;
        self.unavailable = false;
        if Self::is_panel_mode(mode) {
            self.last.clear();
            self.reposition();
            self.update(providers, true);
        } else {
            self.hide();
        }
    }

    /// Periodic upkeep + redraw-on-change. Call on every provider update.
    pub fn update(&mut self, providers: &[ProviderData], force: bool) {
        if !Self::is_panel_mode(self.mode) || self.suppressed {
            return;
        }
        self.reposition();
        let state: Vec<Option<u8>> = providers
            .iter()
            .map(|d| provider_pct(d).map(|p| p.round() as u8))
            .collect();
        if force || state != self.last {
            self.last = state;
            self.redraw(providers);
        }
    }

    /// Show the hover tooltip — the provider summary in a dark, rounded,
    /// borderless pill centered above the overlay — painted by us so it matches
    /// the shell's own tooltips. Idempotent while shown; no-op if the panel is
    /// hidden (no rect). Called from the overlay's CursorEntered.
    pub fn show_tooltip(&mut self, providers: &[ProviderData]) {
        #[cfg(target_os = "windows")]
        {
            if self.tip_shown || self.tip_hwnd == 0 {
                return;
            }
            let Some((px, py, pw, _ph)) = self.rect else {
                return;
            };
            let text = crate::ui::tray::tooltip(providers);
            let light = crate::platform::system_uses_light_theme();
            let pm = crate::ui::tray::render_tooltip(&text, self.size.1 as f32, light);
            let (w, h) = (pm.width() as i32, pm.height() as i32);
            // The pixmap carries the drop shadow around the box, so the box's
            // edge facing the bar sits `shadow` inside the pixmap's — add it
            // back or the gap to the bar comes out short by that much. The
            // tooltip opens away from the bar: above a bottom bar, below a top
            // one, beside a side one, centred on the panel either way.
            let shadow = crate::ui::tray::tip_shadow_inset(self.size.1 as f32);
            let gap = crate::ui::tray::TIP_GAP;
            let (pw, ph) = (pw as i32, _ph as i32);
            let (tx, ty) = match self.edge {
                Edge::Bottom => (px + (pw - w) / 2, py - gap - h + shadow),
                Edge::Top => (px + (pw - w) / 2, py + ph + gap - shadow),
                Edge::Left => (px + pw + gap - shadow, py + (ph - h) / 2),
                Edge::Right => (px - gap - w + shadow, py + (ph - h) / 2),
            };
            let bgra = pixmap_to_bgra(&pm);
            let _ = crate::platform::present_layered(self.tip_hwnd, &bgra, tx, ty, w, h);
            self.tip_shown = true;
        }
        #[cfg(not(target_os = "windows"))]
        let _ = providers;
    }

    /// Hide the hover tooltip (cursor left the overlay, or the panel is hiding).
    pub fn hide_tooltip(&mut self) {
        #[cfg(target_os = "windows")]
        if self.tip_shown {
            crate::platform::hide_window(self.tip_hwnd);
            self.tip_shown = false;
        }
    }

    /// Whether the hover tooltip is currently shown.
    #[cfg(target_os = "windows")]
    pub fn tooltip_shown(&self) -> bool {
        self.tip_shown
    }

    /// Re-assert the overlay's topmost z-order after another window covered it
    /// (the tray overflow flyout, Start menu, …). Cheap: a single SetWindowPos,
    /// no reposition or repaint. No-op when hidden or not in a panel mode.
    /// A popup menu that reaches over the panel is the one thing it does not
    /// come back on top of: it steps under that menu instead, and the raise
    /// that follows the menu closing lands as usual.
    pub fn raise(&self) {
        if !Self::is_panel_mode(self.mode) || self.rect.is_none() {
            return;
        }
        #[cfg(target_os = "windows")]
        {
            if self.yield_to_menu() {
                return;
            }
            crate::platform::raise_panel_topmost(self.hwnd());
        }
    }

    /// Step under a popup menu that reaches over the panel, so the menu is
    /// drawn on top of it as it would be over any other window. Nothing else
    /// changes: the panel keeps its place and its picture, and the next
    /// `raise` - sent when the menu closes - puts it back on top.
    /// Answers whether it did step under one.
    pub fn yield_to_menu(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            if !Self::is_panel_mode(self.mode) {
                return false;
            }
            let Some((x, y, w, h)) = self.rect else {
                return false;
            };
            if let Some(menu) = crate::platform::popup_menu_over((x, y, x + w as i32, y + h as i32))
            {
                crate::platform::place_below(self.hwnd(), menu);
                return true;
            }
        }
        false
    }

    /// True when the panel SHOULD be visible (a Panel mode, positioned) but the
    /// taskbar is composited OVER it — the cursor-edge "rude topmost" peek that
    /// a floating overlay cannot beat. Read from what the compositor actually
    /// shows at the panel's center (WindowFromPoint); drives the tray fallback.
    #[cfg(target_os = "windows")]
    /// The panel cannot place itself at all — as opposed to being hidden with
    /// an auto-hidden bar, which is normal.
    ///
    /// `is_covered` cannot answer this: with no rectangle it reads `false`,
    /// i.e. "not obstructed", so a panel that never made it onto the screen
    /// looked exactly like a healthy one and the tray substitute stayed hidden.
    /// The user was left with no indicator at all and no way to tell why.
    pub fn is_unavailable(&self) -> bool {
        Self::is_panel_mode(self.mode) && self.unavailable
    }

    pub fn is_covered(&self) -> bool {
        if !Self::is_panel_mode(self.mode) {
            return false;
        }
        let Some((x, y, w, h)) = self.rect else {
            return false;
        };
        let owner = crate::platform::point_owner(x + w as i32 / 2, y + h as i32 / 2);
        // A popup menu over the panel is not the panel being covered - the
        // panel put itself under that menu on purpose, and it comes back the
        // moment the menu goes. Counting it would raise the tray icon for as
        // long as a right-click menu is open, which is not an indicator the
        // user asked for and not one that helps: the panel is still there,
        // under a menu, exactly as every other window is.
        let covered =
            owner != 0 && owner != self.hwnd() && !crate::platform::window_is_popup_menu(owner);
        if covered {
            tracing::trace!(
                "panel at {x},{y} {w}x{h} covered by {}",
                crate::platform::describe_window(owner)
            );
        }
        covered
    }

    /// Restart the panel: forget every cached judgement and place it again.
    ///
    /// **Why switching displays was not enough.** `set_display` changed the
    /// target but left `suppressed` alone, and `on_taskbar_moved` returns early
    /// while suppressed — so a panel parked by a fullscreen app could not be
    /// revived by moving it, toggling it, or anything else short of restarting
    /// the whole application. That is what the user hit.
    ///
    /// Clearing `last` matters too: it is the "what did I draw" cache, and a
    /// stale entry means the redraw is skipped as a no-op precisely when the
    /// panel needs re-presenting.
    pub fn restart(&mut self, providers: &[ProviderData]) {
        self.suppressed = false;
        self.unavailable = false;
        self.last.clear();
        if !Self::is_panel_mode(self.mode) {
            self.hide();
            return;
        }
        self.reposition();
        self.redraw(providers);
    }

    /// The taskbar moved (auto-hide slide / resolution change) or the tray
    /// area changed width (icon pinned/unpinned) — follow it.
    pub fn on_taskbar_moved(&mut self, providers: &[ProviderData]) {
        if !Self::is_panel_mode(self.mode) || self.suppressed {
            return;
        }
        self.reposition();
        self.redraw(providers);
    }

    /// A fullscreen app took the screen: hide with the taskbar (which sits
    /// under the fullscreen window without moving, so `reposition` cannot see
    /// it) and gate every presentation path until restored — a provider
    /// update tick must not resurrect the overlay over a game. Idempotent.
    /// While hidden, `raise()` and `is_covered()` are no-ops (rect=None).
    pub fn suppress_for_fullscreen(&mut self) {
        if !Self::is_panel_mode(self.mode) || self.suppressed {
            return;
        }
        self.suppressed = true;
        self.hide();
    }

    /// The fullscreen app is gone (alt-tab back to the desktop) — return to
    /// the bar exactly like it does: re-measure the slot and re-present.
    pub fn restore_from_fullscreen(&mut self, providers: &[ProviderData]) {
        self.suppressed = false;
        if !Self::is_panel_mode(self.mode) {
            return;
        }
        self.reposition();
        self.redraw(providers);
    }

    /// The overlay size, from the bar's thickness (its height lying down, its
    /// width standing up), scaled so digits stay clock-sized. On a horizontal
    /// bar: two "percent + bar" rows across, the bar's full height. On a side
    /// bar: the bar's full width, and the two providers stacked, each a
    /// percent over its own short bar — the way the clock stacks its lines.
    fn desired_size(&self, edge: Edge, thickness: i32) -> (u32, u32) {
        let scale = (thickness as f32 / 48.0).clamp(1.0, 3.0);
        let thick = thickness.max(1) as u32;
        if edge.vertical() {
            (thick, (STACK_H * scale).round() as u32)
        } else {
            (
                ((PAD_X * 2.0 + NUM_W + NUM_GAP + BAR_W) * scale).round() as u32,
                thick,
            )
        }
    }

    fn hide(&mut self) {
        self.hide_tooltip();
        #[cfg(target_os = "windows")]
        crate::platform::hide_window(self.hwnd());
        self.rect = None;
    }

    fn resize_pixmap(&mut self, w: u32, h: u32) {
        self.size = (w, h);
        if let Some(pm) = Pixmap::new(w, h) {
            self.pixmap = pm;
        }
    }

    /// Track the taskbar: hide with it (auto-hide), or float just before the
    /// notification area along the bar's axis, centred across its thickness,
    /// on whichever monitor edge the bar sits.
    fn reposition(&mut self) {
        #[cfg(target_os = "windows")]
        {
            use crate::platform::taskbar_geom::{panel_origin, thickness};
            let before = self.rect;
            // The panel's own footprint along the axis, so the scan for the
            // row of app buttons does not take the panel for one of them.
            let own = before.map(|(x, y, w, h)| {
                if self.edge.vertical() {
                    (y, y + h as i32)
                } else {
                    (x, x + w as i32)
                }
            });
            let read_screen = before.is_none()
                || self
                    .room_read
                    .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(60));
            let Some(slot) = crate::platform::taskbar_slot(self.display, own, read_screen) else {
                // No taskbar at all: the panel has nowhere to live, and the tray
                // has nowhere either — but say so, so the indicator can degrade
                // instead of silently showing nothing.
                if before.is_some() {
                    tracing::debug!("panel hidden: no taskbar resolved for {:?}", self.display);
                }
                self.unavailable = true;
                self.hide();
                return;
            };
            self.edge = slot.edge;
            if !slot.visible {
                // The bar slid away (auto-hide). NOT unavailable: the tray icon
                // sits in that same bar and is hidden with it, so substituting
                // one for the other would gain nothing and flicker on every
                // slide.
                if before.is_some() {
                    tracing::debug!(
                        "panel hidden: bar auto-hidden ({:?} edge, bar {:?})",
                        slot.edge,
                        slot.bar
                    );
                }
                self.unavailable = false;
                self.hide();
                return;
            }
            let (w, h) = self.desired_size(slot.edge, thickness(slot.edge, slot.bar));
            // An estimated tray edge is a guess at where the clock starts, so
            // keep a little more air than when the edge was measured.
            let margin = if slot.tray_found {
                TRAY_MARGIN
            } else {
                TRAY_MARGIN * 2
            };
            // A bar with no room between its last app button and the tray gets
            // no panel: drawn there it would sit on the buttons. The tray icon
            // stands in, and the panel returns when room appears (a window
            // closes, the bar grows) — the scan runs on every placement.
            if read_screen && slot.tray_start != 0 {
                self.room_read = Some(std::time::Instant::now());
            }
            // The panel used to stand down when the stretch before the tray
            // was too short for it. That verdict drove itself: hiding the
            // panel put the tray icon up, the tray icon widened the tray by
            // its own 32 px, the stretch changed, and the verdict flipped -
            // twice a second, the panel jumping between two places over
            // whatever was on screen. Nothing is withheld now; a bar packed
            // to its end will have the panel over the last button instead,
            // which is the smaller fault and a steady one.
            self.unavailable = false;
            let (x, y) = panel_origin(
                slot.edge,
                slot.bar,
                slot.tray_start,
                (w as i32, h as i32),
                margin,
            );
            if self.size != (w, h) {
                self.resize_pixmap(w, h);
                self.last.clear();
            }
            // Re-presenting is needed when reappearing after a hide.
            if self.rect.is_none() {
                self.last.clear();
            }
            let x = x + self.offset.0;
            let y = y + self.offset.1;
            if before.map(|(bx, by, _, _)| (bx, by)) != Some((x, y)) {
                tracing::debug!(
                    "panel placed at {x},{y} {w}x{h} (target {:?}, {:?} edge, bar {:?}, tray at {}, measured {}, buttons end at {:?})",
                    self.display,
                    slot.edge,
                    slot.bar,
                    slot.tray_start,
                    slot.tray_found,
                    slot.band_end
                );
            }
            self.rect = Some((x, y, w, h));
        }
    }

    /// Paint (near-transparent background) and present via UpdateLayeredWindow:
    /// monochrome, system-theme-aware, two clock-sized "percent + bar" rows.
    pub fn redraw(&mut self, providers: &[ProviderData]) {
        if !Self::is_panel_mode(self.mode) {
            return;
        }
        let Some((x, y, w, h)) = self.rect else {
            return;
        };
        let (fg, track) = theme_ink(crate::platform::system_uses_light_theme());
        let pm = &mut self.pixmap;
        // A per-pixel-alpha layered window passes mouse clicks THROUGH any
        // fully transparent (alpha 0) pixel to the window beneath — here the
        // taskbar — so a right-click on the gaps between the digits/bars used
        // to open the taskbar's own menu instead of ours, and hover never
        // reached the panel. Fill with a 1/255 alpha so every pixel of the
        // window catches the cursor (clicks + the hover tooltip) while staying
        // visually transparent (≈0.4% — imperceptible over the bar).
        pm.fill(color(0, 0, 0, HIT_ALPHA));

        paint_rows(pm, self.edge.vertical(), &first_two(providers), fg, track);

        // Premultiplied RGBA (tiny-skia) → premultiplied BGRA (top-down) for
        // UpdateLayeredWindow.
        let data = self.pixmap.data();
        let mut bgra = vec![0u8; data.len()];
        for (s, d) in data.chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
        #[cfg(target_os = "windows")]
        match crate::platform::present_layered(self.hwnd(), &bgra, x, y, w as i32, h as i32) {
            Ok(()) => {
                if self.last_present_error.is_some() {
                    tracing::info!("taskbar panel present recovered");
                    self.last_present_error = None;
                }
            }
            Err(code) => {
                if self.last_present_error != Some(code) {
                    tracing::warn!("taskbar panel present failed, error {code}");
                    self.last_present_error = Some(code);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (&bgra, x, y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Metric, MetricUnit, MetricWindow, ProviderId, ProviderStatus};
    use chrono::Utc;

    fn data(id: ProviderId, pct: u64) -> ProviderData {
        ProviderData {
            id,
            status: ProviderStatus::Ok,
            metrics: vec![Metric {
                label: "Session".into(),
                used: pct,
                limit: Some(100),
                unit: MetricUnit::Percent,
                reset_at: None,
                window: MetricWindow::Session,
            }],
            updated_at: Utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    /// Both layouts over the bar's dark grey, at the 100% bar size, to
    /// %TEMP%: `cargo test preview_panel -- --ignored`.
    #[test]
    #[ignore]
    fn preview_panel() {
        let rows = [data(ProviderId::Claude, 16), data(ProviderId::Codex, 0)];
        let top: Vec<&ProviderData> = rows.iter().collect();
        let (fg, track) = theme_ink(false);
        for (name, vertical, w, h) in [("rows", false, 119, 48), ("stack", true, 48, 64)] {
            let mut pm = Pixmap::new(w, h).unwrap();
            pm.fill(color(32, 32, 32, 255));
            paint_rows(&mut pm, vertical, &top, fg, track);
            let path = std::env::temp_dir().join(format!("ailimits_panel_{name}.png"));
            std::fs::write(&path, pm.encode_png().unwrap()).unwrap();
            println!("{}", path.display());
        }
    }

    /// Every painted row must land inside the pixmap: with two providers the
    /// stacked layout paints two bars, and the lower one is not lost off the
    /// bottom edge.
    #[test]
    fn the_stacked_layout_paints_both_bars_inside_the_pixmap() {
        let rows = [data(ProviderId::Claude, 16), data(ProviderId::Codex, 0)];
        let top: Vec<&ProviderData> = rows.iter().collect();
        let (fg, track) = theme_ink(false);
        let mut pm = Pixmap::new(48, 64).unwrap();
        pm.fill(color(0, 0, 0, 255));
        paint_rows(&mut pm, true, &top, fg, track);
        // rows with ink, top to bottom; a bar is a run of lit rows after a
        // gap below the digits
        let lit: Vec<bool> = (0..64)
            .map(|y| (0..48).any(|x| pm.pixel(x, y).map(|p| p.red() > 40).unwrap_or(false)))
            .collect();
        let runs = lit
            .iter()
            .enumerate()
            .filter(|(i, &l)| l && (*i == 0 || !lit[i - 1]))
            .count();
        assert!(
            runs >= 4,
            "digits, bar, digits, bar: {runs} runs of ink in {lit:?}"
        );
    }
}
