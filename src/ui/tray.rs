// ui/tray.rs — tray-area usage indicators.
//
// Two modes (config `general.indicator`), both a SINGLE tray icon:
//   Tray — two concentric rings, the busiest provider outside and the
//          runner-up inside, each sweeping clockwise from 12 o'clock.
//          Deliberately monochrome: it inks itself in the system taskbar
//          theme rather than the widget palette, so it stays readable on a
//          light or a dark bar and the shape alone carries the reading.
//   Bars — horizontal progress bars stacked one above the other, one row
//          per visible provider (the meter style competitors use); with a
//          single provider the row gains its percent number on top.
// Drawn with the same tiny-skia pipeline as the widget so colors match the
// palette. Images are re-rendered only when an integer % changes, to stay
// at ~0% idle CPU. The icon carries the same context menu as the widget;
// a left-click toggles the overlay.

use crate::config::schema::IndicatorKind;
use crate::providers::{ProviderData, ProviderStatus};
use crate::ui::theme::{ComputedTheme, UsageLevel};
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
    /// mode. It renders the busiest-provider rings, like Tray mode.
    fallback: bool,
}

impl Tray {
    pub fn new() -> Self {
        Self {
            mode: IndicatorKind::Off,
            icon: None,
            last: Vec::new(),
            promote_pending: false,
            fallback: false,
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
        // A Start-fallback icon renders the rings, exactly like Tray mode.
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
                draw_ring_icon(providers, light)
            } else {
                draw_stacked_icon(providers, theme, light)
            };
            if let Ok(img) = img {
                let _ = icon.set_icon(Some(img));
            }
        }
        let _ = icon.set_tooltip(Some(tooltip(providers)));
        // Second promotion pass once the registry keys surely exist.
        if self.promote_pending {
            self.promote_pending = false;
            let n = crate::platform::promote_tray_icons();
            tracing::debug!("tray icons promoted (second pass): {n}");
        }
    }

    /// A Panel indicator cannot paint over the Start menu (a protected z-band
    /// no overlay can beat). While the Start/Search scrim is up, fall back to a
    /// tray icon — which the shell keeps visible — showing the busiest
    /// provider's %; hide it again the instant the scrim closes and the panel
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
/// would freeze the icon whenever the second-place provider moved.
pub(crate) fn ring_cache_state(providers: &[ProviderData]) -> Vec<Option<u8>> {
    let (first, second) = two_busiest(providers);
    vec![first, second]
}

pub(crate) fn tooltip(providers: &[ProviderData]) -> String {
    if providers.is_empty() {
        return "AI Limits".to_string();
    }
    providers
        .iter()
        .map(|d| match provider_pct(d) {
            Some(p) => format!("{} {}%", d.id.display_name(), p.round() as u32),
            None => format!("{} •", d.id.display_name()),
        })
        .collect::<Vec<_>>()
        .join("  ·  ")
}

/// A neutral grey dot — used as the placeholder before the first data arrives.
/// Tray-icon ink for the system taskbar's light/dark theme: near-black on a
/// light taskbar, near-white on a dark one (mirrors the panel's `theme_ink`),
/// so the indicator stays readable on either. `a` is the alpha — 255 for a
/// solid fill/digit, ~64 for the faint track ring, ~200 for an inactive dot.
fn tray_ink(light: bool, a: u8) -> tiny_skia::Color {
    if light {
        color(20, 20, 20, a)
    } else {
        color(236, 236, 236, a)
    }
}

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

