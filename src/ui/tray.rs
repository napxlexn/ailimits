// ui/tray.rs — tray-area usage indicators.
//
// Two modes (config `general.indicator`), both a SINGLE tray icon:
//   Tray — two concentric rings for the two providers the taskbar panel
//          shows (app.rs `visible_data`: those ticked in the Taskbar column,
//          untouched the first two in the widget), the busier outside and
//          the other inside, sweeping clockwise from 12 o'clock. Monochrome,
//          inked in the system taskbar theme rather than the widget palette,
//          so the shape alone carries the reading on a light or a dark bar.
//   Bars — horizontal progress bars stacked, one row per provider shown
//          (the same two); with a single provider the row gains its percent
//          number on top.
// A third icon lives here too: the stand-in the Panel modes raise while the
// panel cannot be seen (`set_scrim_fallback`).
//
// Drawn with the widget's own tiny-skia pipeline. Re-rendered only when an
// integer % changes, to stay at ~0% idle CPU. The icon carries the widget's
// context menu; a left-click toggles the overlay.

use crate::config::schema::{IndicatorKind, UiNumbers};
use crate::config::tune::TuneTray;
use crate::providers::{ProviderData, ProviderStatus};
use crate::rows::{ModelLabel, RowView};
use crate::ui::theme::{ComputedTheme, UsageLevel};
use crate::utc::{self, Timestamp};
use anyhow::{Context, Result};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Source render size; Windows scales it down to the tray's DPI size.
const ICON_SIZE: u32 = 32;

pub struct Tray {
    mode: IndicatorKind,
    icon: Option<TrayIcon>,
    /// Last drawn integer % per provider row (a single entry in Tray mode),
    /// so we skip redraws when nothing changed.
    last: Vec<Option<u8>>,
    /// Re-run icon promotion on the next update: Explorer may create the
    /// NotifyIconSettings registry keys a moment after the icon appears.
    promote_pending: bool,
    /// The current icon is a transient Start-menu fallback (shown while a Panel
    /// indicator is occluded by the Start/Search scrim), not a configured tray
    /// mode. It renders the rings, like Tray mode.
    fallback: bool,
    /// Nothing chosen yet (a fresh install, `rows::nothing_chosen`): the
    /// tooltip says what to do instead of the bare app name.
    first_install: bool,
    /// The rings' stroke and spacing (`[tune.tray]`).
    rings: Rings,
    /// The icon's own tooltip (`tray_tip_text`): the rows chosen in the
    /// Tooltip column, as the panel's tooltip shows them. Empty until the
    /// app first sets it.
    tip: String,
}

impl Tray {
    pub fn new() -> Self {
        Self {
            mode: IndicatorKind::Off,
            icon: None,
            last: Vec::new(),
            promote_pending: false,
            fallback: false,
            first_install: false,
            rings: Rings::DEFAULT,
            tip: String::new(),
        }
    }

    /// The `[tune.tray]` knobs; the icon is redrawn when the rings they
    /// give changed.
    pub fn set_tune(&mut self, tune: &TuneTray, providers: &[ProviderData], theme: &ComputedTheme) {
        let rings = Rings::of(tune);
        if self.rings == rings {
            return;
        }
        self.rings = rings;
        self.update(providers, theme, true);
    }

    /// Whether nothing is chosen yet. Set it whenever the providers allowed
    /// or switched on change; the tooltip follows at once.
    pub fn set_first_install(&mut self, on: bool, providers: &[ProviderData]) {
        if self.first_install != on {
            self.first_install = on;
            if let Some(icon) = self.icon.as_ref() {
                let _ = icon.set_tooltip(Some(self.tip_for(providers)));
            }
        }
    }

    /// Switch the indicator mode, creating/dropping the tray icon as needed.
    pub fn set_mode(
        &mut self,
        mode: IndicatorKind,
        menu: &muda::Menu,
        providers: &[ProviderData],
        theme: &ComputedTheme,
    ) {
        self.mode = mode;
        // Only the icon modes belong to the tray; the panel modes (and Off)
        // drop the icon — the TaskbarPanel owns those.
        if !matches!(mode, IndicatorKind::Tray | IndicatorKind::Bars) {
            // Dropping the TrayIcon removes it from the tray. A panel-mode
            // Start-fallback icon (if any) goes with it.
            self.icon = None;
            self.last.clear();
            self.fallback = false;
            return;
        }
        if self.icon.is_none() {
            match build_icon(menu, "AI Limits") {
                Ok(t) => self.icon = Some(t),
                Err(e) => tracing::warn!("tray create failed: {e}"),
            }
            // Pin our icon to the visible taskbar corner (out of the overflow).
            let n = crate::platform::promote_tray_icons();
            tracing::debug!("tray icons promoted: {n}");
            self.promote_pending = true;
        }
        self.update(providers, theme, true);
    }

    /// Refresh the icon image (only when a % changed, or `force`) and tooltip.
    pub fn update(&mut self, providers: &[ProviderData], theme: &ComputedTheme, force: bool) {
        let Some(icon) = self.icon.as_ref() else {
            return;
        };
        let rings = self.fallback || matches!(self.mode, IndicatorKind::Tray);
        let state: Vec<Option<u8>> = if rings {
            // Both rings — see ring_cache_state.
            ring_cache_state(providers)
        } else if matches!(self.mode, IndicatorKind::Bars) {
            // One bar row per provider.
            providers
                .iter()
                .map(|d| provider_pct(d).map(|p| p.round() as u8))
                .collect()
        } else {
            return;
        };
        if force || state != self.last {
            self.last = state;
            // The tray icon sits on the system taskbar, which has its own
            // light/dark theme independent of the widget — render its ink to
            // match so it stays readable on a light taskbar.
            let light = crate::platform::system_uses_light_theme();
            let img = if rings {
                // The ring icon is monochrome by design — it takes no theme.
                draw_ring_icon(providers, light, self.rings)
            } else {
                draw_stacked_icon(providers, theme, light)
            };
            if let Ok(img) = img {
                let _ = icon.set_icon(Some(img));
            }
        }
        let _ = icon.set_tooltip(Some(self.tip_for(providers)));
        // Second promotion pass once the registry keys surely exist.
        if self.promote_pending {
            self.promote_pending = false;
            let n = crate::platform::promote_tray_icons();
            tracing::debug!("tray icons promoted (second pass): {n}");
        }
    }

    /// A Panel indicator cannot paint over the Start menu (a protected z-band
    /// no overlay can beat). While the Start/Search scrim is up, fall back to a
    /// tray icon — which the shell keeps visible — showing the rings of the
    /// providers the panel shows; hide it again the instant the scrim closes and the panel
    /// overlay is visible once more. No-op outside the Panel modes (Tray/Bars
    /// already show an icon; Off shows nothing). Event-driven (the foreground
    /// watch), so idle CPU is unaffected.
    pub fn set_scrim_fallback(
        &mut self,
        active: bool,
        menu: &muda::Menu,
        providers: &[ProviderData],
        theme: &ComputedTheme,
    ) {
        if !matches!(
            self.mode,
            IndicatorKind::PanelRows | IndicatorKind::PanelGrid
        ) {
            return;
        }
        if active {
            if self.icon.is_none() {
                match build_icon(menu, "AI Limits") {
                    Ok(t) => self.icon = Some(t),
                    Err(e) => {
                        tracing::warn!("tray fallback create failed: {e}");
                        return;
                    }
                }
                self.fallback = true;
                self.last.clear();
                let n = crate::platform::promote_tray_icons();
                tracing::debug!("tray fallback promoted: {n}");
                self.promote_pending = true;
            }
            if let Some(icon) = self.icon.as_ref() {
                let _ = icon.set_visible(true);
            }
            self.update(providers, theme, true);
        } else if self.fallback {
            // Keep the icon alive (so it stays promoted) but hide it; the panel
            // overlay takes over again.
            if let Some(icon) = self.icon.as_ref() {
                let _ = icon.set_visible(false);
            }
        }
    }

    /// The provider set changed (a provider appeared/disappeared) — redraw
    /// with the new row count. The single icon itself is unaffected.
    pub fn sync_providers(
        &mut self,
        _menu: &muda::Menu,
        providers: &[ProviderData],
        theme: &ComputedTheme,
    ) {
        self.update(providers, theme, true);
    }
}

impl Default for Tray {
    fn default() -> Self {
        Self::new()
    }
}

fn build_icon(menu: &muda::Menu, tooltip: &str) -> Result<TrayIcon> {
    TrayIconBuilder::new()
        // Right-click on the tray shows the SAME context menu as the widget.
        .with_menu(Box::new(menu.clone()))
        .with_tooltip(tooltip)
        .with_icon(neutral_icon(crate::platform::system_uses_light_theme())?)
        .build()
        .map_err(|e| anyhow::anyhow!("tray build: {e}"))
        .inspect(|icon| {
            // tray-icon opens the menu itself: its window is watched so the
            // look item under the pointer shows (platform::menu_hover).
            #[cfg(target_os = "windows")]
            crate::platform::menu_hover::watch(icon.window_handle() as isize);
            #[cfg(not(target_os = "windows"))]
            let _ = icon;
        })
}

impl Tray {
    /// The tooltip anew (`tray_tip_text`); shown at once when it changed.
    pub fn set_tip(&mut self, text: String) {
        if self.tip == text {
            return;
        }
        self.tip = text;
        if let Some(icon) = self.icon.as_ref() {
            let _ = icon.set_tooltip(Some(&self.tip));
        }
    }

    /// What the icon's tooltip says: the app's text, or before the app
    /// first sets one, the providers the icon shows.
    fn tip_for(&self, providers: &[ProviderData]) -> String {
        if self.tip.is_empty() {
            tooltip(providers, self.first_install)
        } else {
            self.tip.clone()
        }
    }
}

/// Windows keeps 128 UTF-16 units of a tray icon's tooltip, the last one
/// its end (`NOTIFYICONDATAW::szTip`).
const TRAY_TIP_UNITS: usize = 127;

/// The tray icon's own tooltip: the same rows and parts as the panel's
/// tooltip (the Tooltip column, `[ui.tooltip]`; `tooltip_lines`), one line
/// each, so hovering either shows the same list. Whole lines within what
/// Windows keeps, and a last line "…" when some did not fit.
pub(crate) fn tray_tip_text(rows: &[RowView], first_install: bool, tip: &UiNumbers) -> String {
    let lines: Vec<String> = tooltip_lines(rows, first_install, tip)
        .into_iter()
        .map(|l| {
            if l.indent {
                format!("   {}", l.text)
            } else {
                l.text
            }
        })
        .collect();
    let units = |s: &str| s.encode_utf16().count();
    let all = lines.join("\n");
    if units(&all) <= TRAY_TIP_UNITS {
        return all;
    }
    let mut kept = String::new();
    for line in &lines {
        let next = if kept.is_empty() {
            line.clone()
        } else {
            format!("{kept}\n{line}")
        };
        if units(&next) + 2 > TRAY_TIP_UNITS {
            break;
        }
        kept = next;
    }
    if kept.is_empty() {
        // Not even the first line fits: as much of it as does.
        for ch in lines.first().map(String::as_str).unwrap_or("").chars() {
            if units(&kept) + ch.len_utf16() + 1 > TRAY_TIP_UNITS {
                break;
            }
            kept.push(ch);
        }
        return format!("{kept}…");
    }
    format!("{kept}\n…")
}

/// A provider's primary % if its data is usable.
pub(crate) fn provider_pct(data: &ProviderData) -> Option<f32> {
    matches!(data.status, ProviderStatus::Ok | ProviderStatus::Estimated)
        .then(|| data.primary_percentage())
        .flatten()
}

