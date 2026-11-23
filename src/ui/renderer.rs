// ui/renderer.rs — UI rendering via tiny-skia.
//
// The ONLY place that draws. Colors come ONLY from ui/theme.rs,
// zones from ui/layout.rs.

use super::{
    layout::{self, BodyLayout, Rect, RowTier, Slots, WidgetMetrics, WindowLayout},
    theme::{Color, ComputedTheme},
};
use crate::config::schema::{DetailLevel, UiNumbers};
use crate::config::tune::Countdown;
use crate::providers::{
    Metric, MetricUnit, MetricWindow, ProviderData, ProviderId, ProviderStatus,
};
use crate::rows::{ModelLabel, RowView};
use crate::utc::{self, Timestamp};
use anyhow::{Context, Result};
use fontdue::{
    layout::{CoordinateSystem, Layout as TextLayout, TextStyle},
    Font,
};
use std::collections::HashMap;
use tiny_skia::{Paint, PathBuilder, Pixmap, Transform};

/// System font candidates, in preference order. Segoe UI is the Win11 default
/// (and the clock font the taskbar panel matches; full Cyrillic coverage);
/// arial/tahoma/verdana are fallbacks for debloated installs, Server Core,
/// LTSC SKUs or custom images where Segoe UI may be absent — without a
/// fallback the whole overlay silently fails to start (no console).
pub(crate) const FONT_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\segoeui.ttf",
    r"C:\Windows\Fonts\arial.ttf",
    r"C:\Windows\Fonts\tahoma.ttf",
    r"C:\Windows\Fonts\verdana.ttf",
];

/// Load the first usable UI font from the candidate list. None only when
/// not one of them can be read and parsed.
fn load_ui_font() -> Option<Font> {
    for path in FONT_CANDIDATES {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = Font::from_bytes(data, fontdue::FontSettings::default()) {
                return Some(font);
            }
        }
    }
    None
}

/// The UI font, read and parsed once for the whole app: the widget, the
/// picker, the tray icon, its tooltip and the taskbar panel all draw and
/// measure with this one. fontdue parses every glyph up front, about 20 MB
/// for Segoe UI, so a second copy (the tray module had one of its own, first
/// loaded by the tray tooltip's wrapping) doubled the app's private memory.
pub(crate) fn ui_font() -> Option<&'static Font> {
    static FONT: std::sync::OnceLock<Option<Font>> = std::sync::OnceLock::new();
    FONT.get_or_init(load_ui_font).as_ref()
}

/// Name column width in vertical compact. Wide enough for the longest
/// provider name at the compact size: "Antigravity" at 11px Segoe UI runs
/// 52.6px, and at 48 the name reached into the gap before the bar (the
/// site's port, which clips the box, showed it as "Antigravit").
pub(super) const NAME_WIDTH: f32 = 56.0;
/// Percent column width.
pub(super) const PCT_WIDTH: f32 = 30.0;
/// Gap between row elements.
pub(super) const ROW_GAP: f32 = 7.0;
/// Text size of Medium's meta line (the line under the bar).
pub(super) const META_SIZE: f32 = 10.5;
/// The widest reset countdown `format_duration` writes: what the meta line's
/// reset is held to when deciding the room a model gets beside it.
pub(super) const LONGEST_RESET: &str = "23h 59min";

/// The widest countdown `format_countdown` writes in `style` (see
/// `LONGEST_RESET`; a test holds each to it).
pub(super) fn longest_countdown(style: Countdown) -> &'static str {
    match style {
        Countdown::Standard => LONGEST_RESET,
        Countdown::Compact => "23h59m",
        Countdown::Long => "99d 23h 59min",
    }
}

/// A rows-layout bar's thickness at today's size: Compact's in its row,
/// Medium's and Expanded's on their line of their own (`[tune.widget]
/// bar_scale` multiplies it).
pub(super) fn row_bar_thickness(detail: &DetailLevel) -> f32 {
    match detail {
        DetailLevel::Compact => 3.0,
        DetailLevel::Medium => 4.0,
        DetailLevel::Expanded => 5.0,
    }
}
/// A row's texts where they share one line (a Compact row, a row without
/// its bar): the name's size, the percentage's and the reset's, as the
/// detail level draws them elsewhere.
pub(super) fn line_sizes(detail: &DetailLevel) -> (f32, f32, f32) {
    match detail {
        DetailLevel::Compact => (11.0, 11.0, META_SIZE),
        DetailLevel::Medium => (12.0, 13.0, META_SIZE),
        DetailLevel::Expanded => (13.0, 15.0, 11.5),
    }
}

/// The size of the text on the left of Expanded's meta line.
pub(super) const EXPANDED_LEFT_SIZE: f32 = 11.0;

/// The widest text Expanded's meta line puts on its left for `data`: its
/// weekly limit as the hover writes it ("Weekly: 23h 59min"), or a count
/// ("123 / 300"). None when it has neither.
pub(super) fn expanded_left_widest(data: &ProviderData, style: Countdown) -> Option<String> {
    if let Some(metric) = long_window_metric(data) {
        let pct = format!("{}: 100%", metric.label);
        let time = format!("{}: {}", metric.label, longest_countdown(style));
        return Some(if time.len() > pct.len() { time } else { pct });
    }
    data.headline_metric()
        .filter(|m| !is_percent_metric(m))
        .map(Metric::display_text)
}

/// The widest provider name: what the columns' width is made for.
const WIDEST_PROVIDER_NAME: &str = "Antigravity";
/// Size of a model label on Expanded's line of its own.
const MODEL_EXPANDED_SIZE: f32 = 11.0;
/// The first-install lines (rows.js `widgetPixmap`).
const CHOOSE_TITLE: &str = "Setup required";
const CHOOSE_LINE: &str = "Click to set up. Nothing is read until then.";
/// Rows are chosen, but none of them has anything to show now
/// (`BodyLayout::Unshown`): an extra row the plan does not have, a row of a
/// provider that is not connected. A click opens the picker.
const UNSHOWN_TITLE: &str = "No data yet";
const UNSHOWN_LINE: &str = "Selected rows have no data. Click to change.";

/// Bar and text colors, accounting for staleness and estimation —
/// both render grey. Stale data greys even while a reset is still ahead: the
/// countdown keeps running beside it, but the value has stopped updating and
/// must not read as live.
fn row_colors(theme: &ComputedTheme, data: &ProviderData, pct: f32) -> (Color, Color) {
    if data.is_aged() {
        (theme.meta, theme.meta)
    } else {
        let level = theme.level_at(pct);
        (level.bar, level.text)
    }
}

/// Percent text: "≈0%" for estimates.
fn pct_text(data: &ProviderData, pct: f32) -> String {
    if matches!(data.status, ProviderStatus::Estimated) {
        format!("≈{}%", pct.round() as u32)
    } else {
        format!("{}%", pct.round() as u32)
    }
}

/// Meta label for stale/estimated data.
fn stale_age_secs_or_estimated(data: &ProviderData, style: Countdown) -> Option<String> {
    let age = data.stale_age_secs()?;
    if matches!(data.status, ProviderStatus::Estimated) {
        Some(format!("est · {} ago", format_countdown(age, style)))
    } else {
        Some(format!("{} ago", format_countdown(age, style)))
    }
}

/// Text alignment.
#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub struct Renderer {
    font: &'static Font,
}