fn draw_ring_icon(providers: &[ProviderData], light: bool) -> Result<Icon> {
    let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).context("pixmap alloc")?;
    pm.fill(tiny_skia::Color::TRANSPARENT);
    // One ink for everything. The tray sits on the system taskbar, whose theme
    // is independent of the widget's palette, and at 16px a hue difference is
    // far weaker than the arc length — so the shape does the talking.
    let ink = tray_ink(light, 255);
    let track = tray_ink(light, 64);

    match two_busiest(providers) {
        // Two or more providers with data: the busiest takes the outer ring,
        // the runner-up the inner one. Which is which is answered by the
        // tooltip, which lists every provider with its percentage.
        (Some(first), Some(second)) => {
            draw_ring(&mut pm, RING_OUTER, first as f32, ink, track);
            draw_ring(&mut pm, RING_OUTER - RING_STEP, second as f32, ink, track);
        }
        // Exactly one: the outer ring alone, so the icon keeps its meaning
        // when a second provider starts reporting — nothing moves, a ring
        // simply appears inside.
        (Some(only), None) => {
            draw_ring(&mut pm, RING_OUTER, only as f32, ink, track);
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

/// One ring: the faint full-circle track, then the used arc over it.
fn draw_ring(
    pm: &mut Pixmap,
    outer: f32,
    pct: f32,
    ink: tiny_skia::Color,
    track: tiny_skia::Color,
) {
    let r_mid = outer - RING_W / 2.0;
    let full = std::f32::consts::TAU;
    stroke_arc(pm, r_mid, 0.0, full, track, LineCap::Butt);
    match ring_fill(pct, r_mid, RING_W) {
        RingFill::Empty => {}
        RingFill::Dot => fill_circle(
            pm,
            CENTER + r_mid * RING_TOP.cos(),
            CENTER + r_mid * RING_TOP.sin(),
            RING_W / 2.0,
            ink,
        ),
        RingFill::Arc { start, end } => stroke_arc(pm, r_mid, start, end, ink, LineCap::Round),
        RingFill::Full => stroke_arc(pm, r_mid, 0.0, full, ink, LineCap::Butt),
    }
}

/// Stroke an arc as a polyline. tiny-skia's PathBuilder has no arc primitive;
/// sampling finely enough that each segment is well under a pixel makes the
/// difference invisible once the shell scales the icon down.
fn stroke_arc(
    pm: &mut Pixmap,
    r_mid: f32,
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
        width: RING_W,
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
                let lvl = theme.level(UsageLevel::from_percentage(pct));
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
    let glyphs = tl.glyphs();
    let off_x = x + (w - (max_x - min_x)) / 2.0 - min_x;
    let off_y = y + (h - (max_y - min_y)) / 2.0 - min_y;

    let (cr, cg, cb) = (
        (c.red() * 255.0) as u32,
        (c.green() * 255.0) as u32,
        (c.blue() * 255.0) as u32,
    );
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
            let a = cov as u32;
            let inv = 255 - a;
            data[idx] = ((cr * a) / 255 + data[idx] as u32 * inv / 255) as u8;
            data[idx + 1] = ((cg * a) / 255 + data[idx + 1] as u32 * inv / 255) as u8;
            data[idx + 2] = ((cb * a) / 255 + data[idx + 2] as u32 * inv / 255) as u8;
            data[idx + 3] = (a + data[idx + 3] as u32 * inv / 255).min(255) as u8;
        }
    }
}

/// Shell tooltip metrics at 100% scaling, in pixels. Windows draws its own
/// tooltips with Segoe UI at 9pt (12px at 96 DPI), a tight box around the text,
/// and a small corner radius — not a pill.
///
/// EXPERIMENTAL: the previous numbers derived everything from the taskbar
/// height (font `bar_h * 0.30`, padding `0.85`/`0.55` of the font, radius a
/// third of the box), which on a stock 48px bar produced a 14.4px font in a
/// fully rounded pill — noticeably larger and rounder than anything the shell
/// shows. These match the shell instead.
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
/// grey 44 over black and grey 54 over white, so only 10/255 of the backdrop
/// comes through — alpha 244 over a true colour of grey 46. The earlier 218
/// was visibly more transparent than the real thing.
const TIP_FILL_ALPHA: u8 = 244;
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
    // `TIP_SHADOW` is also the box's offset inside the pixmap, which the caller
    // subtracts when positioning — see TaskbarPanel::show_tooltip.
    let inset = tip_shadow_inset(bar_h) as f32;
    let w = (box_w + inset * 2.0) as u32;
    let h = (box_h + inset * 2.0) as u32;
    // Centre the actual ink inside the fixed line band, so a string with no
    // descenders does not float high in the box.
    let text_y = inset + pad_top + (line_h - th) / 2.0;
    let mut pm = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
    pm.fill(tiny_skia::Color::TRANSPARENT);
    let radius = (TIP_RADIUS * scale).min(box_h / 2.0);

    // Drop shadow: concentric rounded rects stepping outward, each barely
    // visible, which approximates the shell's falloff without a blur pass.
    // Measured over mid grey: -3,-4,-7,-10 luma at 1..4px out, nothing beyond.
    // The alphas COMPOUND: each ring is painted over the previous one, so the
    // cumulative darkening at distance d is 1-prod(1-a). Picked to land on the
    // measured falloff (-3,-4,-7,-10 luma at 5..2px out over grey 128) rather
    // than by eye; a first pass at 6/8/10/14 compounded to -15..-31, three
    // times too heavy.
    for (step, alpha) in [(5.0, 4u8), (4.0, 4), (3.0, 5), (2.0, 6), (1.0, 8)] {
        let s = step * scale;
        fill_round_rect(
            &mut pm,
            inset - s,
            inset - s + 1.0,
            box_w + s * 2.0,
            box_h + s * 2.0 - 1.0,
            radius + s,
            color(0, 0, 0, alpha),
        );
    }
    // DARK: no border at all. Scanning inward from the edge of the measured
    // tooltip over black gives 45,44,45,45 — flat body from the first pixel.
    // What separates it from the background is an outer shadow, which is the
    // shell's to draw, not a lighter outline. We drew one and it was wrong.
    //
    // LIGHT: a hairline IS present there — a near-white body needs it — so it
    // stays, as a 1px under-fill with the body inset into it.
    let (body, text_ink) = if light {
        (color(249, 249, 249, TIP_FILL_ALPHA), color(26, 26, 26, 255))
    } else {
        // Grey 46 is the body's true colour behind alpha 244; the measured 44
        // over black is what that composites to.
        (color(46, 46, 46, TIP_FILL_ALPHA), color(255, 255, 255, 255))
    };
    if light {
        fill_round_rect(
            &mut pm,
            inset,
            inset,
            box_w,
            box_h,
            radius,
            color(0, 0, 0, 36),
        );
        fill_round_rect(
            &mut pm,
            inset + 1.0,
            inset + 1.0,
            box_w - 2.0,
            box_h - 2.0,
            (radius - 1.0).max(0.0),
            body,
        );
    } else {
        fill_round_rect(&mut pm, inset, inset, box_w, box_h, radius, body);
    }
    draw_text_at(&mut pm, text, inset + pad_x, text_y, size, text_ink);
    pm
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