/// The two busiest providers, highest first, as rounded percentages.
///
/// Providers without a usable percentage are never candidates. Ties keep the
/// order the widget shows them in, so two providers sitting at the same number
/// do not trade rings from one refresh to the next.
pub(crate) fn two_busiest(providers: &[ProviderData]) -> (Option<u8>, Option<u8>) {
    let mut pcts: Vec<u8> = providers
        .iter()
        .filter_map(provider_pct)
        .map(|p| p.round() as u8)
        .collect();
    // A stable sort keeps equal percentages in widget order.
    pcts.sort_by(|a, b| b.cmp(a));
    let mut it = pcts.into_iter();
    (it.next(), it.next())
}

/// What the repaint cache must hold: BOTH rings. Caching only the busiest
/// would freeze the icon whenever the second-place provider moved - it
/// compiles, looks right, and stops updating minutes later in normal use.
pub(crate) fn ring_cache_state(providers: &[ProviderData]) -> Vec<Option<u8>> {
    let (first, second) = two_busiest(providers);
    vec![first, second]
}

/// The providers the icon shows and their percentages, as one line: the
/// icon's tooltip until the app first gives it `tray_tip_text`. On a fresh
/// install with nothing chosen it says what to do.
pub(crate) fn tooltip(providers: &[ProviderData], first_install: bool) -> String {
    summary(providers.iter(), first_install)
}

fn summary<'a>(providers: impl Iterator<Item = &'a ProviderData>, first_install: bool) -> String {
    let parts: Vec<String> = providers
        .map(|d| match provider_pct(d) {
            Some(p) => format!("{} {}%", d.id.display_name(), p.round() as u32),
            None => format!("{} •", d.id.display_name()),
        })
        .collect();
    if parts.is_empty() {
        return if first_install {
            FIRST_INSTALL_TIP.to_string()
        } else {
            "AI Limits".to_string()
        };
    }
    parts.join("  ·  ")
}

/// What the tray and the panel tooltips say before anything is chosen.
const FIRST_INSTALL_TIP: &str = "AI Limits: choose what to show";

/// One line of the panel's drawn tooltip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TipLine {
    pub text: String,
    /// The continuation of a row too long for one line: set in a little.
    pub indent: bool,
}

/// What the panel's drawn tooltip says, line by line: the tooltip's rows
/// (`rows::visible_rows`, Surface::Tooltip), each with what `tip` asks for.
/// While every row is a provider's own with no model label and only the
/// percentage is asked for it is the single line of 0.7.0, the same text as
/// the tray's (and so it is with no rows at all). Otherwise, as by default,
/// one row per line: "Claude 25%",
/// "Claude 2h 14min" or "Claude 25% · 2h 14min" (the time left until the
/// reset of the number the row shows), then the model label complete (the
/// tooltip has no width limit), a line wider than `TIP_WRAP_PX` wrapped at
/// a " · ". A row with nothing to say keeps the no-data dot.
pub(crate) fn tooltip_lines(
    rows: &[RowView],
    first_install: bool,
    tip: &UiNumbers,
) -> Vec<TipLine> {
    let plain = tip.percent && !tip.reset;
    if rows.is_empty() || plain && rows.iter().all(|r| !r.is_extra() && r.model.is_none()) {
        return vec![TipLine {
            text: summary(rows.iter().map(|r| &r.data), first_install),
            indent: false,
        }];
    }
    let now = utc::now();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut line = row.name().to_string();
        let pct = provider_pct(&row.data).filter(|_| tip.percent);
        let reset = own_reset(&row.data).filter(|_| tip.reset);
        match pct {
            Some(p) => line.push_str(&format!(" {}%", p.round() as u32)),
            // The percentage asked for and not known, or nothing known.
            None if tip.percent || reset.is_none() => line.push_str(" •"),
            None => {}
        }
        if let Some(reset) = reset {
            let secs = (reset - now).as_secs();
            line.push_str(if pct.is_some() { " · " } else { " " });
            line.push_str(&crate::ui::renderer::format_duration(secs));
        }
        if let Some(label) = row.model.as_ref().and_then(ModelLabel::full) {
            // After the no-data dot the label follows with a space: "Grok •
            // Grok 4.3", never "• ·".
            line.push_str(if line.ends_with('•') { " " } else { " · " });
            line.push_str(&label);
        }
        out.extend(wrap_tip_line(&line));
    }
    out
}

/// The multi-line tooltip wraps a line wider than this (100% scaling) at a
/// " · ", so a long model label never makes a tooltip half a screen wide.
const TIP_WRAP_PX: f32 = 400.0;
/// How far a wrapped line's continuation is set in (100% scaling).
const TIP_WRAP_INDENT: f32 = 20.0;
/// Space between two lines of the multi-line tooltip (100% scaling).
const TIP_LINE_GAP: f32 = 5.0;

/// `line` as it fits `TIP_WRAP_PX`: whole, or broken at its " · " marks.
/// A part wider than the limit on its own stays whole: a word is never cut.
fn wrap_tip_line(line: &str) -> Vec<TipLine> {
    let width = |text: &str, indent: bool| {
        text_extent(text, TIP_FONT_PX).0 + if indent { TIP_WRAP_INDENT } else { 0.0 }
    };
    if width(line, false) <= TIP_WRAP_PX {
        return vec![TipLine {
            text: line.to_string(),
            indent: false,
        }];
    }
    let mut out: Vec<TipLine> = Vec::new();
    let mut current = String::new();
    for part in line.split(" · ") {
        if current.is_empty() {
            current = part.to_string();
            continue;
        }
        let joined = format!("{current} · {part}");
        if width(&joined, !out.is_empty()) <= TIP_WRAP_PX {
            current = joined;
        } else {
            let indent = !out.is_empty();
            out.push(TipLine {
                text: std::mem::replace(&mut current, part.to_string()),
                indent,
            });
        }
    }
    let indent = !out.is_empty();
    out.push(TipLine {
        text: current,
        indent,
    });
    out
}

/// Tray-icon ink for the system taskbar's light/dark theme
/// (`theme::tray_ink`). `a` is the alpha — 255 for a solid fill/digit, ~64
/// for the faint track ring, ~200 for an inactive dot.
fn tray_ink(light: bool, a: u8) -> tiny_skia::Color {
    crate::ui::theme::tray_ink(light, a).to_skia_color()
}

/// A neutral grey dot: the placeholder before the first data arrives.
fn neutral_icon(light: bool) -> Result<Icon> {
    let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).context("pixmap alloc")?;
    pm.fill(tiny_skia::Color::TRANSPARENT);
    fill_circle(&mut pm, CENTER, CENTER, 5.0, tray_ink(light, 200));
    to_icon(&pm)
}

/// Ring geometry, in the 32px icon space.
///
/// Rings read smaller than the filled disc they replaced even at an identical
/// outer radius, because a thin outline carries far less ink. These numbers buy
/// that presence back: the outer edge sits 1px from the canvas edge (as close
/// as the antialiased stroke can go), the strokes are heavy, and the hole in the
/// middle is small.
///
/// `RING_STEP` is the drop in outer radius from one ring to the next. It is NOT
/// free to raise the stroke width alone: `RING_STEP - RING_W` is the bare space
/// between the two rings, and the shell halves it when it scales the icon to
/// 16px. At 2.0 here that is a whole pixel of separation — below roughly 1.5 the
/// rings start merging into one thick band, which in a monochrome icon destroys
/// the only cue that there are two providers.
///
/// The other cost of a thicker stroke lands on the inner ring: a round cap
/// spans `RING_W/2` of a circumference that shrinks with every step inward, so
/// values under ~15% no longer fit as an arc there and degrade to a dot.
const RING_W: f32 = 5.0;
/// The icon square's centre — every ring is concentric on it.
const CENTER: f32 = ICON_SIZE as f32 / 2.0;
const RING_OUTER: f32 = 15.0;
const RING_STEP: f32 = 7.0;
/// The smallest hole the inner ring leaves at the centre, radius in canvas
/// pixels, whatever `[tune.tray]` asks.
const RING_HOLE_MIN: f32 = 1.0;

/// The rings as drawn: the stroke width and `step`, the drop in outer
/// radius from one ring to the next (the stroke plus the bare gap). The
/// `[tune.tray]` knobs, fitted to the icon; `DEFAULT` is the constants above.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rings {
    w: f32,
    step: f32,
}

impl Rings {
    const DEFAULT: Rings = Rings {
        w: RING_W,
        step: RING_STEP,
    };

    /// The knobs' rings. The outer ring keeps its place 1px inside the
    /// canvas; a gap that would close the inner ring's hole is cut down so
    /// the hole keeps `RING_HOLE_MIN`.
    fn of(tune: &TuneTray) -> Self {
        let w = tune.ring_width();
        let gap = tune
            .ring_gap()
            .min(RING_OUTER - RING_HOLE_MIN - 2.0 * w)
            .max(0.0);
        Self { w, step: w + gap }
    }
}

fn draw_ring_icon(providers: &[ProviderData], light: bool, rings: Rings) -> Result<Icon> {
    let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).context("pixmap alloc")?;
    pm.fill(tiny_skia::Color::TRANSPARENT);
    // One ink for everything. The tray sits on the system taskbar, whose theme
    // is independent of the widget's palette, and at 16px a hue difference is
    // far weaker than the arc length — so the shape does the talking.
    let ink = tray_ink(light, 255);
    let track = tray_ink(light, 64);

    match two_busiest(providers) {
        // Two providers with data: the busier takes the outer ring, the
        // other the inner one. Which is which is answered by the tooltip,
        // which lists both with their percentages.
        (Some(first), Some(second)) => {
            draw_ring(&mut pm, RING_OUTER, rings.w, first as f32, ink, track);
            let inner = RING_OUTER - rings.step;
            draw_ring(&mut pm, inner, rings.w, second as f32, ink, track);
        }
        // Exactly one: the outer ring alone, so the icon keeps its meaning
        // when a second provider starts reporting — nothing moves, a ring
        // simply appears inside.
        (Some(only), None) => {
            draw_ring(&mut pm, RING_OUTER, rings.w, only as f32, ink, track);
        }
        // No percentage data yet (loading / binary / error).
        _ => fill_circle(&mut pm, CENTER, CENTER, 5.0, tray_ink(light, 200)),
    }

    to_icon(&pm)
}

/// 12 o'clock, where every ring starts.
const RING_TOP: f32 = -std::f32::consts::FRAC_PI_2;

/// What a percentage should actually paint on a ring.
///
/// The subtlety this type exists for: a ROUND cap bulges half a stroke width
/// past each end of the arc. On the inner ring that overhang is nearly a tenth
/// of the circumference at both ends together, so an uncompensated 90% painted
/// as a closed circle — indistinguishable from 100%. `Arc` therefore carries
/// the angles to *sweep*, already pulled in by one cap at each end, so that the
/// visible ink spans exactly the percentage and no more.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RingFill {
    /// Nothing to paint.
    Empty,
    /// Shorter than the two caps it would grow — a single cap-sized dot at the
    /// top, so a small non-zero value never reads as zero.
    Dot,
    /// Sweep from `start` to `end` radians.
    Arc { start: f32, end: f32 },
    /// A closed ring. Only a true 100% earns this.
    Full,
}

/// The angular overhang a round cap adds beyond each end of an arc.
fn cap_angle(w: f32, r_mid: f32) -> f32 {
    (w / 2.0) / r_mid
}

/// Decide what `pct` paints on a ring of stroke `w` centred on radius `r_mid`.
fn ring_fill(pct: f32, r_mid: f32, w: f32) -> RingFill {
    let frac = (pct / 100.0).clamp(0.0, 1.0);
    if frac <= 0.0 {
        return RingFill::Empty;
    }
    if frac >= 1.0 {
        return RingFill::Full;
    }
    let sweep = frac * std::f32::consts::TAU;
    let cap = cap_angle(w, r_mid);
    if sweep <= 2.0 * cap {
        return RingFill::Dot;
    }
    RingFill::Arc {
        start: RING_TOP + cap,
        end: RING_TOP + sweep - cap,
    }
}

