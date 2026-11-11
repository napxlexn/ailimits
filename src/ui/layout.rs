// ui/layout.rs — layout logic (vertical / horizontal).
//
// The layout hands out zone rectangles; the renderer (the single place
// that draws) does the in-zone arrangement.

use super::renderer::{self, NAME_WIDTH, PCT_WIDTH, ROW_GAP};
use super::theme::Dimensions;
use crate::config::schema::{ColumnFlow, DetailLevel, Layout, UIConfig, WidthScale};
use crate::rows::RowView;

/// Rectangle in window coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn snap(self) -> Self {
        Self {
            x: self.x.round(),
            y: self.y.round(),
            w: self.w.round().max(1.0),
            h: self.h.round().max(1.0),
        }
    }
}

/// Window body per layout kind.
#[derive(Debug, Clone)]
pub enum BodyLayout {
    /// Full-width rows + a bottom hint.
    Vertical { rows: Vec<Rect>, hint: Rect },
    /// Columns with vertical bars.
    Horizontal { cols: Vec<Rect> },
    /// A fresh install with nothing chosen yet: what to do, in two lines
    /// ("Choose what to show" over "Click here...").
    Choose { area: Rect },
}

/// How much of a row fits at the current width. A property of the window,
/// not of an individual row: every row in the rows layout is the same width,
/// so one decision keeps them uniform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowTier {
    /// Name, bar and percent.
    Full,
    /// Bar and percent; the name is dropped, not squeezed.
    Nameless,
    /// The percent alone, centred. No bar is built at all.
    PercentOnly,
}

/// Full window layout.
#[derive(Debug, Clone)]
pub struct WindowLayout {
    pub width: f32,
    pub height: f32,
    pub body: BodyLayout,
    /// Row presentation the width allows; meaningful for the rows layout.
    pub row_tier: RowTier,
    /// One entry per row or column: the band added under it for its model's
    /// line of its own, or None when it has none (see `Shape`).
    pub model_lines: Vec<Option<Rect>>,
}

/// What the widget's size depends on besides the config: one entry per row
/// (whether the row carries a line of its own for its model) and whether the
/// widget is waiting for a first choice. The app keeps the last one and lays
/// the window out again only when it changes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shape {
    pub model_lines: Vec<bool>,
    /// A fresh install with nothing chosen (`rows::nothing_chosen`).
    pub choose: bool,
}

impl Shape {
    /// `n` rows, none with a model line: every widget before model labels.
    pub fn plain(n: usize) -> Self {
        Self {
            model_lines: vec![false; n],
            choose: false,
        }
    }

    /// The shape of `rows` under `ui`. `measure` is `Renderer::text_width`.
    ///
    /// Where a model label goes (the "own line" design):
    /// - Compact rows: nowhere; the tooltip carries it.
    /// - Medium rows: on the meta line, in what the reset leaves of it; a
    ///   line of its own when the meta line's left part is taken (a count
    ///   such as Copilot's "123 / 300"), or when its right part at its
    ///   widest (a weekly "Weekly: 23h 59min") leaves less than the narrowest
    ///   name slot, so a name held to `rows::name_fits` always shows in full.
    /// - Expanded rows and every columns layout: a line of its own.
    ///
    /// Only a row that shows a model ever gets the line.
    pub fn of(
        ui: &UIConfig,
        rows: &[RowView],
        choose: bool,
        measure: impl Fn(&str, f32) -> f32,
    ) -> Self {
        let columns = matches!(ui.layout, Layout::Horizontal);
        let row_w = rows_width(&ui.detail, &ui.width_scale) - Dimensions::PADDING_H * 2.0;
        let model_lines = rows
            .iter()
            .map(|row| {
                row.model.is_some()
                    && (columns
                        || match ui.detail {
                            DetailLevel::Compact => false,
                            DetailLevel::Expanded => true,
                            DetailLevel::Medium => medium_needs_line(row, row_w, &measure),
                        })
            })
            .collect();
        Self {
            model_lines,
            choose: choose && rows.is_empty(),
        }
    }
}

/// Whether a Medium row's model needs a line of its own (see `Shape::of`).
fn medium_needs_line(row: &RowView, row_w: f32, measure: &impl Fn(&str, f32) -> f32) -> bool {
    match renderer::medium_meta_widest(&row.data) {
        None => true,
        Some(right) => {
            let room = medium_model_room(row_w, measure(&right, renderer::META_SIZE));
            room < medium_name_slot()
        }
    }
}

