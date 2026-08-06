// ui/tray.rs — tray-area usage indicators.
//
// Two modes (config `general.indicator`), both a SINGLE tray icon:
//   Tray — a pie gauge of the highest-usage provider's %.
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
use tiny_skia::{Paint, PathBuilder, Pixmap, Transform};
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
    /// mode. It renders the busiest-provider pie, like Tray mode.
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
        // A Start-fallback icon renders the busiest-% pie, exactly like Tray.
        let pie = self.fallback || matches!(self.mode, IndicatorKind::Tray);
        let state: Vec<Option<u8>> = if pie {
            // Both halves of the pie — see pie_cache_state.
            pie_cache_state(providers)
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
            let img = if pie {
                draw_pie_icon(providers, theme, light)
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
/// do not trade halves from one refresh to the next.
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

/// What the pie repaint cache must hold: BOTH halves. Caching only the busiest
/// would freeze the icon whenever the second-place provider moved.
pub(crate) fn pie_cache_state(providers: &[ProviderData]) -> Vec<Option<u8>> {
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
    fill_circle(&mut pm, 16.0, 16.0, 5.0, tray_ink(light, 200));
    to_icon(&pm)
}

fn draw_pie_icon(providers: &[ProviderData], theme: &ComputedTheme, light: bool) -> Result<Icon> {
    let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).context("pixmap alloc")?;
    pm.fill(tiny_skia::Color::TRANSPARENT);
    let (cx, cy, r) = (16.0, 16.0, 14.0);

    // Usage colour for a percentage, honouring the monochrome palette.
    // Monochrome greys are tuned for the dark widget and vanish on a light
    // taskbar — use the system-theme ink instead; coloured palettes already
    // contrast on either taskbar, so keep their hue.
    let ink = |pct: f32| {
        if theme.monochrome {
            tray_ink(light, 255)
        } else {
            let lvl = theme.level(UsageLevel::from_percentage(pct));
            color(lvl.bar.r, lvl.bar.g, lvl.bar.b, 255)
        }
    };

    match two_busiest(providers) {
        // Two or more providers with data: split the circle down the middle,
        // busiest on the left. Which half is which is answered by the tooltip,
        // which already lists every provider with its percentage.
        (Some(first), Some(second)) => {
            fill_half_track(&mut pm, cx, cy, r, Half::Left, tray_ink(light, 64));
            fill_half_track(&mut pm, cx, cy, r, Half::Right, tray_ink(light, 64));
            fill_half_pie(
                &mut pm,
                cx,
                cy,
                r,
                Half::Left,
                first as f32,
                ink(first as f32),
            );
            fill_half_pie(
                &mut pm,
                cx,
                cy,
                r,
                Half::Right,
                second as f32,
                ink(second as f32),
            );
        }
        // Exactly one: the full circle, pixel-identical to before. Single
        // provider setups see no change at all.
        (Some(only), None) => {
            fill_circle(&mut pm, cx, cy, r, tray_ink(light, 64));
            fill_pie(&mut pm, cx, cy, r, only as f32, ink(only as f32));
        }
        // No percentage data yet (loading / binary / error).
        _ => fill_circle(&mut pm, cx, cy, 5.0, tray_ink(light, 200)),
    }

    to_icon(&pm)
}

/// Which half of the split pie a sector belongs to.
#[derive(Clone, Copy, PartialEq)]
enum Half {
    Left,
    Right,
}

/// A vertical hairline keeps the two halves from reading as one circle.
const HALF_SPLIT: f32 = 1.0;

/// The faint track behind one half.
fn fill_half_track(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, half: Half, c: tiny_skia::Color) {
    half_sector(pm, cx, cy, r, half, 1.0, c);
}

/// One half's usage sector, filling from the top towards the bottom: the left
/// half anticlockwise, the right half clockwise, so both grow away from the
/// dividing line and can be compared at a glance.
fn fill_half_pie(
    pm: &mut Pixmap,
    cx: f32,
    cy: f32,
    r: f32,
    half: Half,
    pct: f32,
    c: tiny_skia::Color,
) {
    let frac = (pct / 100.0).clamp(0.0, 1.0);
    if frac <= 0.0 {
        return;
    }
    half_sector(pm, cx, cy, r, half, frac, c);
}

/// Paint `frac` of a half-disc, hinged on the vertical centre line.
fn half_sector(
    pm: &mut Pixmap,
    cx: f32,
    cy: f32,
    r: f32,
    half: Half,
    frac: f32,
    c: tiny_skia::Color,
) {
    use std::f32::consts::PI;
    let inset = HALF_SPLIT / 2.0;
    let (hinge, dir) = match half {
        Half::Left => (cx - inset, -1.0),
        Half::Right => (cx + inset, 1.0),
    };

    let mut pb = PathBuilder::new();
    pb.move_to(hinge, cy);
    // Start at the top of the dividing line and sweep half a turn at most.
    pb.line_to(hinge, cy - r);
    let steps = ((frac * 48.0).ceil() as usize).max(1);
    for i in 0..=steps {
        let f = (i as f32 / steps as f32) * frac;
        let ang = -PI / 2.0 + dir * f * PI;
        pb.line_to(hinge + r * ang.cos(), cy + r * ang.sin());
    }
    pb.close();
    fill_path(pm, pb, c);
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

/// Render the hover tooltip pixmap for the taskbar panel: the provider summary
/// as a fully rounded pill drawn by us (not a native control) so it matches the
/// shell's own tooltip. It follows the system theme like the real Win11 tooltip:
/// a borderless dark pill in dark mode, a near-white pill with a hairline border
/// in light mode. Sized to the text; `bar_h` scales the font to the bar.
pub(crate) fn render_tooltip(text: &str, bar_h: f32, light: bool) -> Pixmap {
    let size = (bar_h * 0.30).clamp(11.0, 24.0);
    let (tw, th) = text_extent(text, size);
    let pad_x = (size * 0.85).round();
    let pad_y = (size * 0.55).round();
    let w = (tw + pad_x * 2.0).ceil().max(8.0) as u32;
    let h = (th + pad_y * 2.0).ceil().max(8.0) as u32;
    let mut pm = Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap());
    pm.fill(tiny_skia::Color::TRANSPARENT);
    let radius = (h as f32 * 0.34).min(12.0);
    if light {
        // Win11 light tooltip: near-white pill + dark text, with a hairline border
        // (drawn as a 1px under-fill) so it reads on a light background.
        fill_round_rect(
            &mut pm,
            0.0,
            0.0,
            w as f32,
            h as f32,
            radius,
            color(0, 0, 0, 38),
        );
        fill_round_rect(
            &mut pm,
            1.0,
            1.0,
            w as f32 - 2.0,
            h as f32 - 2.0,
            radius - 1.0,
            color(249, 249, 249, 255),
        );
        draw_text_at(&mut pm, text, pad_x, pad_y, size, color(26, 26, 26, 255));
    } else {
        // Dark pill; transparent corners give true rounded edges (no border).
        fill_round_rect(
            &mut pm,
            0.0,
            0.0,
            w as f32,
            h as f32,
            radius,
            color(44, 44, 44, 255),
        );
        draw_text_at(&mut pm, text, pad_x, pad_y, size, color(236, 236, 236, 255));
    }
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

/// A pie sector from 12 o'clock, clockwise, covering `pct`% of the circle.
fn fill_pie(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, pct: f32, c: tiny_skia::Color) {
    use std::f32::consts::PI;
    let frac = (pct / 100.0).clamp(0.0, 1.0);
    if frac <= 0.0 {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy);
    let steps = ((frac * 64.0).ceil() as usize).max(1);
    for i in 0..=steps {
        let f = (i as f32 / steps as f32) * frac;
        // Start at -90° (top); increasing angle in screen coords goes clockwise.
        let ang = -PI / 2.0 + f * 2.0 * PI;
        pb.line_to(cx + r * ang.cos(), cy + r * ang.sin());
    }
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

    #[test]
    fn the_two_busiest_providers_are_returned_highest_first() {
        let providers = vec![
            data(ProviderId::Claude, 40),
            data(ProviderId::Codex, 93),
            data(ProviderId::Copilot, 71),
        ];

        let (first, second) = two_busiest(&providers);

        assert_eq!(first, Some(93), "the busiest goes to the left half");
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
    fn equal_percentages_keep_the_widget_order_so_the_halves_do_not_swap() {
        // Both at 80: without a stable tie-break the halves would trade places
        // between refreshes and the icon would flicker for no reason.
        let providers = vec![data(ProviderId::Claude, 80), data(ProviderId::Codex, 80)];

        assert_eq!(two_busiest(&providers), (Some(80), Some(80)));
        assert_eq!(
            two_busiest(&providers),
            two_busiest(&providers),
            "the same input must give the same halves"
        );
    }

    #[test]
    fn the_repaint_cache_tracks_both_halves() {
        // Caching only the busiest would freeze the icon whenever the
        // second-place provider moved - it compiles, looks right, and stops
        // updating minutes later in normal use.
        let before = vec![data(ProviderId::Claude, 90), data(ProviderId::Codex, 30)];
        let after = vec![data(ProviderId::Claude, 90), data(ProviderId::Codex, 55)];

        assert_ne!(
            pie_cache_state(&before),
            pie_cache_state(&after),
            "a change in the second half must invalidate the cache"
        );
    }

    /// Design preview for the split pie: the tray square is far too small to
    /// judge live, so write the variants out at several percentage pairs.
    /// Run: `cargo test preview_split_pie -- --ignored`.
    #[test]
    #[ignore]
    fn preview_split_pie() {
        let theme = ComputedTheme::compute(&crate::config::schema::UIConfig::default());
        let dir = std::env::temp_dir();
        for (name, providers) in [
            ("pie_none.png", vec![]),
            ("pie_one.png", vec![data(ProviderId::Claude, 93)]),
            (
                "pie_two.png",
                vec![data(ProviderId::Claude, 93), data(ProviderId::Codex, 18)],
            ),
            (
                "pie_even.png",
                vec![data(ProviderId::Claude, 50), data(ProviderId::Codex, 50)],
            ),
            (
                "pie_full.png",
                vec![data(ProviderId::Claude, 100), data(ProviderId::Codex, 100)],
            ),
            (
                "pie_four.png",
                vec![
                    data(ProviderId::Claude, 71),
                    data(ProviderId::Codex, 100),
                    data(ProviderId::Copilot, 40),
                    data(ProviderId::Antigravity, 7),
                ],
            ),
        ] {
            let mut pm = Pixmap::new(ICON_SIZE, ICON_SIZE).unwrap();
            pm.fill(tiny_skia::Color::TRANSPARENT);
            let (cx, cy, r) = (16.0, 16.0, 14.0);
            let ink = |pct: f32| {
                let lvl = theme.level(UsageLevel::from_percentage(pct));
                color(lvl.bar.r, lvl.bar.g, lvl.bar.b, 255)
            };
            match two_busiest(&providers) {
                (Some(a), Some(b)) => {
                    fill_half_track(&mut pm, cx, cy, r, Half::Left, tray_ink(false, 64));
                    fill_half_track(&mut pm, cx, cy, r, Half::Right, tray_ink(false, 64));
                    fill_half_pie(&mut pm, cx, cy, r, Half::Left, a as f32, ink(a as f32));
                    fill_half_pie(&mut pm, cx, cy, r, Half::Right, b as f32, ink(b as f32));
                }
                (Some(a), None) => {
                    fill_circle(&mut pm, cx, cy, r, tray_ink(false, 64));
                    fill_pie(&mut pm, cx, cy, r, a as f32, ink(a as f32));
                }
                _ => fill_circle(&mut pm, cx, cy, 5.0, tray_ink(false, 200)),
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