/// One ring, `w` thick: the faint full-circle track, then the used arc over
/// it.
fn draw_ring(
    pm: &mut Pixmap,
    outer: f32,
    w: f32,
    pct: f32,
    ink: tiny_skia::Color,
    track: tiny_skia::Color,
) {
    let r_mid = outer - w / 2.0;
    let full = std::f32::consts::TAU;
    stroke_arc(pm, r_mid, w, 0.0, full, track, LineCap::Butt);
    match ring_fill(pct, r_mid, w) {
        RingFill::Empty => {}
        RingFill::Dot => fill_circle(
            pm,
            CENTER + r_mid * RING_TOP.cos(),
            CENTER + r_mid * RING_TOP.sin(),
            w / 2.0,
            ink,
        ),
        RingFill::Arc { start, end } => stroke_arc(pm, r_mid, w, start, end, ink, LineCap::Round),
        RingFill::Full => stroke_arc(pm, r_mid, w, 0.0, full, ink, LineCap::Butt),
    }
}

/// Stroke an arc `w` wide as a polyline. tiny-skia's PathBuilder has no arc
/// primitive; sampling finely enough that each segment is well under a pixel
/// makes the difference invisible once the shell scales the icon down.
fn stroke_arc(
    pm: &mut Pixmap,
    r_mid: f32,
    w: f32,
    start: f32,
    end: f32,
    c: tiny_skia::Color,
    cap: LineCap,
) {
    let span = end - start;
    // ~0.5px per segment, never fewer than a handful for very short arcs.
    let steps = (((span.abs() * r_mid) / 0.5).ceil() as usize).clamp(6, 512);
    let mut pb = PathBuilder::new();
    for i in 0..=steps {
        let a = start + span * (i as f32 / steps as f32);
        let (x, y) = (CENTER + r_mid * a.cos(), CENTER + r_mid * a.sin());
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color(c);
    paint.anti_alias = true;
    let stroke = Stroke {
        width: w,
        line_cap: cap,
        // Round joins keep the sampled polyline from showing facets.
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

/// Horizontal progress bars stacked one above the other — one row per
/// provider: the percent number on the left, the bar filling left-to-right
/// in the usage-level color on the right. With a single provider the number
/// sits big on top of a full-width bar. A provider without data shows a
/// short grey sliver and no number. The 16px tray square is the hard limit
/// here: with 3-4 rows the digits get tiny — the tooltip always has them.
fn draw_stacked_icon(
    providers: &[ProviderData],
    theme: &ComputedTheme,
    light: bool,
) -> Result<Icon> {
    let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).context("pixmap alloc")?;
    pm.fill(tiny_skia::Color::TRANSPARENT);
    let n = providers.len();
    if n == 0 {
        fill_circle(&mut pm, 16.0, 16.0, 5.0, tray_ink(light, 200));
        return to_icon(&pm);
    }
    // Digits in the system-theme ink so they read on a light or dark taskbar.
    let ink = tray_ink(light, 255);

    // Row geometry: (bar_x, bar_w, y, h) + an optional digit zone per row.
    let mut rows: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(n);
    let (digit_w, gap) = (13.0, 2.0);
    if n == 1 {
        // Big digits on top, a full-width bar along the bottom edge.
        if let Some(pct) = provider_pct(&providers[0]) {
            let val = (pct.round().clamp(0.0, 100.0) as u32).to_string();
            draw_digits_fit(&mut pm, &val, 0.0, 0.0, 32.0, 22.0, ink);
        }
        rows.push((1.0, 30.0, 24.0, 7.0));
    } else {
        // Evenly stacked rows, digits left, bar right.
        let bar_h = ((30.0 - gap * (n - 1) as f32) / n as f32).floor();
        let mut y = (ICON_SIZE as f32 - (bar_h * n as f32 + gap * (n - 1) as f32)) / 2.0;
        for data in providers {
            if let Some(pct) = provider_pct(data) {
                let val = (pct.round().clamp(0.0, 100.0) as u32).to_string();
                draw_digits_fit(&mut pm, &val, 0.0, y, digit_w, bar_h, ink);
            }
            // Cap the bar's visual thickness so 2 rows read as bars, not squares.
            let vis_h = bar_h.min(9.0);
            rows.push((
                digit_w + 2.0,
                32.0 - digit_w - 3.0,
                y + (bar_h - vis_h) / 2.0,
                vis_h,
            ));
            y += bar_h + gap;
        }
    }

    for (data, (x, w, y, h)) in providers.iter().zip(rows) {
        let rad = (h / 2.0).min(3.5);
        fill_round_rect(&mut pm, x, y, w, h, rad, tray_ink(light, 64));
        match provider_pct(data) {
            Some(pct) => {
                let frac = (pct / 100.0).clamp(0.0, 1.0);
                // Keep a sliver visible at low usage so the bar never looks absent.
                let fill_w = (w * frac).max(3.0);
                // Monochrome greys vanish on a light taskbar — use the system
                // ink; colored palettes keep their hue (they contrast either way).
                let lvl = theme.palette_level(UsageLevel::at(pct, theme.mid_at, theme.high_at));
                let fill = if theme.monochrome {
                    tray_ink(light, 255)
                } else {
                    color(lvl.bar.r, lvl.bar.g, lvl.bar.b, 255)
                };
                fill_round_rect(&mut pm, x, y, fill_w, h, rad, fill);
            }
            None => fill_round_rect(&mut pm, x, y, 3.0, h, rad, tray_ink(light, 200)),
        }
    }
    to_icon(&pm)
}

/// The shared UI font, loaded once (same candidate chain renderer.rs uses, so
/// the tray/panel survive a missing Segoe UI exactly like the main widget).
fn font() -> Option<&'static fontdue::Font> {
    use std::sync::OnceLock;
    static FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();
    FONT.get_or_init(crate::ui::renderer::load_ui_font).as_ref()
}

/// Measure the rendered ink bounds of `text` at `size`.
fn digits_bounds(
    font: &fontdue::Font,
    text: &str,
    size: f32,
) -> Option<(fontdue::layout::Layout, f32, f32, f32, f32)> {
    use fontdue::layout::{CoordinateSystem, Layout, TextStyle};
    let mut tl = Layout::new(CoordinateSystem::PositiveYDown);
    tl.append(&[font], &TextStyle::new(text, size, 0));
    let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
    let (mut min_y, mut max_y) = (f32::MAX, f32::MIN);
    for g in tl.glyphs() {
        min_x = min_x.min(g.x);
        max_x = max_x.max(g.x + g.width as f32);
        min_y = min_y.min(g.y);
        max_y = max_y.max(g.y + g.height as f32);
    }
    (min_x <= max_x).then_some((tl, min_x, max_x, min_y, max_y))
}

/// Draw `text` centered inside the given zone, auto-shrinking the font so
/// it fits both the zone's height and width ("100" vs "9").
pub(crate) fn draw_digits_fit(
    pm: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    c: tiny_skia::Color,
) {
    let Some(font) = font() else {
        return;
    };
    // Start from the height budget, then shrink to fit both axes.
    let mut size = h * 1.45;
    let Some((_, min_x, max_x, min_y, max_y)) = digits_bounds(font, text, size) else {
        return;
    };
    let (ink_w, ink_h) = (max_x - min_x, max_y - min_y);
    let scale = (w / ink_w).min(h / ink_h).min(1.0);
    size *= scale;
    let Some((tl, min_x, max_x, min_y, max_y)) = digits_bounds(font, text, size) else {
        return;
    };
    let off_x = x + (w - (max_x - min_x)) / 2.0 - min_x;
    let off_y = y + (h - (max_y - min_y)) / 2.0 - min_y;
    blit_glyphs(pm, font, &tl, off_x, off_y, c);
}

/// Source-over a laid-out text onto the pixmap at an offset, in `c`; the
/// ink's own alpha scales the coverage.
fn blit_glyphs(
    pm: &mut Pixmap,
    font: &fontdue::Font,
    tl: &fontdue::layout::Layout,
    off_x: f32,
    off_y: f32,
    c: tiny_skia::Color,
) {
    let glyphs = tl.glyphs();
    let (cr, cg, cb) = (
        (c.red() * 255.0) as u32,
        (c.green() * 255.0) as u32,
        (c.blue() * 255.0) as u32,
    );
    // The ink's own alpha scales the coverage, so a dimmed ink (the taskbar
    // panel's stale rows) draws dimmer digits. An opaque ink — every tray
    // icon's — leaves the coverage exactly as it was.
    let alpha = (c.alpha() * 255.0).round() as u32;
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let data = pm.data_mut();
    for g in glyphs {
        let (metrics, bitmap) = font.rasterize_config(g.key);
        for (i, &cov) in bitmap.iter().enumerate() {
            if cov == 0 {
                continue;
            }
            let px = (off_x + g.x) as i32 + (i % metrics.width) as i32;
            let py = (off_y + g.y) as i32 + (i / metrics.width) as i32;
            if px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }
            // Source-over onto the transparent canvas (premultiplied RGBA).
            let idx = ((py * w + px) * 4) as usize;
            let a = cov as u32 * alpha / 255;
            let inv = 255 - a;
            data[idx] = ((cr * a) / 255 + data[idx] as u32 * inv / 255) as u8;
            data[idx + 1] = ((cg * a) / 255 + data[idx + 1] as u32 * inv / 255) as u8;
            data[idx + 2] = ((cb * a) / 255 + data[idx + 2] as u32 * inv / 255) as u8;
            data[idx + 3] = (a + data[idx + 3] as u32 * inv / 255).min(255) as u8;
        }
    }
}

/// The reference ink a countdown is sized and placed by: `draw_digits_fit`
/// fits the whole ink of "25%" to its band, and the % reaches a little past
/// the digits, so text sized by this string gets the percent's own digit
/// size and baseline.
const DIGIT_REF: &str = "0%";

/// Draw `text` with its digits as tall as a percent drawn by
/// `draw_digits_fit` in a band `digit_h` tall at `y`, on the same baseline,
/// whatever letters it carries ("2h14" has an ascender digits lack):
/// centred in [x, x + w], or from `x` when `left`, and shrunk about the
/// band's middle when wider than `w`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text_like_digits(
    pm: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    w: f32,
    digit_h: f32,
    left: bool,
    c: tiny_skia::Color,
) {
    let Some(font) = font() else {
        return;
    };
    let size_for = |dh: f32| {
        digits_bounds(font, DIGIT_REF, 100.0).map(|(_, _, _, y0, y1)| 100.0 * dh / (y1 - y0))
    };
    let Some(mut size) = size_for(digit_h) else {
        return;
    };
    let mut top = y;
    let Some((_, x0, x1, _, _)) = digits_bounds(font, text, size) else {
        return;
    };
    if x1 - x0 > w && w > 0.0 {
        let k = w / (x1 - x0);
        size *= k;
        top = y + digit_h * (1.0 - k) / 2.0;
    }
    let (Some((tl, x0, x1, _, _)), Some((_, _, _, ry0, ry1))) = (
        digits_bounds(font, text, size),
        digits_bounds(font, DIGIT_REF, size),
    ) else {
        return;
    };
    let ink_w = x1 - x0;
    let start = if left { x } else { x + (w - ink_w) / 2.0 };
    // Where draw_digits_fit puts the reference ink: centred in the band.
    let band = size_for(digit_h).map_or(digit_h, |full| digit_h * size / full);
    let ref_top = top + (band - (ry1 - ry0)) / 2.0;
    blit_glyphs(pm, font, &tl, start - x0, ref_top - ry0, c);
}

/// When the limit a row shows resets: its headline window's own reset, in
/// the future. Never another window's (`ProviderData::headline_reset` falls
/// back to the nearest reset of any window, which would put a weekly's
/// "3d 4h" beside a session's "0%"). None without a usable percentage.
pub(crate) fn own_reset(data: &ProviderData) -> Option<Timestamp> {
    provider_pct(data)?;
    data.headline_metric()?.reset_at.filter(|t| *t > utc::now())
}