/// How a row is drawn besides its rectangle: what it shows and the slots
/// for it, the width's tier, the pointer, the forecast and the reason a
/// row that is not live gives on hover.
struct Line<'a> {
    numbers: &'a UiNumbers,
    slots: &'a Slots,
    tier: RowTier,
    cursor: Option<(f32, f32)>,
    forecast: Option<&'a str>,
    reason: Option<&'a str>,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        let font = ui_font().with_context(|| {
            format!(
                "no usable system font found (tried {})",
                FONT_CANDIDATES.join(", ")
            )
        })?;
        Ok(Self { font })
    }

    /// Draw the whole UI into the pixmap.
    ///
    /// `rows` — the widget's rows (`rows::visible_rows`, Surface::Widget), in
    /// the order `layout` was computed for. With the default config that is
    /// one row per visible provider, carrying its data untouched.
    ///
    /// `opacity` — background opacity from the config (0.10–0.85). Applied
    /// ONLY to the background alpha; text stays readable.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        pixmap: &mut Pixmap,
        layout: &WindowLayout,
        theme: &ComputedTheme,
        rows: &[RowView],
        opacity: f32,
        detail: &DetailLevel,
        cursor: Option<(f32, f32)>,
        forecasts: &HashMap<ProviderId, Timestamp>,
        errors: &HashMap<ProviderId, String>,
    ) {
        // Fully transparent base — the blur backdrop shows through.
        pixmap.fill(tiny_skia::Color::TRANSPARENT);

        let bg = Color {
            a: (opacity.clamp(0.10, 0.85) * 255.0) as u8,
            ..theme.bg
        };
        self.fill_rect(
            pixmap,
            Rect {
                x: 0.0,
                y: 0.0,
                w: layout.width,
                h: layout.height,
            },
            bg,
            0.0,
        );

        // The sizes the layout was computed with ([tune.widget]).
        let m = &layout.metrics;

        // A fresh install: say what to do rather than show an empty box.
        if let BodyLayout::Choose { area } = &layout.body {
            self.draw_two_lines(pixmap, area, theme, m, CHOOSE_TITLE, CHOOSE_LINE);
            return;
        }
        // Rows chosen, none of them with data: say so, and that a click
        // chooses others.
        if let BodyLayout::Unshown { area } = &layout.body {
            self.draw_two_lines(pixmap, area, theme, m, UNSHOWN_TITLE, UNSHOWN_LINE);
            return;
        }

        // First-run / everything-unconfigured: with no visible providers the
        // window would be a blank translucent box with no path forward. Draw a
        // single guidance line pointing to the menu instead of nothing.
        if rows.is_empty() {
            self.draw_text(
                pixmap,
                "Right-click → set up providers",
                0.0,
                (layout.height - m.t(11.0)) / 2.0,
                m.t(10.5),
                theme.provider_name,
                Align::Center,
                layout.width,
            );
            return;
        }

        let model_line = |i: usize| layout.model_lines.get(i).copied().flatten();
        match &layout.body {
            BodyLayout::Vertical { rows: rects, hint } => {
                for (i, (rect, row)) in rects.iter().zip(rows.iter()).enumerate() {
                    let fc = forecast_label(forecasts, &row.data, m.countdown);
                    let reason = hover_reason(&row.data, errors);
                    self.draw_vertical_row(
                        pixmap,
                        rect,
                        theme,
                        m,
                        row,
                        model_line(i).as_ref(),
                        detail,
                        layout.row_tier,
                        (&layout.numbers, &layout.slots),
                        cursor,
                        fc.as_deref(),
                        reason.as_deref(),
                    );
                }
                // The hint is written for the natural width; a narrowed window
                // would paint it past its own edge, so it goes with the names.
                if *detail == DetailLevel::Compact
                    && hint.h > 0.0
                    && layout.row_tier == RowTier::Full
                {
                    let hover_hint = hovered_hint_text(rects, rows, cursor, errors, m.countdown);
                    self.draw_hint(pixmap, hint, theme, m, rows, hover_hint.as_deref());
                }
            }
            BodyLayout::Horizontal { cols } => {
                for (i, (col, row)) in cols.iter().zip(rows.iter()).enumerate() {
                    let fc = forecast_label(forecasts, &row.data, m.countdown);
                    let reason = hover_reason(&row.data, errors);
                    self.draw_horizontal_col(
                        pixmap,
                        col,
                        theme,
                        m,
                        row,
                        model_line(i).as_ref(),
                        detail,
                        &layout.numbers,
                        cursor,
                        fc.as_deref(),
                        reason.as_deref(),
                    );
                }
            }
            BodyLayout::Choose { .. } | BodyLayout::Unshown { .. } => {}
        }
    }

    /// The two-line box: a title over a line saying what to do (the first
    /// install, rows.js `widgetPixmap`, its first branch; and a widget whose
    /// rows have nothing to show).
    fn draw_two_lines(
        &self,
        pixmap: &mut Pixmap,
        area: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        title: &str,
        line: &str,
    ) {
        self.draw_text(
            pixmap,
            title,
            area.x,
            area.y + m.t(4.0),
            m.t(12.0),
            theme.provider_name,
            Align::Left,
            area.w,
        );
        self.draw_text(
            pixmap,
            line,
            area.x,
            area.y + m.t(23.0),
            m.t(META_SIZE),
            theme.meta,
            Align::Left,
            area.w,
        );
    }

    // ─── Vertical ────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn draw_vertical_row(
        &self,
        pixmap: &mut Pixmap,
        rect: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        row: &RowView,
        model_line: Option<&Rect>,
        detail: &DetailLevel,
        tier: RowTier,
        (numbers, slots): (&UiNumbers, &Slots),
        cursor: Option<(f32, f32)>,
        forecast: Option<&str>,
        reason: Option<&str>,
    ) {
        let line = Line {
            numbers,
            slots,
            tier,
            cursor,
            forecast,
            reason,
        };
        match detail {
            // Compact has no per-row reset slot while its rows show the bar
            // and the percentage; the reason shows in the hint line. Nor a
            // model: the tooltip carries it.
            DetailLevel::Compact if numbers.bar && numbers.percent => {
                self.vertical_compact(pixmap, rect, theme, m, row, tier, cursor)
            }
            // Any other Compact row is one line, its reset on it.
            DetailLevel::Compact => {
                self.draw_line(pixmap, rect, rect, theme, m, row, detail, &line, true)
            }
            DetailLevel::Medium if !numbers.bar => {
                self.vertical_bare(pixmap, rect, theme, m, row, model_line, detail, &line)
            }
            DetailLevel::Medium => {
                self.vertical_medium(pixmap, rect, theme, m, row, model_line, false, &line)
            }
            DetailLevel::Expanded => {
                self.vertical_medium(pixmap, rect, theme, m, row, model_line, true, &line)
            }
        }
    }

    /// A row on one line: the name, the bar when the row shows one, the
    /// reset and the percentage right-aligned in their slots
    /// (`layout::Slots`), on one baseline. `band` is what the line is
    /// centred in, `row` the whole row (its hover). With `reason_here`, a
    /// hovered row that is not live says why in place of its numbers.
    #[allow(clippy::too_many_arguments)]
    fn draw_line(
        &self,
        pixmap: &mut Pixmap,
        band: &Rect,
        row: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        view: &RowView,
        detail: &DetailLevel,
        line: &Line,
        reason_here: bool,
    ) {
        let data = &view.data;
        let n = line.numbers;
        let (name_size, pct_size, reset_size) = line_sizes(detail);
        let (name_size, pct_size, reset_size) = (m.t(name_size), m.t(pct_size), m.t(reset_size));
        let gap = m.t(ROW_GAP);
        // One baseline for the three sizes: the name's, centred in the band
        // as a Compact row centres its text.
        let name_y = band.y + (band.h - name_size * 13.0 / 11.0) / 2.0;
        let base = name_y + self.ascent(name_size);
        let y_of = |size: f32| base - self.ascent(size);
        let end = band.x + band.w;
        let right = layout::line_right(detail, m, n, line.slots);
        let name_w = layout::line_name(detail, m, line.slots);

        let after_name = if matches!(line.tier, RowTier::Full | RowTier::Short) {
            self.draw_text(
                pixmap,
                self.fit_name(view.names, name_size, || name_w),
                band.x,
                y_of(name_size),
                name_size,
                theme.name_color(&view.provider),
                Align::Left,
                name_w,
            );
            band.x + name_w + gap
        } else {
            band.x
        };

        let hovered = line
            .reason
            .filter(|_| reason_here && line.cursor.is_some_and(|p| point_in_rect(p, row)));
        if let Some(text) = hovered {
            let room = end - after_name;
            let text = self.fit_line(text, reset_size, room);
            let y = y_of(reset_size);
            self.draw_text(
                pixmap,
                &text,
                after_name,
                y,
                reset_size,
                theme.meta,
                Align::Left,
                room,
            );
            return;
        }

        let active = active_progress_metric(data, row, line.cursor);
        let shown_pct = match (&data.status, active.and_then(Metric::percentage)) {
            (ProviderStatus::Ok | ProviderStatus::Estimated, Some(pct)) => Some(pct),
            _ => None,
        };
        let Some(pct) = shown_pct else {
            // A dot and words after the name, where a Compact row puts them
            // in its bar's place.
            let h = m.t(row_bar_thickness(&DetailLevel::Compact));
            let at = Rect {
                x: after_name,
                y: band.y + (band.h - h) / 2.0,
                w: end - after_name,
                h,
            };
            let text_y = band.y + (band.h - m.t(13.0)) / 2.0;
            self.draw_status_inline(pixmap, &at, text_y, theme, m, data, end - after_name);
            return;
        };
        let (bar_color, text_color) = row_colors(theme, data, pct);
        let reset_on = layout::reset_on_line(n) && line.tier != RowTier::Short;
        let reset = reset_on
            .then(|| {
                let room = line.slots.reset as f32;
                self.line_reset(data, active, line.forecast, theme, m, reset_size, room)
            })
            .flatten();

        // Too narrow for anything but a number: the percentage, else the
        // reset, centred on the row.
        if line.tier == RowTier::PercentOnly {
            let (text, size, color) = if n.percent {
                (pct_text(data, pct), pct_size, text_color)
            } else if let Some((text, color)) = reset {
                (text, reset_size, color)
            } else {
                return;
            };
            self.draw_text(
                pixmap,
                &text,
                band.x,
                y_of(size),
                size,
                color,
                Align::Center,
                band.w,
            );
            return;
        }

        let mut x = end;
        if n.percent {
            let w = layout::pct_slot(detail, m);
            let y = y_of(pct_size);
            let text = pct_text(data, pct);
            self.draw_text(
                pixmap,
                &text,
                x - w,
                y,
                pct_size,
                text_color,
                Align::Right,
                w,
            );
            x -= w + gap;
        }
        if reset_on {
            let w = line.slots.reset as f32;
            if let Some((text, color)) = &reset {
                self.draw_text(
                    pixmap,
                    text,
                    x - w,
                    y_of(reset_size),
                    reset_size,
                    *color,
                    Align::Right,
                    w,
                );
            }
        }
        if n.bar {
            let bar_end = if right > 0.0 { end - right - gap } else { end };
            let h = row_bar_thickness(&DetailLevel::Compact) * m.bar;
            let bar = Rect {
                x: after_name,
                y: band.y + (band.h - h) / 2.0,
                w: (bar_end - after_name).max(0.0),
                h,
            };
            self.draw_bar(pixmap, &bar, theme, bar_color, pct);
        }
    }

    /// What a line shows for its reset, held to `room`: as Medium's meta
    /// line writes it ("2h 14min", a weekly's "Weekly: 6d 2h"), else the
    /// forecast, else how old the figure is.
    #[allow(clippy::too_many_arguments)]
    fn line_reset(
        &self,
        data: &ProviderData,
        active: Option<&Metric>,
        forecast: Option<&str>,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        size: f32,
        room: f32,
    ) -> Option<(String, Color)> {
        let future = |t: &Timestamp| *t > utc::now();
        let text = match active {
            Some(metric) if is_long_window_metric(metric) => Some((
                match metric.reset_at.filter(future) {
                    Some(reset) => {
                        let secs = (reset - utc::now()).as_secs();
                        let label = Some(metric.label.as_str());
                        self.countdown_text(label, secs, m.countdown, size, || room)
                    }
                    None => metric.label.clone(),
                },
                theme.meta,
            )),
            _ => match active.and_then(|m| m.reset_at).filter(future) {
                Some(reset) => {
                    let secs = (reset - utc::now()).as_secs();
                    Some((
                        self.countdown_text(None, secs, m.countdown, size, || room),
                        theme.meta,
                    ))
                }
                None => forecast
                    .map(|fc| (fc.to_string(), theme.high.text))
                    .or_else(|| {
                        stale_age_secs_or_estimated(data, m.countdown).map(|l| (l, theme.meta))
                    }),
            },
        };
        text.map(|(t, c)| (self.fit_line(&t, size, room), c))
    }

    /// Medium without the bar: one line, the name on the left, the reset
    /// and the percentage on the right. A model is on a line of its own.
    #[allow(clippy::too_many_arguments)]
    fn vertical_bare(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        view: &RowView,
        model_line: Option<&Rect>,
        detail: &DetailLevel,
        line: &Line,
    ) {
        let band = Rect {
            h: row.h - model_line.map_or(0.0, |l| l.h),
            ..*row
        };
        self.draw_line(pixmap, &band, row, theme, m, view, detail, line, true);
        if let (Some(label), Some(at)) = (&view.model, model_line) {
            let size = m.t(model_size(detail, false));
            if let Some(text) = self.fit_label(label, size, at.w) {
                let y = at.y - m.t(2.0);
                self.draw_text(pixmap, &text, at.x, y, size, theme.meta, Align::Left, at.w);
            }
        }
    }

    /// Where the top of a line of text at `size` is above its baseline.
    fn ascent(&self, size: f32) -> f32 {
        self.font
            .horizontal_line_metrics(size)
            .map_or(size * 1.08, |l| l.ascent)
    }

    /// Compact: name | bar | pct in one line, minus whatever the width cannot
    /// carry (see `layout::row_tier`).
    #[allow(clippy::too_many_arguments)]
    fn vertical_compact(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        view: &RowView,
        tier: RowTier,
        cursor: Option<(f32, f32)>,
    ) {
        let data = &view.data;
        let size = m.t(11.0);
        let text_y = row.y + (row.h - m.t(13.0)) / 2.0;
        if tier == RowTier::Full {
            self.draw_text(
                pixmap,
                self.fit_name(view.names, size, || m.t(NAME_WIDTH)),
                row.x,
                text_y,
                size,
                theme.name_color(&view.provider),
                Align::Left,
                m.t(NAME_WIDTH),
            );
        }

        let active_metric = active_progress_metric(data, row, cursor);
        let Some(bar) = vertical_compact_bar_rect(row, tier, m) else {
            // Too narrow for a bar: the percent alone, centred on the row.
            // Providers are told apart by position, as in the taskbar panel.
            if let (ProviderStatus::Ok | ProviderStatus::Estimated, Some(pct)) =
                (&data.status, active_metric.and_then(Metric::percentage))
            {
                let (_, text_color) = row_colors(theme, data, pct);
                self.draw_text(
                    pixmap,
                    &pct_text(data, pct),
                    row.x,
                    text_y,
                    size,
                    text_color,
                    Align::Center,
                    row.w,
                );
            }
            return;
        };

        match (&data.status, active_metric.and_then(Metric::percentage)) {
            (ProviderStatus::Ok | ProviderStatus::Estimated, Some(pct)) => {
                let (bar_color, text_color) = row_colors(theme, data, pct);
                self.draw_bar(pixmap, &bar, theme, bar_color, pct);
                self.draw_text(
                    pixmap,
                    &pct_text(data, pct),
                    bar.x + bar.w + m.t(ROW_GAP),
                    text_y,
                    size,
                    text_color,
                    Align::Right,
                    m.t(PCT_WIDTH),
                );
            }
            _ => self.draw_status_inline(
                pixmap,
                &bar,
                text_y,
                theme,
                m,
                data,
                bar.w + m.t(ROW_GAP) + m.t(PCT_WIDTH),
            ),
        }
    }

    /// Medium/expanded: name+pct / bar / meta line, and the model: in what
    /// the reset leaves of Medium's meta line, or on a line of its own
    /// (`model_line`, see `layout::Shape`).
    #[allow(clippy::too_many_arguments)]
    fn vertical_medium(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        view: &RowView,
        model_line: Option<&Rect>,
        expanded: bool,
        line: &Line,
    ) {
        let (numbers, tier, cursor, forecast, reason) = (
            line.numbers,
            line.tier,
            line.cursor,
            line.forecast,
            line.reason,
        );
        let data = &view.data;
        // Hovering a greyed/estimated row answers "why": the meta line shows
        // the stored failure + the action instead of the usual metric/reset.
        let hovered_reason =
            reason.filter(|_| cursor.is_some_and(|point| point_in_rect(point, row)));
        let name_size = m.t(if expanded { 13.0 } else { 12.0 });
        let pct_size = m.t(if expanded { 15.0 } else { 13.0 });
        let meta_size = m.t(if expanded { 11.0 } else { 10.5 });
        let active_metric = active_progress_metric(data, row, cursor);
        let shown_pct = match (&data.status, active_metric.and_then(Metric::percentage)) {
            (ProviderStatus::Ok | ProviderStatus::Estimated, Some(pct)) => Some(pct),
            _ => None,
        };
        // Name and percent share this line with no clipping between them, so a
        // row too narrow for both drops the name rather than overlapping it.
        // A name with shorter forms takes the longest that ends before the
        // percent.
        if tier == RowTier::Full {
            let room = || match shown_pct.filter(|_| numbers.percent) {
                Some(pct) => row.w - self.text_width(&pct_text(data, pct), pct_size) - m.t(6.0),
                None => row.w,
            };
            self.draw_text(
                pixmap,
                self.fit_name(view.names, name_size, room),
                row.x,
                row.y + m.t(3.0),
                name_size,
                theme.name_color(&view.provider),
                Align::Left,
                row.w,
            );
        }

        let detail = if expanded {
            DetailLevel::Expanded
        } else {
            DetailLevel::Medium
        };
        // Expanded without the bar: its meta line comes up under the name.
        let bare = !numbers.bar;
        // Medium without the reset: no meta line under the bar.
        let short = !expanded && !numbers.reset;
        let bar_h = if bare {
            0.0
        } else {
            row_bar_thickness(&detail) * m.bar
        };
        let bar_y = row.y
            + m.t(if bare {
                19.0
            } else if expanded {
                26.0
            } else {
                20.0
            });
        let bar = Rect {
            x: row.x,
            y: bar_y,
            w: row.w,
            h: bar_h,
        };
        let meta_y = bar_y + bar_h + m.t(4.0);

        // What the meta line's right part holds, for a model sharing it:
        // Some(text) right-aligned ("" for nothing), None when the line is
        // taken from the left (a count, the hover reason).
        let mut meta_right: Option<String> = Some(String::new());
        match (&data.status, shown_pct) {
            (_, Some(pct)) => {
                let stale = stale_age_secs_or_estimated(data, m.countdown);
                let (bar_color, text_color) = row_colors(theme, data, pct);
                // A short row's reason takes the percentage's place.
                if let Some(text) = hovered_reason.filter(|_| short) {
                    let name_w =
                        self.text_width(view.names.first().copied().unwrap_or(""), name_size);
                    let room = (row.w - name_w - m.t(ROW_GAP)).max(0.0);
                    let text = self.fit_line(text, meta_size, room);
                    let y = row.y + m.t(3.0) + self.ascent(name_size) - self.ascent(meta_size);
                    self.draw_text(
                        pixmap,
                        &text,
                        row.x,
                        y,
                        meta_size,
                        theme.meta,
                        Align::Right,
                        row.w,
                    );
                } else if numbers.percent {
                    self.draw_text(
                        pixmap,
                        &pct_text(data, pct),
                        row.x,
                        row.y + m.t(2.0),
                        pct_size,
                        text_color,
                        Align::Right,
                        row.w,
                    );
                }
                if !bare {
                    self.draw_bar(pixmap, &bar, theme, bar_color, pct);
                }

                if let Some(text) = hovered_reason.filter(|_| !short) {
                    meta_right = None;
                    self.draw_text(
                        pixmap,
                        text,
                        row.x,
                        meta_y,
                        meta_size,
                        theme.meta,
                        Align::Left,
                        row.w,
                    );
                }
                let weekly_active_in_expanded =
                    expanded && active_metric.is_some_and(is_long_window_metric);
                if hovered_reason.is_none() && !weekly_active_in_expanded && !short {
                    if !numbers.reset {
                        // The reset left out: the meta line keeps its left
                        // part, a count or the model, and nothing right.
                        if let Some(metric) = active_metric
                            .filter(|m| !is_percent_metric(m) && !is_long_window_metric(m))
                        {
                            meta_right = None;
                            self.draw_text(
                                pixmap,
                                &metric.display_text(),
                                row.x,
                                meta_y,
                                meta_size,
                                theme.meta,
                                Align::Left,
                                row.w / 2.0,
                            );
                        }
                    } else if active_metric.is_some_and(is_long_window_metric) {
                        meta_right = self.draw_weekly_time_only(
                            pixmap,
                            row,
                            theme,
                            m,
                            active_metric,
                            meta_y,
                        );
                    } else {
                        // Meta line: the metric on the left, the reset on the right.
                        if let Some(metric) = active_metric
                            .filter(|m| !is_percent_metric(m) || is_long_window_metric(m))
                        {
                            meta_right = None;
                            self.draw_text(
                                pixmap,
                                &metric.display_text(),
                                row.x,
                                meta_y,
                                meta_size,
                                theme.meta,
                                Align::Left,
                                row.w / 2.0,
                            );
                        }
                        // The reset countdown OWNS this slot: it is the hard
                        // fact the user plans around, so a prediction never
                        // covers it. The forecast (when enabled) only fills in
                        // while there is no future reset to show.
                        let right_size = m.t(if expanded { 11.5 } else { 10.5 });
                        let right = if let Some(reset) = active_metric
                            .and_then(|m| m.reset_at)
                            .filter(|t| *t > utc::now())
                        {
                            let secs = (reset - utc::now()).as_secs();
                            let half = || row.w / 2.0;
                            let text =
                                self.countdown_text(None, secs, m.countdown, right_size, half);
                            Some((text, theme.meta))
                        } else if let Some(fc) = forecast {
                            Some((fc.to_string(), theme.high.text))
                        } else {
                            stale.map(|label| (label, theme.meta))
                        };
                        if let Some((text, color)) = right {
                            self.draw_text(
                                pixmap,
                                &text,
                                row.x + row.w / 2.0,
                                meta_y,
                                right_size,
                                color,
                                Align::Right,
                                row.w / 2.0,
                            );
                            if meta_right.is_some() {
                                meta_right = Some(text);
                            }
                        }
                    }
                }
            }
            // Status on the bar line, so it does not overlap the provider name.
            _ => {
                if matches!(data.status, ProviderStatus::Ok | ProviderStatus::Estimated)
                    && long_window_metric(data).is_some()
                {
                    // Without the bar, on the meta line it comes up to.
                    let y = if bare { meta_y } else { bar.y - m.t(5.0) };
                    self.draw_text(
                        pixmap,
                        "session unavailable",
                        bar.x,
                        y,
                        m.t(10.5),
                        theme.meta,
                        Align::Left,
                        row.w,
                    );
                } else {
                    // No bar is drawn here: the dot keeps to the text, at the
                    // place a bar of today's thickness would centre it.
                    let (y, at_y) = if bare {
                        (meta_y, meta_y + m.t(5.0))
                    } else {
                        (bar.y - m.t(5.0), bar.y)
                    };
                    let at = Rect {
                        y: at_y,
                        h: m.t(row_bar_thickness(&detail)),
                        ..bar
                    };
                    self.draw_status_inline(pixmap, &at, y, theme, m, data, row.w);
                }
            }
        }

        // Expanded shows the second (weekly) metric, without duplicating the
        // active one — suppressed while the hover reason owns the meta line.
        if expanded && hovered_reason.is_none() {
            self.draw_expanded_weekly_metric(pixmap, row, theme, m, data, meta_y, cursor);
        }

        // The model: a line of its own when the layout gave it one, else
        // (Medium only) what the right part of the meta line leaves free.
        let Some(label) = &view.model else {
            return;
        };
        // Expanded, a row in error says so on the bar line and leaves its
        // meta line empty: the label takes that line rather than leaving a
        // gap above it.
        let meta_empty = (expanded
            && !bare
            && hovered_reason.is_none()
            && !matches!(data.status, ProviderStatus::Ok | ProviderStatus::Estimated))
            || short;
        let (x, y, room, size) = match (model_line, expanded) {
            (Some(line), _) => (
                line.x,
                if meta_empty {
                    meta_y
                } else {
                    meta_y + m.t(layout::MODEL_LINE_H)
                },
                line.w,
                m.t(model_size(&detail, false)),
            ),
            (None, false) => {
                let Some(right) = meta_right else {
                    return;
                };
                let right_w = if right.is_empty() {
                    0.0
                } else {
                    self.text_width(&right, m.t(META_SIZE))
                };
                (
                    row.x,
                    meta_y,
                    layout::medium_model_room(row.w, right_w, m),
                    m.t(META_SIZE),
                )
            }
            (None, true) => return,
        };
        if let Some(text) = self.fit_label(label, size, room) {
            self.draw_text(pixmap, &text, x, y, size, theme.meta, Align::Left, room);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_expanded_weekly_metric(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        data: &ProviderData,
        y: f32,
        cursor: Option<(f32, f32)>,
    ) {
        let Some(metric) = long_window_metric(data) else {
            return;
        };
        let size = m.t(11.0);
        let text = if cursor.is_some_and(|point| point_in_rect(point, row)) {
            metric
                .reset_at
                .filter(|t| *t > utc::now())
                .map(|reset| {
                    let secs = (reset - utc::now()).as_secs();
                    let label = Some(metric.label.as_str());
                    self.countdown_text(label, secs, m.countdown, size, || row.w)
                })
                .unwrap_or_else(|| format!("{}: {}", metric.label, metric.display_text()))
        } else {
            format!("{}: {}", metric.label, metric.display_text())
        };
        self.draw_text(
            pixmap,
            &text,
            row.x,
            y,
            size,
            theme.meta,
            Align::Left,
            row.w,
        );
    }

    /// The weekly's "label: time" right-aligned on the meta line; returns
    /// what it drew ("" for nothing), for a model sharing the line.
    fn draw_weekly_time_only(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        metric: Option<&Metric>,
        y: f32,
    ) -> Option<String> {
        let Some(metric) = metric else {
            return Some(String::new());
        };
        let size = m.t(10.5);
        let text = metric
            .reset_at
            .filter(|t| *t > utc::now())
            .map(|reset| {
                let secs = (reset - utc::now()).as_secs();
                let label = Some(metric.label.as_str());
                self.countdown_text(label, secs, m.countdown, size, || row.w)
            })
            .unwrap_or_else(|| metric.label.clone());
        self.draw_text(
            pixmap,
            &text,
            row.x,
            y,
            size,
            theme.meta,
            Align::Right,
            row.w,
        );
        Some(text)
    }

    /// Bottom hint: the nearest reset.
    fn draw_hint(
        &self,
        pixmap: &mut Pixmap,
        hint: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        rows: &[RowView],
        override_text: Option<&str>,
    ) {
        if let Some(text) = override_text {
            self.draw_text(
                pixmap,
                text,
                hint.x,
                hint.y + m.t(1.0),
                m.t(10.5),
                theme.meta,
                Align::Left,
                hint.w,
            );
            return;
        }

        let next = rows
            .iter()
            .map(|r| &r.data)
            .flat_map(|p| {
                p.metrics.iter().filter_map(move |m| {
                    m.reset_at
                        .map(|t| (p.id.display_name(), m.label.as_str(), m.display_text(), t))
                })
            })
            .filter(|(_, _, _, t)| *t > utc::now())
            .min_by_key(|(_, _, _, t)| *t);

        if let Some((provider, metric, value, t)) = next {
            let secs = (t - utc::now()).as_secs();
            if secs > 0 {
                let text = compact_hint_text(provider, metric, &value, secs, m.countdown);
                self.draw_text(
                    pixmap,
                    &text,
                    hint.x,
                    hint.y + m.t(1.0),
                    m.t(10.5),
                    theme.meta,
                    Align::Left,
                    hint.w,
                );
            }
        }
    }

    // ─── Horizontal ──────────────────────────────────────────────

    /// Column: percent on top, a vertical bar, the name below, and the model
    /// on a line under the column when the layout gave it one.
    #[allow(clippy::too_many_arguments)]
    fn draw_horizontal_col(
        &self,
        pixmap: &mut Pixmap,
        col: &Rect,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        view: &RowView,
        model_line: Option<&Rect>,
        detail: &DetailLevel,
        numbers: &UiNumbers,
        cursor: Option<(f32, f32)>,
        forecast: Option<&str>,
        reason: Option<&str>,
    ) {
        let data = &view.data;
        // The narrow column only fits the action half of the hover reason
        // ("run Claude Code"), not the full cause — the vertical layouts and
        // the compact hint line carry the long form.
        let hovered_reason = reason
            .filter(|_| cursor.is_some_and(|point| point_in_rect(point, col)))
            .map(|r| r.rsplit(" — ").next().unwrap_or(r));
        if !numbers.bar {
            let at = BareAt {
                col,
                model_line,
                detail,
                numbers,
                cursor,
                forecast,
                hovered_reason,
            };
            self.draw_bare_col(pixmap, theme, m, view, &at);
            return;
        }
        // Compact has no percent: pct_size = None, the bar starts at the top.
        let (bar_w, mut top_gap, mut bottom_gap, mut pct_size, mut reset_size) = match detail {
            DetailLevel::Compact => (7.0, 4.0, 13.0, None, None),
            DetailLevel::Medium => (8.5, 19.0, 17.0, Some(12.5), None),
            DetailLevel::Expanded => (11.0, 23.0, 30.0, Some(14.5), Some(11.0)),
        };
        // The columns are their bars: what is left out above or below one
        // gives its room to the bar.
        if !numbers.percent && pct_size.is_some() {
            (pct_size, top_gap) = (None, 4.0);
        }
        if !numbers.reset && reset_size.is_some() {
            (reset_size, bottom_gap) = (None, 17.0);
        }
        // The bar's width is its thickness; the gaps above and below it hold
        // text, so they follow the text size.
        let (bar_w, top_gap, bottom_gap) = (bar_w * m.bar, m.t(top_gap), m.t(bottom_gap));
        let pct_size = pct_size.map(|s| m.t(s));
        let reset_size = reset_size.map(|s| m.t(s));
        let name_size = m.t(column_name_size(detail));
        let bar = Rect {
            x: col.x + (col.w - bar_w) / 2.0,
            y: col.y + top_gap,
            w: bar_w,
            h: col.h - top_gap - bottom_gap,
        }
        .snap();

        let active_metric = active_progress_metric(data, col, cursor);
        match (&data.status, active_metric.and_then(Metric::percentage)) {
            (ProviderStatus::Ok | ProviderStatus::Estimated, Some(pct)) => {
                let (bar_color, text_color) = row_colors(theme, data, pct);
                if let Some(pct_size) = pct_size {
                    self.draw_text(
                        pixmap,
                        &pct_text(data, pct),
                        col.x,
                        col.y + 1.0,
                        pct_size,
                        text_color,
                        Align::Center,
                        col.w,
                    );
                }
                self.draw_vertical_bar(pixmap, &bar, theme, bar_color, pct);
            }
            _ => {
                // Status dot centered in the column.
                let dot_color = self.status_dot_color(theme, &data.status);
                self.draw_dot_at(
                    pixmap,
                    col.x + col.w / 2.0,
                    col.y + col.h / 2.0 - m.t(6.0),
                    m.t(3.0),
                    dot_color,
                );
            }
        }

        // A column is made as wide as the widest provider name (layout.rs);
        // a name with shorter forms takes one no wider than that, so the
        // columns keep today's air between their names.
        let name_room = || self.text_width(WIDEST_PROVIDER_NAME, name_size).min(col.w);
        self.draw_text(
            pixmap,
            self.fit_name(view.names, name_size, name_room),
            col.x,
            col.y + col.h - m.t(if reset_size.is_some() { 23.0 } else { 12.0 }),
            name_size,
            theme.name_color(&view.provider),
            Align::Center,
            col.w,
        );

        // The model under the column, half a point smaller than the name;
        // Expanded leaves room for its reset line.
        if let (Some(label), Some(line)) = (&view.model, model_line) {
            let size = m.t(model_size(detail, true));
            if let Some(text) = self.fit_label(label, size, line.w) {
                let y = line.y + m.t(if reset_size.is_some() { 3.0 } else { 0.0 });
                self.draw_text(
                    pixmap,
                    &text,
                    line.x,
                    y,
                    size,
                    theme.meta,
                    Align::Center,
                    line.w,
                );
            }
        }

        if let Some(reset_size) = reset_size {
            let reset_y = col.y + col.h - m.t(10.0);
            // Same slot ownership as the vertical layout, but the hover reason
            // beats both (the user is asking "why"). Shown only while the
            // pointer is on the column, it may take the column's whole width,
            // and no more: a long one ("rate-limited, retrying") never runs
            // over the next column (`fit_line`); the whole of it is in the
            // Medium and Expanded rows and in Compact's hint line.
            if let Some(text) = hovered_reason {
                let text = self.fit_line(text, reset_size, col.w);
                self.draw_text(
                    pixmap,
                    &text,
                    col.x,
                    reset_y,
                    reset_size,
                    theme.meta,
                    Align::Center,
                    col.w,
                );
            } else if let Some(reset) = active_metric
                .and_then(|m| m.reset_at)
                .filter(|t| *t > utc::now())
            {
                let secs = (reset - utc::now()).as_secs();
                // Held to the names' width, so the columns keep their air.
                let text = self.countdown_text(None, secs, m.countdown, reset_size, name_room);
                self.draw_text(
                    pixmap,
                    &text,
                    col.x,
                    reset_y,
                    reset_size,
                    theme.meta,
                    Align::Center,
                    col.w,
                );
            } else if let Some(fc) = forecast {
                let fc = self.fit_line(fc, reset_size, name_room());
                self.draw_text(
                    pixmap,
                    &fc,
                    col.x,
                    reset_y,
                    reset_size,
                    theme.high.text,
                    Align::Center,
                    col.w,
                );
            }
        }
    }

    /// A column without its bar (`layout::bare_column`): the percentage in
    /// the bar's colour, the name, the reset time (or, under the pointer,
    /// why there are no numbers; or the forecast), stacked and centred, and
    /// the model under them. Without a percentage to show, the status dot
    /// takes its line (the reset's, when the percentage is off).
    fn draw_bare_col(
        &self,
        pixmap: &mut Pixmap,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        view: &RowView,
        at: &BareAt,
    ) {
        let (col, data) = (at.col, &view.data);
        let c = layout::bare_column(at.detail);
        let name_size = m.t(column_name_size(at.detail));
        let reset_size = m.t(c.reset_size);
        let name_room = || self.text_width(WIDEST_PROVIDER_NAME, name_size).min(col.w);
        let active_metric = active_progress_metric(data, col, at.cursor);
        let pct = match (&data.status, active_metric.and_then(Metric::percentage)) {
            (ProviderStatus::Ok | ProviderStatus::Estimated, Some(pct)) => Some(pct),
            _ => None,
        };
        let dot = |pixmap: &mut Pixmap, top: f32, line: f32| {
            let color = self.status_dot_color(theme, &data.status);
            self.draw_dot_at(
                pixmap,
                col.x + col.w / 2.0,
                top + line / 2.0,
                m.t(3.0),
                color,
            );
        };
        let mut y = col.y + m.t(layout::BARE_COLUMN_AIR);
        if at.numbers.percent {
            match pct {
                Some(pct) => {
                    let (_, text_color) = row_colors(theme, data, pct);
                    self.draw_text(
                        pixmap,
                        &pct_text(data, pct),
                        col.x,
                        y,
                        m.t(c.pct_size),
                        text_color,
                        Align::Center,
                        col.w,
                    );
                }
                None => dot(pixmap, y, m.t(c.pct_line)),
            }
            y += m.t(c.pct_line);
        }
        self.draw_text(
            pixmap,
            self.fit_name(view.names, name_size, name_room),
            col.x,
            y,
            name_size,
            theme.name_color(&view.provider),
            Align::Center,
            col.w,
        );
        y += m.t(c.name_line);
        if at.numbers.reset {
            let line = if let Some(text) = at.hovered_reason {
                Some((self.fit_line(text, reset_size, col.w), theme.meta))
            } else if pct.is_none() && !at.numbers.percent {
                dot(pixmap, y, m.t(c.reset_line));
                None
            } else if let Some(reset) = active_metric
                .and_then(|m| m.reset_at)
                .filter(|t| *t > utc::now())
            {
                let secs = (reset - utc::now()).as_secs();
                let text = self.countdown_text(None, secs, m.countdown, reset_size, name_room);
                Some((text, theme.meta))
            } else {
                at.forecast
                    .map(|fc| (self.fit_line(fc, reset_size, name_room()), theme.high.text))
            };
            if let Some((text, color)) = line {
                self.draw_text(
                    pixmap,
                    &text,
                    col.x,
                    y,
                    reset_size,
                    color,
                    Align::Center,
                    col.w,
                );
            }
        }
        if let (Some(label), Some(line)) = (&view.model, at.model_line) {
            let size = m.t(model_size(at.detail, true));
            if let Some(text) = self.fit_label(label, size, line.w) {
                self.draw_text(
                    pixmap,
                    &text,
                    line.x,
                    line.y,
                    size,
                    theme.meta,
                    Align::Center,
                    line.w,
                );
            }
        }
    }

    /// `text` as it fits `room` on one line at `size`: whole; else the part
    /// before its first ", " ("rate-limited, retrying" -> "rate-limited");
    /// else as many of that part's words as fit, then "…"; else as many of
    /// its characters. Empty when not even one character fits.
    fn fit_line(&self, text: &str, size: f32, room: f32) -> String {
        let fits = |t: &str| self.text_width(t, size) <= room;
        if fits(text) {
            return text.to_string();
        }
        let head = text.split_once(", ").map_or(text, |(head, _)| head);
        if fits(head) {
            return head.to_string();
        }
        let words: Vec<&str> = head.split_whitespace().collect();
        for n in (1..words.len()).rev() {
            let cut = format!("{}…", words[..n].join(" "));
            if fits(&cut) {
                return cut;
            }
        }
        let mut kept = String::new();
        for c in head.chars() {
            if !fits(&format!("{kept}{c}…")) {
                break;
            }
            kept.push(c);
        }
        if kept.trim().is_empty() {
            String::new()
        } else {
            format!("{}…", kept.trim_end())
        }
    }

    // ─── Shared elements ─────────────────────────────────────────

    /// Progress bar track + fill.
    fn draw_bar(
        &self,
        pixmap: &mut Pixmap,
        bar: &Rect,
        theme: &ComputedTheme,
        color: Color,
        pct: f32,
    ) {
        self.fill_rect(pixmap, *bar, theme.bar_track, 2.0);
        let fill_w = bar.w * (pct / 100.0).clamp(0.0, 1.0);
        if fill_w >= 1.0 {
            self.fill_rect(pixmap, Rect { w: fill_w, ..*bar }, color, 2.0);
        }
    }

    /// Vertical progress bar for the horizontal layout.
    fn draw_vertical_bar(
        &self,
        pixmap: &mut Pixmap,
        bar: &Rect,
        theme: &ComputedTheme,
        color: Color,
        pct: f32,
    ) {
        let bar = bar.snap();
        self.fill_rect(pixmap, bar, theme.bar_track, 2.0);

        let fill_h = (bar.h * (pct / 100.0).clamp(0.0, 1.0)).round();
        if fill_h < 1.0 {
            return;
        }

        let fill = Rect {
            x: bar.x,
            y: (bar.y + bar.h - fill_h).round(),
            w: bar.w,
            h: fill_h.min(bar.h),
        };

        // A partial fill is square to avoid rounded artifacts inside the track.
        let radius = if fill.h >= bar.h - 0.5 { 2.0 } else { 0.0 };
        self.fill_rect(pixmap, fill, color, radius);
    }

    /// Status dot color.
    fn status_dot_color(&self, theme: &ComputedTheme, status: &ProviderStatus) -> Color {
        match status {
            ProviderStatus::Ok => theme.low.bar,
            ProviderStatus::AuthError(_) => theme.mid.bar,
            _ => theme.meta,
        }
    }

    /// Statuses with no progress bar: a dot + text.
    #[allow(clippy::too_many_arguments)]
    fn draw_status_inline(
        &self,
        pixmap: &mut Pixmap,
        bar: &Rect,
        text_y: f32,
        theme: &ComputedTheme,
        m: &WidgetMetrics,
        data: &ProviderData,
        span: f32,
    ) {
        // The dot and the text beside it follow the text size.
        let (dot_x, dot_r, text_x, small) = (m.t(3.0), m.t(3.0), m.t(10.0), m.t(9.0));
        match &data.status {
            ProviderStatus::Ok | ProviderStatus::Estimated => {
                // A binary provider or data without a limit: a green dot + text.
                self.draw_dot_at(
                    pixmap,
                    bar.x + dot_x,
                    bar.y + bar.h / 2.0,
                    dot_r,
                    theme.low.bar,
                );
                let text = match data.metrics.first() {
                    // used=0 with no limit — the label is more informative.
                    Some(m) if m.used == 0 && m.limit.is_none() => m.label.clone(),
                    Some(m) => m.display_text(),
                    None => "OK".to_string(),
                };
                self.draw_text(
                    pixmap,
                    &text,
                    bar.x + text_x,
                    text_y,
                    small,
                    theme.meta,
                    Align::Left,
                    span - text_x,
                );
            }
            ProviderStatus::Loading => {
                self.draw_text(
                    pixmap,
                    "…",
                    bar.x,
                    text_y,
                    m.t(11.0),
                    theme.meta,
                    Align::Left,
                    span,
                );
            }
            ProviderStatus::AuthError(_) => {
                self.draw_dot_at(
                    pixmap,
                    bar.x + dot_x,
                    bar.y + bar.h / 2.0,
                    dot_r,
                    theme.mid.bar,
                );
                self.draw_text(
                    pixmap,
                    error_words(&data.status).as_deref().unwrap_or_default(),
                    bar.x + text_x,
                    text_y,
                    small,
                    theme.meta,
                    Align::Left,
                    span - text_x,
                );
            }
            ProviderStatus::NetworkError(_) => {
                self.draw_text(
                    pixmap,
                    error_words(&data.status).as_deref().unwrap_or_default(),
                    bar.x,
                    text_y,
                    small,
                    theme.meta,
                    Align::Left,
                    span,
                );
            }
            ProviderStatus::NotConfigured => {
                self.draw_text(
                    pixmap,
                    "not configured",
                    bar.x,
                    text_y,
                    small,
                    theme.meta,
                    Align::Left,
                    span,
                );
            }
        }
    }

    /// Status dot of radius `r`.
    fn draw_dot_at(&self, pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
        let mut paint = Paint::default();
        paint.set_color(color.to_skia_color());
        paint.anti_alias = true;
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, r);
        if let Some(path) = pb.finish() {
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    /// Fill a rounded rectangle.
    fn fill_rect(&self, pixmap: &mut Pixmap, rect: Rect, color: Color, radius: f32) {
        let mut paint = Paint::default();
        paint.set_color(color.to_skia_color());
        paint.anti_alias = true;

        let path = if radius > 0.0 {
            rounded_rect_path(rect, radius)
        } else {
            PathBuilder::from_rect(
                match tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h) {
                    Some(r) => r,
                    None => return,
                },
            )
        };
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Draw text via fontdue with per-pixel alpha.
    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        align: Align,
        max_width: f32,
    ) {
        let mut tl = TextLayout::new(CoordinateSystem::PositiveYDown);
        tl.append(&[self.font], &TextStyle::new(text, size, 0));

        // Text width for alignment.
        let text_w = laid_out_width(&tl);

        let offset_x = match align {
            Align::Left => x,
            Align::Center => x + ((max_width - text_w) / 2.0).max(0.0),
            Align::Right => x + (max_width - text_w).max(0.0),
        };

        let pm_w = pixmap.width() as i32;
        let pm_h = pixmap.height() as i32;

        for glyph in tl.glyphs() {
            let (metrics, bitmap) = self.font.rasterize_config(glyph.key);
            for (i, &coverage) in bitmap.iter().enumerate() {
                if coverage == 0 {
                    continue;
                }
                let gx = offset_x as i32 + glyph.x as i32 + (i % metrics.width) as i32;
                let gy = y as i32 + glyph.y as i32 + (i / metrics.width) as i32;
                if gx < 0 || gy < 0 || gx >= pm_w || gy >= pm_h {
                    continue;
                }
                // Alpha-blend the glyph over the background.
                let alpha = (coverage as u32 * color.a as u32) / 255;
                blend_pixel(pixmap, gx as u32, gy as u32, color, alpha as u8);
            }
        }
    }
}

impl Renderer {
    /// The width `draw_text` gives `text` at `size`, in pixels: the measure
    /// every "does it fit" decision uses, so a label that is judged to fit is
    /// exactly as wide when drawn.
    pub fn text_width(&self, text: &str, size: f32) -> f32 {
        let mut tl = TextLayout::new(CoordinateSystem::PositiveYDown);
        tl.append(&[self.font], &TextStyle::new(text, size, 0));
        laid_out_width(&tl)
    }

    /// The longest of a row's names that fits `room` at `size`, else its
    /// shortest. A provider's own row has one name and draws it as it always
    /// has, without measuring.
    fn fit_name(
        &self,
        names: &[&'static str],
        size: f32,
        room: impl FnOnce() -> f32,
    ) -> &'static str {
        match names {
            [] => "",
            [only] => only,
            [.., last] => {
                let room = room();
                names
                    .iter()
                    .copied()
                    .find(|n| self.text_width(n, size) <= room)
                    .unwrap_or(last)
            }
        }
    }

    /// A time left `secs` in `style` ("label: time" with a `label`), as drawn
    /// at `size` in `room`. A long-style time too wide for the room is written
    /// in the standard style instead (an Expanded column, a narrowed meta
    /// line). Only the long style measures, and asks for the room; the others
    /// cost what they always have.
    fn countdown_text(
        &self,
        label: Option<&str>,
        secs: i64,
        style: Countdown,
        size: f32,
        room: impl FnOnce() -> f32,
    ) -> String {
        let write = |style| {
            let time = format_countdown(secs, style);
            match label {
                Some(label) => format!("{label}: {time}"),
                None => time,
            }
        };
        let text = write(style);
        if style == Countdown::Long && self.text_width(&text, size) > room() {
            write(Countdown::Standard)
        } else {
            text
        }
    }

    /// The longest form of a model label that fits `room` at `size`: effort
    /// steps out first, then the version, then the model (`steps()` order).
    /// None when not even the shortest fits; a word is never cut.
    fn fit_label(&self, label: &ModelLabel, size: f32, room: f32) -> Option<String> {
        label
            .steps()
            .into_iter()
            .find(|s| self.text_width(s, size) <= room)
    }
}

/// The provider name's size under a column.
/// Where and what a column without its bar draws (`draw_bare_col`).
struct BareAt<'a> {
    col: &'a Rect,
    model_line: Option<&'a Rect>,
    detail: &'a DetailLevel,
    numbers: &'a UiNumbers,
    cursor: Option<(f32, f32)>,
    forecast: Option<&'a str>,
    /// The short reason there are no numbers, while the pointer is on the
    /// column.
    hovered_reason: Option<&'a str>,
}

