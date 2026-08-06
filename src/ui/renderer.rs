// ui/renderer.rs — UI rendering via tiny-skia.
//
// The ONLY place that draws. Colors come ONLY from ui/theme.rs,
// zones from ui/layout.rs.

use super::{
    layout::{BodyLayout, Rect, RowTier, WindowLayout},
    theme::{Color, ComputedTheme, UsageLevel},
};
use crate::config::schema::DetailLevel;
use crate::providers::{
    Metric, MetricUnit, MetricWindow, ProviderData, ProviderId, ProviderStatus,
};
use anyhow::{Context, Result};
use chrono::Utc;
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

/// Load the first usable UI font from the candidate list. Returns None only
/// if NONE of them can be read/parsed.
pub(crate) fn load_ui_font() -> Option<Font> {
    for path in FONT_CANDIDATES {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = Font::from_bytes(data, fontdue::FontSettings::default()) {
                return Some(font);
            }
        }
    }
    None
}

/// Name column width in vertical compact.
pub(super) const NAME_WIDTH: f32 = 48.0;
/// Percent column width.
pub(super) const PCT_WIDTH: f32 = 30.0;
/// Gap between row elements.
pub(super) const ROW_GAP: f32 = 7.0;

/// Bar and text colors, accounting for staleness and estimation —
/// both render grey.
fn row_colors(theme: &ComputedTheme, data: &ProviderData, pct: f32) -> (Color, Color) {
    let estimated = matches!(data.status, ProviderStatus::Estimated);
    let stale_without_future_reset = data.stale_age_secs().is_some() && data.next_reset().is_none();
    if estimated || stale_without_future_reset {
        (theme.meta, theme.meta)
    } else {
        let level = theme.level(UsageLevel::from_percentage(pct));
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
fn stale_age_secs_or_estimated(data: &ProviderData) -> Option<String> {
    let age = data.stale_age_secs()?;
    if matches!(data.status, ProviderStatus::Estimated) {
        Some(format!("est · {} ago", format_duration(age)))
    } else {
        Some(format!("{} ago", format_duration(age)))
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
    font: Font,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        let font = load_ui_font().with_context(|| {
            format!(
                "no usable system font found (tried {})",
                FONT_CANDIDATES.join(", ")
            )
        })?;
        Ok(Self { font })
    }

    /// Draw the whole UI into the pixmap.
    ///
    /// `opacity` — background opacity from the config (0.10–0.85). Applied
    /// ONLY to the background alpha; text stays readable.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        pixmap: &mut Pixmap,
        layout: &WindowLayout,
        theme: &ComputedTheme,
        providers: &[ProviderData],
        opacity: f32,
        detail: &DetailLevel,
        cursor: Option<(f32, f32)>,
        forecasts: &HashMap<ProviderId, chrono::DateTime<Utc>>,
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

        // First-run / everything-unconfigured: with no visible providers the
        // window would be a blank translucent box with no path forward. Draw a
        // single guidance line pointing to the menu instead of nothing.
        if providers.is_empty() {
            self.draw_text(
                pixmap,
                "Right-click → set up providers",
                0.0,
                (layout.height - 11.0) / 2.0,
                10.5,
                theme.provider_name,
                Align::Center,
                layout.width,
            );
            return;
        }

        match &layout.body {
            BodyLayout::Vertical { rows, hint } => {
                for (row, data) in rows.iter().zip(providers.iter()) {
                    let fc = forecast_label(forecasts, data);
                    let reason = hover_reason(data, errors);
                    self.draw_vertical_row(
                        pixmap,
                        row,
                        theme,
                        data,
                        detail,
                        layout.row_tier,
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
                    let hover_hint = hovered_hint_text(rows, providers, cursor, errors);
                    self.draw_hint(pixmap, hint, theme, providers, hover_hint.as_deref());
                }
            }
            BodyLayout::Horizontal { cols } => {
                for (col, data) in cols.iter().zip(providers.iter()) {
                    let fc = forecast_label(forecasts, data);
                    let reason = hover_reason(data, errors);
                    self.draw_horizontal_col(
                        pixmap,
                        col,
                        theme,
                        data,
                        detail,
                        cursor,
                        fc.as_deref(),
                        reason.as_deref(),
                    );
                }
            }
        }
    }

    // ─── Vertical ────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn draw_vertical_row(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        data: &ProviderData,
        detail: &DetailLevel,
        tier: RowTier,
        cursor: Option<(f32, f32)>,
        forecast: Option<&str>,
        reason: Option<&str>,
    ) {
        match detail {
            // Compact has no per-row reset slot; the reason shows in the hint line.
            DetailLevel::Compact => self.vertical_compact(pixmap, row, theme, data, tier, cursor),
            DetailLevel::Medium => self.vertical_medium(
                pixmap, row, theme, data, false, tier, cursor, forecast, reason,
            ),
            DetailLevel::Expanded => self.vertical_medium(
                pixmap, row, theme, data, true, tier, cursor, forecast, reason,
            ),
        }
    }

    /// Compact: name | bar | pct in one line, minus whatever the width cannot
    /// carry (see `layout::row_tier`).
    fn vertical_compact(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        data: &ProviderData,
        tier: RowTier,
        cursor: Option<(f32, f32)>,
    ) {
        let text_y = row.y + (row.h - 13.0) / 2.0;
        if tier == RowTier::Full {
            self.draw_text(
                pixmap,
                data.id.display_name(),
                row.x,
                text_y,
                11.0,
                theme.provider_name,
                Align::Left,
                NAME_WIDTH,
            );
        }

        let active_metric = active_progress_metric(data, row, cursor);
        let Some(bar) = vertical_compact_bar_rect(row, tier) else {
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
                    11.0,
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
                    bar.x + bar.w + ROW_GAP,
                    text_y,
                    11.0,
                    text_color,
                    Align::Right,
                    PCT_WIDTH,
                );
            }
            _ => self.draw_status_inline(
                pixmap,
                &bar,
                text_y,
                theme,
                data,
                bar.w + ROW_GAP + PCT_WIDTH,
            ),
        }
    }

    /// Medium/expanded: name+pct / bar / meta line.
    #[allow(clippy::too_many_arguments)]
    fn vertical_medium(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        data: &ProviderData,
        expanded: bool,
        tier: RowTier,
        cursor: Option<(f32, f32)>,
        forecast: Option<&str>,
        reason: Option<&str>,
    ) {
        // Hovering a greyed/estimated row answers "why": the meta line shows
        // the stored failure + the action instead of the usual metric/reset.
        let hovered_reason =
            reason.filter(|_| cursor.is_some_and(|point| point_in_rect(point, row)));
        let name_size = if expanded { 13.0 } else { 12.0 };
        // Name and percent share this line with no clipping between them, so a
        // row too narrow for both drops the name rather than overlapping it.
        if tier == RowTier::Full {
            self.draw_text(
                pixmap,
                data.id.display_name(),
                row.x,
                row.y + 3.0,
                name_size,
                theme.provider_name,
                Align::Left,
                row.w,
            );
        }

        let bar_h = if expanded { 5.0 } else { 4.0 };
        let bar_y = row.y + if expanded { 26.0 } else { 20.0 };
        let bar = Rect {
            x: row.x,
            y: bar_y,
            w: row.w,
            h: bar_h,
        };
        let meta_y = bar_y + bar_h + 4.0;

        let active_metric = active_progress_metric(data, row, cursor);
        match (&data.status, active_metric.and_then(Metric::percentage)) {
            (ProviderStatus::Ok | ProviderStatus::Estimated, Some(pct)) => {
                let stale = stale_age_secs_or_estimated(data);
                let (bar_color, text_color) = row_colors(theme, data, pct);
                // Percent at the top right.
                self.draw_text(
                    pixmap,
                    &pct_text(data, pct),
                    row.x,
                    row.y + 2.0,
                    if expanded { 15.0 } else { 13.0 },
                    text_color,
                    Align::Right,
                    row.w,
                );
                self.draw_bar(pixmap, &bar, theme, bar_color, pct);

                if let Some(text) = hovered_reason {
                    self.draw_text(
                        pixmap,
                        text,
                        row.x,
                        meta_y,
                        if expanded { 11.0 } else { 10.5 },
                        theme.meta,
                        Align::Left,
                        row.w,
                    );
                }
                let weekly_active_in_expanded =
                    expanded && active_metric.is_some_and(is_long_window_metric);
                if hovered_reason.is_none() && !weekly_active_in_expanded {
                    if active_metric.is_some_and(is_long_window_metric) {
                        self.draw_weekly_time_only(pixmap, row, theme, active_metric, meta_y);
                    } else {
                        // Meta line: the metric on the left, the reset on the right.
                        if let Some(m) = active_metric
                            .filter(|m| !is_percent_metric(m) || is_long_window_metric(m))
                        {
                            self.draw_text(
                                pixmap,
                                &m.display_text(),
                                row.x,
                                meta_y,
                                if expanded { 11.0 } else { 10.5 },
                                theme.meta,
                                Align::Left,
                                row.w / 2.0,
                            );
                        }
                        // The reset countdown OWNS this slot: it is the hard
                        // fact the user plans around, so a prediction never
                        // covers it. The forecast (when enabled) only fills in
                        // while there is no future reset to show.
                        if let Some(reset) = active_metric
                            .and_then(|m| m.reset_at)
                            .filter(|t| *t > Utc::now())
                        {
                            let secs = reset.signed_duration_since(Utc::now()).num_seconds();
                            self.draw_text(
                                pixmap,
                                &format_duration(secs),
                                row.x + row.w / 2.0,
                                meta_y,
                                if expanded { 11.5 } else { 10.5 },
                                theme.meta,
                                Align::Right,
                                row.w / 2.0,
                            );
                        } else if let Some(fc) = forecast {
                            self.draw_text(
                                pixmap,
                                fc,
                                row.x + row.w / 2.0,
                                meta_y,
                                if expanded { 11.5 } else { 10.5 },
                                theme.high.text,
                                Align::Right,
                                row.w / 2.0,
                            );
                        } else if let Some(label) = stale {
                            // Show the age only when there is no future reset.
                            self.draw_text(
                                pixmap,
                                &label,
                                row.x + row.w / 2.0,
                                meta_y,
                                if expanded { 11.5 } else { 10.5 },
                                theme.meta,
                                Align::Right,
                                row.w / 2.0,
                            );
                        }
                    }
                }
            }
            // Status on the bar line, so it does not overlap the provider name.
            _ => {
                if matches!(data.status, ProviderStatus::Ok | ProviderStatus::Estimated)
                    && long_window_metric(data).is_some()
                {
                    self.draw_text(
                        pixmap,
                        "session unavailable",
                        bar.x,
                        bar.y - 5.0,
                        10.5,
                        theme.meta,
                        Align::Left,
                        row.w,
                    );
                } else {
                    self.draw_status_inline(pixmap, &bar, bar.y - 5.0, theme, data, row.w);
                }
            }
        }

        // Expanded shows the second (weekly) metric, without duplicating the
        // active one — suppressed while the hover reason owns the meta line.
        if expanded && hovered_reason.is_none() {
            self.draw_expanded_weekly_metric(pixmap, row, theme, data, meta_y, cursor);
        }
    }

    fn draw_expanded_weekly_metric(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        data: &ProviderData,
        y: f32,
        cursor: Option<(f32, f32)>,
    ) {
        let Some(metric) = long_window_metric(data) else {
            return;
        };
        let text = if cursor.is_some_and(|point| point_in_rect(point, row)) {
            metric
                .reset_at
                .filter(|t| *t > Utc::now())
                .map(|reset| {
                    let secs = reset.signed_duration_since(Utc::now()).num_seconds();
                    format!("{}: {}", metric.label, format_duration(secs))
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
            11.0,
            theme.meta,
            Align::Left,
            row.w,
        );
    }

    fn draw_weekly_time_only(
        &self,
        pixmap: &mut Pixmap,
        row: &Rect,
        theme: &ComputedTheme,
        metric: Option<&Metric>,
        y: f32,
    ) {
        let Some(metric) = metric else {
            return;
        };
        let text = metric
            .reset_at
            .filter(|t| *t > Utc::now())
            .map(|reset| {
                let secs = reset.signed_duration_since(Utc::now()).num_seconds();
                format!("{}: {}", metric.label, format_duration(secs))
            })
            .unwrap_or_else(|| metric.label.clone());
        self.draw_text(
            pixmap,
            &text,
            row.x,
            y,
            10.5,
            theme.meta,
            Align::Right,
            row.w,
        );
    }

    /// Bottom hint: the nearest reset.
    fn draw_hint(
        &self,
        pixmap: &mut Pixmap,
        hint: &Rect,
        theme: &ComputedTheme,
        providers: &[ProviderData],
        override_text: Option<&str>,
    ) {
        if let Some(text) = override_text {
            self.draw_text(
                pixmap,
                text,
                hint.x,
                hint.y + 1.0,
                10.5,
                theme.meta,
                Align::Left,
                hint.w,
            );
            return;
        }

        let next = providers
            .iter()
            .flat_map(|p| {
                p.metrics.iter().filter_map(move |m| {
                    m.reset_at
                        .map(|t| (p.id.display_name(), m.label.as_str(), m.display_text(), t))
                })
            })
            .filter(|(_, _, _, t)| *t > Utc::now())
            .min_by_key(|(_, _, _, t)| *t);

        if let Some((provider, metric, value, t)) = next {
            let secs = t.signed_duration_since(Utc::now()).num_seconds();
            if secs > 0 {
                let text = compact_hint_text(provider, metric, &value, secs);
                self.draw_text(
                    pixmap,
                    &text,
                    hint.x,
                    hint.y + 1.0,
                    10.5,
                    theme.meta,
                    Align::Left,
                    hint.w,
                );
            }
        }
    }

    // ─── Horizontal ──────────────────────────────────────────────

    /// Column: percent on top, a vertical bar, the name below.
    #[allow(clippy::too_many_arguments)]
    fn draw_horizontal_col(
        &self,
        pixmap: &mut Pixmap,
        col: &Rect,
        theme: &ComputedTheme,
        data: &ProviderData,
        detail: &DetailLevel,
        cursor: Option<(f32, f32)>,
        forecast: Option<&str>,
        reason: Option<&str>,
    ) {
        // The narrow column only fits the action half of the hover reason
        // ("run Claude Code"), not the full cause — the vertical layouts and
        // the compact hint line carry the long form.
        let hovered_reason = reason
            .filter(|_| cursor.is_some_and(|point| point_in_rect(point, col)))
            .map(|r| r.rsplit(" — ").next().unwrap_or(r));
        // Compact has no percent: pct_size = None, the bar starts at the top.
        let (bar_w, top_gap, bottom_gap, pct_size, name_size, reset_size) = match detail {
            DetailLevel::Compact => (7.0, 4.0, 13.0, None, 9.5, None),
            DetailLevel::Medium => (8.5, 19.0, 17.0, Some(12.5), 10.5, None),
            DetailLevel::Expanded => (11.0, 23.0, 30.0, Some(14.5), 11.5, Some(11.0)),
        };
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
                    col.y + col.h / 2.0 - 6.0,
                    dot_color,
                );
            }
        }

        self.draw_text(
            pixmap,
            data.id.display_name(),
            col.x,
            col.y + col.h - if reset_size.is_some() { 23.0 } else { 12.0 },
            name_size,
            theme.provider_name,
            Align::Center,
            col.w,
        );

        if let Some(reset_size) = reset_size {
            // Same slot ownership as the vertical layout: the reset countdown
            // first, the forecast only when there is no future reset — but the
            // hover reason beats both (the user is asking "why").
            if let Some(text) = hovered_reason {
                self.draw_text(
                    pixmap,
                    text,
                    col.x,
                    col.y + col.h - 10.0,
                    reset_size,
                    theme.meta,
                    Align::Center,
                    col.w,
                );
            } else if let Some(reset) = active_metric
                .and_then(|m| m.reset_at)
                .filter(|t| *t > Utc::now())
            {
                let secs = reset.signed_duration_since(Utc::now()).num_seconds();
                self.draw_text(
                    pixmap,
                    &format_duration(secs),
                    col.x,
                    col.y + col.h - 10.0,
                    reset_size,
                    theme.meta,
                    Align::Center,
                    col.w,
                );
            } else if let Some(fc) = forecast {
                self.draw_text(
                    pixmap,
                    fc,
                    col.x,
                    col.y + col.h - 10.0,
                    reset_size,
                    theme.high.text,
                    Align::Center,
                    col.w,
                );
            }
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
    fn draw_status_inline(
        &self,
        pixmap: &mut Pixmap,
        bar: &Rect,
        text_y: f32,
        theme: &ComputedTheme,
        data: &ProviderData,
        span: f32,
    ) {
        match &data.status {
            ProviderStatus::Ok | ProviderStatus::Estimated => {
                // A binary provider or data without a limit: a green dot + text.
                self.draw_dot_at(pixmap, bar.x + 3.0, bar.y + bar.h / 2.0, theme.low.bar);
                let text = match data.metrics.first() {
                    // used=0 with no limit — the label is more informative.
                    Some(m) if m.used == 0 && m.limit.is_none() => m.label.clone(),
                    Some(m) => m.display_text(),
                    None => "OK".to_string(),
                };
                self.draw_text(
                    pixmap,
                    &text,
                    bar.x + 10.0,
                    text_y,
                    9.0,
                    theme.meta,
                    Align::Left,
                    span - 10.0,
                );
            }
            ProviderStatus::Loading => {
                self.draw_text(
                    pixmap,
                    "…",
                    bar.x,
                    text_y,
                    11.0,
                    theme.meta,
                    Align::Left,
                    span,
                );
            }
            ProviderStatus::AuthError(_) => {
                self.draw_dot_at(pixmap, bar.x + 3.0, bar.y + bar.h / 2.0, theme.mid.bar);
                self.draw_text(
                    pixmap,
                    "check the key",
                    bar.x + 10.0,
                    text_y,
                    9.0,
                    theme.meta,
                    Align::Left,
                    span - 10.0,
                );
            }
            ProviderStatus::NetworkError(msg) => {
                // A short provider message beats a generic "offline";
                // long technical ones collapse.
                let text = if msg.chars().count() <= 32 {
                    msg.as_str()
                } else {
                    "⚠ offline"
                };
                self.draw_text(
                    pixmap,
                    text,
                    bar.x,
                    text_y,
                    9.0,
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
                    9.0,
                    theme.meta,
                    Align::Left,
                    span,
                );
            }
        }
    }

    /// Status dot.
    fn draw_dot_at(&self, pixmap: &mut Pixmap, cx: f32, cy: f32, color: Color) {
        let mut paint = Paint::default();
        paint.set_color(color.to_skia_color());
        paint.anti_alias = true;
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, 3.0);
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
        tl.append(&[&self.font], &TextStyle::new(text, size, 0));

        // Text width for alignment.
        let text_w: f32 = tl
            .glyphs()
            .iter()
            .last()
            .map(|g| g.x + g.width as f32)
            .unwrap_or(0.0);

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
fn vertical_compact_bar_rect(row: &Rect, tier: RowTier) -> Option<Rect> {
    let (x, w) = match tier {
        RowTier::Full => (
            row.x + NAME_WIDTH + ROW_GAP,
            row.w - NAME_WIDTH - PCT_WIDTH - ROW_GAP * 2.0,
        ),
        // The name column is reclaimed whole; the percent keeps its slot.
        RowTier::Nameless => (row.x, row.w - PCT_WIDTH - ROW_GAP),
        RowTier::PercentOnly => return None,
    };
    Some(Rect {
        x,
        y: row.y + (row.h - 3.0) / 2.0,
        w,
        h: 3.0,
    })
}

/// Hover explanation for a greyed/estimated row: the last stored fetch
/// failure plus the action that fixes it. None while the data is live —
/// hovering then keeps its usual meaning (the weekly metric).
pub fn hover_reason(data: &ProviderData, errors: &HashMap<ProviderId, String>) -> Option<String> {
    let aged = data.stale_age_secs().is_some() || matches!(data.status, ProviderStatus::Estimated);
    if !aged {
        return None;
    }
    let cause = errors
        .get(&data.id)
        .cloned()
        .unwrap_or_else(|| "no fresh data".to_string());
    // Some provider messages already carry their action ("… — run Codex CLI
    // once"); a transient rate-limit needs no action at all.
    if cause.contains("run ") || cause.starts_with("rate-limited") {
        return Some(cause);
    }
    let action = match data.id {
        ProviderId::Claude => "run Claude Code",
        ProviderId::Codex => "run Codex CLI",
        ProviderId::Copilot => "check gh auth",
        ProviderId::Antigravity => "run Antigravity",
    };
    Some(format!("{cause} — {action}"))
}

fn hovered_hint_text(
    rows: &[Rect],
    providers: &[ProviderData],
    cursor: Option<(f32, f32)>,
    errors: &HashMap<ProviderId, String>,
) -> Option<String> {
    let point = cursor?;
    rows.iter().zip(providers.iter()).find_map(|(row, data)| {
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
                .filter(|reset| *reset > Utc::now())
                .map(|reset| {
                    let secs = reset.signed_duration_since(Utc::now()).num_seconds();
                    format!("{prefix} {}", format_duration(secs))
                })
                .unwrap_or(prefix)
        })
    })
}