const MIN: i64 = 60;
const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

/// The taskbar panel's countdown, compact like the clock beside it:
/// "3d4h", "2h14", "12m", "<1m".
pub(crate) fn countdown_compact(secs: i64) -> String {
    let (d, h, m) = (secs / DAY, secs % DAY / HOUR, secs % HOUR / MIN);
    if d > 0 {
        if h > 0 {
            format!("{d}d{h}h")
        } else {
            format!("{d}d")
        }
    } else if h > 0 {
        format!("{h}h{m:02}")
    } else if m > 0 {
        format!("{m}m")
    } else {
        "<1m".into()
    }
}

/// Seconds until `countdown_compact(secs)` reads differently: the next hour
/// while it counts days, the next minute while it counts hours or minutes,
/// the reset itself below a minute. At least one second.
pub(crate) fn countdown_changes_in(secs: i64) -> i64 {
    let step = if secs >= DAY {
        HOUR
    } else if secs >= MIN {
        MIN
    } else {
        return secs.max(0) + 1;
    };
    secs % step + 1
}

/// Shell tooltip metrics at 100% scaling, in pixels. Windows draws its own
/// tooltips with Segoe UI at 9pt (12px at 96 DPI), a tight box around the text,
/// and a small corner radius — not a pill.
///
/// EXPERIMENTAL: the previous numbers derived everything from the taskbar
/// height (font `bar_h * 0.30`, padding `0.85`/`0.55` of the font, radius a
/// third of the box), which on a stock 48px bar produced a 14.4px font in a
/// fully rounded pill — noticeably larger and rounder than anything the shell
/// shows.
const TIP_FONT_PX: f32 = 12.0;
const TIP_PAD_X: f32 = 10.0;
/// Vertically asymmetric, as the shell's is: 10 above the text, 8 below.
const TIP_PAD_TOP: f32 = 10.0;
const TIP_PAD_BOTTOM: f32 = 8.0;
/// The text band the padding is measured against. Fixing it (rather than using
/// the ink height of whatever string we happen to show) is what keeps the box
/// 30px tall for "Claude 68%" and for a string with descenders alike — the
/// shell's box does not breathe with its text either.
const TIP_LINE_H: f32 = 12.0;
/// Fitted, not guessed. Sub-pixel coverage of the shell's top-left corner gives
/// insets of 1.71, 0.60, 0.03, -0.14 px over the first four rows; least squares
/// over radii 2.0..5.0 puts the minimum squarely at 4.0 (error 0.018, an order
/// of magnitude better than 3.5).
const TIP_RADIUS: f32 = 4.0;
/// Room reserved around the box for the drop shadow: one more than the
/// outermost ring steps out, so it is not clipped. The shell's shadow reaches
/// about 5px to the side (peaking at 8% darkening) and barely 2px above.
const TIP_SHADOW: f32 = 6.0;
/// Gap between the tooltip and the top of the taskbar. Measured at 12px; ours
/// sat at 4 and read as glued to the bar.
pub(crate) const TIP_GAP: i32 = 12;
/// Very nearly opaque. Measured, not chosen: the shell's tooltip body reads
/// grey 44 over black and grey 54 over white, so only ~10/255 of the backdrop
/// comes through, over a true colour of grey 46. The earlier 218 was visibly
/// more transparent than the real thing.
///
/// 246, not 244: 244 was fitted while the shadow still showed through the body
/// and darkened it. With the box punched out of the shadow the body stands
/// alone, and needs the extra two counts to land on the same measured value.
/// The value, and the light theme's more translucent 219 (Windows' light
/// acrylic is not a plain alpha blend), live in `theme::tip_colors`.
/// The bar height that means 100% scaling; the bar is our only DPI signal here.
const TIP_BASE_BAR_H: f32 = 48.0;

/// How far the drawn box sits inside the returned pixmap, for `bar_h`. The
/// caller needs it to place the box (not the shadow) against the taskbar.
pub(crate) fn tip_shadow_inset(bar_h: f32) -> i32 {
    ((bar_h / TIP_BASE_BAR_H).clamp(0.85, 3.0) * TIP_SHADOW).round() as i32
}

/// Render the hover tooltip pixmap for the taskbar panel: the provider summary
/// drawn by us (not a native control) so it matches the shell's own tooltip. It
/// follows the system theme like the real Win11 tooltip: a borderless dark box
/// in dark mode, a near-white box with a hairline border in light mode. Sized to
/// the text; `bar_h` carries the display scaling.
pub(crate) fn render_tooltip(text: &str, bar_h: f32, light: bool) -> Pixmap {
    // Scale by the bar, but from the shell's own metrics rather than from a
    // fraction of the bar. Clamped so an odd bar height cannot make the
    // tooltip unreadable or enormous.
    let scale = (bar_h / TIP_BASE_BAR_H).clamp(0.85, 3.0);
    let size = TIP_FONT_PX * scale;
    let (tw, th) = text_extent(text, size);
    let pad_x = (TIP_PAD_X * scale).round();
    let pad_top = (TIP_PAD_TOP * scale).round();
    let pad_bottom = (TIP_PAD_BOTTOM * scale).round();
    let line_h = (TIP_LINE_H * scale).round();
    let box_w = (tw + pad_x * 2.0).ceil().max(8.0);
    let box_h = (line_h + pad_top + pad_bottom).ceil().max(8.0);
    // The pixmap is bigger than the box: the shadow needs room around it.
    let inset = tip_shadow_inset(bar_h) as f32;
    let w = (box_w + inset * 2.0) as u32;
    let h = (box_h + inset * 2.0) as u32;
    // Centre the actual ink inside the fixed line band, so a string with no
    // descenders does not float high in the box.
    let text_y = inset + pad_top + (line_h - th) / 2.0;
    let mut pm = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
    pm.fill(tiny_skia::Color::TRANSPARENT);
    let radius = (TIP_RADIUS * scale).min(box_h / 2.0);
    let text_ink = paint_tip_box(&mut pm, inset, box_w, box_h, radius, scale, light);
    draw_text_at(&mut pm, text, inset + pad_x, text_y, size, text_ink);
    pm
}

/// The panel's tooltip with one line per entry of `lines` (`tooltip_lines`):
/// the same box, shadow, padding and text as `render_tooltip`, lines
/// `TIP_LINE_GAP` apart on a common baseline grid. A single line is exactly
/// `render_tooltip`.
pub(crate) fn render_tooltip_lines(lines: &[TipLine], bar_h: f32, light: bool) -> Pixmap {
    match lines {
        [] => return render_tooltip("AI Limits", bar_h, light),
        [one] if !one.indent => return render_tooltip(&one.text, bar_h, light),
        _ => {}
    }
    let scale = (bar_h / TIP_BASE_BAR_H).clamp(0.85, 3.0);
    let size = TIP_FONT_PX * scale;
    let pad_x = (TIP_PAD_X * scale).round();
    let pad_top = (TIP_PAD_TOP * scale).round();
    let pad_bottom = (TIP_PAD_BOTTOM * scale).round();
    let line_h = (TIP_LINE_H * scale).round();
    let gap = (TIP_LINE_GAP * scale).round();
    let indent = (TIP_WRAP_INDENT * scale).round();
    let n = lines.len() as f32;
    let text_w = lines
        .iter()
        .map(|l| text_extent(&l.text, size).0 + if l.indent { indent } else { 0.0 })
        .fold(0.0, f32::max);
    let box_w = (text_w + pad_x * 2.0).ceil().max(8.0);
    let box_h = (pad_top + line_h * n + gap * (n - 1.0) + pad_bottom)
        .ceil()
        .max(8.0);
    let inset = tip_shadow_inset(bar_h) as f32;
    let w = (box_w + inset * 2.0) as u32;
    let h = (box_h + inset * 2.0) as u32;
    let mut pm = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
    pm.fill(tiny_skia::Color::TRANSPARENT);
    let radius = (TIP_RADIUS * scale).min(box_h / 2.0);
    let ink = paint_tip_box(&mut pm, inset, box_w, box_h, radius, scale, light);

    // Every line sits on the same grid: its capital height centred in its
    // band, as the single line's ink is, so the lines keep one baseline step
    // whatever letters they hold.
    let Some(font) = font() else {
        return pm;
    };
    let Some((_, _, _, cap_top, cap_bottom)) = digits_bounds(font, "H", size) else {
        return pm;
    };
    let cap_h = cap_bottom - cap_top;
    for (i, line) in lines.iter().enumerate() {
        let Some((_, _, _, ink_top, _)) = digits_bounds(font, &line.text, size) else {
            continue;
        };
        let band_top = inset + pad_top + (line_h + gap) * i as f32;
        let cap_y = band_top + (line_h - cap_h) / 2.0;
        let x = inset + pad_x + if line.indent { indent } else { 0.0 };
        draw_text_at(
            &mut pm,
            &line.text,
            x,
            cap_y + (ink_top - cap_top),
            size,
            ink,
        );
    }
    pm
}

/// The tooltip's box inside `pm`: shadow, body and, in light mode, the
/// hairline. Returns the ink for its text.
fn paint_tip_box(
    pm: &mut Pixmap,
    inset: f32,
    box_w: f32,
    box_h: f32,
    radius: f32,
    scale: f32,
    light: bool,
) -> tiny_skia::Color {
    // Drop shadow: concentric rounded rects stepping outward, each barely
    // visible, which approximates the shell's falloff without a blur pass.
    // Measured over mid grey: -3,-4,-7,-10 luma at 1..4px out, nothing beyond.
    // The alphas COMPOUND: each ring is painted over the previous one, so the
    // cumulative darkening at distance d is 1-prod(1-a). Picked to land on the
    // measured falloff (-3,-4,-7,-10 luma at 5..2px out over grey 128) rather
    // than by eye; a first pass at 6/8/10/14 compounded to -15..-31, three
    // times too heavy.
    // The light theme casts a much softer shadow: measured -2/-4/-6 at 4..2px
    // out against the dark theme's -4/-7/-10. Reusing the dark ramp made the
    // light tooltip look like it was floating higher than the shell's.
    let colors = crate::ui::theme::tip_colors(light);
    for (step, alpha) in colors.shadow {
        let s = step * scale;
        fill_round_rect(
            pm,
            inset - s,
            inset - s + 1.0,
            box_w + s * 2.0,
            box_h + s * 2.0 - 1.0,
            radius + s,
            crate::ui::theme::Color {
                a: alpha,
                ..colors.shadow_color
            }
            .to_skia_color(),
        );
    }
    // Punch the box out of the shadow before painting the body. Without this
    // the shadow rings lie UNDER the box as well, and a body at alpha 244 lets
    // ~4% of them through - so the body's final colour depended on the shadow's
    // alphas, and tuning either one silently moved the other.
    clear_round_rect(pm, inset, inset, box_w, box_h, radius);

    // DARK: no border at all. Scanning inward from the edge of the measured
    // tooltip over black gives 45,44,45,45 — flat body from the first pixel.
    // What separates it from the background is an outer shadow, which is the
    // shell's to draw, not a lighter outline. We drew one and it was wrong.
    //
    // LIGHT: a hairline IS present there — a near-white body needs it — so it
    // stays, as a 1px under-fill with the body inset into it.
    let (body, text_ink) = (colors.body.to_skia_color(), colors.text.to_skia_color());
    if let Some(hairline) = colors.hairline {
        fill_round_rect(
            pm,
            inset,
            inset,
            box_w,
            box_h,
            radius,
            hairline.to_skia_color(),
        );
        fill_round_rect(
            pm,
            inset + 1.0,
            inset + 1.0,
            box_w - 2.0,
            box_h - 2.0,
            (radius - 1.0).max(0.0),
            body,
        );
    } else {
        fill_round_rect(pm, inset, inset, box_w, box_h, radius, body);
    }
    text_ink
}