fn column_name_size(detail: &DetailLevel) -> f32 {
    match detail {
        DetailLevel::Compact => 9.5,
        DetailLevel::Medium => 10.5,
        DetailLevel::Expanded => 11.5,
    }
}

/// What the widget says of a provider in error, in place of its bar: a
/// short provider message as it is ("token expired"), a long technical one
/// collapsed, a rejected key as the action. None for any other status. The
/// picker says the same of the provider's row.
pub fn error_words(status: &ProviderStatus) -> Option<std::borrow::Cow<'static, str>> {
    match status {
        ProviderStatus::AuthError(_) => Some("key rejected".into()),
        // A short provider message beats a generic "offline".
        ProviderStatus::NetworkError(msg) if msg.chars().count() <= 32 => Some(msg.clone().into()),
        ProviderStatus::NetworkError(_) => Some("offline".into()),
        _ => None,
    }
}

/// The size a model label is drawn at: under a column half a point below
/// the name; in rows Medium's meta size, Expanded's own line a little larger.
/// Compact rows draw no model.
pub(super) fn model_size(detail: &DetailLevel, columns: bool) -> f32 {
    if columns {
        column_name_size(detail) - 0.5
    } else if *detail == DetailLevel::Expanded {
        MODEL_EXPANDED_SIZE
    } else {
        META_SIZE
    }
}