/// What Medium's meta line leaves a model beside a right-aligned text
/// `right_w` wide. The one formula the renderer, the shape and the
/// name-slot test share.
pub fn medium_model_room(row_w: f32, right_w: f32) -> f32 {
    if right_w > 0.0 {
        row_w - right_w - MODEL_GAP
    } else {
        row_w
    }
}

/// The narrowest place a model name is drawn at Medium's meta size: what a
/// name held to `rows::name_fits` is promised.
fn medium_name_slot() -> f32 {
    crate::rows::NAME_SLOTS
        .iter()
        .filter(|(_, size)| *size == renderer::META_SIZE)
        .map(|(w, _)| *w)
        .fold(0.0, f32::max)
}

/// Air between a model and the reset sharing Medium's meta line.
pub const MODEL_GAP: f32 = 8.0;
/// The line a row grows by for its model (rows layout).
pub const MODEL_LINE_H: f32 = 14.0;
/// The line a column grows by for its model (columns layout).
pub const MODEL_COL_LINE_H: f32 = 13.0;

/// Hint area height.
const HINT_HEIGHT: f32 = 20.0 * Dimensions::DENSITY_SCALE;
/// Minimal top padding (there is no header).
const TOP_PADDING: f32 = 4.0;
/// Below this a bar carries no information worth the pixels.
const MIN_BAR: f32 = 24.0;
/// Space between provider columns, per arrangement and the same at every
/// detail level. Side by side a column is already fenced off by its own
/// width — the bar sits centred with the name beneath it — so it needs only
/// enough air to read as separate. Stacked, one provider's name would
/// otherwise sit directly against the next one's percentage, which reads as
/// one run-on block, so that direction gets more.
const ROW_FLOW_GAP: f32 = 2.5 * Dimensions::DENSITY_SCALE;
const STACK_FLOW_GAP: f32 = 10.0 * Dimensions::DENSITY_SCALE;

/// Pick the row presentation from the space a row actually has, not from
/// which width step produced it. Keyed on pixels so a changed constant needs
/// no new case here — only a new row in the table test.
///
/// Names are dropped whole, never squeezed: `Renderer::draw_text` does not
/// clip glyphs to `max_width`, so a narrowed text box would paint over its
/// neighbour rather than truncate.
///
/// The two row shapes run out of room differently, so each is measured on its
/// own terms rather than through one formula that fits neither:
///
/// - Compact packs name, bar and percent onto a single line, so the bar is
///   what a narrow row destroys first.
/// - Medium and Expanded put name and percent on a top line and give the bar
///   a full-width line of its own. The bar is never the constraint there; the
///   two texts colliding on the top line is.
fn row_tier(detail: &DetailLevel, row_w: f32) -> RowTier {
    match detail {
        DetailLevel::Compact => {
            let bar_with_name = row_w - NAME_WIDTH - PCT_WIDTH - ROW_GAP * 2.0;
            let bar_without_name = row_w - PCT_WIDTH - ROW_GAP;

            if bar_with_name >= MIN_BAR {
                RowTier::Full
            } else if bar_without_name >= MIN_BAR {
                RowTier::Nameless
            } else {
                RowTier::PercentOnly
            }
        }
        DetailLevel::Medium | DetailLevel::Expanded => {
            if row_w >= NAME_WIDTH + PCT_WIDTH + ROW_GAP {
                RowTier::Full
            } else if row_w >= PCT_WIDTH {
                RowTier::Nameless
            } else {
                RowTier::PercentOnly
            }
        }
    }
}

