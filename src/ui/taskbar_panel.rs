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
fn theme_ink(light: bool) -> (Color, Color) {
    if light {
        (color(20, 20, 20, 255), color(20, 20, 20, 64))
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
    /// Our own hover-tooltip window (a raw layered top-level window we paint),
    /// and whether it is currently shown. Painted dark/rounded/borderless to
    /// match the shell's tooltips, which a native control cannot.
    #[cfg(target_os = "windows")]
    tip_hwnd: isize,
    #[cfg(target_os = "windows")]
    tip_shown: bool,
    /// Last UpdateLayeredWindow error code, so a failing present is logged
    /// once per distinct cause instead of on every provider tick.
    last_present_error: u32,
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
            #[cfg(target_os = "windows")]
            tip_hwnd: crate::platform::create_tooltip_window(),
            #[cfg(target_os = "windows")]
            tip_shown: false,
            last_present_error: 0,
        })
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
        self.suppressed = false;
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
            let tx = px + (pw as i32 - w) / 2;
            let ty = py - h - 4;
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
    pub fn raise(&self) {
        if !Self::is_panel_mode(self.mode) || self.rect.is_none() {
            return;
        }
        #[cfg(target_os = "windows")]
        crate::platform::raise_panel_topmost(self.hwnd());
    }

    /// True when the panel SHOULD be visible (a Panel mode, positioned) but the
    /// taskbar is composited OVER it — the cursor-edge "rude topmost" peek that
    /// a floating overlay cannot beat. Read from what the compositor actually
    /// shows at the panel's center (WindowFromPoint); drives the tray fallback.
    #[cfg(target_os = "windows")]
    pub fn is_covered(&self) -> bool {
        if !Self::is_panel_mode(self.mode) {
            return false;
        }
        let Some((x, y, w, h)) = self.rect else {
            return false;
        };
        let owner = crate::platform::point_owner(x + w as i32 / 2, y + h as i32 / 2);
        owner != 0 && owner != self.hwnd()
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

    /// The overlay size: a two-row width and a height fit to the taskbar,
    /// both scaled by the taskbar's DPI so digits stay clock-sized.
    fn desired_size(&self, slot_h: i32) -> (u32, u32) {
        let scale = (slot_h as f32 / 48.0).clamp(1.0, 3.0);
        // Full taskbar height: the two rows must hold clock-sized digits, so
        // the panel needs the same vertical room the clock's two lines use.
        let h = slot_h.max(1) as u32;
        let w = ((PAD_X * 2.0 + NUM_W + NUM_GAP + BAR_W) * scale).round() as u32;
        (w, h)
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

    /// Track the taskbar: hide with it (auto-hide), or float just left of the
    /// notification area, vertically centered in the bar.
    fn reposition(&mut self) {
        #[cfg(target_os = "windows")]
        {
            let Some(slot) = crate::platform::taskbar_slot() else {
                self.hide();
                return;
            };
            if !slot.visible {
                self.hide();
                return;
            }
            // The panel only supports a bottom taskbar. A left/right (vertical)
            // taskbar reports a full-screen-tall slot; sizing the panel to it
            // would draw a screen-height strip off the edge. Bail out (the tray
            // indicator still works) rather than misrender. A 200%-DPI bottom
            // bar is ~96px, so anything taller than this is not a bottom bar.
            const MAX_BAR_HEIGHT: i32 = 200;
            if slot.height > MAX_BAR_HEIGHT {
                self.hide();
                return;
            }
            let (w, h) = self.desired_size(slot.height);
            let x = slot.tray_left - w as i32 - TRAY_MARGIN;
            let y = slot.top + (slot.height - h as i32) / 2;
            if self.size != (w, h) {
                self.resize_pixmap(w, h);
                self.last.clear();
            }
            // Re-presenting is needed when reappearing after a hide.
            if self.rect.is_none() {
                self.last.clear();
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
        let (fw, fh) = (w as f32, h as f32);
        let pm = &mut self.pixmap;
        // A per-pixel-alpha layered window passes mouse clicks THROUGH any
        // fully transparent (alpha 0) pixel to the window beneath — here the
        // taskbar — so a right-click on the gaps between the digits/bars used
        // to open the taskbar's own menu instead of ours, and hover never
        // reached the panel. Fill with a 1/255 alpha so every pixel of the
        // window catches the cursor (clicks + the hover tooltip) while staying
        // visually transparent (≈0.4% — imperceptible over the bar).
        pm.fill(color(0, 0, 0, HIT_ALPHA));

        let top = first_two(providers);
        // Digit ink height matched to the taskbar clock: the user tuned it to
        // Segoe UI 13px (== 9px ink), which is 0.225 of this taskbar's height.
        // draw_digits_fit fits the ink to digit_h, so digit_h IS the ink
        // height and stays constant regardless of the row count. The tight
        // centered block mirrors the clock's time-over-date stack.
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
            let label = match provider_pct(data) {
                Some(p) => format!("{}%", p.round().clamp(0.0, 100.0) as u32),
                None => "—".to_string(),
            };
            draw_digits_fit(pm, &label, pad_x, cy - digit_h / 2.0, num_w, digit_h, fg);
            let by = cy - bar_h / 2.0;
            let rad = bar_h / 2.0;
            fill_round_rect(pm, bx, by, bar_w, bar_h, rad, track);
            if let Some(p) = provider_pct(data) {
                let frac = (p / 100.0).clamp(0.0, 1.0);
                // Keep a sliver visible at low usage so the bar never looks absent.
                let fwid = (bar_w * frac).max(bar_h);
                fill_round_rect(pm, bx, by, fwid, bar_h, rad, fg);
            }
        }

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
                if self.last_present_error != 0 {
                    tracing::info!("taskbar panel present recovered");
                    self.last_present_error = 0;
                }
            }
            Err(code) => {
                if code != self.last_present_error {
                    tracing::warn!("taskbar panel present failed, error {code}");
                    self.last_present_error = code;
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (&bgra, x, y);
        }
    }
}