/// What Medium's meta line can hold at its widest beside a model, for the
/// row's headline metric (what `vertical_medium` draws when not hovered):
/// the right-aligned text with its countdown at its longest, "" when the
/// line is empty (a status sits on the bar line instead), None when its left
/// part is taken by a count ("123 / 300"). See `layout::Shape::of`.
pub(super) fn medium_meta_widest(data: &ProviderData, style: Countdown) -> Option<String> {
    let metric = data.headline_metric();
    let longest = longest_countdown(style);
    match (&data.status, metric.and_then(Metric::percentage), metric) {
        (ProviderStatus::Ok | ProviderStatus::Estimated, Some(_), Some(m)) => {
            if is_long_window_metric(m) {
                Some(if m.reset_at.is_some() {
                    format!("{}: {longest}", m.label)
                } else {
                    m.label.clone()
                })
            } else if !is_percent_metric(m) {
                None
            } else {
                Some(longest.to_string())
            }
        }
        _ => Some(String::new()),
    }
}

/// The right edge of the last glyph of a laid-out line.
fn laid_out_width(tl: &TextLayout) -> f32 {
    tl.glyphs()
        .iter()
        .last()
        .map(|g| g.x + g.width as f32)
        .unwrap_or(0.0)
}

/// Rounded rectangle path.
fn rounded_rect_path(rect: Rect, radius: f32) -> tiny_skia::Path {
    let r = radius.min(rect.w / 2.0).min(rect.h / 2.0);
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    let mut pb = PathBuilder::new();
    // Clockwise outline with cubic arcs; k approximates a circle quadrant.
    let k = 0.5523 * r;
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish().unwrap_or_else(|| {
        // A degenerate rect cannot happen with our layouts, but never panic.
        PathBuilder::from_rect(tiny_skia::Rect::from_xywh(x, y, w.max(1.0), h.max(1.0)).unwrap())
    })
}

/// Alpha-blend a single pixel (tiny-skia premultiplied alpha format).
fn blend_pixel(pixmap: &mut Pixmap, x: u32, y: u32, color: Color, alpha: u8) {
    let idx = ((y * pixmap.width() + x) * 4) as usize;
    let data = pixmap.data_mut();
    if idx + 3 >= data.len() {
        return;
    }
    let a = alpha as u32;
    let inv = 255 - a;
    data[idx] = ((color.r as u32 * a + data[idx] as u32 * inv) / 255) as u8;
    data[idx + 1] = ((color.g as u32 * a + data[idx + 1] as u32 * inv) / 255) as u8;
    data[idx + 2] = ((color.b as u32 * a + data[idx + 2] as u32 * inv) / 255) as u8;
    data[idx + 3] = (a + data[idx + 3] as u32 * inv / 255).min(255) as u8;
}

/// Active metric for the percent/bar. Hovering a row temporarily reveals the
/// long-window limit; otherwise the row shows whatever `headline_metric`
/// decides, so the widget can never disagree with the tray or the panel.
fn active_progress_metric<'a>(
    data: &'a ProviderData,
    hover_area: &Rect,
    cursor: Option<(f32, f32)>,
) -> Option<&'a Metric> {
    if cursor.is_some_and(|point| point_in_rect(point, hover_area)) {
        return long_window_metric(data)
            .filter(|metric| metric.percentage().is_some())
            .or_else(|| data.headline_metric());
    }
    data.headline_metric()
}

/// The long-window (general) metric, if the provider reports one.
fn long_window_metric(data: &ProviderData) -> Option<&Metric> {
    data.metrics
        .iter()
        .find(|metric| is_long_window_metric(metric))
}

fn is_long_window_metric(metric: &Metric) -> bool {
    matches!(metric.window, MetricWindow::Long)
}

fn point_in_rect(point: (f32, f32), rect: &Rect) -> bool {
    point.0 >= rect.x
        && point.0 <= rect.x + rect.w
        && point.1 >= rect.y
        && point.1 <= rect.y + rect.h
}

/// The bar rectangle for a compact row, or None when the row is too narrow
/// to carry one. Never returns a negative width: the tier decides before any
/// arithmetic that could go below zero.
fn vertical_compact_bar_rect(row: &Rect, tier: RowTier, m: &WidgetMetrics) -> Option<Rect> {
    let (name_w, pct_w, gap) = (m.t(NAME_WIDTH), m.t(PCT_WIDTH), m.t(ROW_GAP));
    let (x, w) = match tier {
        // A Compact row with its bar never has a line's tier.
        RowTier::Full | RowTier::Short => {
            (row.x + name_w + gap, row.w - name_w - pct_w - gap * 2.0)
        }
        // The name column is reclaimed whole; the percent keeps its slot.
        RowTier::Nameless => (row.x, row.w - pct_w - gap),
        RowTier::PercentOnly => return None,
    };
    let h = row_bar_thickness(&DetailLevel::Compact) * m.bar;
    Some(Rect {
        x,
        y: row.y + (row.h - h) / 2.0,
        w,
        h,
    })
}

/// Hover explanation for a greyed/estimated row: the last stored fetch
/// failure plus the action that fixes it. None while the data is live —
/// hovering then keeps its usual meaning (the weekly metric).
pub fn hover_reason(data: &ProviderData, errors: &HashMap<ProviderId, String>) -> Option<String> {
    if !data.is_aged() {
        return None;
    }
    let cause = errors
        .get(&data.id)
        .cloned()
        .unwrap_or_else(|| "no fresh data".to_string());
    // Some provider messages already carry their action ("… — run Codex CLI
    // once", "sign in to Cursor again"); a transient error that retries by
    // itself ("rate-limited, retrying", "state database busy, retrying")
    // needs no action at all.
    if cause.contains("run ")
        || cause.starts_with("sign in")
        || cause.ends_with(", retrying")
        || cause == "no limited quota"
    {
        return Some(cause);
    }
    // A key or token the user pasted: running the tool would not mend it.
    let own = ["usage token", "API key", "has no Copilot access"];
    if own.iter().any(|w| cause.contains(w)) {
        return Some(format!("{cause} — paste a new one in Settings"));
    }
    let action = match data.id {
        ProviderId::Claude => "run Claude Code",
        ProviderId::Codex => "run Codex CLI",
        ProviderId::Copilot => "check gh auth",
        // Only the CLI renews the sign-in the app reads; the editor keeps
        // its own.
        ProviderId::Antigravity => "run agy",
        ProviderId::Grok => "run Grok CLI",
        // Its Sign-in in Settings mends every Cursor session.
        ProviderId::Cursor => "sign in to Cursor",
    };
    Some(format!("{cause} — {action}"))
}

fn hovered_hint_text(
    rects: &[Rect],
    rows: &[RowView],
    cursor: Option<(f32, f32)>,
    errors: &HashMap<ProviderId, String>,
    style: Countdown,
) -> Option<String> {
    let point = cursor?;
    rects.iter().zip(rows.iter()).find_map(|(row, view)| {
        let data = &view.data;
        if !point_in_rect(point, row) {
            return None;
        }

        // A greyed/estimated row explains itself; live rows show the weekly.
        if let Some(reason) = hover_reason(data, errors) {
            return Some(format!("{}: {}", data.id.display_name(), reason));
        }

        long_window_metric(data).map(|metric| {
            let prefix = format!("{} {}:", data.id.display_name(), metric.label);
            metric
                .reset_at
                .filter(|reset| *reset > utc::now())
                .map(|reset| {
                    let secs = (reset - utc::now()).as_secs();
                    format!("{prefix} {}", format_countdown(secs, style))
                })
                .unwrap_or(prefix)
        })
    })
}

fn compact_hint_text(
    provider: &str,
    metric: &str,
    value: &str,
    secs: i64,
    style: Countdown,
) -> String {
    let duration = format_countdown(secs, style);
    if metric == "Session" {
        format!("{provider}: {value} / {duration}")
    } else {
        format!("{provider} {metric}: {value} / {duration}")
    }
}

/// Whether the metric duplicates the large percent.
fn is_percent_metric(metric: &Metric) -> bool {
    matches!(&metric.unit, MetricUnit::Percent)
}

/// "~Xh Ym" forecast label for a provider: time left until the projected 100%
/// hit, derived live from the stored absolute moment — the label ticks down
/// between updates and disappears once the moment passes. The `~` marks it as
/// a prediction (like `≈` marks an estimate). Stale data gets NO forecast: a
/// projection computed before the source went quiet would keep masking the
/// honest reset countdown / "x ago" age label in the same slot.
fn forecast_label(
    forecasts: &HashMap<ProviderId, Timestamp>,
    data: &ProviderData,
    style: Countdown,
) -> Option<String> {
    if data.stale_age_secs().is_some() {
        return None;
    }
    forecasts
        .get(&data.id)
        .map(|hit| (*hit - utc::now()).as_secs())
        .filter(|s| *s > 0)
        .map(|s| format!("~{}", format_countdown(s, style)))
}

/// Format a duration: "1h 12min"; over 24 hours — day format "2d 5h".
/// `format_countdown` in the standard style: the tooltip's, the toasts'.
pub fn format_duration(secs: i64) -> String {
    format_countdown(secs, Countdown::Standard)
}