/// Compute the window layout for a widget of `shape` (one row or column per
/// entry; see `Shape`).
pub fn compute(
    layout: &Layout,
    detail: &DetailLevel,
    width_scale: &WidthScale,
    column_flow: &ColumnFlow,
    shape: &Shape,
) -> WindowLayout {
    // A fresh install: two lines saying what to do, in a box of the Medium
    // rows' width whatever the chosen layout, so the second line fits.
    if shape.choose {
        return choose();
    }
    // No visible providers (all unconfigured / disabled): the renderer draws a
    // single guidance line. Force a vertical compact box wide enough for that
    // hint regardless of the user's chosen layout, so it never clips (a
    // horizontal 0-provider window would otherwise be ~60px wide).
    if shape.model_lines.is_empty() {
        let mut win = vertical(&DetailLevel::Compact, &WidthScale::Full, &[false]);
        win.model_lines.clear();
        return win;
    }
    let lines = &shape.model_lines;
    match layout {
        Layout::Vertical => vertical(detail, width_scale, lines),
        Layout::Horizontal => horizontal(detail, column_flow, lines),
        // Legacy config value: the 2x2 mode was removed from the UI.
        Layout::Grid => vertical(detail, width_scale, lines),
    }
}

/// The rows layout's window width.
fn rows_width(detail: &DetailLevel, width_scale: &WidthScale) -> f32 {
    Dimensions::scale(match detail {
        DetailLevel::Compact => 212.0,
        DetailLevel::Medium => 250.0,
        DetailLevel::Expanded => 290.0,
    }) * width_scale.fraction()
}

/// The first-install box: one Medium row's width and height.
fn choose() -> WindowLayout {
    let width = rows_width(&DetailLevel::Medium, &WidthScale::Full);
    let pad_h = Dimensions::PADDING_H;
    let area = Rect {
        x: pad_h,
        y: TOP_PADDING,
        w: width - pad_h * 2.0,
        h: Dimensions::row_height(&DetailLevel::Medium),
    };
    WindowLayout {
        width,
        height: area.y + area.h + 4.0,
        body: BodyLayout::Choose { area },
        row_tier: RowTier::Full,
        model_lines: Vec::new(),
    }
}

/// Vertical layout: one row per entry of `lines`, a row with a model line
/// taller by that line.
fn vertical(detail: &DetailLevel, width_scale: &WidthScale, lines: &[bool]) -> WindowLayout {
    let width: f32 = rows_width(detail, width_scale);
    let row_h = Dimensions::row_height(detail);
    let pad_h = Dimensions::PADDING_H;

    let mut rows = Vec::with_capacity(lines.len());
    let mut model_lines = Vec::with_capacity(lines.len());
    let mut y = TOP_PADDING;
    for &line in lines {
        let extra = if line { MODEL_LINE_H } else { 0.0 };
        rows.push(Rect {
            x: pad_h,
            y,
            w: width - pad_h * 2.0,
            h: row_h + extra,
        });
        model_lines.push(line.then_some(Rect {
            x: pad_h,
            y: y + row_h,
            w: width - pad_h * 2.0,
            h: MODEL_LINE_H,
        }));
        y += row_h + extra;
    }

    // The hint is written for the natural width and the renderer drops it once
    // the rows lose their names, so its height must go with it — otherwise a
    // narrowed widget keeps a band of empty background under the last row.
    let tier = row_tier(detail, width - pad_h * 2.0);
    let show_hint = *detail == DetailLevel::Compact && tier == RowTier::Full;
    let hint_h = if show_hint { HINT_HEIGHT } else { 0.0 };
    let hint = Rect {
        x: pad_h,
        y,
        w: width - pad_h * 2.0,
        h: hint_h,
    };
    let height = y + hint_h + 4.0;

    WindowLayout {
        width,
        height,
        body: BodyLayout::Vertical { rows, hint },
        row_tier: tier,
        model_lines,
    }
}