/// Ink width/height of `text` rendered at `size` px.
fn text_extent(text: &str, size: f32) -> (f32, f32) {
    let Some(font) = font() else {
        return (0.0, 0.0);
    };
    match digits_bounds(font, text, size) {
        Some((_, min_x, max_x, min_y, max_y)) => (max_x - min_x, max_y - min_y),
        None => (0.0, 0.0),
    }
}

/// Draw `text` at `size` px with its ink top-left at (x, y) — like
/// draw_digits_fit but with no auto-shrink and top-left (not centered) anchor.
fn draw_text_at(pm: &mut Pixmap, text: &str, x: f32, y: f32, size: f32, c: tiny_skia::Color) {
    let Some(font) = font() else {
        return;
    };
    let Some((tl, min_x, _, min_y, _)) = digits_bounds(font, text, size) else {
        return;
    };
    let (off_x, off_y) = (x - min_x, y - min_y);
    let (cr, cg, cb) = (
        (c.red() * 255.0) as u32,
        (c.green() * 255.0) as u32,
        (c.blue() * 255.0) as u32,
    );
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let data = pm.data_mut();
    for g in tl.glyphs() {
        let (metrics, bitmap) = font.rasterize_config(g.key);
        for (i, &cov) in bitmap.iter().enumerate() {
            if cov == 0 {
                continue;
            }
            let px = (off_x + g.x) as i32 + (i % metrics.width) as i32;
            let py = (off_y + g.y) as i32 + (i / metrics.width) as i32;
            if px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }
            let idx = ((py * w + px) * 4) as usize;
            // Coverage is blended LINEARLY. An earlier pass gamma-corrected it
            // (cov^(1/2.8)) on the theory that DirectWrite does, but that was
            // fitted against a different string than the shell was showing.
            // Measured on the SAME string, gamma 1.0 puts the mean lit luma at
            // 128.3 against the shell's 129.5, while 2.8 pushed it to 138 —
            // brighter and heavier than the shell, which is what it looked like.
            let a = cov as u32;
            let inv = 255 - a;
            data[idx] = ((cr * a) / 255 + data[idx] as u32 * inv / 255) as u8;
            data[idx + 1] = ((cg * a) / 255 + data[idx + 1] as u32 * inv / 255) as u8;
            data[idx + 2] = ((cb * a) / 255 + data[idx + 2] as u32 * inv / 255) as u8;
            data[idx + 3] = (a + data[idx + 3] as u32 * inv / 255).min(255) as u8;
        }
    }
}

pub(crate) fn color(r: u8, g: u8, b: u8, a: u8) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(r, g, b, a)
}

fn fill_circle(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, c: tiny_skia::Color) {
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, r);
    fill_path(pm, pb, c);
}

/// Erase a rounded rect to full transparency (BlendMode::Clear), so whatever
/// is painted there next composites against nothing rather than against what
/// was already drawn underneath.
fn clear_round_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    if let Some(path) = pb.finish() {
        let paint = Paint {
            blend_mode: tiny_skia::BlendMode::Clear,
            anti_alias: true,
            ..Default::default()
        };
        pm.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

pub(crate) fn fill_round_rect(
    pm: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    c: tiny_skia::Color,
) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    fill_path(pm, pb, c);
}