/// Format a duration in `style` (`[tune.format] countdown`):
/// standard "2d 5h", "1h 12min", "12min", "45s"; compact "2d5h", "1h12m",
/// "12m", "45s"; long as standard, but a day-long time keeps its minutes,
/// "2d 5h 12min".
pub fn format_countdown(secs: i64, style: Countdown) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    match style {
        Countdown::Standard | Countdown::Long => {
            if d > 0 && style == Countdown::Long {
                format!("{d}d {h}h {m}min")
            } else if d > 0 {
                format!("{d}d {h}h")
            } else if h > 0 {
                format!("{h}h {m}min")
            } else if m > 0 {
                format!("{m}min")
            } else {
                format!("{secs}s")
            }
        }
        Countdown::Compact => {
            if d > 0 {
                format!("{d}d{h}h")
            } else if h > 0 {
                format!("{h}h{m}m")
            } else if m > 0 {
                format!("{m}m")
            } else {
                format!("{secs}s")
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::providers::MetricWindow;
    use crate::utc::SignedDuration;

    fn row(w: f32) -> Rect {
        Rect {
            x: 11.1,
            y: 4.0,
            w,
            h: 22.0,
        }
    }

    #[test]
    fn a_nameless_row_gives_the_name_column_to_the_bar() {
        let r = row(96.0);

        let full = vertical_compact_bar_rect(&r, RowTier::Full, &WidgetMetrics::DEFAULT)
            .expect("Full keeps a bar");
        let nameless = vertical_compact_bar_rect(&r, RowTier::Nameless, &WidgetMetrics::DEFAULT)
            .expect("Nameless keeps a bar");

        assert_eq!(
            nameless.x, r.x,
            "without a name the bar starts at the row's left edge"
        );
        assert!(
            nameless.w > full.w,
            "the reclaimed name column must widen the bar: full {} vs nameless {}",
            full.w,
            nameless.w
        );
    }

    #[test]
    fn a_percent_only_row_builds_no_bar_at_all() {
        assert!(
            vertical_compact_bar_rect(&row(60.0), RowTier::PercentOnly, &WidgetMetrics::DEFAULT)
                .is_none(),
            "PercentOnly must not produce a rectangle - a negative width would be drawn"
        );
    }

    /// Design preview: renders every width step at every detail level to
    /// %TEMP% so the degradation ladder can be judged by eye — the drawing
    /// itself is not unit tested. Run explicitly:
    /// `cargo test preview_width_steps -- --ignored`.
    #[test]
    #[ignore = "writes design previews to %TEMP%"]
    fn preview_width_steps() {
        use crate::config::schema::{Layout, UIConfig, WidthScale};
        use crate::ui::layout;

        let theme = ComputedTheme::compute(&UIConfig::default());
        let renderer = Renderer::new().expect("a system font");
        let providers = vec![
            {
                let mut d = data(vec![pct_metric("session", 18, MetricWindow::Session)]);
                d.id = ProviderId::Claude;
                d
            },
            {
                let mut d = data(vec![pct_metric("weekly", 100, MetricWindow::Long)]);
                d.id = ProviderId::Codex;
                d
            },
            {
                let mut d = data(vec![pct_metric("session", 44, MetricWindow::Session)]);
                d.id = ProviderId::Copilot;
                d
            },
            {
                // The longest name: it sets the name column's width.
                let mut d = data(vec![pct_metric("session", 61, MetricWindow::Session)]);
                d.id = ProviderId::Antigravity;
                d
            },
            {
                let mut d = data(vec![pct_metric("weekly", 1, MetricWindow::Long)]);
                d.id = ProviderId::Grok;
                d
            },
        ];

        let dir = std::env::temp_dir();
        for (dname, detail) in [
            ("compact", DetailLevel::Compact),
            ("medium", DetailLevel::Medium),
            ("expanded", DetailLevel::Expanded),
        ] {
            for (wname, scale) in [
                ("100", WidthScale::Full),
                ("075", WidthScale::ThreeQuarters),
                ("050", WidthScale::Half),
            ] {
                let win = layout::compute(
                    &Layout::Vertical,
                    &detail,
                    &scale,
                    &crate::config::schema::ColumnFlow::Row,
                    &crate::ui::layout::Shape::plain(providers.len()),
                );
                let mut pm =
                    Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
                renderer.draw(
                    &mut pm,
                    &win,
                    &theme,
                    &rows_of(&providers),
                    0.85,
                    &detail,
                    None,
                    &HashMap::new(),
                    &HashMap::new(),
                );
                let path = dir.join(format!("ailimits_width_{dname}_{wname}.png"));
                std::fs::write(&path, pm.encode_png().unwrap()).unwrap();
                println!("{} -> {:?}", path.display(), win.row_tier);
            }
        }
    }

    /// Design preview for the column arrangement, same purpose as
    /// `preview_width_steps`. Run: `cargo test preview_arrangement -- --ignored`.
    #[test]
    #[ignore = "writes design previews to %TEMP%"]
    fn preview_arrangement() {
        use crate::config::schema::{ColumnFlow, Layout, UIConfig, WidthScale};
        use crate::ui::layout;

        let theme = ComputedTheme::compute(&UIConfig::default());
        let renderer = Renderer::new().expect("a system font");
        let providers = vec![
            {
                let mut d = data(vec![pct_metric("session", 18, MetricWindow::Session)]);
                d.id = ProviderId::Claude;
                d
            },
            {
                let mut d = data(vec![pct_metric("weekly", 100, MetricWindow::Long)]);
                d.id = ProviderId::Codex;
                d
            },
            {
                let mut d = data(vec![pct_metric("session", 44, MetricWindow::Session)]);
                d.id = ProviderId::Copilot;
                d
            },
            {
                let mut d = data(vec![pct_metric("session", 61, MetricWindow::Session)]);
                d.id = ProviderId::Antigravity;
                d
            },
            {
                let mut d = data(vec![pct_metric("weekly", 1, MetricWindow::Long)]);
                d.id = ProviderId::Grok;
                d
            },
            {
                let mut d = data(vec![pct_metric("Pro", 37, MetricWindow::Session)]);
                d.id = ProviderId::Cursor;
                d
            },
        ];

        let dir = std::env::temp_dir();
        for (fname, flow) in [("row", ColumnFlow::Row), ("column", ColumnFlow::Column)] {
            for (dname, detail) in [
                ("compact", DetailLevel::Compact),
                ("medium", DetailLevel::Medium),
                ("expanded", DetailLevel::Expanded),
            ] {
                let win = layout::compute(
                    &Layout::Horizontal,
                    &detail,
                    &WidthScale::Full,
                    &flow,
                    &crate::ui::layout::Shape::plain(providers.len()),
                );
                let mut pm =
                    Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
                renderer.draw(
                    &mut pm,
                    &win,
                    &theme,
                    &rows_of(&providers),
                    0.85,
                    &detail,
                    None,
                    &HashMap::new(),
                    &HashMap::new(),
                );
                let path = dir.join(format!("ailimits_flow_{fname}_{dname}.png"));
                std::fs::write(&path, pm.encode_png().unwrap()).unwrap();
                println!("{} {}x{}", path.display(), win.width, win.height);
            }
        }
    }

    /// Design preview for the rows layout with all five providers, Cursor in
    /// each of its states (live, unlimited, a token that expired before any
    /// data arrived). Run: `cargo test preview_five_providers -- --ignored`.
    #[test]
    #[ignore = "writes design previews to %TEMP%"]
    fn preview_five_providers() {
        use crate::config::schema::{ColumnFlow, Layout, UIConfig, WidthScale};
        use crate::ui::layout;

        let theme = ComputedTheme::compute(&UIConfig::default());
        let renderer = Renderer::new().expect("a system font");
        let with_id = |id: ProviderId, mut d: ProviderData| {
            d.id = id;
            d
        };
        let others = vec![
            with_id(
                ProviderId::Claude,
                data(vec![pct_metric("Session", 18, MetricWindow::Session)]),
            ),
            with_id(
                ProviderId::Codex,
                data(vec![pct_metric("Weekly", 100, MetricWindow::Long)]),
            ),
            with_id(
                ProviderId::Copilot,
                data(vec![pct_metric("Premium", 44, MetricWindow::Session)]),
            ),
            with_id(
                ProviderId::Antigravity,
                data(vec![pct_metric("Gemini", 61, MetricWindow::Long)]),
            ),
        ];
        let live = {
            let mut m = pct_metric("Pro", 37, MetricWindow::Session);
            m.reset_at = Some(utc::now() + SignedDuration::from_hours(18 * 24));
            with_id(ProviderId::Cursor, data(vec![m]))
        };
        let unlimited = with_id(
            ProviderId::Cursor,
            data(vec![Metric {
                label: "Unlimited".into(),
                used: 0,
                limit: None,
                unit: MetricUnit::Percent,
                reset_at: None,
                window: MetricWindow::Session,
                unified_pool: false,
                extra_only: false,
            }]),
        );
        let expired = ProviderData {
            status: ProviderStatus::NetworkError("token expired".into()),
            ..with_id(ProviderId::Cursor, data(vec![]))
        };

        let dir = std::env::temp_dir();
        for (cname, cursor) in [
            ("live", live),
            ("unlimited", unlimited),
            ("expired", expired),
        ] {
            let mut providers = others.clone();
            providers.push(cursor);
            for (dname, detail) in [
                ("compact", DetailLevel::Compact),
                ("medium", DetailLevel::Medium),
                ("expanded", DetailLevel::Expanded),
            ] {
                let win = layout::compute(
                    &Layout::Vertical,
                    &detail,
                    &WidthScale::Full,
                    &ColumnFlow::Row,
                    &crate::ui::layout::Shape::plain(providers.len()),
                );
                let mut pm =
                    Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
                renderer.draw(
                    &mut pm,
                    &win,
                    &theme,
                    &rows_of(&providers),
                    0.85,
                    &detail,
                    None,
                    &HashMap::new(),
                    &HashMap::new(),
                );
                let path = dir.join(format!("ailimits_five_{cname}_{dname}.png"));
                std::fs::write(&path, pm.encode_png().unwrap()).unwrap();
                println!("{} {}x{}", path.display(), win.width, win.height);
            }
        }
    }

    /// Design preview for data that stopped updating: live rows beside stale
    /// ones whose reset is still ahead, one with no reset at all and one whose
    /// window provably reset (the `≈0%` estimate). Every layout and detail
    /// level. Run: `cargo test preview_stale_rows -- --ignored`.
    #[test]
    #[ignore = "writes design previews to %TEMP%"]
    fn preview_stale_rows() {
        use crate::config::schema::{ColumnFlow, Layout, UIConfig, WidthScale};
        use crate::ui::layout;
        use MetricWindow::{Long, Session};

        let theme = ComputedTheme::compute(&UIConfig::default());
        let renderer = Renderer::new().expect("a system font");
        let metric = |label: &str, pct: u64, window, reset: Option<SignedDuration>| Metric {
            reset_at: reset.map(|d| utc::now() + d),
            ..pct_metric(label, pct, window)
        };
        let live = |id: ProviderId, m: Metric| ProviderData {
            id,
            ..data(vec![m])
        };
        // A disk-cache entry or retained data: no monotonic anchor, so the
        // wall-clock age decides.
        let stale = |id: ProviderId, m: Metric, age: SignedDuration| ProviderData {
            id,
            updated_at: utc::now() - age,
            received_at: None,
            ..data(vec![m])
        };
        let providers: Vec<ProviderData> = [
            live(
                ProviderId::Claude,
                metric("Session", 18, Session, Some(SignedDuration::from_hours(3))),
            ),
            // Stale with the reset still ahead: grey, the countdown still runs.
            stale(
                ProviderId::Codex,
                metric("Session", 72, Session, Some(SignedDuration::from_hours(2))),
                SignedDuration::from_mins(40),
            ),
            stale(
                ProviderId::Copilot,
                metric(
                    "Premium",
                    91,
                    Session,
                    Some(SignedDuration::from_hours(12 * 24)),
                ),
                SignedDuration::from_hours(2),
            ),
            // Stale with no reset known: greyed before and after.
            stale(
                ProviderId::Antigravity,
                metric("Gemini", 61, Long, None),
                SignedDuration::from_hours(5),
            ),
            // Stale with the window provably reset: the estimate.
            stale(
                ProviderId::Grok,
                metric("Weekly", 64, Long, Some(-SignedDuration::from_hours(1))),
                SignedDuration::from_hours(3),
            ),
            live(
                ProviderId::Cursor,
                metric(
                    "Pro",
                    37,
                    Session,
                    Some(SignedDuration::from_hours(18 * 24)),
                ),
            ),
        ]
        .iter()
        // What the app hands the renderer (app.rs `visible_data`).
        .map(ProviderData::aged_for_display)
        .collect();

        let dir = std::env::temp_dir();
        for (lname, arrangement) in [("rows", Layout::Vertical), ("cols", Layout::Horizontal)] {
            for (dname, detail) in [
                ("compact", DetailLevel::Compact),
                ("medium", DetailLevel::Medium),
                ("expanded", DetailLevel::Expanded),
            ] {
                let win = layout::compute(
                    &arrangement,
                    &detail,
                    &WidthScale::Full,
                    &ColumnFlow::Row,
                    &crate::ui::layout::Shape::plain(providers.len()),
                );
                let mut pm =
                    Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
                renderer.draw(
                    &mut pm,
                    &win,
                    &theme,
                    &rows_of(&providers),
                    0.85,
                    &detail,
                    None,
                    &HashMap::new(),
                    &HashMap::new(),
                );
                let path = dir.join(format!("ailimits_stale_{lname}_{dname}.png"));
                std::fs::write(&path, pm.encode_png().unwrap()).unwrap();
                println!("{} {}x{}", path.display(), win.width, win.height);
            }
        }
    }

    /// The widget's rows for `providers` with the default config: one row
    /// per provider, its data untouched (rows.rs guarantees it).
    fn rows_of(providers: &[ProviderData]) -> Vec<RowView> {
        crate::rows::visible_rows(
            &crate::config::schema::Config::default(),
            providers,
            crate::config::schema::Surface::Widget,
            &crate::models::Models::new(),
        )
    }

    /// Draw one widget the way app.rs does.
    #[allow(clippy::too_many_arguments)]
    fn baseline_widget(
        renderer: &Renderer,
        providers: &[ProviderData],
        layout: crate::config::schema::Layout,
        detail: DetailLevel,
        scale: crate::config::schema::WidthScale,
        flow: crate::config::schema::ColumnFlow,
        ui: &crate::config::schema::UIConfig,
        opacity: f32,
        cursor: Option<(f32, f32)>,
        forecasts: &HashMap<ProviderId, Timestamp>,
        errors: &HashMap<ProviderId, String>,
    ) -> Pixmap {
        let rows = rows_of(providers);
        let shape = crate::ui::layout::Shape::plain(rows.len());
        let win = crate::ui::layout::compute(&layout, &detail, &scale, &flow, &shape);
        let mut pm = Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
        let theme = ComputedTheme::compute(ui);
        renderer.draw(
            &mut pm, &win, &theme, &rows, opacity, &detail, cursor, forecasts, errors,
        );
        pm
    }

    /// The widget, pinned: every layout x detail x width/flow for every
    /// fixture set, every palette, the colour controls, both opacity ends,
    /// hover, a forecast and the empty widget. See ui/visual_baseline.rs.
    #[test]
    fn visual_baseline_widget() {
        use crate::config::schema::{ColumnFlow, Layout, Palette, UIConfig, WidthScale};
        use crate::ui::visual_baseline as vb;

        let renderer = Renderer::new().expect("a system font");
        let ui = UIConfig::default();
        let none_f = HashMap::new();
        let none_e = HashMap::new();
        let details = [
            ("compact", DetailLevel::Compact),
            ("medium", DetailLevel::Medium),
            ("expanded", DetailLevel::Expanded),
        ];
        let mut items = Vec::new();

        for (set, providers) in vb::sets() {
            for (dname, detail) in details.clone() {
                for (wname, scale) in [
                    ("100", WidthScale::Full),
                    ("075", WidthScale::ThreeQuarters),
                    ("050", WidthScale::Half),
                ] {
                    let pm = baseline_widget(
                        &renderer,
                        &providers,
                        Layout::Vertical,
                        detail.clone(),
                        scale,
                        ColumnFlow::Row,
                        &ui,
                        0.45,
                        None,
                        &none_f,
                        &none_e,
                    );
                    items.push((format!("{set}-rows-{dname}-{wname}"), pm));
                }
                for (fname, flow) in [("side", ColumnFlow::Row), ("stacked", ColumnFlow::Column)] {
                    let pm = baseline_widget(
                        &renderer,
                        &providers,
                        Layout::Horizontal,
                        detail.clone(),
                        WidthScale::Full,
                        flow,
                        &ui,
                        0.45,
                        None,
                        &none_f,
                        &none_e,
                    );
                    items.push((format!("{set}-columns-{dname}-{fname}"), pm));
                }
            }
        }

        let typical = vb::typical();
        let medium = |ui: &UIConfig, opacity: f32, cursor: Option<(f32, f32)>| {
            baseline_widget(
                &renderer,
                &typical,
                Layout::Vertical,
                DetailLevel::Medium,
                WidthScale::Full,
                ColumnFlow::Row,
                ui,
                opacity,
                cursor,
                &none_f,
                &none_e,
            )
        };
        for (pname, palette) in [
            ("default", Palette::Default),
            ("ocean", Palette::Ocean),
            ("sunset", Palette::Sunset),
            ("forest", Palette::Forest),
            ("neon", Palette::Neon),
            ("ice", Palette::Ice),
            ("rose", Palette::Rose),
            ("slate", Palette::Slate),
        ] {
            let ui = UIConfig {
                palette,
                ..UIConfig::default()
            };
            items.push((format!("palette-{pname}"), medium(&ui, 0.45, None)));
        }
        let mono = UIConfig {
            monochrome: true,
            ..UIConfig::default()
        };
        items.push(("monochrome".to_string(), medium(&mono, 0.45, None)));
        let dim = UIConfig {
            brightness: 40,
            saturation: 0,
            ..UIConfig::default()
        };
        items.push((
            "brightness40-saturation0".to_string(),
            medium(&dim, 0.45, None),
        ));
        let vivid = UIConfig {
            brightness: 100,
            saturation: 100,
            ..UIConfig::default()
        };
        items.push((
            "brightness100-saturation100".to_string(),
            medium(&vivid, 0.45, None),
        ));
        items.push(("opacity-010".to_string(), medium(&ui, 0.10, None)));
        items.push(("opacity-085".to_string(), medium(&ui, 0.85, None)));

        // Hover over the first row: the long window takes the row.
        items.push((
            "hover-medium".to_string(),
            medium(&ui, 0.45, Some((30.0, 12.0))),
        ));
        for (dname, detail) in details.clone() {
            let pm = baseline_widget(
                &renderer,
                &typical,
                Layout::Horizontal,
                detail,
                WidthScale::Full,
                ColumnFlow::Row,
                &ui,
                0.45,
                Some((20.0, 20.0)),
                &none_f,
                &none_e,
            );
            items.push((format!("hover-columns-{dname}"), pm));
        }
        // Hover over an aged row: the stored failure and its action.
        let states = vb::states();
        let mut errors = HashMap::new();
        errors.insert(ProviderId::Copilot, "offline, retrying".to_string());
        for (dname, detail) in details.clone() {
            let win = crate::ui::layout::compute(
                &Layout::Vertical,
                &detail,
                &WidthScale::Full,
                &ColumnFlow::Row,
                &crate::ui::layout::Shape::plain(states.len()),
            );
            let BodyLayout::Vertical { rows, .. } = &win.body else {
                unreachable!()
            };
            let last = rows[states.len() - 1];
            let pm = baseline_widget(
                &renderer,
                &states,
                Layout::Vertical,
                detail,
                WidthScale::Full,
                ColumnFlow::Row,
                &ui,
                0.45,
                Some((last.x + 10.0, last.y + 5.0)),
                &none_f,
                &errors,
            );
            items.push((format!("hover-aged-{dname}"), pm));
        }

        // A forecast fills the reset slot when no reset is known.
        let mut no_reset = vb::single();
        for m in &mut no_reset[0].metrics {
            m.reset_at = None;
        }
        let mut forecasts = HashMap::new();
        forecasts.insert(
            ProviderId::Claude,
            utc::now() + SignedDuration::from_secs(90 * 60 + 30),
        );
        for (dname, detail) in details.clone() {
            let pm = baseline_widget(
                &renderer,
                &no_reset,
                Layout::Vertical,
                detail,
                WidthScale::Full,
                ColumnFlow::Row,
                &ui,
                0.45,
                None,
                &forecasts,
                &none_e,
            );
            items.push((format!("forecast-{dname}"), pm));
        }

        // Nothing to show: the guidance line.
        for (dname, detail) in details {
            let pm = baseline_widget(
                &renderer,
                &[],
                Layout::Vertical,
                detail,
                WidthScale::Full,
                ColumnFlow::Row,
                &ui,
                0.45,
                None,
                &none_f,
                &none_e,
            );
            items.push((format!("empty-{dname}"), pm));
        }

        vb::check("widget", items);
    }

    // ─── Rows and models: the new looks ───────────────────────────

    /// A reset `secs` from now plus half a minute (see visual_baseline.rs).
    fn reset_in(secs: i64) -> Option<Timestamp> {
        Some(utc::now() + SignedDuration::from_secs(secs + 30))
    }

    fn pct_reset(label: &str, used: u64, window: MetricWindow, secs: i64) -> Metric {
        Metric {
            reset_at: reset_in(secs),
            ..pct_metric(label, used, window)
        }
    }

    /// Every provider with every metric an extra row can show.
    pub(crate) fn rows_fixture() -> Vec<ProviderData> {
        use MetricWindow::{Long, Session};
        const H: i64 = 3600;
        const D: i64 = 86_400;
        let of = |id: ProviderId, metrics: Vec<Metric>| ProviderData {
            id,
            ..data(metrics)
        };
        vec![
            of(
                ProviderId::Claude,
                vec![
                    pct_reset("Session", 25, Session, 2 * H + 14 * 60),
                    pct_reset("Weekly", 34, Long, 3 * D + 4 * H),
                    pct_reset("Opus", 61, Long, 3 * D + 4 * H),
                    pct_reset("Sonnet", 12, Long, 3 * D + 4 * H),
                    // Only its extra row shows it: every entry without
                    // that row is the same as before Fable was reported.
                    Metric {
                        extra_only: true,
                        ..pct_reset("Fable", 37, Long, 4 * D + 15 * H)
                    },
                    // Weeks of models Claude added later: rows made from
                    // the reply (rows.rs `offered_rows`), one name cut.
                    Metric {
                        extra_only: true,
                        ..pct_reset("Haiku", 41, Long, 4 * D + 15 * H)
                    },
                    Metric {
                        extra_only: true,
                        ..pct_reset("Mythos Preview Extended", 9, Long, 4 * D + 15 * H)
                    },
                ],
            ),
            of(
                ProviderId::Codex,
                vec![
                    pct_reset("Session", 48, Session, 4 * H + 51 * 60),
                    pct_reset("Weekly", 52, Long, 5 * D + 2 * H),
                ],
            ),
            of(
                ProviderId::Grok,
                vec![
                    pct_reset("Weekly", 12, Long, 6 * D + 2 * H),
                    pct_reset("Build", 7, Long, 6 * D + 2 * H),
                ],
            ),
            of(
                ProviderId::Antigravity,
                vec![
                    pct_reset("Gemini", 30, Long, 5 * D),
                    pct_reset("Claude/GPT", 64, Long, 5 * D),
                ],
            ),
            of(
                ProviderId::Copilot,
                vec![Metric {
                    label: "Premium".into(),
                    used: 123,
                    limit: Some(300),
                    unit: MetricUnit::Requests,
                    reset_at: reset_in(20 * D + 3 * H),
                    window: Session,
                    unified_pool: false,
                    extra_only: false,
                }],
            ),
            of(
                ProviderId::Cursor,
                vec![
                    pct_reset("Premium", 12, Session, 6 * D + 2 * H),
                    pct_reset("Grok Bot", 3, Session, 6 * D + 2 * H),
                ],
            ),
        ]
    }

    /// The parts of a model label switched on.
    #[derive(Clone, Copy)]
    pub(crate) enum Parts {
        Full,
        NoEffort,
        NoVersion,
        ModelOnly,
        EffortOnly,
    }

    /// The label each provider's tool would give, with `parts` on (rows.rs
    /// `model_label`: version needs the model, effort can go alone).
    fn label_for(id: &ProviderId, parts: Parts) -> Option<ModelLabel> {
        let (model, version, effort) = match id {
            ProviderId::Claude => ("Opus", Some("Opus 5.5"), Some("xhigh")),
            ProviderId::Codex => ("GPT", Some("GPT-6.1 Sol"), Some("high")),
            ProviderId::Grok => ("Grok", Some("Grok 4.7"), Some("high")),
            ProviderId::Antigravity => ("Claude Sonnet", Some("Claude Sonnet 4.6"), None),
            ProviderId::Copilot => ("Auto", None, Some("medium")),
            // The model chosen for its Agent in the editor.
            ProviderId::Cursor => ("Grok", Some("Grok 4.7"), Some("high")),
        };
        let (m, v, e) = match parts {
            Parts::Full => (true, true, true),
            Parts::NoEffort => (true, true, false),
            Parts::NoVersion => (true, false, true),
            Parts::ModelOnly => (true, false, false),
            Parts::EffortOnly => (false, false, true),
        };
        let label = ModelLabel {
            model: m.then(|| model.to_string()),
            with_version: version.filter(|_| m && v).map(String::from),
            effort: effort.filter(|_| e).map(String::from),
        };
        (label.model.is_some() || label.effort.is_some()).then_some(label)
    }

    /// The widget's rows for `ids` (None: the default rows), each provider's
    /// own row labelled with `parts` (None: no labels).
    pub(crate) fn widget_rows(
        providers: &[ProviderData],
        ids: Option<&[&str]>,
        parts: Option<Parts>,
    ) -> Vec<RowView> {
        let mut config = crate::config::schema::Config::default();
        config.ui.rows.widget = ids.map(|ids| ids.iter().map(|s| s.to_string()).collect());
        let mut rows = crate::rows::visible_rows(
            &config,
            providers,
            crate::config::schema::Surface::Widget,
            &crate::models::Models::new(),
        );
        for row in rows.iter_mut().filter(|r| !r.is_extra()) {
            row.model = parts.and_then(|p| label_for(&row.provider, p));
        }
        rows
    }

    /// Draw a widget of `rows` the way app.rs does: shape, layout, draw.
    pub(crate) fn rows_widget(
        renderer: &Renderer,
        rows: &[RowView],
        ui: &crate::config::schema::UIConfig,
        choose: bool,
        cursor: Option<(f32, f32)>,
    ) -> Pixmap {
        let shape =
            crate::ui::layout::Shape::of(ui, rows, choose, |t, s| renderer.text_width(t, s));
        let win = crate::ui::layout::compute(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &shape,
        );
        let mut pm = Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
        let theme = ComputedTheme::compute(ui);
        renderer.draw(
            &mut pm,
            &win,
            &theme,
            rows,
            0.45,
            &ui.detail,
            cursor,
            &HashMap::new(),
            &HashMap::new(),
        );
        pm
    }

    /// Every layout of the widget, named as in the pinned group.
    fn every_layout() -> Vec<(String, crate::config::schema::UIConfig)> {
        use crate::config::schema::{ColumnFlow, Layout, UIConfig, WidthScale};
        let mut out = Vec::new();
        for (dname, detail) in [
            ("compact", DetailLevel::Compact),
            ("medium", DetailLevel::Medium),
            ("expanded", DetailLevel::Expanded),
        ] {
            for (wname, width_scale) in [
                ("100", WidthScale::Full),
                ("075", WidthScale::ThreeQuarters),
                ("050", WidthScale::Half),
            ] {
                let ui = UIConfig {
                    detail: detail.clone(),
                    width_scale,
                    ..UIConfig::default()
                };
                out.push((format!("rows-{dname}-{wname}"), ui));
            }
            for (fname, column_flow) in [("side", ColumnFlow::Row), ("stacked", ColumnFlow::Column)]
            {
                let ui = UIConfig {
                    layout: Layout::Horizontal,
                    detail: detail.clone(),
                    column_flow,
                    ..UIConfig::default()
                };
                out.push((format!("columns-{dname}-{fname}"), ui));
            }
        }
        out
    }

    /// The new looks of rows and models, recorded as their own group (the
    /// 0.7.0 group `widget` stays as it was): extra rows with their short
    /// names, model labels at every step in every layout, Medium beside the
    /// longest reset, the Expanded and columns line, a count with a model,
    /// hover, stale data, the workshop's scenarios and the first install.
    /// What each row shows (`[ui.widget]`), every mix but the default, in
    /// every layout: the rows on one line without the bar, Compact's reset
    /// on its line, the columns that keep their bars. With models and with a
    /// row that is not live, hovered, where the reason takes the numbers.
    /// See ui/visual_baseline.rs.
    #[test]
    fn visual_baseline_widget_numbers() {
        use crate::config::schema::UiNumbers;
        use crate::ui::visual_baseline as vb;

        let renderer = Renderer::new().expect("a system font");
        let fixture = rows_fixture();
        let rows = widget_rows(&fixture, None, None);
        let models = widget_rows(&fixture, None, Some(Parts::Full));
        let mixes = [
            ("pr", (true, true, false)),
            ("pb", (true, false, true)),
            ("rb", (false, true, true)),
            ("p", (true, false, false)),
            ("r", (false, true, false)),
            ("b", (false, false, true)),
        ];
        let mut items = Vec::new();
        for (name, base) in every_layout() {
            for (mname, (percent, reset, bar)) in mixes {
                let ui = crate::config::schema::UIConfig {
                    widget: UiNumbers {
                        percent,
                        reset,
                        bar,
                    },
                    ..base.clone()
                };
                items.push((
                    format!("{mname}-{name}"),
                    rows_widget(&renderer, &rows, &ui, false, None),
                ));
                if name.ends_with("-100") || name.starts_with("columns") {
                    items.push((
                        format!("{mname}-{name}-models"),
                        rows_widget(&renderer, &models, &ui, false, None),
                    ));
                }
            }
        }
        vb::check("widget-numbers", items);
    }

    #[test]
    fn visual_baseline_widget_rows() {
        use crate::config::schema::{Layout, UIConfig, WidthScale};
        use crate::ui::visual_baseline as vb;

        let renderer = Renderer::new().expect("a system font");
        let fixture = rows_fixture();
        let mut items = Vec::new();

        // Every extra row, in every layout: names step to their short forms.
        let all = [
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
        ];
        let extra = widget_rows(&fixture, Some(&all), None);
        assert_eq!(extra.len(), all.len());
        for (name, ui) in every_layout() {
            items.push((
                format!("extra-{name}"),
                rows_widget(&renderer, &extra, &ui, false, None),
            ));
        }

        // A row whose last answers were turned away (rate-limited) and whose
        // data has aged, beside two live ones, in every layout: as it stands,
        // and with the pointer on it, when its reason takes the reset slot.
        // In a column the reason is held to the column, never written over
        // its neighbours.
        {
            use crate::ui::layout::{compute, BodyLayout, Shape};
            let mut three: Vec<ProviderData> = fixture.iter().take(3).cloned().collect();
            three[0].updated_at = utc::now() - SignedDuration::from_secs(2 * 3600 + 30 * 60 + 30);
            three[0].received_at = None;
            let rows = widget_rows(&three, None, None);
            let mut errors = HashMap::new();
            errors.insert(ProviderId::Claude, "rate-limited, retrying".to_string());
            for (name, ui) in every_layout() {
                let shape = Shape::of(&ui, &rows, false, |t, s| renderer.text_width(t, s));
                let win = compute(
                    &ui.layout,
                    &ui.detail,
                    &ui.width_scale,
                    &ui.column_flow,
                    &shape,
                );
                let first = match &win.body {
                    BodyLayout::Vertical { rows, .. } => rows[0],
                    BodyLayout::Horizontal { cols } => cols[0],
                    BodyLayout::Choose { .. } | BodyLayout::Unshown { .. } => unreachable!(),
                };
                let on_it = (first.x + first.w / 2.0, first.y + first.h / 2.0);
                for (hname, cursor) in [("", None), ("-hover", Some(on_it))] {
                    let mut pm =
                        Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
                    renderer.draw(
                        &mut pm,
                        &win,
                        &ComputedTheme::compute(&ui),
                        &rows,
                        0.45,
                        &ui.detail,
                        cursor,
                        &HashMap::new(),
                        &errors,
                    );
                    items.push((format!("rate-limited-{name}{hname}"), pm));
                }
            }
        }

        // Rows of models Claude added later, in every layout, beside
        // Fable's: named and stepped down as the catalogue's rows are.
        let added = [
            "claude",
            "claude.fable",
            "claude.haiku",
            "claude.mythos-preview-extended",
        ];
        let new_models = widget_rows(&fixture, Some(&added), None);
        assert_eq!(new_models.len(), added.len());
        for (name, ui) in every_layout() {
            items.push((
                format!("new-models-{name}"),
                rows_widget(&renderer, &new_models, &ui, false, None),
            ));
        }

        // Claude's Fable week, chosen: a row of its own under Claude.
        let fable = widget_rows(
            &fixture,
            Some(&["claude", "claude.fable", "codex", "grok"]),
            None,
        );
        assert_eq!(fable.len(), 4);
        for (name, ui) in every_layout() {
            items.push((
                format!("fable-{name}"),
                rows_widget(&renderer, &fable, &ui, false, None),
            ));
        }

        // Model labels: the whole label in every layout, each step of it in
        // Medium and Expanded rows and the Medium columns.
        let full = widget_rows(&fixture, None, Some(Parts::Full));
        for (name, ui) in every_layout() {
            items.push((
                format!("labels-full-{name}"),
                rows_widget(&renderer, &full, &ui, false, None),
            ));
        }
        for (pname, parts) in [
            ("noeffort", Parts::NoEffort),
            ("noversion", Parts::NoVersion),
            ("model", Parts::ModelOnly),
            ("effort", Parts::EffortOnly),
        ] {
            let rows = widget_rows(&fixture, None, Some(parts));
            for (name, ui) in every_layout() {
                let wanted = name.starts_with("rows-medium")
                    || name.starts_with("rows-expanded")
                    || name == "columns-medium-side";
                if wanted {
                    let pm = rows_widget(&renderer, &rows, &ui, false, None);
                    items.push((format!("labels-{pname}-{name}"), pm));
                }
            }
        }

        // Medium beside the longest reset, with a name at the slot's limit
        // and a weekly whose "Weekly: 23h 59min" leaves too little when
        // narrow: the name shows whole, the effort steps out first.
        let measure = |t: &str, s: f32| renderer.text_width(t, s);
        let at_limit = crate::rows::fit_name("Sonnet 4.6 Extended Thinking", measure);
        let longest = vec![
            ProviderData {
                id: ProviderId::Claude,
                ..data(vec![pct_reset(
                    "Session",
                    25,
                    MetricWindow::Session,
                    23 * 3600 + 59 * 60,
                )])
            },
            ProviderData {
                id: ProviderId::Grok,
                ..data(vec![pct_reset(
                    "Weekly",
                    12,
                    MetricWindow::Long,
                    23 * 3600 + 59 * 60,
                )])
            },
        ];
        let mut longest_rows = widget_rows(&longest, None, None);
        for row in &mut longest_rows {
            row.model = Some(ModelLabel {
                model: Some(at_limit.clone()),
                with_version: None,
                effort: Some("xhigh".into()),
            });
        }
        for (wname, width_scale) in [
            ("100", WidthScale::Full),
            ("075", WidthScale::ThreeQuarters),
            ("050", WidthScale::Half),
        ] {
            let ui = UIConfig {
                detail: DetailLevel::Medium,
                width_scale,
                ..UIConfig::default()
            };
            let pm = rows_widget(&renderer, &longest_rows, &ui, false, None);
            items.push((format!("longest-reset-medium-{wname}"), pm));
        }

        // A count (Copilot's requests on the meta line's left) with a model:
        // the model takes a line of its own even in Medium.
        let count = widget_rows(&fixture, Some(&["copilot", "codex"]), Some(Parts::Full));
        for (dname, detail) in [
            ("medium", DetailLevel::Medium),
            ("expanded", DetailLevel::Expanded),
        ] {
            for (wname, width_scale) in [("100", WidthScale::Full), ("050", WidthScale::Half)] {
                let ui = UIConfig {
                    detail: detail.clone(),
                    width_scale,
                    ..UIConfig::default()
                };
                let pm = rows_widget(&renderer, &count, &ui, false, None);
                items.push((format!("count-model-{dname}-{wname}"), pm));
            }
        }

        // Hover over a labelled row: the weekly takes the meta line and the
        // model steps down beside it.
        let medium = |width_scale| UIConfig {
            detail: DetailLevel::Medium,
            width_scale,
            ..UIConfig::default()
        };
        for (wname, scale) in [("100", WidthScale::Full), ("050", WidthScale::Half)] {
            let pm = rows_widget(&renderer, &full, &medium(scale), false, Some((30.0, 12.0)));
            items.push((format!("hover-labels-medium-{wname}"), pm));
        }

        // Every honest state with a label: loading, sign-in trouble, offline
        // (the meta line is free), estimated, aged (the age owns the slot).
        let aged_rows = widget_rows(&vb::states(), None, Some(Parts::Full));
        for (dname, detail) in [
            ("medium", DetailLevel::Medium),
            ("expanded", DetailLevel::Expanded),
        ] {
            let ui = UIConfig {
                detail,
                ..UIConfig::default()
            };
            let pm = rows_widget(&renderer, &aged_rows, &ui, false, None);
            items.push((format!("states-labels-{dname}"), pm));
        }

        // The workshop's scenarios: two providers and one by model; a small
        // widget with only the model on.
        let mix = widget_rows(
            &fixture,
            Some(&["codex", "copilot", "claude.opus", "claude.sonnet"]),
            None,
        );
        let small = widget_rows(&fixture, Some(&["claude", "codex"]), Some(Parts::ModelOnly));
        for (dname, detail) in [
            ("compact", DetailLevel::Compact),
            ("medium", DetailLevel::Medium),
            ("expanded", DetailLevel::Expanded),
        ] {
            let ui = UIConfig {
                detail,
                ..UIConfig::default()
            };
            items.push((
                format!("scenario-mix-{dname}"),
                rows_widget(&renderer, &mix, &ui, false, None),
            ));
            items.push((
                format!("scenario-small-{dname}"),
                rows_widget(&renderer, &small, &ui, false, None),
            ));
        }

        // A fresh install with nothing chosen, whatever the layout.
        for (lname, layout) in [("rows", Layout::Vertical), ("columns", Layout::Horizontal)] {
            let ui = UIConfig {
                layout,
                ..UIConfig::default()
            };
            items.push((
                format!("first-install-{lname}"),
                rows_widget(&renderer, &[], &ui, true, None),
            ));
        }

        // Rows chosen, none with data (an extra row the plan lacks), while
        // providers run: what the widget says instead of "set up
        // providers", whatever the layout.
        for (lname, layout) in [("rows", Layout::Vertical), ("columns", Layout::Horizontal)] {
            let ui = UIConfig {
                layout,
                ..UIConfig::default()
            };
            let shape = crate::ui::layout::Shape::of(&ui, &[], false, measure).with_unshown(true);
            let win = crate::ui::layout::compute(
                &ui.layout,
                &ui.detail,
                &ui.width_scale,
                &ui.column_flow,
                &shape,
            );
            let mut pm = Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
            renderer.draw(
                &mut pm,
                &win,
                &ComputedTheme::compute(&ui),
                &[],
                0.45,
                &ui.detail,
                None,
                &HashMap::new(),
                &HashMap::new(),
            );
            items.push((format!("unshown-{lname}"), pm));
        }

        vb::check("widget-rows", items);
    }

    // ─── [tune.widget] and [tune.format]: the tuned looks ─────────

    /// Draw a widget of `rows` the way app.rs does with `tune`: the tuned
    /// shape, layout and theme.
    fn tuned_widget(
        renderer: &Renderer,
        rows: &[RowView],
        ui: &crate::config::schema::UIConfig,
        tune: &crate::config::tune::Tune,
        choose: bool,
        cursor: Option<(f32, f32)>,
    ) -> Pixmap {
        use crate::ui::layout::{compute_tuned, Shape};
        let m = WidgetMetrics::from_tune(tune);
        let shape = Shape::of_tuned(ui, &m, rows, choose, |t, s| renderer.text_width(t, s));
        let win = compute_tuned(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &shape,
            &m,
        );
        let mut pm = Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
        let theme = ComputedTheme::tuned(ui, tune);
        renderer.draw(
            &mut pm,
            &win,
            &theme,
            rows,
            0.45,
            &ui.detail,
            cursor,
            &HashMap::new(),
            &HashMap::new(),
        );
        pm
    }

    /// The knobs of `[tune.widget]` and `[tune.format]` away from their
    /// defaults, each alone and a few together, in the layouts they touch.
    /// A group of its own: the pinned `widget` group holds the default look.
    #[test]
    fn visual_baseline_widget_tune() {
        use crate::config::schema::{ColumnFlow, Layout, UIConfig, WidthScale};
        use crate::config::tune::{Tune, TuneFormat, TuneWidget};
        use crate::ui::visual_baseline as vb;
        use MetricWindow::{Long, Session};

        let renderer = Renderer::new().expect("a system font");
        let tune = |text: &str| -> Tune { toml::from_str(text).expect("a [tune] table") };
        let at = |layout: Layout, detail: DetailLevel, flow: ColumnFlow| UIConfig {
            layout,
            detail,
            column_flow: flow,
            ..UIConfig::default()
        };
        let rows_at = |detail| at(Layout::Vertical, detail, ColumnFlow::Row);
        let side = |detail| at(Layout::Horizontal, detail, ColumnFlow::Row);
        let stacked = |detail| at(Layout::Horizontal, detail, ColumnFlow::Column);
        use DetailLevel::{Compact, Expanded, Medium};

        // One usage at each default level: low, mid, high.
        const H: i64 = 3600;
        const D: i64 = 86_400;
        let spread = rows_of(&[
            ProviderData {
                id: ProviderId::Claude,
                ..data(vec![pct_reset("Session", 25, Session, 2 * H + 14 * 60)])
            },
            ProviderData {
                id: ProviderId::Codex,
                ..data(vec![pct_reset("Session", 65, Session, 4 * H + 51 * 60)])
            },
            ProviderData {
                id: ProviderId::Grok,
                ..data(vec![pct_reset("Weekly", 92, Long, 3 * D + 4 * H + 12 * 60)])
            },
        ]);
        let typical = rows_of(&vb::typical());
        let states = rows_of(&vb::states());
        let labelled = widget_rows(&rows_fixture(), None, Some(Parts::Full));

        let mut items = Vec::new();
        let mut push = |name: &str, rows: &[RowView], ui: &UIConfig, t: &Tune| {
            items.push((
                name.to_string(),
                tuned_widget(&renderer, rows, ui, t, false, None),
            ));
        };

        // Text size: smaller and larger, in every layout it reshapes.
        for (tname, text) in [("080", 0.8), ("130", 1.3), ("150", 1.5)] {
            let t = Tune {
                widget: TuneWidget {
                    text_scale: Some(text),
                    ..TuneWidget::default()
                },
                ..Tune::default()
            };
            for (dname, detail) in [
                ("compact", Compact),
                ("medium", Medium),
                ("expanded", Expanded),
            ] {
                push(
                    &format!("text{tname}-rows-{dname}"),
                    &typical,
                    &rows_at(detail.clone()),
                    &t,
                );
                push(
                    &format!("text{tname}-columns-{dname}"),
                    &typical,
                    &side(detail),
                    &t,
                );
            }
            push(
                &format!("text{tname}-states-medium"),
                &states,
                &rows_at(Medium),
                &t,
            );
            push(
                &format!("text{tname}-labels-medium"),
                &labelled,
                &rows_at(Medium),
                &t,
            );
        }

        // Bar thickness, row gap, padding: each alone.
        let bars = tune("[widget]\nbar_scale = 2.5\n");
        let thin = tune("[widget]\nbar_scale = 0.5\n");
        let gap = tune("[widget]\nrow_gap = 8\n");
        let roomy = tune("[widget]\npadding_scale = 2.0\n");
        let tight = tune("[widget]\npadding_scale = 0.0\n");
        for (dname, detail) in [
            ("compact", Compact),
            ("medium", Medium),
            ("expanded", Expanded),
        ] {
            push(
                &format!("bar250-rows-{dname}"),
                &typical,
                &rows_at(detail.clone()),
                &bars,
            );
            push(
                &format!("bar250-columns-{dname}"),
                &typical,
                &side(detail.clone()),
                &bars,
            );
            push(
                &format!("bar050-rows-{dname}"),
                &typical,
                &rows_at(detail.clone()),
                &thin,
            );
            push(
                &format!("gap8-rows-{dname}"),
                &typical,
                &rows_at(detail.clone()),
                &gap,
            );
            push(
                &format!("gap8-columns-side-{dname}"),
                &typical,
                &side(detail.clone()),
                &gap,
            );
            push(
                &format!("gap8-columns-stacked-{dname}"),
                &typical,
                &stacked(detail.clone()),
                &gap,
            );
            push(
                &format!("padding200-rows-{dname}"),
                &typical,
                &rows_at(detail.clone()),
                &roomy,
            );
            push(
                &format!("padding200-columns-{dname}"),
                &typical,
                &side(detail.clone()),
                &roomy,
            );
            push(
                &format!("padding000-rows-{dname}"),
                &typical,
                &rows_at(detail),
                &tight,
            );
        }
        push("bar250-states-medium", &states, &rows_at(Medium), &bars);

        // Colours: the levels, the providers' accents, both ignored in
        // Monochrome; a colour that does not parse keeps the palette's.
        let colours = tune(
            "[widget]\nlow = \"#3fb950\"\nmid = \"#d29922\"\nhigh = \"#f85149\"\n\
             provider_colors = { claude = \"#d97757\", codex = \"#10a37f\", grok = \"#8b949e\" }\n",
        );
        let half_bad = tune("[widget]\nlow = \"green\"\nhigh = \"#ff00ff\"\n");
        let mono = UIConfig {
            monochrome: true,
            ..rows_at(Medium)
        };
        for (dname, detail) in [
            ("compact", Compact),
            ("medium", Medium),
            ("expanded", Expanded),
        ] {
            push(
                &format!("colours-rows-{dname}"),
                &spread,
                &rows_at(detail.clone()),
                &colours,
            );
            push(
                &format!("colours-columns-{dname}"),
                &spread,
                &side(detail),
                &colours,
            );
        }
        push("colours-monochrome-medium", &spread, &mono, &colours);
        push(
            "colours-invalid-low-medium",
            &spread,
            &rows_at(Medium),
            &half_bad,
        );
        let dim = UIConfig {
            brightness: 50,
            ..rows_at(Medium)
        };
        push("colours-brightness50-medium", &spread, &dim, &colours);

        // Thresholds: the same usages read higher, and lower.
        let low_bar = Tune {
            format: TuneFormat {
                mid_at: Some(20.0),
                high_at: Some(50.0),
                ..TuneFormat::default()
            },
            ..Tune::default()
        };
        let high_bar = tune("[format]\nmid_at = 70\nhigh_at = 95\n");
        for (tname, t) in [("20-50", &low_bar), ("70-95", &high_bar)] {
            push(
                &format!("levels{tname}-rows-medium"),
                &spread,
                &rows_at(Medium),
                t,
            );
            push(
                &format!("levels{tname}-columns-medium"),
                &spread,
                &side(Medium),
                t,
            );
        }

        // The countdown styles where a countdown shows: Medium's and
        // Expanded's meta line, Expanded's columns, Compact's hint line,
        // the age of an aged row.
        for style in ["compact", "long"] {
            let t = tune(&format!("[format]\ncountdown = \"{style}\"\n"));
            push(
                &format!("countdown-{style}-rows-compact"),
                &typical,
                &rows_at(Compact),
                &t,
            );
            push(
                &format!("countdown-{style}-rows-medium"),
                &typical,
                &rows_at(Medium),
                &t,
            );
            push(
                &format!("countdown-{style}-rows-expanded"),
                &typical,
                &rows_at(Expanded),
                &t,
            );
            push(
                &format!("countdown-{style}-columns-expanded"),
                &typical,
                &side(Expanded),
                &t,
            );
            push(
                &format!("countdown-{style}-states-medium"),
                &states,
                &rows_at(Medium),
                &t,
            );
            push(
                &format!("countdown-{style}-labels-medium"),
                &labelled,
                &rows_at(Medium),
                &t,
            );
        }

        // Everything at once, and the two-line box of a first install.
        let all = tune(
            "[widget]\ntext_scale = 1.2\nbar_scale = 2\nrow_gap = 4\npadding_scale = 1.5\n\
             low = \"#3fb950\"\nmid = \"#d29922\"\nhigh = \"#f85149\"\n\
             provider_colors = { claude = \"#d97757\" }\n\
             [format]\nmid_at = 40\nhigh_at = 60\ncountdown = \"compact\"\n",
        );
        for (dname, detail) in [
            ("compact", Compact),
            ("medium", Medium),
            ("expanded", Expanded),
        ] {
            push(
                &format!("all-rows-{dname}"),
                &spread,
                &rows_at(detail.clone()),
                &all,
            );
            push(
                &format!("all-columns-{dname}"),
                &spread,
                &side(detail.clone()),
                &all,
            );
            push(
                &format!("all-labels-{dname}"),
                &labelled,
                &rows_at(detail),
                &all,
            );
        }
        let half = UIConfig {
            width_scale: WidthScale::Half,
            ..rows_at(Medium)
        };
        push("all-labels-medium-050", &labelled, &half, &all);
        drop(push);
        items.push((
            "all-first-install".to_string(),
            tuned_widget(&renderer, &[], &rows_at(Medium), &all, true, None),
        ));

        vb::check("widget-tune", items);
    }

    /// What a repaint costs: the rows built from the data (as app.rs does on
    /// every repaint) and the draw, with the default config and with every
    /// label on; and the shape, which the app builds on a data update only.
    /// Run: `cargo test --release time_draw -- --ignored --nocapture`.
    #[test]
    #[ignore = "a timing, for --release by hand"]
    fn time_draw() {
        use crate::config::schema::{ColumnFlow, Config, Layout, Surface, UIConfig, WidthScale};
        use crate::ui::layout::{compute, Shape};
        use std::time::Instant;

        let renderer = Renderer::new().expect("a system font");
        let providers = crate::ui::visual_baseline::typical();
        let config = Config::default();
        let models = crate::models::Models::new();
        let measure = |t: &str, s: f32| renderer.text_width(t, s);
        let n = 2000;
        for (name, layout, detail) in [
            ("rows-compact", Layout::Vertical, DetailLevel::Compact),
            ("rows-medium", Layout::Vertical, DetailLevel::Medium),
            ("rows-expanded", Layout::Vertical, DetailLevel::Expanded),
            ("columns-medium", Layout::Horizontal, DetailLevel::Medium),
        ] {
            let ui = UIConfig {
                layout,
                detail: detail.clone(),
                ..UIConfig::default()
            };
            let theme = ComputedTheme::compute(&ui);
            for labelled in [false, true] {
                let build = || {
                    // app.rs: `visible_data` (a fresh Vec), then the rows.
                    let visible: Vec<ProviderData> = providers.to_vec();
                    let mut rows =
                        crate::rows::visible_rows(&config, &visible, Surface::Widget, &models);
                    if labelled {
                        for row in &mut rows {
                            row.model = label_for(&row.provider, Parts::Full);
                        }
                    }
                    rows
                };
                let rows = build();
                let shape = Shape::of(&ui, &rows, false, measure);
                let win = compute(
                    &ui.layout,
                    &ui.detail,
                    &WidthScale::Full,
                    &ColumnFlow::Row,
                    &shape,
                );
                let mut pm =
                    Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
                let (none_f, none_e) = (HashMap::new(), HashMap::new());
                let started = Instant::now();
                for _ in 0..n {
                    let rows = build();
                    renderer.draw(
                        &mut pm, &win, &theme, &rows, 0.45, &detail, None, &none_f, &none_e,
                    );
                }
                let per_draw = started.elapsed().as_secs_f64() * 1e6 / n as f64;
                let started = Instant::now();
                for _ in 0..n {
                    std::hint::black_box(Shape::of(&ui, &rows, false, measure));
                }
                let per_shape = started.elapsed().as_secs_f64() * 1e6 / n as f64;
                println!(
                    "{name}{}: rows + draw {per_draw:.1} us, shape {per_shape:.1} us",
                    if labelled { " labelled" } else { "" }
                );
            }
        }
    }

    // ─── Model names: the places they are drawn ───────────────────

    /// One provider's own row of every meta-line form, each with `label`:
    /// a session percent (the reset on the right), weeklies of two label
    /// lengths ("Weekly: 23h 59min" on the right), a count on the left, and
    /// an error (nothing on the meta line).
    fn rows_of_every_form(label: &ModelLabel) -> Vec<RowView> {
        use crate::providers::MetricWindow::{Long, Session};
        let hours = |h: i64| Some(utc::now() + SignedDuration::from_hours(h));
        let with_reset = |label: &str, pct: u64, window, h: i64| Metric {
            reset_at: hours(h),
            ..pct_metric(label, pct, window)
        };
        let of = |id: ProviderId, metrics: Vec<Metric>| ProviderData {
            id,
            ..data(metrics)
        };
        let providers = vec![
            of(
                ProviderId::Claude,
                vec![
                    with_reset("Session", 25, Session, 23),
                    with_reset("Weekly", 34, Long, 80),
                ],
            ),
            of(ProviderId::Grok, vec![with_reset("Weekly", 12, Long, 23)]),
            of(
                ProviderId::Antigravity,
                vec![with_reset("Gemini", 30, Long, 23)],
            ),
            of(
                ProviderId::Copilot,
                vec![Metric {
                    label: "Premium".into(),
                    used: 123,
                    limit: Some(300),
                    unit: MetricUnit::Requests,
                    reset_at: hours(23),
                    window: Session,
                    unified_pool: false,
                    extra_only: false,
                }],
            ),
            ProviderData {
                status: ProviderStatus::AuthError("token expired".into()),
                ..of(ProviderId::Codex, vec![])
            },
        ];
        let mut rows = rows_of(&providers);
        for row in &mut rows {
            row.model = Some(label.clone());
        }
        rows
    }

    /// Every place the widget draws a model name, as (where, room, size):
    /// every layout x detail x width x column flow, Medium's meta line beside
    /// its right part at its widest (the longest reset, "23h 59min"), the
    /// lines of their own. Built from the same layout, shape and size
    /// functions the drawing uses.
    fn model_name_places(renderer: &Renderer) -> Vec<(String, f32, f32)> {
        use crate::config::schema::{ColumnFlow, Layout, UIConfig, WidthScale};
        use crate::ui::layout::{compute, medium_model_room, Shape};

        let measure = |t: &str, s: f32| renderer.text_width(t, s);
        let label = ModelLabel {
            model: Some("Opus".into()),
            with_version: None,
            effort: None,
        };
        let rows = rows_of_every_form(&label);
        let details = [
            DetailLevel::Compact,
            DetailLevel::Medium,
            DetailLevel::Expanded,
        ];
        let mut configs = Vec::new();
        for detail in &details {
            for scale in [
                WidthScale::Full,
                WidthScale::ThreeQuarters,
                WidthScale::Half,
            ] {
                configs.push(UIConfig {
                    layout: Layout::Vertical,
                    detail: detail.clone(),
                    width_scale: scale,
                    ..UIConfig::default()
                });
            }
            for flow in [ColumnFlow::Row, ColumnFlow::Column] {
                configs.push(UIConfig {
                    layout: Layout::Horizontal,
                    detail: detail.clone(),
                    column_flow: flow,
                    ..UIConfig::default()
                });
            }
        }

        let mut places = Vec::new();
        for ui in &configs {
            let shape = Shape::of(ui, &rows, false, measure);
            let win = compute(
                &ui.layout,
                &ui.detail,
                &ui.width_scale,
                &ui.column_flow,
                &shape,
            );
            let columns = ui.layout == Layout::Horizontal;
            let rects = match &win.body {
                BodyLayout::Vertical { rows, .. } => rows.clone(),
                BodyLayout::Horizontal { cols } => cols.clone(),
                BodyLayout::Choose { .. } | BodyLayout::Unshown { .. } => unreachable!(),
            };
            assert_eq!(rects.len(), rows.len());
            for (i, row) in rows.iter().enumerate() {
                let at = format!(
                    "{:?} {:?} {:?} {:?} {}",
                    ui.layout, ui.detail, ui.width_scale, ui.column_flow, row.id
                );
                let size = model_size(&ui.detail, columns);
                match (win.model_lines[i], &ui.detail, columns) {
                    (Some(line), _, _) => places.push((at, line.w, size)),
                    (None, _, true) => panic!("a column with a model has its line: {at}"),
                    (None, DetailLevel::Compact, false) => {}
                    (None, DetailLevel::Expanded, false) => {
                        panic!("an Expanded row with a model has its line: {at}")
                    }
                    (None, DetailLevel::Medium, false) => {
                        let right = medium_meta_widest(&row.data, Countdown::Standard)
                            .expect("a meta line taken from the left gets a line of its own");
                        let room = medium_model_room(
                            rects[i].w,
                            measure(&right, META_SIZE),
                            &WidgetMetrics::DEFAULT,
                        );
                        places.push((format!("{at} beside {right:?}"), room, size));
                    }
                }
            }
        }
        places
    }

    /// The two-line boxes' texts fit the box: the first install's and the
    /// one for rows chosen without data.
    #[test]
    fn the_two_line_boxes_fit_their_text() {
        let renderer = Renderer::new().expect("a system font");
        let ui = crate::config::schema::UIConfig::default();
        let measure = |t: &str, s: f32| renderer.text_width(t, s);
        let shape = crate::ui::layout::Shape::of(&ui, &[], false, measure).with_unshown(true);
        let win = crate::ui::layout::compute(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &shape,
        );
        let BodyLayout::Unshown { area } = win.body else {
            panic!("the unshown box");
        };
        for (title, line) in [(CHOOSE_TITLE, CHOOSE_LINE), (UNSHOWN_TITLE, UNSHOWN_LINE)] {
            assert!(renderer.text_width(title, 12.0) <= area.w, "{title}");
            assert!(renderer.text_width(line, META_SIZE) <= area.w, "{line}");
        }
    }

    /// A line held to a column: whole when it fits, then the part before its
    /// comma, then whole words and "…", then characters; never wider than
    /// its room. The rate-limit reason under an Expanded column is the case
    /// that ran over the next column.
    #[test]
    fn a_line_held_to_a_column_never_runs_past_it() {
        let r = Renderer::new().expect("a system font");
        let size = 11.0;
        let text = "rate-limited, retrying";
        let w = |t: &str| r.text_width(t, size);
        assert_eq!(r.fit_line(text, size, w(text)), text);
        assert_eq!(r.fit_line(text, size, w("rate-limited")), "rate-limited");
        assert_eq!(
            r.fit_line("state database busy, retrying", size, w("state database…")),
            "state database…"
        );
        let cut = r.fit_line(text, size, w("rate-l…"));
        assert_eq!(cut, "rate-l…");
        assert_eq!(r.fit_line(text, size, 1.0), "");
        // Whatever the room, what comes back fits it.
        for room in (4..120).map(|x| x as f32) {
            let line = r.fit_line(text, size, room);
            assert!(w(&line) <= room, "{line:?} in {room}");
        }
    }

    /// `rows::NAME_SLOTS` is what a renamed model is held to. Every place
    /// above must be at least as wide as the slot of its size, so a name that
    /// passes `rows::name_fits` always shows in full; and each slot must be
    /// the narrowest place of its size, not a guess well under it.
    #[test]
    fn every_model_name_place_holds_its_slot() {
        let renderer = Renderer::new().expect("a system font");
        let places = model_name_places(&renderer);
        let mut narrowest: Vec<(f32, f32, String)> = Vec::new();
        for (at, room, size) in &places {
            match narrowest.iter_mut().find(|(s, _, _)| s == size) {
                Some(n) if *room < n.1 => *n = (*size, *room, at.clone()),
                Some(_) => {}
                None => narrowest.push((*size, *room, at.clone())),
            }
        }
        for (size, room, at) in &narrowest {
            println!("narrowest at {size} px: {room:.2} px ({at})");
        }
        for (size, room, at) in &narrowest {
            let slot = crate::rows::NAME_SLOTS
                .iter()
                .find(|(_, s)| s == size)
                .unwrap_or_else(|| panic!("no name slot for {size} px, the size at {at}"));
            assert!(
                slot.0 <= *room,
                "{at}: {room:.2} px at {size} px is narrower than the slot {} px",
                slot.0
            );
            assert!(
                slot.0 > room - 0.5,
                "the slot at {size} px ({} px) is well under its narrowest place {room:.2} px ({at})",
                slot.0
            );
        }
        for (w, size) in crate::rows::NAME_SLOTS {
            assert!(
                narrowest.iter().any(|(s, _, _)| s == size),
                "the slot ({w}, {size}) matches no place a model name is drawn"
            );
        }
    }

    /// The meta line's room is decided against `LONGEST_RESET`: no countdown
    /// `format_duration` writes may be wider; nor, in each style, than
    /// `longest_countdown` of that style.
    #[test]
    fn the_longest_reset_is_the_widest_countdown() {
        let renderer = Renderer::new().expect("a system font");
        let mut secs: Vec<i64> = (0..60).collect();
        secs.extend((1..60).map(|m| m * 60));
        secs.extend((1..24).flat_map(|h| (0..60).map(move |m| h * 3600 + m * 60)));
        secs.extend((1..100).flat_map(|d| {
            (0..24).flat_map(move |h| [0, 59].map(|m| d * 86_400 + h * 3600 + m * 60))
        }));
        assert_eq!(longest_countdown(Countdown::Standard), LONGEST_RESET);
        for style in [Countdown::Standard, Countdown::Compact, Countdown::Long] {
            let longest = longest_countdown(style);
            let widest = renderer.text_width(longest, META_SIZE);
            for &s in &secs {
                let text = format_countdown(s, style);
                assert!(
                    renderer.text_width(&text, META_SIZE) <= widest,
                    "{style:?}: {text:?} is wider than {longest:?}"
                );
            }
            assert!(
                secs.iter().any(|&s| format_countdown(s, style) == longest),
                "{style:?}: {longest:?} is written for some time"
            );
        }
        for &s in &secs {
            assert_eq!(format_duration(s), format_countdown(s, Countdown::Standard));
        }
    }

    /// The three countdown styles, side by side.
    #[test]
    fn countdown_styles_write_what_their_names_say() {
        let at = |d: i64, h: i64, m: i64, s: i64| d * 86_400 + h * 3600 + m * 60 + s;
        let cases = [
            (at(2, 5, 12, 0), "2d 5h", "2d5h", "2d 5h 12min"),
            (at(0, 1, 12, 30), "1h 12min", "1h12m", "1h 12min"),
            (at(0, 0, 12, 0), "12min", "12m", "12min"),
            (at(0, 0, 0, 45), "45s", "45s", "45s"),
        ];
        for (secs, standard, compact, long) in cases {
            assert_eq!(format_countdown(secs, Countdown::Standard), standard);
            assert_eq!(format_countdown(secs, Countdown::Compact), compact);
            assert_eq!(format_countdown(secs, Countdown::Long), long);
        }
    }

    /// A name cut by `rows::fit_name` (what the picker's rename keeps) is
    /// drawn whole in every place, whatever its letters.
    #[test]
    fn a_name_held_to_its_slot_shows_in_full_everywhere() {
        let renderer = Renderer::new().expect("a system font");
        let measure = |t: &str, s: f32| renderer.text_width(t, s);
        let places = model_name_places(&renderer);
        for typed in [
            "WWWWWWWWWWWWWWWWWWWW",
            "@@@@@@@@@@@@",
            "Sonnet 4.6 Thinking",
            "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii",
            "GPT-6.1 Sol Codex Max",
        ] {
            let name = crate::rows::fit_name(typed, measure);
            assert!(!name.is_empty());
            let label = ModelLabel {
                model: Some(name.clone()),
                with_version: None,
                effort: Some("xhigh".into()),
            };
            for (at, room, size) in &places {
                let shown = renderer.fit_label(&label, *size, *room);
                assert!(
                    shown
                        .as_deref()
                        .is_some_and(|s| s.starts_with(name.as_str())),
                    "{name:?} at {at}: shown as {shown:?}"
                );
            }
        }
    }

    fn pct_metric(label: &str, pct: u64, window: MetricWindow) -> Metric {
        Metric {
            label: label.into(),
            used: pct,
            limit: Some(100),
            unit: MetricUnit::Percent,
            reset_at: None,
            window,
            unified_pool: false,
            extra_only: false,
        }
    }

    fn data(metrics: Vec<Metric>) -> ProviderData {
        ProviderData {
            id: ProviderId::Claude,
            status: ProviderStatus::Ok,
            metrics,
            updated_at: utc::now(),
            received_at: Some(std::time::Instant::now()),
        }
    }

    fn any_rect() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        }
    }

    #[test]
    fn stale_data_greys_even_with_a_reset_ahead() {
        use crate::config::schema::UIConfig;

        let theme = ComputedTheme::compute(&UIConfig::default());
        let mut session = pct_metric("Session", 72, MetricWindow::Session);
        session.reset_at = Some(utc::now() + SignedDuration::from_hours(2));
        let fresh = data(vec![session.clone()]);
        // Retained data 40 minutes old, its window still two hours from reset.
        let stale = ProviderData {
            updated_at: utc::now() - SignedDuration::from_mins(40),
            received_at: None,
            ..data(vec![session])
        };
        assert!(stale.next_reset().is_some());

        assert_eq!(
            row_colors(&theme, &stale, 72.0),
            (theme.meta, theme.meta),
            "a value that stopped updating must not keep its live colours"
        );
        let level = theme.level_at(72.0);
        assert_eq!(
            row_colors(&theme, &fresh, 72.0),
            (level.bar, level.text),
            "fresh data keeps its level colours"
        );
        assert_ne!(level.bar, theme.meta);
    }

    #[test]
    fn default_view_shows_session_when_weekly_not_exhausted() {
        let d = data(vec![
            pct_metric("Session", 30, MetricWindow::Session),
            pct_metric("Weekly", 60, MetricWindow::Long),
        ]);
        let active = active_progress_metric(&d, &any_rect(), None).unwrap();
        assert_eq!(active.label, "Session");
    }

    #[test]
    fn exhausted_weekly_takes_over_default_view() {
        // Weekly maxed while the session reads low: the bar must reflect the
        // blocking weekly limit without needing a hover.
        let d = data(vec![
            pct_metric("Session", 20, MetricWindow::Session),
            pct_metric("Weekly", 100, MetricWindow::Long),
        ]);
        let active = active_progress_metric(&d, &any_rect(), None).unwrap();
        assert_eq!(active.label, "Weekly");
        assert_eq!(active.percentage(), Some(100.0));
    }

    #[test]
    fn exhausted_weekly_shown_when_session_window_absent() {
        // When the weekly cap is hit the source may stop reporting the session
        // window entirely; the weekly must still drive the bar.
        let d = data(vec![pct_metric("Weekly", 100, MetricWindow::Long)]);
        let active = active_progress_metric(&d, &any_rect(), None).unwrap();
        assert_eq!(active.label, "Weekly");
    }

    #[test]
    fn hover_still_reveals_weekly() {
        let d = data(vec![
            pct_metric("Session", 30, MetricWindow::Session),
            pct_metric("Weekly", 60, MetricWindow::Long),
        ]);
        let active = active_progress_metric(&d, &any_rect(), Some((5.0, 5.0))).unwrap();
        assert_eq!(active.label, "Weekly");
    }
}