/// Horizontal layout: vertical bars with names below, arranged either side by
/// side or stacked downwards. Only the geometry differs between the two — the
/// column body and the renderer's per-column drawing are shared, because a
/// column is drawn relative to its own rectangle wherever that sits.
///
/// A column with a model line keeps its body as it is and gains the line
/// under it. Side by side every column shares one height, so the window grows
/// by the line once any column has one; stacked, only those columns grow.
fn horizontal(detail: &DetailLevel, flow: &ColumnFlow, lines: &[bool]) -> WindowLayout {
    let n = lines.len();
    // Nothing is drawn in the side margins, so they stay small and identical
    // at every detail level: a bigger detail level buys bigger bars and text,
    // not more empty background around them.
    let pad_h = Dimensions::scale(6.0);
    // Wide enough for the provider name underneath the bar, and no wider —
    // the leftover width reads as distance between providers. Antigravity is
    // the longest name and sets the floor here.
    let col_w = Dimensions::scale(match detail {
        DetailLevel::Compact => 46.0,
        DetailLevel::Medium => 54.0,
        DetailLevel::Expanded => 62.0,
    });
    let body_h = Dimensions::scale(match detail {
        DetailLevel::Compact => 42.0,
        DetailLevel::Medium => 68.0,
        DetailLevel::Expanded => 88.0,
    });
    let stacked = *flow == ColumnFlow::Column;
    let gap = if stacked {
        STACK_FLOW_GAP
    } else {
        ROW_FLOW_GAP
    };
    let gaps = gap * n.saturating_sub(1) as f32;
    let extra = |line: bool| if line { MODEL_COL_LINE_H } else { 0.0 };
    // Stacked: each column's own line; side by side: one shared line.
    let extras: f32 = if stacked {
        lines.iter().map(|&l| extra(l)).sum()
    } else {
        extra(lines.iter().any(|&l| l))
    };
    let (width, height) = if stacked {
        (
            pad_h * 2.0 + col_w,
            TOP_PADDING + body_h * n as f32 + gaps + extras + 6.0,
        )
    } else {
        (
            pad_h * 2.0 + col_w * n as f32 + gaps,
            TOP_PADDING + body_h + extras + 6.0,
        )
    };

    let mut cols = Vec::with_capacity(n);
    let mut model_lines = Vec::with_capacity(n);
    // The lines of the columns above, stacked.
    let mut above = 0.0;
    for (i, &line) in lines.iter().enumerate() {
        let (x, y) = if stacked {
            (pad_h, TOP_PADDING + (body_h + gap) * i as f32 + above)
        } else {
            (pad_h + (col_w + gap) * i as f32, TOP_PADDING)
        };
        cols.push(Rect {
            x,
            y,
            w: col_w,
            h: body_h,
        });
        model_lines.push(line.then_some(Rect {
            x,
            y: y + body_h,
            w: col_w,
            h: MODEL_COL_LINE_H,
        }));
        if stacked {
            above += extra(line);
        }
    }

    WindowLayout {
        width,
        height,
        body: BodyLayout::Horizontal { cols },
        row_tier: RowTier::Full,
        model_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::WidthScale;

    /// The natural width of the rows layout, unchanged from before width steps.
    fn full_width(detail: &DetailLevel) -> f32 {
        compute(
            &Layout::Vertical,
            detail,
            &WidthScale::Full,
            &ColumnFlow::Row,
            &Shape::plain(2),
        )
        .width
    }

    /// Every width step at every detail level, pinned. The thresholds derive
    /// from DENSITY_SCALE, PADDING_H, NAME_WIDTH and PCT_WIDTH — a change to
    /// any of them must fail here rather than surface as a clipped widget.
    #[test]
    fn tier_table_pins_every_width_and_detail_combination() {
        use DetailLevel::{Compact, Expanded, Medium};

        let cases = [
            (Compact, WidthScale::Full, RowTier::Full),
            (Compact, WidthScale::ThreeQuarters, RowTier::Full),
            (Compact, WidthScale::Half, RowTier::Nameless),
            (Medium, WidthScale::Full, RowTier::Full),
            (Medium, WidthScale::ThreeQuarters, RowTier::Full),
            (Medium, WidthScale::Half, RowTier::Full),
            (Expanded, WidthScale::Full, RowTier::Full),
            (Expanded, WidthScale::ThreeQuarters, RowTier::Full),
            (Expanded, WidthScale::Half, RowTier::Full),
        ];

        for (detail, scale, want) in cases {
            let got = compute(
                &Layout::Vertical,
                &detail,
                &scale,
                &ColumnFlow::Row,
                &Shape::plain(2),
            )
            .row_tier;
            assert_eq!(
                got, want,
                "{detail:?} at {scale:?}: expected {want:?}, got {got:?}"
            );
        }
    }

    /// Stacking turns the columns layout on its side: one column wide, n tall.
    #[test]
    fn stacked_columns_share_one_column_and_grow_downwards() {
        let side_by_side = compute_columns(&DetailLevel::Medium, &ColumnFlow::Row, 3);
        let stacked = compute_columns(&DetailLevel::Medium, &ColumnFlow::Column, 3);

        let BodyLayout::Horizontal { cols } = &stacked.body else {
            panic!("stacking keeps the columns body");
        };

        let first = cols[0];
        for (i, col) in cols.iter().enumerate() {
            assert_eq!(col.x, first.x, "column {i} must share the left edge");
            assert_eq!(col.w, first.w, "column {i} must share the width");
        }
        for pair in cols.windows(2) {
            assert!(
                pair[1].y > pair[0].y,
                "columns must advance downwards: {} then {}",
                pair[0].y,
                pair[1].y
            );
        }

        assert!(
            stacked.width < side_by_side.width,
            "stacked is one column wide: {} vs {}",
            stacked.width,
            side_by_side.width
        );
        assert!(
            stacked.height > side_by_side.height,
            "stacked is n columns tall: {} vs {}",
            stacked.height,
            side_by_side.height
        );
    }

    /// A hint that is not drawn must not reserve its height, or a narrowed
    /// Compact widget carries a band of empty background along its bottom.
    #[test]
    fn a_hidden_hint_leaves_no_empty_band_behind() {
        let narrow = compute(
            &Layout::Vertical,
            &DetailLevel::Compact,
            &WidthScale::Half,
            &ColumnFlow::Row,
            &Shape::plain(2),
        );
        assert_ne!(
            narrow.row_tier,
            RowTier::Full,
            "this case exists to cover a hidden hint"
        );

        let BodyLayout::Vertical { rows, hint } = &narrow.body else {
            panic!("rows body");
        };
        assert_eq!(hint.h, 0.0, "a hidden hint has no height");

        let rows_bottom = rows.last().map(|r| r.y + r.h).unwrap();
        assert!(
            narrow.height - rows_bottom < 8.0,
            "only padding may follow the last row, found {} px",
            narrow.height - rows_bottom
        );
    }

    #[test]
    fn stacking_is_spaced_more_generously_than_a_row() {
        let row = arrangement_gap(&ColumnFlow::Row);
        let column = arrangement_gap(&ColumnFlow::Column);

        assert!(
            row < column,
            "a row needs less air than a stack: row {row}, column {column}"
        );
        assert!(row >= 2.0, "a row still needs a visible gap, got {row}");
    }

    fn arrangement_gap(flow: &ColumnFlow) -> f32 {
        let layout = compute_columns(&DetailLevel::Medium, flow, 3);
        let BodyLayout::Horizontal { cols } = &layout.body else {
            panic!("columns body");
        };
        let (a, b) = (cols[0], cols[1]);
        match flow {
            ColumnFlow::Row => b.x - (a.x + a.w),
            ColumnFlow::Column => b.y - (a.y + a.h),
        }
    }

    #[test]
    fn side_margins_do_not_grow_with_the_detail_level() {
        let margin = |detail| {
            let layout = compute_columns(detail, &ColumnFlow::Row, 3);
            let BodyLayout::Horizontal { cols } = &layout.body else {
                panic!("columns body");
            };
            let left = cols[0].x;
            let right = layout.width - (cols.last().unwrap().x + cols.last().unwrap().w);
            (left, right)
        };

        let compact = margin(&DetailLevel::Compact);
        for detail in [&DetailLevel::Medium, &DetailLevel::Expanded] {
            let (left, right) = margin(detail);
            assert!(
                (left - compact.0).abs() < 0.01,
                "{detail:?} left margin {left} differs from Compact's {}",
                compact.0
            );
            assert!(
                (right - compact.1).abs() < 0.01,
                "{detail:?} right margin {right} differs from Compact's {}",
                compact.1
            );
        }
    }

    /// Providers must not touch in either arrangement.
    #[test]
    fn providers_are_separated_by_a_gap_in_both_arrangements() {
        for flow in [ColumnFlow::Row, ColumnFlow::Column] {
            let layout = compute_columns(&DetailLevel::Medium, &flow, 3);
            let BodyLayout::Horizontal { cols } = &layout.body else {
                panic!("columns body");
            };

            for pair in cols.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let gap = match flow {
                    ColumnFlow::Row => b.x - (a.x + a.w),
                    ColumnFlow::Column => b.y - (a.y + a.h),
                };
                assert!(
                    gap >= 2.0,
                    "{flow:?}: providers are {gap} px apart, too close to tell apart"
                );
            }
        }
    }

    fn compute_columns(detail: &DetailLevel, flow: &ColumnFlow, n: usize) -> WindowLayout {
        compute(
            &Layout::Horizontal,
            detail,
            &WidthScale::Full,
            flow,
            &Shape::plain(n),
        )
    }

    // ─── Shape: the lines models take ─────────────────────────────

    use crate::providers::{Metric, MetricUnit, MetricWindow, ProviderData, ProviderId};
    use crate::rows::ModelLabel;

    /// A fixed-advance stand-in for the font: half the size per character.
    fn mono(text: &str, size: f32) -> f32 {
        text.chars().count() as f32 * size * 0.5
    }

    fn metric(label: &str, unit: MetricUnit, window: MetricWindow) -> Metric {
        Metric {
            label: label.to_string(),
            used: 30,
            limit: Some(100),
            unit,
            reset_at: Some(chrono::Utc::now() + chrono::Duration::hours(3)),
            window,
            unified_pool: false,
        }
    }

    /// A provider's own row with `metrics`, and a model when `model`.
    fn row(id: ProviderId, metrics: Vec<Metric>, model: bool) -> RowView {
        let data = ProviderData {
            id,
            status: crate::providers::ProviderStatus::Ok,
            metrics,
            updated_at: chrono::Utc::now(),
            received_at: None,
        };
        let mut rows = crate::rows::visible_rows(
            &crate::config::schema::Config::default(),
            &[data],
            crate::config::schema::Surface::Widget,
            &crate::models::Models::new(),
        );
        let mut row = rows.remove(0);
        row.model = model.then(|| ModelLabel {
            model: Some("Opus".into()),
            with_version: None,
            effort: None,
        });
        row
    }

    fn session(model: bool) -> RowView {
        let m = metric("Session", MetricUnit::Percent, MetricWindow::Session);
        row(ProviderId::Claude, vec![m], model)
    }

    fn weekly(model: bool) -> RowView {
        let m = metric("Weekly", MetricUnit::Percent, MetricWindow::Long);
        row(ProviderId::Grok, vec![m], model)
    }

    fn count(model: bool) -> RowView {
        let m = metric("Premium", MetricUnit::Requests, MetricWindow::Session);
        row(ProviderId::Copilot, vec![m], model)
    }

    fn ui(layout: Layout, detail: DetailLevel, scale: WidthScale, flow: ColumnFlow) -> UIConfig {
        UIConfig {
            layout,
            detail,
            width_scale: scale,
            column_flow: flow,
            ..UIConfig::default()
        }
    }

    fn lines(ui: &UIConfig, rows: &[RowView]) -> Vec<bool> {
        Shape::of(ui, rows, false, mono).model_lines
    }

    #[test]
    fn only_a_row_with_a_model_ever_gets_a_line() {
        let rows = [session(false), weekly(false), count(false)];
        for layout in [Layout::Vertical, Layout::Horizontal] {
            for detail in [
                DetailLevel::Compact,
                DetailLevel::Medium,
                DetailLevel::Expanded,
            ] {
                let ui = ui(layout.clone(), detail, WidthScale::Half, ColumnFlow::Row);
                assert_eq!(lines(&ui, &rows), [false, false, false]);
            }
        }
    }

    #[test]
    fn compact_rows_leave_the_model_to_the_tooltip_and_expanded_gives_it_a_line() {
        let rows = [session(true), weekly(true), count(true), session(false)];
        let full = |d| ui(Layout::Vertical, d, WidthScale::Full, ColumnFlow::Row);
        assert_eq!(lines(&full(DetailLevel::Compact), &rows), [false; 4]);
        assert_eq!(
            lines(&full(DetailLevel::Expanded), &rows),
            [true, true, true, false]
        );
    }

    #[test]
    fn medium_shares_the_meta_line_unless_it_is_taken_or_too_full() {
        let rows = [session(true), weekly(true), count(true)];
        let at = |scale| {
            ui(
                Layout::Vertical,
                DetailLevel::Medium,
                scale,
                ColumnFlow::Row,
            )
        };
        // A count takes the left part at every width; a weekly's
        // "Weekly: 23h 59min" leaves too little only when narrow.
        assert_eq!(lines(&at(WidthScale::Full), &rows), [false, false, true]);
        assert_eq!(lines(&at(WidthScale::Half), &rows), [false, true, true]);
    }

    #[test]
    fn a_model_line_grows_only_its_own_row() {
        let rows = [session(false), weekly(true), session(false)];
        let ui = ui(
            Layout::Vertical,
            DetailLevel::Expanded,
            WidthScale::Full,
            ColumnFlow::Row,
        );
        let shape = Shape::of(&ui, &rows, false, mono);
        let with = compute(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &shape,
        );
        let without = compute(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &Shape::plain(3),
        );
        let (BodyLayout::Vertical { rows: a, .. }, BodyLayout::Vertical { rows: b, .. }) =
            (&with.body, &without.body)
        else {
            panic!("rows body");
        };
        assert_eq!(a[0].h, b[0].h);
        assert_eq!(a[1].h, b[1].h + MODEL_LINE_H);
        assert_eq!(a[2].y, b[2].y + MODEL_LINE_H);
        assert!((with.height - without.height - MODEL_LINE_H).abs() < 0.01);
        assert!(with.model_lines[0].is_none());
        let line = with.model_lines[1].expect("the weekly row's line");
        assert_eq!(
            line.y,
            a[1].y + b[1].h,
            "the line is the band the row grew by"
        );
    }

    #[test]
    fn columns_side_by_side_grow_once_and_stacked_grow_per_column() {
        let rows = [session(true), weekly(false), count(true)];
        for detail in [
            DetailLevel::Compact,
            DetailLevel::Medium,
            DetailLevel::Expanded,
        ] {
            for (flow, grows) in [
                (ColumnFlow::Row, MODEL_COL_LINE_H),
                (ColumnFlow::Column, 2.0 * MODEL_COL_LINE_H),
            ] {
                let ui = ui(Layout::Horizontal, detail.clone(), WidthScale::Full, flow);
                let shape = Shape::of(&ui, &rows, false, mono);
                assert_eq!(shape.model_lines, [true, false, true]);
                let with = compute(&ui.layout, &ui.detail, &ui.width_scale, &flow, &shape);
                let without = compute(
                    &ui.layout,
                    &ui.detail,
                    &ui.width_scale,
                    &flow,
                    &Shape::plain(3),
                );
                assert!(
                    (with.height - without.height - grows).abs() < 0.01,
                    "{detail:?} {flow:?}: grew {} px",
                    with.height - without.height
                );
                assert_eq!(with.width, without.width, "a model line never widens");
                let BodyLayout::Horizontal { cols } = &with.body else {
                    panic!("columns body");
                };
                let line = with.model_lines[0].expect("the first column's line");
                assert_eq!(line.y, cols[0].y + cols[0].h, "under the column body");
                assert_eq!(line.w, cols[0].w);
                if flow == ColumnFlow::Column {
                    // The column after one with a line starts below it.
                    assert!(cols[1].y >= line.y + line.h);
                }
            }
        }
    }

    #[test]
    fn a_fresh_install_gets_the_choose_box_and_an_empty_widget_keeps_its_line() {
        let ui = UIConfig::default();
        let choose = Shape::of(&ui, &[], true, mono);
        assert!(choose.choose);
        let win = compute(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &choose,
        );
        assert!(matches!(win.body, BodyLayout::Choose { .. }));
        // Rows win over the first-install box (they cannot both be).
        assert!(!Shape::of(&ui, &[session(false)], true, mono).choose);
        // Nothing to show but something chosen: today's one-row box.
        let empty = compute(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &Shape::plain(0),
        );
        let BodyLayout::Vertical { rows, .. } = &empty.body else {
            panic!("rows body");
        };
        assert_eq!(rows.len(), 1);
        assert!(empty.model_lines.is_empty());
    }

    #[test]
    fn three_quarters_narrows_the_rows_window_to_three_quarters() {
        let full = full_width(&DetailLevel::Compact);
        let scaled = compute(
            &Layout::Vertical,
            &DetailLevel::Compact,
            &WidthScale::ThreeQuarters,
            &ColumnFlow::Row,
            &Shape::plain(2),
        )
        .width;

        assert!(
            (scaled - full * 0.75).abs() < 0.01,
            "expected {} (75% of {}), got {}",
            full * 0.75,
            full,
            scaled
        );
    }
}