fn compact_hint_text(provider: &str, metric: &str, value: &str, secs: i64) -> String {
    let duration = format_duration(secs);
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
    forecasts: &HashMap<ProviderId, chrono::DateTime<Utc>>,
    data: &ProviderData,
) -> Option<String> {
    if data.stale_age_secs().is_some() {
        return None;
    }
    forecasts
        .get(&data.id)
        .map(|hit| hit.signed_duration_since(Utc::now()).num_seconds())
        .filter(|s| *s > 0)
        .map(|s| format!("~{}", format_duration(s)))
}

/// Format a duration: "1h 12min"; over 24 hours — day format "2d 5h".
pub fn format_duration(secs: i64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}min")
    } else if m > 0 {
        format!("{m}min")
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MetricWindow;

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

        let full = vertical_compact_bar_rect(&r, RowTier::Full).expect("Full keeps a bar");
        let nameless =
            vertical_compact_bar_rect(&r, RowTier::Nameless).expect("Nameless keeps a bar");

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
            vertical_compact_bar_rect(&row(60.0), RowTier::PercentOnly).is_none(),
            "PercentOnly must not produce a rectangle - a negative width would be drawn"
        );
    }

    /// Design preview: renders every width step at every detail level to
    /// %TEMP% so the degradation ladder can be judged by eye — the drawing
    /// itself is not unit tested. Run explicitly:
    /// `cargo test preview_width_steps -- --ignored`.
    #[test]
    #[ignore]
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
                    providers.len(),
                );
                let mut pm =
                    Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
                renderer.draw(
                    &mut pm,
                    &win,
                    &theme,
                    &providers,
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
    #[ignore]
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
        ];

        let dir = std::env::temp_dir();
        for (fname, flow) in [("row", ColumnFlow::Row), ("column", ColumnFlow::Column)] {
            for (dname, detail) in [
                ("compact", DetailLevel::Compact),
                ("medium", DetailLevel::Medium),
            ] {
                let win = layout::compute(
                    &Layout::Horizontal,
                    &detail,
                    &WidthScale::Full,
                    &flow,
                    providers.len(),
                );
                let mut pm =
                    Pixmap::new(win.width.ceil() as u32, win.height.ceil() as u32).unwrap();
                renderer.draw(
                    &mut pm,
                    &win,
                    &theme,
                    &providers,
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

    fn pct_metric(label: &str, pct: u64, window: MetricWindow) -> Metric {
        Metric {
            label: label.into(),
            used: pct,
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

    fn any_rect() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        }
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