// ─── Picker ──────────────────────────────────────────────────────
//
// The Settings window (ui/picker.rs) lays itself out as shapes in
// logical pixels; this paints them at the window's scale factor in the
// colours of `PickerTheme`. The text is the widget's own font: bold and
// italic are made from it (a slight spread to the right, a shear), so no
// second font file is loaded.
//
// It sits after the tests on purpose: a section of its own, apart from the
// widget's drawing above.

use super::picker::{Ink, Level, PickerScene, Shape as PickerShape};
use super::theme::PickerTheme;

/// The glyphs the picker has drawn, rasterized once. The picker's window
/// owns it, so it goes when the window does; a repaint (a hover moving to
/// another row) then costs the blending, not the rasterizing.
#[derive(Default)]
pub struct PickerGlyphs(HashMap<fontdue::layout::GlyphRasterConfig, (fontdue::Metrics, Vec<u8>)>);

#[allow(clippy::items_after_test_module)]
impl Renderer {
    /// Paint the picker into `pixmap`, the window at `scale` (its DPI scale
    /// factor): the chrome, the list clipped to its viewport, the overlay,
    /// an open question's text clipped to its own, then the top.
    pub fn draw_picker(
        &self,
        pixmap: &mut Pixmap,
        scene: &PickerScene,
        theme: &PickerTheme,
        scale: f32,
        glyphs: &mut PickerGlyphs,
    ) {
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        for s in &scene.chrome {
            self.picker_shape(pixmap, s, theme, scale, glyphs);
        }
        self.picker_clipped(pixmap, &scene.viewport, &scene.body, theme, scale, glyphs);
        for s in &scene.overlay {
            self.picker_shape(pixmap, s, theme, scale, glyphs);
        }
        if let Some(vp) = &scene.ask_viewport {
            self.picker_clipped(pixmap, vp, &scene.ask_body, theme, scale, glyphs);
        }
        for s in &scene.top {
            self.picker_shape(pixmap, s, theme, scale, glyphs);
        }
    }