fn fill_path(pm: &mut Pixmap, pb: PathBuilder, c: tiny_skia::Color) {
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(c);
        paint.anti_alias = true;
        pm.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

#[cfg(test)]
thread_local! {
    /// The last pixmap made into an icon, kept for the visual baseline.
    static LAST_ICON: std::cell::RefCell<Option<Pixmap>> = const { std::cell::RefCell::new(None) };
}

fn to_icon(pm: &Pixmap) -> Result<Icon> {
    #[cfg(test)]
    LAST_ICON.with(|c| *c.borrow_mut() = Some(pm.clone()));
    Icon::from_rgba(demultiply(pm), ICON_SIZE, ICON_SIZE).map_err(|e| anyhow::anyhow!("icon: {e}"))
}

/// tiny-skia stores premultiplied RGBA; tray_icon wants straight RGBA.
fn demultiply(pm: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for px in pm.pixels() {
        let c = px.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Metric, MetricUnit, MetricWindow, ProviderData, ProviderId};

    /// The tray, pinned: the neutral dot, the rings and the stacked icon for
    /// every fixture set, and the hover tooltip at three bar heights, on a
    /// dark and a light taskbar. See ui/visual_baseline.rs.
    #[test]
    fn visual_baseline_tray() {
        use crate::ui::visual_baseline as vb;
        let theme = ComputedTheme::compute(&crate::config::schema::UIConfig::default());
        let take = || LAST_ICON.with(|c| c.borrow_mut().take().expect("an icon was drawn"));
        let mut items = Vec::new();
        for light in [false, true] {
            let ink = if light { "light" } else { "dark" };
            neutral_icon(light).unwrap();
            items.push((format!("neutral-{ink}"), take()));
            for (set, providers) in vb::sets() {
                draw_ring_icon(&providers, light, Rings::of(&TuneTray::default())).unwrap();
                items.push((format!("rings-{set}-{ink}"), take()));
                for n in 1..=providers.len().min(4) {
                    let first: Vec<ProviderData> = providers.iter().take(n).cloned().collect();
                    draw_stacked_icon(&first, &theme, light).unwrap();
                    items.push((format!("stacked-{set}-{n}-{ink}"), take()));
                }
                for bar in [40.0, 48.0, 72.0] {
                    let pm = render_tooltip(&tooltip(&providers, false), bar, light);
                    items.push((format!("tooltip-{set}-{bar}-{ink}"), pm));
                }
            }
        }
        vb::check("tray", items);
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
            updated_at: utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    /// Mid-line radius of each ring, as draw_ring computes it.
    fn outer_mid() -> f32 {
        RING_OUTER - RING_W / 2.0
    }
    fn inner_mid() -> f32 {
        RING_OUTER - RING_STEP - RING_W / 2.0
    }

    /// The total angle the ink actually covers, caps included — which is what
    /// the eye sees, and what `ring_fill` has to make come out right.
    fn painted_span(pct: f32, r_mid: f32) -> f32 {
        match ring_fill(pct, r_mid, RING_W) {
            RingFill::Empty => 0.0,
            RingFill::Dot => 2.0 * cap_angle(RING_W, r_mid),
            // A round cap adds one cap-angle beyond each endpoint.
            RingFill::Arc { start, end } => (end - start) + 2.0 * cap_angle(RING_W, r_mid),
            RingFill::Full => std::f32::consts::TAU,
        }
    }

    /// The bug this design exists to fix: round caps used to add their overhang
    /// on top of the swept angle, so on the inner ring — where the same 2px
    /// overhang is a much larger slice of a much smaller circle — 90% already
    /// painted a closed circle. The visible ink must match the percentage.
    #[test]
    fn round_caps_do_not_inflate_the_visible_arc() {
        for (name, r_mid) in [("outer", outer_mid()), ("inner", inner_mid())] {
            for pct in [20.0_f32, 25.0, 50.0, 75.0, 90.0, 99.0] {
                let want = (pct / 100.0) * std::f32::consts::TAU;
                let got = painted_span(pct, r_mid);
                assert!(
                    (got - want).abs() < 1e-4,
                    "{name} ring at {pct}%: painted {got} rad, expected {want}"
                );
            }
        }
    }

    /// The one place the icon knowingly overstates a value: below two cap
    /// widths there is no room for an arc, so a dot stands in — and a dot is
    /// two cap widths wide whatever the value. It cannot be drawn smaller
    /// without vanishing, and vanishing would read as zero, which is a worse
    /// lie than "a little". This test pins how far that overstatement can go:
    /// only under the arc threshold, and never wider than the dot itself.
    #[test]
    fn only_values_too_small_to_draw_are_overstated() {
        for (name, r_mid) in [("outer", outer_mid()), ("inner", inner_mid())] {
            let cap = cap_angle(RING_W, r_mid);
            let threshold = (2.0 * cap) / std::f32::consts::TAU * 100.0;
            assert!(
                threshold < 20.0,
                "{name} ring: a dot would stand in for everything under {threshold}%, \
                 which is too much of the scale"
            );
            // Just above the threshold it is an arc, and therefore exact.
            let just_over = threshold + 1.0;
            let want = (just_over / 100.0) * std::f32::consts::TAU;
            assert!(
                (painted_span(just_over, r_mid) - want).abs() < 1e-4,
                "{name} ring at {just_over}% should already be an exact arc"
            );
            // Below it, the dot never grows beyond its own minimum size.
            for pct in [0.5_f32, 2.0, 5.0] {
                if pct < threshold {
                    assert_eq!(ring_fill(pct, r_mid, RING_W), RingFill::Dot);
                    assert!(painted_span(pct, r_mid) <= 2.0 * cap + 1e-6);
                }
            }
        }
    }

    /// Ninety is not a hundred. The gap left at 90% must stay wide enough to
    /// see after the shell scales the 32px icon down to 16 — the inner ring is
    /// the hard case, since its circumference is roughly half the outer's.
    #[test]
    fn ninety_percent_still_reads_as_an_open_ring() {
        for (name, r_mid) in [("outer", outer_mid()), ("inner", inner_mid())] {
            let gap = std::f32::consts::TAU - painted_span(90.0, r_mid);
            // Arc length of the gap at tray size: the icon is authored at 32
            // and displayed at 16, so lengths halve.
            let px_at_16 = gap * r_mid / 2.0;
            assert!(
                px_at_16 >= 1.5,
                "{name} ring at 90%: only {px_at_16}px of gap survives the downscale"
            );
        }
    }

    /// Only a real 100% closes the ring; anything below keeps a break in it.
    #[test]
    fn only_a_hundred_closes_the_ring() {
        assert_eq!(ring_fill(100.0, outer_mid(), RING_W), RingFill::Full);
        assert_eq!(ring_fill(140.0, outer_mid(), RING_W), RingFill::Full);
        assert!(
            !matches!(ring_fill(99.0, outer_mid(), RING_W), RingFill::Full),
            "99% must leave the ring visibly open"
        );
    }

    /// A small non-zero value must show SOMETHING. Below two cap-widths there
    /// is no room for an arc, so it degrades to a dot rather than vanishing —
    /// an empty ring has to mean zero and nothing else.
    #[test]
    fn a_sliver_of_usage_never_renders_as_empty() {
        assert_eq!(ring_fill(0.0, inner_mid(), RING_W), RingFill::Empty);
        for pct in [1.0_f32, 3.0, 8.0] {
            assert_ne!(
                ring_fill(pct, inner_mid(), RING_W),
                RingFill::Empty,
                "{pct}% must paint something on the inner ring"
            );
        }
    }

    /// The stroke width and the step between rings are coupled, and nothing in
    /// the type system says so: raise RING_W alone and the two rings merge into
    /// a single band. In a monochrome icon that band is indistinguishable from
    /// one thick ring, so the icon would quietly stop showing two providers.
    #[test]
    fn the_rings_stay_separated_and_inside_the_canvas() {
        // black_box keeps these out of const-evaluation, so the assertions
        // report the offending number instead of failing to compile.
        let (w, step, outer, centre) = (
            std::hint::black_box(RING_W),
            std::hint::black_box(RING_STEP),
            std::hint::black_box(RING_OUTER),
            std::hint::black_box(CENTER),
        );
        let bare = (step - w) / 2.0;
        assert!(
            bare >= 0.75,
            "only {bare}px of bare space between the rings survives the downscale"
        );
        assert!(
            outer <= centre - 1.0,
            "the outer stroke would clip the edge of the icon square"
        );
        assert!(
            outer - step - w > 0.0,
            "the inner ring must not swallow its own centre"
        );
    }

    /// `[tune.tray]` left out draws today's rings (the pinned `tray` group
    /// holds them to the pixel).
    #[test]
    fn the_default_tray_knobs_are_todays_rings() {
        assert_eq!(Rings::of(&TuneTray::default()), Rings::DEFAULT);
        assert_eq!(
            Rings::DEFAULT,
            Rings {
                w: RING_W,
                step: RING_STEP
            }
        );
    }

    /// Whatever `[tune.tray]` asks, in range or not, the rings keep their
    /// width range, at least a canvas pixel of bare space between them, and
    /// the hole in the middle; a gap that would close the hole is cut down.
    #[test]
    fn tuned_rings_fit_the_icon() {
        for w in -10..=120 {
            for g in -10..=120 {
                let tune = TuneTray {
                    ring_width: Some(w as f64 / 10.0),
                    ring_gap: Some(g as f64 / 10.0),
                };
                let r = Rings::of(&tune);
                assert!((2.0..=6.0).contains(&r.w), "{tune:?}: width {}", r.w);
                let gap = r.step - r.w;
                assert!(gap >= 1.0 - 1e-5, "{tune:?}: gap {gap}");
                let hole = RING_OUTER - r.step - r.w;
                assert!(hole >= RING_HOLE_MIN - 1e-5, "{tune:?}: hole {hole}");
            }
        }
        let widest = TuneTray {
            ring_width: Some(6.0),
            ring_gap: Some(6.0),
        };
        assert_eq!(Rings::of(&widest), Rings { w: 6.0, step: 8.0 });
        let thin = TuneTray {
            ring_width: Some(3.0),
            ring_gap: Some(4.0),
        };
        assert_eq!(Rings::of(&thin), Rings { w: 3.0, step: 7.0 });
    }

    /// The rings with `[tune.tray]` knobs other than the defaults, on a dark
    /// and a light taskbar (painted over the bar's grey so the PNGs can be
    /// looked at), at the 32px the icon is drawn at and the 16px the shell
    /// shows at 100% scaling. Not pinned: the default rings are in the
    /// pinned `tray` group.
    #[test]
    fn visual_baseline_tray_tune() {
        use crate::ui::visual_baseline as vb;
        let take = || LAST_ICON.with(|c| c.borrow_mut().take().expect("an icon was drawn"));
        let tune = |w: f64, g: f64| TuneTray {
            ring_width: Some(w),
            ring_gap: Some(g),
        };
        let variants = [
            ("thin", tune(3.0, 2.0)),
            ("thick", tune(6.0, 2.0)),
            ("tight", tune(5.0, 1.0)),
            ("apart", tune(4.0, 4.0)),
            ("hairline", tune(2.0, 6.0)),
        ];
        let mut items = Vec::new();
        for (light, bar) in [(false, 32u8), (true, 228u8)] {
            let ink = if light { "light" } else { "dark" };
            for (set, providers) in [("typical", vb::typical()), ("levels", vb::levels())] {
                for (name, t) in &variants {
                    draw_ring_icon(&providers, light, Rings::of(t)).unwrap();
                    let icon = take();
                    for size in [32u32, 16] {
                        let mut pm = Pixmap::new(size, size).unwrap();
                        pm.fill(color(bar, bar, bar, 255));
                        let s = size as f32 / ICON_SIZE as f32;
                        let paint = tiny_skia::PixmapPaint {
                            quality: tiny_skia::FilterQuality::Bicubic,
                            ..Default::default()
                        };
                        let scale = Transform::from_scale(s, s);
                        pm.draw_pixmap(0, 0, icon.as_ref(), &paint, scale, None);
                        items.push((format!("{name}-{set}-{size}-{ink}"), pm));
                    }
                }
            }
        }
        vb::check("tray-tune", items);
    }

    /// Every ring starts at 12 o'clock and grows clockwise.
    #[test]
    fn arcs_start_at_twelve_o_clock() {
        let r = outer_mid();
        let RingFill::Arc { start, end } = ring_fill(50.0, r, RING_W) else {
            panic!("50% should be an arc");
        };
        let cap = cap_angle(RING_W, r);
        assert!((start - cap - RING_TOP).abs() < 1e-4, "arc must open at 12");
        assert!(end > start, "and sweep clockwise from there");
    }

    #[test]
    fn the_two_busiest_providers_are_returned_highest_first() {
        let providers = vec![
            data(ProviderId::Claude, 40),
            data(ProviderId::Codex, 93),
            data(ProviderId::Copilot, 71),
        ];

        let (first, second) = two_busiest(&providers);

        assert_eq!(first, Some(93), "the busiest goes to the outer ring");
        assert_eq!(second, Some(71));
    }

    #[test]
    fn a_provider_without_a_percentage_is_never_a_candidate() {
        let mut blank = data(ProviderId::Copilot, 0);
        blank.metrics.clear();
        let providers = vec![data(ProviderId::Claude, 55), blank];

        assert_eq!(two_busiest(&providers), (Some(55), None));
    }

    #[test]
    fn equal_percentages_keep_the_widget_order_so_the_rings_do_not_swap() {
        // Both at 80: without a stable tie-break the rings would trade places
        // between refreshes and the icon would flicker for no reason.
        let providers = vec![data(ProviderId::Claude, 80), data(ProviderId::Codex, 80)];

        assert_eq!(two_busiest(&providers), (Some(80), Some(80)));
        assert_eq!(
            two_busiest(&providers),
            two_busiest(&providers),
            "the same input must give the same rings"
        );
    }

    #[test]
    fn the_repaint_cache_tracks_both_rings() {
        let before = vec![data(ProviderId::Claude, 90), data(ProviderId::Codex, 30)];
        let after = vec![data(ProviderId::Claude, 90), data(ProviderId::Codex, 55)];

        assert_ne!(
            ring_cache_state(&before),
            ring_cache_state(&after),
            "a change in the second ring must invalidate the cache"
        );
    }

    /// Every measurement behind the tooltip and ring constants was taken at
    /// 100% scaling on a 48px bar. These pin what the scaling does elsewhere,
    /// because a 150% or 200% display is the case nobody re-measures and the
    /// one where a rounding slip turns into a visibly wrong box.
    #[test]
    fn tooltip_geometry_holds_at_every_scale() {
        for (bar_h, label) in [
            (41.0, "85% floor"),
            (48.0, "100%"),
            (72.0, "150%"),
            (96.0, "200%"),
        ] {
            let pm = render_tooltip("Claude 68%  ·  Codex 100%", bar_h, false);
            let inset = tip_shadow_inset(bar_h);
            let scale = (bar_h / TIP_BASE_BAR_H).clamp(0.85, 3.0);

            // The shadow must fit in the margin reserved for it, or the
            // outermost ring is clipped and the tooltip gains a hard edge.
            let outermost = 5.0 * scale;
            assert!(
                inset as f32 >= outermost,
                "{label}: shadow reaches {outermost}px but only {inset}px is reserved"
            );
            // The box must still be inside the pixmap on both axes.
            let box_h = (TIP_LINE_H * scale).round()
                + (TIP_PAD_TOP * scale).round()
                + (TIP_PAD_BOTTOM * scale).round();
            assert!(
                pm.height() as f32 >= box_h + inset as f32 * 2.0,
                "{label}: pixmap {} too short for a {box_h}px box plus {inset}px margins",
                pm.height()
            );
            // And the whole thing must grow with the bar, not jump around.
            assert!(pm.height() > 0 && pm.width() > 0, "{label}: empty pixmap");
        }
    }

    /// Render the tooltip with a GIVEN string, composited over mid grey, so it
    /// can be compared pixel-for-pixel with a capture of the shell's own
    /// tooltip showing the same text. Comparing different strings is not a
    /// comparison: letter mix changes the mean ink and descenders change the
    /// measured text height, which is exactly how an earlier pass concluded our
    /// text was dimmer when the stems were in fact identical.
    /// Run: `cargo test preview_tooltip -- --ignored`.
    #[test]
    #[ignore = "writes design previews to %TEMP%"]
    fn preview_tooltip() {
        let text = std::env::var("TIP_TEXT")
            .unwrap_or_else(|_| "Realtek Digital Output (Realtek USB Audio): 12%".to_string());
        let pm = render_tooltip(&text, 48.0, false);
        // Composite over grey 128, the backdrop the shell was measured against.
        let mut out = Pixmap::new(pm.width(), pm.height()).unwrap();
        out.fill(color(128, 128, 128, 255));
        out.draw_pixmap(
            0,
            0,
            pm.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        let path = std::env::temp_dir().join("ailimits_tooltip_on_grey.png");
        std::fs::write(&path, out.encode_png().unwrap()).unwrap();
        println!("{}", path.display());
    }

    /// Composite a tooltip over one solid backdrop and hand back the pixels.
    fn tooltip_over(text: &str, light: bool, back: (u8, u8, u8)) -> Pixmap {
        let pm = render_tooltip(text, 48.0, light);
        let mut out = Pixmap::new(pm.width(), pm.height()).unwrap();
        out.fill(color(back.0, back.1, back.2, 255));
        out.draw_pixmap(
            0,
            0,
            pm.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        out
    }

    /// WCAG relative luminance of an 8-bit sRGB triple.
    fn luminance(p: &[u8]) -> f64 {
        let ch = |v: u8| {
            let c = v as f64 / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(p[0]) + 0.7152 * ch(p[1]) + 0.0722 * ch(p[2])
    }

    fn contrast(a: f64, b: f64) -> f64 {
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The pixel of the box body (above the text, clear of the corners) and the
    /// most extreme ink pixel inside the box, as luminances.
    fn body_and_ink(out: &Pixmap, light: bool) -> (f64, f64) {
        let inset = tip_shadow_inset(48.0) as u32;
        let px = |x: u32, y: u32| {
            let i = ((y * out.width() + x) * 4) as usize;
            luminance(&out.data()[i..i + 4])
        };
        let body = px(out.width() / 2, inset + 3);
        let mut ink = body;
        for y in (inset + 6)..(out.height() - inset - 4) {
            for x in (inset + 4)..(out.width() - inset - 4) {
                let l = px(x, y);
                if (light && l < ink) || (!light && l > ink) {
                    ink = l;
                }
            }
        }
        (body, ink)
    }

    /// The light theme's tooltip has to be as readable as the dark one. It is
    /// the theme nobody here runs, so it is checked by measurement rather than
    /// by looking: the body is near-white at alpha 219 over a light bar, the
    /// ink is grey 26, and both themes must clear AAA (7:1) by a wide margin.
    /// The numbers print with `cargo test tooltip_text_contrast -- --nocapture`.
    #[test]
    fn tooltip_text_contrast_clears_aaa_in_both_themes() {
        let text = "Claude 68%  ·  Codex 100%";
        for (light, back, label) in [
            (true, (243, 243, 243), "light, over a light bar"),
            (true, (255, 255, 255), "light, over a white window"),
            (false, (32, 32, 32), "dark, over a dark bar"),
            (false, (0, 0, 0), "dark, over black"),
        ] {
            let out = tooltip_over(text, light, back);
            let (body, ink) = body_and_ink(&out, light);
            let ratio = contrast(body, ink);
            println!("{label}: text-on-body contrast {ratio:.1}:1");
            assert!(
                ratio >= 7.0,
                "{label}: {ratio:.1}:1 is below AAA — the tooltip is hard to read"
            );
        }
    }

    // ─── Tooltip rows ─────────────────────────────────────────────

    use crate::config::schema::{Config, Surface};
    use crate::models::Models;

    fn tip_rows(config: &Config, providers: &[ProviderData]) -> Vec<RowView> {
        crate::rows::visible_rows(config, providers, Surface::Tooltip, &Models::new())
    }

    fn opus(effort: Option<&str>) -> ModelLabel {
        ModelLabel {
            model: Some("Opus".into()),
            with_version: Some("Opus 5.5".into()),
            effort: effort.map(String::from),
        }
    }

    /// With the default config the panel's tooltip is today's one line, the
    /// tray's own text, and draws exactly as before.
    #[test]
    fn the_default_tooltip_is_todays_single_line() {
        let providers = crate::ui::visual_baseline::typical();
        let rows = tip_rows(&Config::default(), &providers);
        let lines = tooltip_lines(&rows, false, &UiNumbers::PERCENT_ONLY);
        assert_eq!(
            lines,
            [TipLine {
                text: tooltip(&providers, false),
                indent: false
            }]
        );
        let a = render_tooltip_lines(&lines, 48.0, true);
        let b = render_tooltip(&tooltip(&providers, false), 48.0, true);
        assert_eq!(a.data(), b.data());
        // Rows unticked from the tooltip still make one plain line.
        let mut config = Config::default();
        config.ui.rows.tooltip = Some(vec!["codex".into(), "grok".into()]);
        let lines = tooltip_lines(
            &tip_rows(&config, &providers),
            false,
            &UiNumbers::PERCENT_ONLY,
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Codex 48%  ·  Grok 12%");
    }

    #[test]
    fn extra_rows_or_a_label_give_one_row_per_line() {
        let providers = crate::ui::visual_baseline::typical();
        let mut config = Config::default();
        config.ui.rows.tooltip = Some(vec![
            "claude".into(),
            "codex".into(),
            "antigravity.cg".into(),
        ]);
        let mut rows = tip_rows(&config, &providers);
        rows[0].model = Some(opus(Some("xhigh")));
        let texts = |tip: &UiNumbers| -> Vec<String> {
            let lines = tooltip_lines(&rows, false, tip);
            assert!(lines.iter().all(|l| !l.indent));
            lines.into_iter().map(|l| l.text).collect()
        };
        // The percentage alone: no time.
        assert_eq!(
            texts(&UiNumbers::PERCENT_ONLY),
            [
                "Claude 25% · Opus 5.5 · xhigh",
                "Codex 48%",
                "Antigravity · Claude and GPT 64%",
            ]
        );
        // Both: the time left until the reset of the number shown.
        let both = UiNumbers::default();
        assert_eq!(
            texts(&both),
            [
                "Claude 25% · 2h 14min · Opus 5.5 · xhigh",
                "Codex 48% · 4h 51min",
                "Antigravity · Claude and GPT 64% · 5d 0h",
            ]
        );
        // The time alone.
        let reset = UiNumbers {
            percent: false,
            reset: true,
            bar: true,
        };
        assert_eq!(
            texts(&reset),
            [
                "Claude 2h 14min · Opus 5.5 · xhigh",
                "Codex 4h 51min",
                "Antigravity · Claude and GPT 5d 0h",
            ]
        );
    }

    /// The time asked for gives one row per line even with providers' own
    /// rows alone and no label; with the percentage left out, a row whose
    /// reset is not known keeps the no-data dot.
    #[test]
    fn the_reset_time_alone_or_with_the_percentage() {
        let providers = crate::ui::visual_baseline::typical();
        let mut config = Config::default();
        config.ui.rows.tooltip = Some(vec!["claude".into(), "codex".into()]);
        let mut rows = tip_rows(&config, &providers);
        let reset = UiNumbers {
            percent: false,
            reset: true,
            bar: true,
        };
        let texts: Vec<String> = tooltip_lines(&rows, false, &reset)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(texts, ["Claude 2h 14min", "Codex 4h 51min"]);
        for m in &mut rows[1].data.metrics {
            m.reset_at = None;
        }
        let texts: Vec<String> = tooltip_lines(&rows, false, &reset)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(texts, ["Claude 2h 14min", "Codex •"]);
        let both = UiNumbers {
            percent: true,
            reset: true,
            bar: true,
        };
        let texts: Vec<String> = tooltip_lines(&rows, false, &both)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(texts, ["Claude 25% · 2h 14min", "Codex 48%"]);
    }

    /// By default each provider has a line of its own, with how much is
    /// used and when that resets; nothing chosen yet still says what to do.
    #[test]
    fn by_default_each_row_says_its_percent_and_reset_time() {
        let providers = crate::ui::visual_baseline::typical();
        let mut config = Config::default();
        config.ui.rows.tooltip = Some(vec!["claude".into(), "codex".into()]);
        let rows = tip_rows(&config, &providers);
        let texts: Vec<String> = tooltip_lines(&rows, false, &config.ui.tooltip)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(texts, ["Claude 25% · 2h 14min", "Codex 48% · 4h 51min"]);
        assert_eq!(
            tooltip_lines(&[], true, &config.ui.tooltip)[0].text,
            "AI Limits: choose what to show"
        );
    }

    /// The panel's countdown reads like the clock beside it, and changes
    /// on the next hour while it counts days, on the next minute below.
    #[test]
    fn the_compact_countdown_and_when_it_changes() {
        assert_eq!(countdown_compact(3 * 86_400 + 4 * 3600 + 59), "3d4h");
        assert_eq!(countdown_compact(2 * 86_400 + 30), "2d");
        assert_eq!(countdown_compact(2 * 3600 + 14 * 60 + 5), "2h14");
        assert_eq!(countdown_compact(3600 + 5 * 60), "1h05");
        assert_eq!(countdown_compact(12 * 60 + 59), "12m");
        assert_eq!(countdown_compact(59), "<1m");
        assert_eq!(countdown_changes_in(2 * 3600 + 14 * 60 + 5), 6);
        assert_eq!(countdown_changes_in(2 * 3600 + 14 * 60), 1);
        assert_eq!(countdown_changes_in(3 * 86_400 + 4 * 3600 + 59), 60);
        assert_eq!(countdown_changes_in(59), 60);
        assert_eq!(countdown_changes_in(0), 1);
    }

    /// A row's time is its headline window's own reset: a session that has
    /// not started says nothing rather than borrow the weekly's.
    #[test]
    fn a_row_tells_its_own_reset_never_another_windows() {
        let providers = crate::ui::visual_baseline::typical();
        let mut claude = providers[0].clone();
        assert!(own_reset(&claude).is_some());
        claude.metrics[0].reset_at = None;
        assert!(claude.next_reset().is_some(), "the weekly still has one");
        assert_eq!(own_reset(&claude), None);
    }

    /// A row without a number keeps its dot, and its label follows the dot
    /// with a space: never "• ·".
    #[test]
    fn a_label_after_the_no_data_dot_takes_no_second_dot() {
        let providers = crate::ui::visual_baseline::typical();
        let mut config = Config::default();
        config.ui.rows.tooltip = Some(vec!["claude".into(), "codex".into()]);
        let mut rows = tip_rows(&config, &providers);
        rows[0].data.status = ProviderStatus::NetworkError("token expired".into());
        rows[0].data.metrics.clear();
        rows[0].model = Some(opus(Some("xhigh")));
        let texts: Vec<String> = tooltip_lines(&rows, false, &UiNumbers::PERCENT_ONLY)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(texts[0], "Claude • Opus 5.5 · xhigh");
        assert!(texts.iter().all(|t| !t.contains("• ·")));
    }

    /// The tray icon's own text lists providers only, whatever the rows.
    #[test]
    fn the_tray_text_never_carries_models_or_extra_rows() {
        let providers = crate::ui::visual_baseline::typical();
        assert_eq!(
            tooltip(&providers, false),
            "Claude 25%  ·  Codex 48%  ·  Grok 12%  ·  Antigravity 30%  ·  Copilot 41%"
        );
        assert_eq!(tooltip(&[], false), "AI Limits");
        assert_eq!(tooltip(&[], true), "AI Limits: choose what to show");
        // Before anything is chosen the panel's tooltip says the same.
        assert_eq!(
            tooltip_lines(&[], true, &UiNumbers::default())[0].text,
            "AI Limits: choose what to show"
        );
    }

    #[test]
    fn a_long_line_wraps_at_a_dot_and_never_cuts_a_word() {
        let long = "Antigravity · Claude and GPT 64% (5d 0h) · Claude Sonnet 4.6 Thinking \
                    Extended Preview · xhigh";
        let lines = wrap_tip_line(long);
        assert!(lines.len() > 1, "{lines:?}");
        assert!(!lines[0].indent && lines[1..].iter().all(|l| l.indent));
        let joined: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(joined.join(" · "), long, "nothing lost, nothing cut");
        for l in &lines[..lines.len() - 1] {
            let w =
                text_extent(&l.text, TIP_FONT_PX).0 + if l.indent { TIP_WRAP_INDENT } else { 0.0 };
            assert!(w <= TIP_WRAP_PX, "{:?} is {w} px", l.text);
        }
        // One part wider than the limit on its own stays whole.
        let one = "W".repeat(80);
        assert_eq!(wrap_tip_line(&one).len(), 1);
    }

    /// `typical()` with the metrics extra rows show: Claude's Opus and Sonnet
    /// weeks, Grok Build, Cursor and its Grok Bot share.
    fn rows_providers() -> Vec<ProviderData> {
        let mut providers = crate::ui::visual_baseline::typical();
        let like = |m: &Metric, label: &str, used: u64| Metric {
            label: label.into(),
            used,
            ..m.clone()
        };
        for p in providers.iter_mut() {
            let week = p.metrics.last().cloned().expect("a metric");
            match p.id {
                ProviderId::Claude => {
                    p.metrics.push(like(&week, "Opus", 61));
                    p.metrics.push(like(&week, "Sonnet", 12));
                }
                ProviderId::Grok => p.metrics.push(like(&week, "Build", 7)),
                _ => {}
            }
        }
        let grok_week = providers[2].metrics[0].clone();
        providers.push(ProviderData {
            id: ProviderId::Cursor,
            metrics: vec![
                like(&grok_week, "Premium", 12),
                like(&grok_week, "Grok Bot", 3),
            ],
            ..providers[2].clone()
        });
        providers
    }

    /// The whole label each tool would give (renderer tests use the same).
    fn full_label(id: &ProviderId) -> Option<ModelLabel> {
        let (m, v, e) = match id {
            ProviderId::Claude => ("Opus", Some("Opus 5.5"), Some("xhigh")),
            ProviderId::Codex => ("GPT", Some("GPT-6.1 Sol"), Some("high")),
            ProviderId::Grok => ("Grok", Some("Grok 4.7"), Some("high")),
            ProviderId::Antigravity => ("Claude Sonnet", Some("Claude Sonnet 4.6 Thinking"), None),
            ProviderId::Copilot => ("Auto", None, Some("medium")),
            ProviderId::Cursor => return None,
        };
        Some(ModelLabel {
            model: Some(m.into()),
            with_version: v.map(String::from),
            effort: e.map(String::from),
        })
    }

    fn labelled_rows(ids: &[&str], labels: bool) -> Vec<RowView> {
        let mut config = Config::default();
        config.ui.rows.tooltip = Some(ids.iter().map(|s| s.to_string()).collect());
        let mut rows = tip_rows(&config, &rows_providers());
        assert_eq!(rows.len(), ids.len(), "every row has its metric");
        for row in rows.iter_mut().filter(|r| !r.is_extra() && labels) {
            row.model = full_label(&row.provider);
        }
        rows
    }

    /// The tray icon's tooltip says what the panel's does: the rows chosen
    /// in the Tooltip column, line for line, whatever the icon itself shows
    /// (its two providers). Within the 127 units Windows keeps, whole lines
    /// and a last "…" when some did not fit.
    #[test]
    fn the_tray_tooltip_lists_the_tooltip_rows_like_the_panel() {
        let both = UiNumbers {
            percent: true,
            reset: true,
            bar: true,
        };
        let four = labelled_rows(&["claude", "codex", "grok", "copilot"], false);
        let text = tray_tip_text(&four, false, &both);
        let lines: Vec<String> = tooltip_lines(&four, false, &both)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(text.split('\n').collect::<Vec<_>>(), lines);
        assert_eq!(text.lines().count(), 4, "{text}");
        // Percent only and no labels: the panel's one line, the same here.
        let one = tray_tip_text(&four, false, &UiNumbers::PERCENT_ONLY);
        assert_eq!(one.lines().count(), 1);
        assert_eq!(
            one,
            tooltip_lines(&four, false, &UiNumbers::PERCENT_ONLY)[0].text
        );
        // Every row with its model: cut to whole lines, then "…".
        let every = labelled_rows(
            &[
                "claude",
                "claude.opus",
                "claude.sonnet",
                "codex",
                "grok",
                "copilot",
                "cursor",
            ],
            true,
        );
        let long = tray_tip_text(&every, false, &both);
        assert!(long.encode_utf16().count() <= TRAY_TIP_UNITS, "{long}");
        assert!(long.ends_with("\n…"), "{long}");
        let full = tooltip_lines(&every, false, &both);
        for (got, want) in long.lines().zip(full.iter()) {
            if got != "…" {
                assert!(got.trim_start() == want.text, "{got} vs {}", want.text);
            }
        }
        // Nothing chosen yet: what to do, as the panel says it.
        assert_eq!(tray_tip_text(&[], true, &both), FIRST_INSTALL_TIP);
    }

    /// The panel's multi-line tooltip, recorded as a new group (the 0.7.0
    /// tooltips in `tray` stay as they were): the workshop's scenarios (two
    /// providers and one by model; every row with its label) in both themes
    /// and at three bar sizes, a label that wraps, rows without labels, and
    /// the first install.
    #[test]
    fn visual_baseline_tooltip_rows() {
        use crate::ui::visual_baseline as vb;
        let mix = labelled_rows(
            &["claude", "claude.opus", "claude.sonnet", "codex", "copilot"],
            true,
        );
        let every = labelled_rows(
            &[
                "claude",
                "claude.opus",
                "claude.sonnet",
                "codex",
                "grok",
                "grok.build",
                "antigravity",
                "antigravity.cg",
                "copilot",
                "cursor",
                "cursor.bot",
            ],
            true,
        );
        let unlabelled = labelled_rows(&["claude", "claude.opus", "codex"], false);
        let mut long = labelled_rows(&["claude", "antigravity"], true);
        long[1].model = Some(ModelLabel {
            model: Some("Claude Sonnet".into()),
            with_version: Some("Claude Sonnet 4.6 Thinking Extended Preview Edition".into()),
            effort: Some("max".into()),
        });
        let mut items = Vec::new();
        for light in [false, true] {
            let ink = if light { "light" } else { "dark" };
            let mut add_with =
                |name: &str, rows: &[RowView], first: bool, bar: f32, tip: UiNumbers| {
                    let lines = tooltip_lines(rows, first, &tip);
                    let pm = render_tooltip_lines(&lines, bar, light);
                    items.push((format!("{name}-{bar}-{ink}"), pm));
                };
            let both = UiNumbers {
                percent: true,
                reset: true,
                bar: true,
            };
            let reset_only = UiNumbers {
                percent: false,
                reset: true,
                bar: true,
            };
            add_with("mix-reset", &mix, false, 48.0, both);
            add_with(
                "unlabelled-reset-only",
                &unlabelled,
                false,
                48.0,
                reset_only,
            );
            let mut add = |name: &str, rows: &[RowView], first: bool, bar: f32| {
                add_with(name, rows, first, bar, UiNumbers::PERCENT_ONLY)
            };
            add("mix", &mix, false, 48.0);
            for bar in [40.0, 48.0, 72.0] {
                add("every", &every, false, bar);
            }
            add("wrap", &long, false, 48.0);
            add("unlabelled", &unlabelled, false, 48.0);
            add("first-install", &[], true, 48.0);
        }
        vb::check("tooltip-rows", items);
    }

    /// The tooltip is sized from the BAR's thickness, and from nothing else.
    /// The panel's own height is not that thickness on a side bar - the
    /// stacked layout stands 64px tall where the bar is 48 - and passing it
    /// made the tooltip a third too big there while the tray icon's stayed
    /// right. This pins what the difference was, so the wrong number cannot
    /// come back unnoticed.
    #[test]
    fn the_tooltip_is_sized_by_the_bar_not_by_the_panel() {
        let text = "Claude 68%  ·  Codex 100%";
        let bar = render_tooltip(text, 48.0, false);
        let panel_height_on_a_side_bar = render_tooltip(text, 64.0, false);
        assert!(
            panel_height_on_a_side_bar.height() as f32 / bar.height() as f32 > 1.25,
            "the 64px reading should be visibly bigger: {} vs {}",
            panel_height_on_a_side_bar.height(),
            bar.height()
        );
        // And every bar of the same thickness gives the same tooltip,
        // whichever edge it is on.
        assert_eq!(
            (bar.width(), bar.height()),
            {
                let again = render_tooltip(text, 48.0, false);
                (again.width(), again.height())
            },
            "the same bar must give the same tooltip"
        );
        println!(
            "48px bar: {}x{};  panel height on a side bar (64): {}x{}",
            bar.width(),
            bar.height(),
            panel_height_on_a_side_bar.width(),
            panel_height_on_a_side_bar.height()
        );
    }

    /// Both themes side by side over the backdrops each actually sits on, for
    /// the eye to confirm what the contrast test measures.
    /// Run: `cargo test preview_tooltip_themes -- --ignored`.
    #[test]
    #[ignore = "writes design previews to %TEMP%"]
    fn preview_tooltip_themes() {
        let text = "Claude 68%  ·  Codex 100%";
        for (light, back, name) in [
            (true, (243, 243, 243), "light_on_bar"),
            (true, (255, 255, 255), "light_on_white"),
            (false, (32, 32, 32), "dark_on_bar"),
            (false, (0, 0, 0), "dark_on_black"),
        ] {
            let out = tooltip_over(text, light, back);
            let path = std::env::temp_dir().join(format!("ailimits_tooltip_{name}.png"));
            std::fs::write(&path, out.encode_png().unwrap()).unwrap();
            println!("{}", path.display());
        }
    }

    /// Design preview for the ring icon: the tray square is far too small to
    /// judge live, so write the states out to %TEMP% at 32px. The top of the
    /// scale (90/95/99/100) is the pair worth staring at.
    /// Run: `cargo test preview_rings -- --ignored`.
    #[test]
    #[ignore = "writes design previews to %TEMP%"]
    fn preview_rings() {
        let dir = std::env::temp_dir();
        for (name, providers) in [
            ("rings_none.png", vec![]),
            ("rings_one.png", vec![data(ProviderId::Claude, 93)]),
            (
                "rings_two.png",
                vec![data(ProviderId::Claude, 93), data(ProviderId::Codex, 18)],
            ),
            (
                "rings_sliver.png",
                vec![data(ProviderId::Claude, 40), data(ProviderId::Codex, 2)],
            ),
            (
                "rings_90.png",
                vec![data(ProviderId::Claude, 90), data(ProviderId::Codex, 90)],
            ),
            (
                "rings_99.png",
                vec![data(ProviderId::Claude, 99), data(ProviderId::Codex, 99)],
            ),
            (
                "rings_full.png",
                vec![data(ProviderId::Claude, 100), data(ProviderId::Codex, 100)],
            ),
        ] {
            let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).unwrap();
            pm.fill(tiny_skia::Color::TRANSPARENT);
            let (ink, track) = (tray_ink(false, 255), tray_ink(false, 64));
            match two_busiest(&providers) {
                (Some(a), Some(b)) => {
                    draw_ring(&mut pm, RING_OUTER, RING_W, a as f32, ink, track);
                    let inner = RING_OUTER - RING_STEP;
                    draw_ring(&mut pm, inner, RING_W, b as f32, ink, track);
                }
                (Some(a), None) => draw_ring(&mut pm, RING_OUTER, RING_W, a as f32, ink, track),
                _ => fill_circle(&mut pm, CENTER, CENTER, 5.0, tray_ink(false, 200)),
            }
            let path = dir.join(format!("ailimits_{name}"));
            std::fs::write(&path, pm.encode_png().unwrap()).unwrap();
            println!("{}", path.display());
        }
    }

    /// Design preview: renders the stacked icon variants to %TEMP% for a
    /// visual check (the tray square is too small to judge live). Run
    /// explicitly: `cargo test preview_stacked -- --ignored`.
    #[test]
    #[ignore = "writes design previews to %TEMP%"]
    fn preview_stacked_icons() {
        let theme = ComputedTheme::compute(&crate::config::schema::UIConfig::default());
        let dir = std::env::temp_dir();
        for (name, providers) in [
            ("icon_1.png", vec![data(ProviderId::Claude, 93)]),
            (
                "icon_2.png",
                vec![data(ProviderId::Claude, 93), data(ProviderId::Codex, 81)],
            ),
            (
                "icon_4.png",
                vec![
                    data(ProviderId::Claude, 93),
                    data(ProviderId::Codex, 81),
                    data(ProviderId::Copilot, 40),
                    data(ProviderId::Antigravity, 7),
                ],
            ),
            (
                "icon_6.png",
                vec![
                    data(ProviderId::Claude, 93),
                    data(ProviderId::Codex, 81),
                    data(ProviderId::Copilot, 40),
                    data(ProviderId::Antigravity, 7),
                    data(ProviderId::Grok, 1),
                    data(ProviderId::Cursor, 55),
                ],
            ),
        ] {
            let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).unwrap();
            pm.fill(tiny_skia::Color::TRANSPARENT);
            // Re-draw through the public path: draw_stacked_icon returns an
            // Icon (no pixels back), so rebuild the pixmap the same way.
            let _ = draw_stacked_icon(&providers, &theme, false).unwrap();
            let png = render_preview(&providers, &theme);
            std::fs::write(dir.join(name), png).unwrap();
        }
    }

    /// Paint the same image draw_stacked_icon builds, but keep the pixmap.
    fn render_preview(providers: &[ProviderData], theme: &ComputedTheme) -> Vec<u8> {
        // Duplicate of draw_stacked_icon's painting (kept in sync manually;
        // this is a throwaway preview helper).
        let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).unwrap();
        pm.fill(tiny_skia::Color::TRANSPARENT);
        let n = providers.len();
        let white = color(235, 235, 235, 255);
        let mut rows: Vec<(f32, f32, f32, f32)> = Vec::new();
        let (digit_w, gap) = (13.0, 2.0);
        if n == 1 {
            if let Some(pct) = provider_pct(&providers[0]) {
                let val = (pct.round() as u32).to_string();
                draw_digits_fit(&mut pm, &val, 0.0, 0.0, 32.0, 22.0, white);
            }
            rows.push((1.0, 30.0, 24.0, 7.0));
        } else {
            let bar_h = ((30.0 - gap * (n - 1) as f32) / n as f32).floor();
            let mut y = (ICON_SIZE as f32 - (bar_h * n as f32 + gap * (n - 1) as f32)) / 2.0;
            for d in providers {
                if let Some(pct) = provider_pct(d) {
                    let val = (pct.round() as u32).to_string();
                    draw_digits_fit(&mut pm, &val, 0.0, y, digit_w, bar_h, white);
                }
                let vis_h = bar_h.min(9.0);
                rows.push((
                    digit_w + 2.0,
                    32.0 - digit_w - 3.0,
                    y + (bar_h - vis_h) / 2.0,
                    vis_h,
                ));
                y += bar_h + gap;
            }
        }
        for (d, (x, w, y, h)) in providers.iter().zip(rows) {
            let rad = (h / 2.0).min(3.5);
            fill_round_rect(&mut pm, x, y, w, h, rad, color(150, 150, 150, 70));
            if let Some(pct) = provider_pct(d) {
                let frac = (pct / 100.0).clamp(0.0, 1.0);
                let fill_w = (w * frac).max(3.0);
                let c = theme.level(UsageLevel::from_percentage(pct)).bar;
                fill_round_rect(&mut pm, x, y, fill_w, h, rad, color(c.r, c.g, c.b, 255));
            }
        }
        pm.encode_png().unwrap()
    }
}
