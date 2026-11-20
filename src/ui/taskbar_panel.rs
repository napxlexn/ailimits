// ui/taskbar_panel.rs — the mini indicator painted OVER the taskbar.
//
// NOT a window with a background: a per-pixel-alpha layered overlay presented
// with UpdateLayeredWindow, so only the painted digits and bars are visible —
// like a native tray glyph, with no pasted rectangle. A free-floating overlay
// rather than a SetParent child of Shell_TrayWnd: nothing depends on the shell
// accepting a foreign child window.
//
// Design goal: look like a continuation of the taskbar. MONOCHROME, following
// the SYSTEM theme (near-black ink on a light bar, near-white on a dark one,
// read from the registry and repainted on WindowEvent::ThemeChanged), digits at
// the system-clock size, usage as the bar's fill and never a hue. A value
// that is not live (stale, or an estimate) is inked dimmer, as the widget
// greys it. Layout (`general.indicator`; PanelRows and PanelGrid both map
// here): two rows for the first two providers in the WIDGET's order, each
// "percent + progress bar"; the rest stay in the tooltip. Auto-hide is
// tracked event-driven via SetWinEventHook (UserEvent::TaskbarMoved) — it
// slides away with the bar.

use crate::config::schema::{IndicatorKind, UiNumbers};
use crate::platform::taskbar_geom::Edge;
use crate::providers::ProviderData;
use crate::rows::RowView;
use crate::ui::tray::{
    countdown_changes_in, countdown_compact, draw_digits_fit, draw_text_like_digits,
    fill_round_rect, own_reset, provider_pct,
};
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
/// The otherwise-transparent overlay's background (`theme::PANEL_HIT`):
/// every pixel catches the mouse, so the whole window is the right-click and
/// hover target, not only the painted glyphs.
fn hit_fill() -> Color {
    crate::ui::theme::PANEL_HIT.to_skia_color()
}

/// Ink + track colors for the current system theme (`theme::panel_ink`,
/// where the measurements behind them are).
fn theme_ink(light: bool) -> (Color, Color) {
    let (ink, track) = crate::ui::theme::panel_ink(light);
    (ink.to_skia_color(), track.to_skia_color())
}

/// The ink of a row that is not live — stale, or an estimate: the live ink
/// with its alpha halfway down to the track's. It reads dimmer at a glance
/// yet stays legible on either bar (digits measured at 4:1 on the light one
/// and 6:1 on the dark, where live digits are 12:1 and 13:1), and its bar
/// fill still stands clear of the groove. Halfway is 62% on the light bar,
/// the opacity WinUI gives its own secondary text there.
fn aged_ink(ink: Color, track: Color) -> Color {
    let mut aged = ink;
    aged.set_alpha((ink.alpha() + track.alpha()) / 2.0);
    aged
}

/// The first two providers in the widget's display order. The mini-panel mirrors
/// the order shown in the main widget — it does NOT re-sort by who is busiest
/// (`providers` already arrives in the widget's order, via `visible_data`).
fn first_two(providers: &[ProviderData]) -> Vec<&ProviderData> {
    providers.iter().take(2).collect()
}

/// What the panel's picture depends on, for skipping no-op redraws: per
/// painted row, the integer percent, whether the value is aged (not live)
/// and, while the panel shows reset times, the time as drawn. Only the
/// first two providers are painted, so nobody further down the list can ask
/// for a repaint; a row turning stale or fresh does.
fn drawn_state(providers: &[ProviderData], numbers: &UiNumbers) -> Vec<DrawnRow> {
    first_two(providers)
        .into_iter()
        .map(|d| {
            (
                provider_pct(d).map(|p| p.round() as u8),
                d.is_aged(),
                numbers.reset.then(|| countdown_of(d)),
            )
        })
        .collect()
}

/// One painted row's state (`drawn_state`).
type DrawnRow = (Option<u8>, bool, Option<String>);

/// A row's countdown as the panel draws it: "2h14", or a dash when its
/// headline window's reset is not known.
fn countdown_of(data: &ProviderData) -> String {
    match own_reset(data) {
        Some(t) => countdown_compact(t.signed_duration_since(chrono::Utc::now()).num_seconds()),
        None => NO_TIME.to_string(),
    }
}