    /// Shapes in `vp`'s own coordinates, drawn into it and nowhere else.
    fn picker_clipped(
        &self,
        pixmap: &mut Pixmap,
        vp: &Rect,
        shapes: &[PickerShape],
        theme: &PickerTheme,
        scale: f32,
        glyphs: &mut PickerGlyphs,
    ) {
        let (vx, vy) = ((vp.x * scale).round(), (vp.y * scale).round());
        let vw = ((vp.x + vp.w) * scale).round() - vx;
        let vh = ((vp.y + vp.h) * scale).round() - vy;
        if let Some(mut clip) = Pixmap::new(vw.max(1.0) as u32, vh.max(1.0) as u32) {
            for s in shapes {
                self.picker_shape(&mut clip, s, theme, scale, glyphs);
            }
            pixmap.draw_pixmap(
                vx as i32,
                vy as i32,
                clip.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
    }

    fn picker_shape(
        &self,
        pm: &mut Pixmap,
        shape: &PickerShape,
        theme: &PickerTheme,
        s: f32,
        glyphs: &mut PickerGlyphs,
    ) {
        let sc = |r: &Rect| Rect {
            x: r.x * s,
            y: r.y * s,
            w: r.w * s,
            h: r.h * s,
        };
        match shape {
            PickerShape::Fill {
                rect,
                radius,
                ink,
                alpha,
            } => self.fill_rect(pm, sc(rect), picker_ink(theme, *ink, *alpha), radius * s),
            PickerShape::Stroke {
                rect,
                radius,
                width,
                ink,
                alpha,
            } => self.picker_outline(
                pm,
                sc(rect),
                radius * s,
                width * s,
                picker_ink(theme, *ink, *alpha),
            ),
            PickerShape::Text {
                x,
                y,
                text,
                size,
                ink,
                bold,
                italic,
                alpha,
            } => self.picker_text(
                pm,
                glyphs,
                text,
                (x * s, y * s),
                size * s,
                picker_ink(theme, *ink, *alpha),
                (*bold, *italic),
            ),
            PickerShape::Check { rect, on, alpha } => {
                let r = sc(rect);
                if *on {
                    self.fill_rect(pm, r, picker_ink(theme, Ink::Accent, *alpha), 4.0 * s);
                    // The mock's mark: a 12 x 12 path, centred.
                    let (ox, oy) = (r.x + (r.w - 12.0 * s) / 2.0, r.y + (r.h - 12.0 * s) / 2.0);
                    let mut pb = PathBuilder::new();
                    pb.move_to(ox + 2.2 * s, oy + 6.3 * s);
                    pb.line_to(ox + 4.8 * s, oy + 8.8 * s);
                    pb.line_to(ox + 9.8 * s, oy + 3.4 * s);
                    if let Some(path) = pb.finish() {
                        self.picker_line(
                            pm,
                            &path,
                            1.6 * s,
                            picker_ink(theme, Ink::OnAccent, *alpha),
                        );
                    }
                } else {
                    self.picker_outline(pm, r, 4.0 * s, s, picker_ink(theme, Ink::Muted, *alpha));
                }
            }
            PickerShape::Radio { rect, on, alpha } => {
                let r = sc(rect);
                let (cx, cy, rad) = (r.x + r.w / 2.0, r.y + r.h / 2.0, r.w / 2.0);
                if *on {
                    self.picker_disc(pm, cx, cy, rad, picker_ink(theme, Ink::Accent, *alpha));
                    self.picker_disc(
                        pm,
                        cx,
                        cy,
                        rad - 5.0 * s,
                        picker_ink(theme, Ink::OnAccent, *alpha),
                    );
                } else if let Some(path) = PathBuilder::from_circle(cx, cy, rad - 0.5 * s) {
                    self.picker_line(pm, &path, s, picker_ink(theme, Ink::Muted, *alpha));
                }
            }
            PickerShape::TrayIcon { rect, style, alpha } => {
                let r = sc(rect);
                let ink = picker_ink(theme, Ink::Text, *alpha).to_skia_color();
                let size = r.w.round().max(1.0) as u32;
                if let Some(icon) = crate::ui::tray::style_preview(*style, ink, size) {
                    pm.draw_pixmap(
                        r.x.round() as i32,
                        r.y.round() as i32,
                        icon.as_ref(),
                        &tiny_skia::PixmapPaint::default(),
                        Transform::identity(),
                        None,
                    );
                }
            }
            PickerShape::Cross { rect, ink } => {
                let r = sc(rect);
                let (cx, cy, d) = (r.x + r.w / 2.0, r.y + r.h / 2.0, 5.0 * s);
                let mut pb = PathBuilder::new();
                pb.move_to(cx - d, cy - d);
                pb.line_to(cx + d, cy + d);
                pb.move_to(cx + d, cy - d);
                pb.line_to(cx - d, cy + d);
                if let Some(path) = pb.finish() {
                    self.picker_line(pm, &path, s, picker_ink(theme, *ink, Level::Full));
                }
            }
            PickerShape::Shadow { rect, radius } => {
                // Concentric rings whose alphas compound, as the tray
                // tooltip's shadow. The box is opaque and covers the middle,
                // so each ring is filled around it only (even-odd, the box a
                // hole): the same pixels for a fraction of the work.
                let r = sc(rect);
                let hole = rounded_rect_path(r, radius * s);
                for (spread, alpha) in theme.shadow {
                    let sp = spread * s;
                    let ring = Rect {
                        x: r.x - sp,
                        y: r.y - sp + theme.shadow_drop * s,
                        w: r.w + sp * 2.0,
                        h: r.h + sp * 2.0,
                    };
                    let mut pb = PathBuilder::new();
                    pb.push_path(&rounded_rect_path(ring, radius * s + sp));
                    pb.push_path(&hole);
                    let Some(path) = pb.finish() else {
                        continue;
                    };
                    if alpha == 0 {
                        continue;
                    }
                    let mut paint = Paint::default();
                    paint.set_color(
                        Color {
                            a: alpha,
                            ..theme.shadow_color
                        }
                        .to_skia_color(),
                    );
                    paint.anti_alias = true;
                    pm.fill_path(
                        &path,
                        &paint,
                        tiny_skia::FillRule::EvenOdd,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    }

    /// A rounded outline `width` wide, inside `r`.
    fn picker_outline(&self, pm: &mut Pixmap, r: Rect, radius: f32, width: f32, color: Color) {
        let half = width / 2.0;
        let inner = Rect {
            x: r.x + half,
            y: r.y + half,
            w: r.w - width,
            h: r.h - width,
        };
        if inner.w <= 0.0 || inner.h <= 0.0 {
            return;
        }
        let path = rounded_rect_path(inner, (radius - half).max(0.0));
        self.picker_line(pm, &path, width, color);
    }

    fn picker_line(&self, pm: &mut Pixmap, path: &tiny_skia::Path, width: f32, color: Color) {
        let mut paint = Paint::default();
        paint.set_color(color.to_skia_color());
        paint.anti_alias = true;
        let stroke = tiny_skia::Stroke {
            width,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        pm.stroke_path(path, &paint, &stroke, Transform::identity(), None);
    }

    fn picker_disc(&self, pm: &mut Pixmap, cx: f32, cy: f32, r: f32, color: Color) {
        let Some(path) = PathBuilder::from_circle(cx, cy, r.max(0.5)) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(color.to_skia_color());
        paint.anti_alias = true;
        pm.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// One line of text with its line box's top at `at`. `style` is (bold,
    /// italic): bold lends 70% of each pixel's coverage to its right
    /// neighbour (about a semibold), italic shears 12 degrees about the
    /// baseline, both with fractional coverage so the edges stay smooth.
    #[allow(clippy::too_many_arguments)]
    fn picker_text(
        &self,
        pm: &mut Pixmap,
        glyphs: &mut PickerGlyphs,
        text: &str,
        at: (f32, f32),
        size: f32,
        color: Color,
        style: (bool, bool),
    ) {
        let mut tl = TextLayout::new(CoordinateSystem::PositiveYDown);
        tl.append(&[self.font], &TextStyle::new(text, size, 0));
        let baseline = tl
            .lines()
            .and_then(|l| l.first().map(|l| l.baseline_y))
            .unwrap_or(size);
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let spread = if style.0 { 0.7 } else { 0.0 };
        let slant = if style.1 { 0.21 } else { 0.0 };
        let mut acc: Vec<f32> = Vec::new();
        for g in tl.glyphs() {
            if g.width == 0 || g.height == 0 {
                continue;
            }
            let (m, bitmap) = glyphs
                .0
                .entry(g.key)
                .or_insert_with(|| self.font.rasterize_config(g.key));
            let (m, bitmap) = (*m, &*bitmap);
            let gx = (at.0 + g.x).round() as i32;
            let gy = (at.1 + g.y).round() as i32;
            for row in 0..m.height {
                let py = gy + row as i32;
                if py < 0 || py >= ph {
                    continue;
                }
                let shift = slant * (baseline - (g.y + row as f32 + 0.5));
                let whole = shift.floor();
                let frac = shift - whole;
                acc.clear();
                acc.resize(m.width + 3, 0.0);
                let mut prev = 0.0f32;
                for col in 0..=m.width {
                    let c = if col < m.width {
                        bitmap[row * m.width + col] as f32
                    } else {
                        0.0
                    };
                    let cov = c.max(prev * spread);
                    prev = c;
                    if cov > 0.0 {
                        acc[col] += cov * (1.0 - frac);
                        acc[col + 1] += cov * frac;
                    }
                }
                for (i, &a) in acc.iter().enumerate() {
                    let px = gx + whole as i32 + i as i32;
                    if a <= 0.0 || px < 0 || px >= pw {
                        continue;
                    }
                    let alpha = (a.min(255.0) * color.a as f32 / 255.0) as u8;
                    if alpha > 0 {
                        blend_pixel(pm, px as u32, py as u32, color, alpha);
                    }
                }
            }
        }
    }
}

/// An ink's colour in the picker theme, its alpha scaled by the theme's
/// factor for `level`.
fn picker_ink(theme: &PickerTheme, ink: Ink, level: Level) -> Color {
    let l = &theme.levels;
    let alpha = match level {
        Level::Full => 1.0,
        Level::FieldHover => l.field_hover,
        Level::Hovered => l.hovered,
        Level::Pressed => l.pressed,
        Level::AccentHover => l.accent_hover,
        Level::AccentPressed => l.accent_pressed,
        Level::MarkPressed => l.mark_pressed,
        Level::Disabled => l.disabled,
        Level::DisabledMark => l.disabled_mark,
        Level::Placeholder => l.placeholder,
        Level::Thumb => l.thumb,
    };
    let c = match ink {
        Ink::Surface => theme.surface,
        Ink::Border => theme.border,
        Ink::Text => theme.text,
        Ink::Muted => theme.muted,
        Ink::Accent => theme.accent,
        Ink::OnAccent => theme.on_accent,
        Ink::Hover => theme.hover,
        Ink::Separator => theme.separator,
        Ink::Dim => theme.dim,
        Ink::Good => theme.good,
        Ink::Warn => theme.warn,
    };
    Color {
        a: (c.a as f32 * alpha).round().clamp(0.0, 255.0) as u8,
        ..c
    }
}