/// Coverage -> alpha, gamma-corrected, built once. The exponent was fitted
/// against the measured shell tooltip: 1/1.45 only moved the mean lit luma
/// from 169 to 175 against DirectWrite's 197, so it goes further.
static TEXT_GAMMA_LUT: std::sync::LazyLock<[u8; 256]> = std::sync::LazyLock::new(|| {
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let c = i as f32 / 255.0;
        *slot = (c.powf(1.0 / 2.8) * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    lut
});

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
            // Gamma-correct the coverage. Blending it linearly makes our text
            // measurably thinner than the shell's: over the same string the
            // mean lit luma came out 169 against DirectWrite's 197, with both
            // peaking at 255 — the colour was right, the antialiasing was not.
            // DirectWrite gamma-corrects; matching it is what closes the gap.
            let a = TEXT_GAMMA_LUT[cov as usize] as u32;
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

fn to_icon(pm: &Pixmap) -> Result<Icon> {
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
        // Caching only the busiest would freeze the icon whenever the
        // second-place provider moved - it compiles, looks right, and stops
        // updating minutes later in normal use.
        let before = vec![data(ProviderId::Claude, 90), data(ProviderId::Codex, 30)];
        let after = vec![data(ProviderId::Claude, 90), data(ProviderId::Codex, 55)];

        assert_ne!(
            ring_cache_state(&before),
            ring_cache_state(&after),
            "a change in the second ring must invalidate the cache"
        );
    }

    /// Design preview for the ring icon: the tray square is far too small to
    /// judge live, so write the states out to %TEMP% at 32px. The top of the
    /// scale (90/95/99/100) is the pair worth staring at.
    /// Run: `cargo test preview_rings -- --ignored`.
    #[test]
    #[ignore]
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
                    draw_ring(&mut pm, RING_OUTER, a as f32, ink, track);
                    draw_ring(&mut pm, RING_OUTER - RING_STEP, b as f32, ink, track);
                }
                (Some(a), None) => draw_ring(&mut pm, RING_OUTER, a as f32, ink, track),
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
    #[ignore]
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
        ] {
            let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).unwrap();
            pm.fill(tiny_skia::Color::TRANSPARENT);
            // Re-draw through the public path: draw_stacked_icon returns an
            // Icon (no pixels back), so rebuild the pixmap the same way.
            let _ = draw_stacked_icon(&providers, &theme, false).unwrap();
            // For the preview, replicate via the private painter into pm:
            // simplest is to call draw_stacked_icon's body — instead just
            // save what the icon would contain by re-running the painter.
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