/// What a row with no known reset shows where its time would be.
const NO_TIME: &str = "\u{2013}";

/// When the earliest countdown on the panel next reads differently, while
/// it shows any: the moment to repaint it. Each row's own reset; none when
/// no time is drawn.
fn next_countdown_tick(top: &[&ProviderData], numbers: &UiNumbers) -> Option<std::time::Instant> {
    if !numbers.reset {
        return None;
    }
    let now = chrono::Utc::now();
    top.iter()
        .filter_map(|d| own_reset(d))
        .map(|t| countdown_changes_in(t.signed_duration_since(now).num_seconds()))
        .min()
        .map(|s| std::time::Instant::now() + std::time::Duration::from_secs(s.max(1) as u64))
}

/// Premultiplied RGBA (tiny-skia) → premultiplied BGRA (top-down) for
/// UpdateLayeredWindow.
#[cfg(target_os = "windows")]
fn pixmap_to_bgra(pm: &Pixmap) -> Vec<u8> {
    let data = pm.data();
    let mut bgra = vec![0u8; data.len()];
    for (s, d) in data
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bgra.as_chunks_mut::<4>().0.iter_mut())
    {
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
/// ink over the transparent fill — dimmed for a row that is not live. With
/// the reset time asked for (`numbers`), the time takes the bar's place
/// beside the percent ("25%  2h14"), or the percent's place before the bar
/// when the percent is left out ("2h14 ▬▬"); the panel keeps its size. Free
/// of the window so a test can look.
fn paint_rows(
    pm: &mut Pixmap,
    vertical: bool,
    top: &[&ProviderData],
    fg: Color,
    track: Color,
    numbers: &UiNumbers,
) {
    let both = numbers.percent && numbers.reset;
    let time_only = !numbers.percent && numbers.reset;
    let (fw, fh) = (pm.width() as f32, pm.height() as f32);
    let label_of = |data: &ProviderData| match provider_pct(data) {
        Some(p) => format!("{}%", p.round().clamp(0.0, 100.0) as u32),
        None => "—".to_string(),
    };
    let aged = aged_ink(fg, track);
    let ink_of = |data: &ProviderData| if data.is_aged() { aged } else { fg };
    // A bar with a sliver kept visible at low usage, so it never looks absent.
    let bar_fill = |pm: &mut Pixmap,
                    data: &ProviderData,
                    bx: f32,
                    by: f32,
                    bar_w: f32,
                    bar_h: f32,
                    ink: Color| {
        let rad = bar_h / 2.0;
        fill_round_rect(pm, bx, by, bar_w, bar_h, rad, track);
        if let Some(p) = provider_pct(data) {
            let frac = (p / 100.0).clamp(0.0, 1.0);
            let fwid = (bar_w * frac).max(bar_h);
            fill_round_rect(pm, bx, by, fwid, bar_h, rad, ink);
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
        let row_h = digit_h + under + if both { digit_h * 0.85 } else { bar_h };
        let row_gap = digit_h * 0.6;
        let block_h = n * row_h + (n - 1.0) * row_gap;
        let top0 = (fh - block_h) / 2.0;
        let pad_x = fw * 0.10;
        let inner = fw - 2.0 * pad_x;
        for (i, data) in top.iter().enumerate() {
            let ry = top0 + i as f32 * (row_h + row_gap);
            let ink = ink_of(data);
            if time_only {
                let t = countdown_of(data);
                draw_text_like_digits(pm, &t, pad_x, ry, inner, digit_h, false, ink);
            } else {
                draw_digits_fit(pm, &label_of(data), pad_x, ry, inner, digit_h, ink);
            }
            if both {
                // The time where the bar was, a little smaller, so percent
                // and time read as one provider's pair.
                let t = countdown_of(data);
                let small = digit_h * 0.85;
                draw_text_like_digits(
                    pm,
                    &t,
                    pad_x,
                    ry + digit_h + under,
                    inner,
                    small,
                    false,
                    ink,
                );
            } else {
                bar_fill(pm, data, pad_x, ry + digit_h + under, inner, bar_h, ink);
            }
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
            let ink = ink_of(data);
            if time_only {
                let t = countdown_of(data);
                draw_text_like_digits(
                    pm,
                    &t,
                    pad_x,
                    cy - digit_h / 2.0,
                    num_w,
                    digit_h,
                    false,
                    ink,
                );
            } else {
                draw_digits_fit(
                    pm,
                    &label_of(data),
                    pad_x,
                    cy - digit_h / 2.0,
                    num_w,
                    digit_h,
                    ink,
                );
            }
            if both {
                // The time takes the bar's place, from where the bar began.
                let t = countdown_of(data);
                draw_text_like_digits(pm, &t, bx, cy - digit_h / 2.0, bar_w, digit_h, true, ink);
            } else {
                bar_fill(pm, data, bx, cy - bar_h / 2.0, bar_w, bar_h, ink);
            }
        }
    }
}

pub struct TaskbarPanel {
    window: Rc<Window>,
    pixmap: Pixmap,
    mode: IndicatorKind,
    /// Last drawn state per painted row (`drawn_state`), to skip no-op
    /// redraws.
    last: Vec<DrawnRow>,
    /// What each row shows: the percentage, the reset time, or both
    /// (`[ui.panel]`).
    numbers: UiNumbers,
    /// When the countdown on the panel next reads differently, while it
    /// shows one and is in sight: the event loop wakes then and repaints
    /// (`redraw`, which asks the shell nothing). None otherwise.
    next_tick: Option<std::time::Instant>,
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
    /// When the bar was last put on another edge, while the panel waits for
    /// it to actually be drawn there.
    #[cfg(target_os = "windows")]
    moved_at: Option<std::time::Instant>,
    /// How many more times to come back and re-assert the panel after such a
    /// move, while the shell finishes arranging the bar.
    #[cfg(target_os = "windows")]
    moved_settle: u8,
    /// When the shell was last asked where the bar's buttons end and its
    /// tray begins. Once a minute while the panel is placed, every two
    /// seconds while it is standing down - that is what it is waiting for.
    room_read: Option<std::time::Instant>,
}

/// The tooltip window is ours and nothing else owns it, so it is destroyed
/// here for the day a second `TaskbarPanel` is ever made.
///
/// It does NOT run on a normal exit: `tao::EventLoop::run` is `-> !` and ends
/// the process from inside, so the closure holding the panel is never dropped
/// and the OS reclaims the window. No live leak is being fixed here.
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
            numbers: UiNumbers::panel_default(),
            next_tick: None,
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
            moved_at: None,
            moved_settle: 0,
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
        // Another bar, so what was asked about the old one says nothing.
        self.room_read = None;
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

    /// What each row shows (`[ui.panel]`); repainted when it changed.
    pub fn set_numbers(&mut self, numbers: UiNumbers, providers: &[ProviderData]) {
        if self.numbers == numbers {
            return;
        }
        self.numbers = numbers;
        self.next_tick = None;
        self.update(providers, true);
    }

    /// When the countdown on the panel next reads differently, if it shows
    /// one: the event loop's deadline for `redraw`.
    pub fn next_tick(&self) -> Option<std::time::Instant> {
        self.next_tick
    }

    /// Periodic upkeep + redraw-on-change. Call on every provider update.
    pub fn update(&mut self, providers: &[ProviderData], force: bool) {
        if !Self::is_panel_mode(self.mode) || self.suppressed {
            return;
        }
        self.reposition();
        let state = drawn_state(providers, &self.numbers);
        if force || state != self.last {
            self.last = state;
            self.redraw(providers);
        }
    }

    /// Show the hover tooltip — the tooltip's rows in a dark, rounded,
    /// borderless box centered above the overlay — painted by us so it matches
    /// the shell's own tooltips. Today's single provider line with the default
    /// config; one row per line, with model labels, once extra rows or labels
    /// are on (`tray::tooltip_lines`). Idempotent while shown; no-op if the
    /// panel is hidden (no rect). Called from the overlay's CursorEntered.
    pub fn show_tooltip(
        &mut self,
        rows: &[RowView],
        first_install: bool,
        tip: &crate::config::schema::UiNumbers,
    ) {
        #[cfg(target_os = "windows")]
        {
            if self.tip_shown || self.tip_hwnd == 0 {
                return;
            }
            let Some((px, py, pw, _ph)) = self.rect else {
                return;
            };
            let lines = crate::ui::tray::tooltip_lines(rows, first_install, tip);
            let light = crate::platform::system_uses_light_theme();
            // The tooltip is sized from the BAR's thickness, which is the
            // only DPI signal here - not from the panel's height, which is
            // the thickness lying down and the stacked layout's 64px
            // standing up. Taking the height made a side bar's tooltip a
            // third too big, while the tray icon's was right.
            let bar_thickness = self.bar_thickness() as f32;
            let pm = crate::ui::tray::render_tooltip_lines(&lines, bar_thickness, light);
            let (w, h) = (pm.width() as i32, pm.height() as i32);
            // The pixmap carries the drop shadow around the box, so the box's
            // edge facing the bar sits `shadow` inside the pixmap's — add it
            // back or the gap to the bar comes out short by that much. The
            // tooltip opens away from the bar: above a bottom bar, below a top
            // one, beside a side one, centred on the panel either way.
            let shadow = crate::ui::tray::tip_shadow_inset(bar_thickness);
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
        let _ = (rows, first_install);
    }

    /// The bar's thickness, taken from the panel's own size: the panel spans
    /// the bar across it, so that is its height lying down and its width
    /// standing up.
    #[cfg(target_os = "windows")]
    fn bar_thickness(&self) -> u32 {
        if self.edge.vertical() {
            self.size.0
        } else {
            self.size.1
        }
    }

    /// Hide the hover tooltip (cursor left the overlay, or the panel is hiding).
    pub fn hide_tooltip(&mut self) {
        #[cfg(target_os = "windows")]
        if self.tip_shown {
            crate::platform::hide_window(self.tip_hwnd);
            self.tip_shown = false;
        }
    }

    /// Where the panel is on screen (left, top, right, bottom), or None when
    /// it is not up. Callers ask so they can tell what actually reaches it.
    pub fn screen_rect(&self) -> Option<(i32, i32, i32, i32)> {
        self.rect
            .map(|(x, y, w, h)| (x, y, x + w as i32, y + h as i32))
    }

    /// Whether the hover tooltip is currently shown.
    #[cfg(target_os = "windows")]
    pub fn tooltip_shown(&self) -> bool {
        self.tip_shown
    }

    /// Re-assert the overlay's topmost z-order after another window covered it
    /// (the tray overflow flyout, Start menu, …). Cheap: a single SetWindowPos,
    /// no reposition or repaint. No-op when hidden or not in a panel mode.
    /// Not over an open popup menu: there it steps under instead.
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
    /// Every field here has stranded the panel before. `suppressed` survived
    /// `set_display` and `on_taskbar_moved` returns early while it is set, so
    /// a panel parked by a fullscreen app could not be revived by moving it
    /// or toggling it - only by restarting the app. `last` is the "what did I
    /// draw" cache, and a stale entry skips the redraw as a no-op precisely
    /// when the panel needs re-presenting.
    pub fn restart(&mut self, providers: &[ProviderData]) {
        self.suppressed = false;
        self.unavailable = false;
        self.last.clear();
        self.room_read = None;
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
    pub fn stand_down(&mut self) {
        if !Self::is_panel_mode(self.mode) || self.suppressed {
            return;
        }
        self.suppressed = true;
        self.hide();
    }

    /// The fullscreen app is gone (alt-tab back to the desktop) — return to
    /// the bar exactly like it does: re-measure the slot and re-present.
    pub fn stand_up(&mut self, providers: &[ProviderData]) {
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
        self.next_tick = None;
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
            use crate::platform::taskbar_geom::{panel_origin, room_for, thickness};
            let before = self.rect;
            // Asking the shell is the dearest thing this function does, so a
            // placed panel asks once a minute. One that is standing down asks
            // often enough to notice room appearing - but not on every event:
            // hidden means `before` is None on every re-check, and every
            // foreground change and every slide of the bar comes through
            // here. Measured while standing down: 5s costs 0.03% of a core,
            // 2s cost 0.07%, and asking on every event cost 0.26%.
            let every = if before.is_none() {
                std::time::Duration::from_secs(5)
            } else {
                std::time::Duration::from_secs(60)
            };
            let read_screen = self.room_read.is_none_or(|t| t.elapsed() >= every);
            let Some(slot) = crate::platform::taskbar_slot(self.display, read_screen) else {
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
            if self.edge != slot.edge {
                // The bar has been moved to another edge. The shell moves its
                // window there long before it draws it: see `moved_at`.
                self.moved_at = Some(std::time::Instant::now());
                self.edge = slot.edge;
            }
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
            if read_screen && slot.tray_start != 0 {
                self.room_read = Some(std::time::Instant::now());
            }
            // A bar with no room between its last button and the tray gets no
            // panel - drawn there it would sit on the buttons. The tray icon
            // stands in until room appears.
            //
            // This verdict used to drive itself, because the stretch was read
            // off the bar's pixels and the tray icon the panel raised on
            // standing down was among those pixels. It is the shell's own
            // answer now (see `win_uia`), which our icon moves in one
            // direction only: it takes a little more room to come back than
            // it took to leave, and it never flips on its own.
            let along = if slot.edge.vertical() { h } else { w } as i32;
            if !room_for(slot.band_end, slot.tray_start, along, margin) {
                if before.is_some() || !self.unavailable {
                    tracing::debug!(
                        "panel hidden: no room on the bar ({:?} edge, buttons end at {:?}, tray at {})",
                        slot.edge,
                        slot.band_end,
                        slot.tray_start
                    );
                }
                self.unavailable = true;
                self.hide();
                return;
            }
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
            // A taskbar put on another edge moves its window there at once
            // and is drawn there much later - measured at a position change:
            // the rectangle moved in 41 ms, the bar appeared 828 ms after
            // that. A panel placed on the rectangle alone hangs over the bare
            // desktop for most of a second. So it waits until the bar is what
            // the screen shows where it is about to be, and gives up waiting
            // after a second in case that test is ever wrong.
            if let Some(since) = self.moved_at {
                let centre = (x + w as i32 / 2, y + h as i32 / 2);
                if !crate::platform::taskbar_drawn_at(centre.0, centre.1) {
                    if since.elapsed() < std::time::Duration::from_secs(1) {
                        // Being drawn moves no window and fires no event, so
                        // the loop has to be asked to look again.
                        crate::platform::wake_in(120);
                        self.hide();
                        return;
                    }
                    tracing::debug!("the bar never appeared on its new edge; placing anyway");
                }
                self.moved_at = None;
                // The shell goes on arranging its bar for a while after it
                // draws it, and lands it over the panel: measured at 343 ms
                // of the bar owning the panel's own pixels. Each of these
                // passes re-presents, which re-asserts the z-order.
                self.moved_settle = 4;
            }
            if self.moved_settle > 0 {
                self.moved_settle -= 1;
                crate::platform::wake_in(150);
            }
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
        // reached the panel.
        pm.fill(hit_fill());

        let top = first_two(providers);
        paint_rows(pm, self.edge.vertical(), &top, fg, track, &self.numbers);
        self.next_tick = next_countdown_tick(&top, &self.numbers);

        let data = self.pixmap.data();
        let mut bgra = vec![0u8; data.len()];
        for (s, d) in data
            .as_chunks::<4>()
            .0
            .iter()
            .zip(bgra.as_chunks_mut::<4>().0.iter_mut())
        {
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
    use crate::ui::tray::color;
    use chrono::Utc;

    /// The taskbar panel, pinned: every fixture set on a bottom bar at 100%
    /// and 150% and on a side bar, dark and light, painted the way `redraw`
    /// paints. See ui/visual_baseline.rs.
    #[test]
    fn visual_baseline_panel() {
        use crate::ui::visual_baseline as vb;
        let mut items = Vec::new();
        for light in [false, true] {
            let (fg, track) = theme_ink(light);
            let ink = if light { "light" } else { "dark" };
            for (set, providers) in vb::sets() {
                for (sname, w, h, vertical) in [
                    ("bottom-100", 100, 48, false),
                    ("bottom-150", 150, 72, false),
                    ("side-100", 62, 96, true),
                ] {
                    let mut pm = Pixmap::new(w, h).unwrap();
                    pm.fill(hit_fill());
                    paint_rows(
                        &mut pm,
                        vertical,
                        &first_two(&providers),
                        fg,
                        track,
                        &UiNumbers::panel_default(),
                    );
                    items.push((format!("{set}-{sname}-{ink}"), pm));
                }
            }
        }
        vb::check("panel", items);
    }

    /// The panel with the reset time: beside the percent in the bar's
    /// place, or alone before the bar; every fixture set, lying down and
    /// standing up, dark and light. Not pinned: the default panel is the
    /// pinned "panel" group above.
    #[test]
    fn visual_baseline_panel_numbers() {
        use crate::ui::visual_baseline as vb;
        let both = UiNumbers::default();
        let time_only = UiNumbers {
            percent: false,
            reset: true,
        };
        let mut items = Vec::new();
        for light in [false, true] {
            let (fg, track) = theme_ink(light);
            let ink = if light { "light" } else { "dark" };
            for (set, providers) in vb::sets() {
                for (mode, numbers) in [("both", both), ("time", time_only)] {
                    for (sname, w, h, vertical) in [
                        ("bottom-100", 100, 48, false),
                        ("bottom-150", 150, 72, false),
                        ("side-100", 62, 96, true),
                    ] {
                        let mut pm = Pixmap::new(w, h).unwrap();
                        pm.fill(hit_fill());
                        paint_rows(
                            &mut pm,
                            vertical,
                            &first_two(&providers),
                            fg,
                            track,
                            &numbers,
                        );
                        items.push((format!("{mode}-{set}-{sname}-{ink}"), pm));
                    }
                }
            }
        }
        vb::check("panel-numbers", items);
    }

    /// With the reset time shown, the drawn time is part of what a repaint
    /// depends on, and the panel knows when it next reads differently;
    /// without it, neither.
    #[test]
    fn the_countdown_asks_for_a_repaint_only_when_shown() {
        let providers = crate::ui::visual_baseline::typical();
        let top = first_two(&providers);
        let percent = UiNumbers::panel_default();
        assert!(drawn_state(&providers, &percent)
            .iter()
            .all(|r| r.2.is_none()));
        assert_eq!(next_countdown_tick(&top, &percent), None);
        let both = UiNumbers::default();
        let state = drawn_state(&providers, &both);
        assert!(state.iter().all(|r| r.2.is_some()), "{state:?}");
        let tick = next_countdown_tick(&top, &both).expect("a time is shown");
        let wait = tick.saturating_duration_since(std::time::Instant::now());
        assert!(wait <= std::time::Duration::from_secs(3600), "{wait:?}");
    }

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
                unified_pool: false,
                extra_only: false,
            }],
            updated_at: Utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    /// The same reading an hour old, from the disk cache: stale on its own
    /// wall-clock age, whatever the scheduler is doing.
    fn stale(id: ProviderId, pct: u64) -> ProviderData {
        ProviderData {
            updated_at: Utc::now() - chrono::Duration::hours(1),
            received_at: None,
            ..data(id, pct)
        }
    }

    /// The light bar as measured under the shell's clock (see `theme_ink`)
    /// and the dark bar's grey.
    const LIGHT_BAR: (u8, u8, u8) = (228, 228, 228);
    const DARK_BAR: (u8, u8, u8) = (32, 32, 32);

    /// Both layouts over both system bars, a live row above a stale one, at
    /// the 100% and 200% bar sizes, to %TEMP%:
    /// `cargo test preview_panel -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn preview_panel() {
        let rows = [data(ProviderId::Claude, 16), stale(ProviderId::Codex, 64)];
        let top: Vec<&ProviderData> = rows.iter().collect();
        for (theme, light, bar) in [("dark", false, DARK_BAR), ("light", true, LIGHT_BAR)] {
            let (fg, track) = theme_ink(light);
            for (name, vertical, w, h) in [("rows", false, 119, 48), ("stack", true, 48, 64)] {
                for scale in [1, 2] {
                    let mut pm = Pixmap::new(w * scale, h * scale).unwrap();
                    pm.fill(color(bar.0, bar.1, bar.2, 255));
                    paint_rows(
                        &mut pm,
                        vertical,
                        &top,
                        fg,
                        track,
                        &UiNumbers::panel_default(),
                    );
                    let at = if scale == 1 {
                        String::new()
                    } else {
                        format!("@{scale}x")
                    };
                    let path =
                        std::env::temp_dir().join(format!("ailimits_panel_{theme}_{name}{at}.png"));
                    std::fs::write(&path, pm.encode_png().unwrap()).unwrap();
                    println!("{}", path.display());
                }
            }
        }
    }

    /// WCAG relative luminance of an 8-bit sRGB pixel.
    fn luminance(p: tiny_skia::PremultipliedColorU8) -> f64 {
        let ch = |v: u8| {
            let c = v as f64 / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(p.red()) + 0.7152 * ch(p.green()) + 0.0722 * ch(p.blue())
    }

    fn contrast(a: f64, b: f64) -> f64 {
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// A stale row must read as not live next to a live one, and still be
    /// read: over both bars, its digits are well under the live row's
    /// contrast and still above 3:1, and its bar fill still stands out from
    /// the groove. The numbers print with
    /// `cargo test an_aged_row -- --nocapture`.
    #[test]
    fn an_aged_row_is_inked_dimmer_but_stays_legible() {
        let rows = [data(ProviderId::Claude, 50), stale(ProviderId::Codex, 50)];
        let top: Vec<&ProviderData> = rows.iter().collect();
        for (light, bar, label) in [
            (true, LIGHT_BAR, "light bar"),
            (true, (243, 243, 243), "light bar, untinted"),
            (false, DARK_BAR, "dark bar"),
        ] {
            let (fg, track) = theme_ink(light);
            let mut pm = Pixmap::new(119, 48).unwrap();
            pm.fill(color(bar.0, bar.1, bar.2, 255));
            paint_rows(&mut pm, false, &top, fg, track, &UiNumbers::panel_default());
            let lum = |x: u32, y: u32| luminance(pm.pixel(x, y).unwrap());
            let back = lum(0, 0);
            // The strongest digit ink in each row: the digits take the left
            // 36% of the 119px panel, the live row is the top half and the
            // stale one the bottom.
            let digits = |ys: std::ops::Range<u32>| {
                ys.flat_map(|y| (0..43).map(move |x| (x, y)))
                    .map(|(x, y)| contrast(lum(x, y), back))
                    .fold(1.0, f64::max)
            };
            let live = digits(0..24);
            let aged = digits(24..48);
            // The stale row's bar, through its middle (y 31): filled at x 60,
            // the bare groove at x 105.
            let fill_on_groove = contrast(lum(60, 31), lum(105, 31));
            println!(
                "{label}: digits live {live:.1}:1, stale {aged:.1}:1; \
                 stale fill on groove {fill_on_groove:.1}:1"
            );
            assert!(
                aged >= 3.0,
                "{label}: stale digits at {aged:.1}:1 are too faint"
            );
            assert!(
                aged < live * 0.6,
                "{label}: stale {aged:.1}:1 vs live {live:.1}:1 does not read as dimmer"
            );
            assert!(
                fill_on_groove >= 2.0,
                "{label}: the stale fill at {fill_on_groove:.1}:1 melts into the groove"
            );
        }
    }

    /// The redraw cache holds what is painted: a row turning stale (or back)
    /// repaints, while a provider below the two painted rows never does.
    #[test]
    fn the_redraw_cache_follows_staleness_of_the_painted_rows_only() {
        let live = [data(ProviderId::Claude, 40), data(ProviderId::Codex, 10)];
        let gone_stale = [data(ProviderId::Claude, 40), stale(ProviderId::Codex, 10)];
        assert_ne!(
            drawn_state(&live, &UiNumbers::panel_default()),
            drawn_state(&gone_stale, &UiNumbers::panel_default())
        );
        let estimated = [
            data(ProviderId::Claude, 40),
            ProviderData {
                status: ProviderStatus::Estimated,
                ..data(ProviderId::Codex, 10)
            },
        ];
        assert_ne!(
            drawn_state(&live, &UiNumbers::panel_default()),
            drawn_state(&estimated, &UiNumbers::panel_default())
        );
        assert_eq!(
            drawn_state(&live, &UiNumbers::panel_default()),
            vec![(Some(40), false, None), (Some(10), false, None)]
        );

        let third = |d: ProviderData| [live[0].clone(), live[1].clone(), d];
        assert_eq!(
            drawn_state(
                &third(data(ProviderId::Copilot, 5)),
                &UiNumbers::panel_default()
            ),
            drawn_state(
                &third(stale(ProviderId::Copilot, 90)),
                &UiNumbers::panel_default()
            )
        );
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
        paint_rows(&mut pm, true, &top, fg, track, &UiNumbers::panel_default());
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
