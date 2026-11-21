// ui/layout.rs — layout logic (vertical / horizontal).
//
// The layout hands out zone rectangles; the renderer (the single place
// that draws) does the in-zone arrangement.

use super::renderer::{self, NAME_WIDTH, PCT_WIDTH, ROW_GAP};
use super::theme::Dimensions;
use crate::config::schema::{ColumnFlow, DetailLevel, Layout, UIConfig, WidthScale};
use crate::config::tune::{Countdown, Tune};
use crate::rows::RowView;

/// The widget's sizes as `[tune.widget]` and `[tune.format]` set them,
/// clamped (`config::tune`). Made when the config changes and carried by
/// the `WindowLayout`, so the renderer draws with the very numbers the
/// layout was computed with. The default is today's widget: every factor
/// 1.0 and every addition 0.0, which leave each size bit for bit as it was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidgetMetrics {
    /// Text size factor: every text and the room text takes.
    pub text: f32,
    /// Bar thickness factor.
    pub bar: f32,
    /// Inner padding factor.
    pub pad: f32,
    /// Extra pixels between two rows or two columns.
    pub gap: f32,
    /// How a time left is written.
    pub countdown: Countdown,
}

impl WidgetMetrics {
    /// Today's widget.
    pub const DEFAULT: Self = Self {
        text: 1.0,
        bar: 1.0,
        pad: 1.0,
        gap: 0.0,
        countdown: Countdown::Standard,
    };

    /// The metrics `tune` sets.
    pub fn from_tune(tune: &Tune) -> Self {
        Self {
            text: tune.widget.text_scale(),
            bar: tune.widget.bar_scale(),
            pad: tune.widget.padding_scale(),
            gap: tune.widget.row_gap(),
            countdown: tune.format.countdown(),
        }
    }

    /// A text size, or a length that follows the text, at this text scale.
    pub fn t(&self, size: f32) -> f32 {
        size * self.text
    }
}

impl Default for WidgetMetrics {
    fn default() -> Self {
        Self::DEFAULT
    }
}

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
    /// Rows are chosen and providers run, but no row chosen has anything to
    /// show: the same two-line box, saying so (`Shape::unshown`).
    Unshown { area: Rect },
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
    /// The sizes this layout was computed with; the renderer draws with them.
    pub metrics: WidgetMetrics,
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
    /// No row to show while providers run: every row chosen is an extra
    /// row the providers do not report, or belongs to a provider that is
    /// not connected. The widget says so and a click opens the picker,
    /// instead of pointing at the providers (`with_unshown`).
    pub unshown: bool,
}

impl Shape {
    /// `n` rows, none with a model line: every widget before model labels.
    pub fn plain(n: usize) -> Self {
        Self {
            model_lines: vec![false; n],
            choose: false,
            unshown: false,
        }
    }

    /// Mark a widget with no rows while `providers_shown` (some provider
    /// runs and is connected) as `unshown`. A widget with rows, or one
    /// waiting for a first choice, is left as it is.
    pub fn with_unshown(mut self, providers_shown: bool) -> Self {
        self.unshown = providers_shown && self.model_lines.is_empty() && !self.choose;
        self
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
    ///
    /// `of_tuned` with today's sizes: the tests' entry.
    #[cfg(test)]
    pub fn of(
        ui: &UIConfig,
        rows: &[RowView],
        choose: bool,
        measure: impl Fn(&str, f32) -> f32,
    ) -> Self {
        Self::of_tuned(ui, &WidgetMetrics::DEFAULT, rows, choose, measure)
    }

    /// `of` for the sizes `m` (`[tune]`): what the app lays out with.
    pub fn of_tuned(
        ui: &UIConfig,
        m: &WidgetMetrics,
        rows: &[RowView],
        choose: bool,
        measure: impl Fn(&str, f32) -> f32,
    ) -> Self {
        let columns = matches!(ui.layout, Layout::Horizontal);
        let (width, pad_h) = rows_frame(&ui.detail, &ui.width_scale, m);
        let row_w = width - pad_h * 2.0;
        let model_lines = rows
            .iter()
            .map(|row| {
                row.model.is_some()
                    && (columns
                        || match ui.detail {
                            DetailLevel::Compact => false,
                            DetailLevel::Expanded => true,
                            DetailLevel::Medium => medium_needs_line(row, row_w, m, &measure),
                        })
            })
            .collect();
        Self {
            model_lines,
            choose: choose && rows.is_empty(),
            unshown: false,
        }
    }
}

/// Whether a Medium row's model needs a line of its own (see `Shape::of`).
fn medium_needs_line(
    row: &RowView,
    row_w: f32,
    m: &WidgetMetrics,
    measure: &impl Fn(&str, f32) -> f32,
) -> bool {
    match renderer::medium_meta_widest(&row.data, m.countdown) {
        None => true,
        Some(right) => {
            let right_w = measure(&right, m.t(renderer::META_SIZE));
            let room = medium_model_room(row_w, right_w, m);
            room < medium_name_slot() * m.text
        }
    }
}

/// What Medium's meta line leaves a model beside a right-aligned text
/// `right_w` wide. The one formula the renderer, the shape and the
/// name-slot test share.
pub fn medium_model_room(row_w: f32, right_w: f32, m: &WidgetMetrics) -> f32 {
    if right_w > 0.0 {
        row_w - right_w - m.t(MODEL_GAP)
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
fn row_tier(detail: &DetailLevel, row_w: f32, m: &WidgetMetrics) -> RowTier {
    // The text columns follow the text size; the bar's floor does not.
    let (name_w, pct_w, gap) = (m.t(NAME_WIDTH), m.t(PCT_WIDTH), m.t(ROW_GAP));
    match detail {
        DetailLevel::Compact => {
            let bar_with_name = row_w - name_w - pct_w - gap * 2.0;
            let bar_without_name = row_w - pct_w - gap;

            if bar_with_name >= MIN_BAR {
                RowTier::Full
            } else if bar_without_name >= MIN_BAR {
                RowTier::Nameless
            } else {
                RowTier::PercentOnly
            }
        }
        DetailLevel::Medium | DetailLevel::Expanded => {
            if row_w >= name_w + pct_w + gap {
                RowTier::Full
            } else if row_w >= pct_w {
                RowTier::Nameless
            } else {
                RowTier::PercentOnly
            }
        }
    }
}

/// Compute the window layout for a widget of `shape` (one row or column per
/// entry; see `Shape`).
///
/// `compute_tuned` with today's sizes: the tests' and previews' entry.
#[cfg(test)]
pub fn compute(
    layout: &Layout,
    detail: &DetailLevel,
    width_scale: &WidthScale,
    column_flow: &ColumnFlow,
    shape: &Shape,
) -> WindowLayout {
    compute_tuned(
        layout,
        detail,
        width_scale,
        column_flow,
        shape,
        &WidgetMetrics::DEFAULT,
    )
}

/// `compute` for the sizes `m` (`[tune]`): what the app lays out with. The
/// layout carries `m` for the renderer.
pub fn compute_tuned(
    layout: &Layout,
    detail: &DetailLevel,
    width_scale: &WidthScale,
    column_flow: &ColumnFlow,
    shape: &Shape,
    m: &WidgetMetrics,
) -> WindowLayout {
    // A fresh install: two lines saying what to do, in a box of the Medium
    // rows' width whatever the chosen layout, so the second line fits.
    if shape.choose {
        return two_line_box(|area| BodyLayout::Choose { area }, m);
    }
    // Rows chosen, none to show: the same box, saying so.
    if shape.unshown {
        return two_line_box(|area| BodyLayout::Unshown { area }, m);
    }
    // No visible providers (all unconfigured / disabled): the renderer draws a
    // single guidance line. Force a vertical compact box wide enough for that
    // hint regardless of the user's chosen layout, so it never clips (a
    // horizontal 0-provider window would otherwise be ~60px wide).
    if shape.model_lines.is_empty() {
        let mut win = vertical(&DetailLevel::Compact, &WidthScale::Full, &[false], m);
        win.model_lines.clear();
        return win;
    }
    let lines = &shape.model_lines;
    match layout {
        Layout::Vertical => vertical(detail, width_scale, lines, m),
        Layout::Horizontal => horizontal(detail, column_flow, lines, m),
        // Legacy config value: the 2x2 mode was removed from the UI.
        Layout::Grid => vertical(detail, width_scale, lines, m),
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

/// The rows layout's window width and side padding at the sizes `m`: the
/// width follows the text, and grows by what the padding adds to today's.
fn rows_frame(detail: &DetailLevel, width_scale: &WidthScale, m: &WidgetMetrics) -> (f32, f32) {
    let pad_h = Dimensions::PADDING_H * m.pad;
    let width = rows_width(detail, width_scale) * m.text + (pad_h - Dimensions::PADDING_H) * 2.0;
    (width, pad_h)
}

/// A row's height in the rows layout: today's at the text size, plus what a
/// thicker bar adds where the bar has a line of its own (Medium, Expanded;
/// a Compact bar is centred in its row).
fn row_height(detail: &DetailLevel, m: &WidgetMetrics) -> f32 {
    let base = Dimensions::row_height(detail) * m.text;
    match detail {
        DetailLevel::Compact => base,
        DetailLevel::Medium | DetailLevel::Expanded => {
            base + renderer::row_bar_thickness(detail) * (m.bar - 1.0)
        }
    }
}

/// The two-line box of the first install and of a widget whose rows have
/// nothing to show: one Medium row's width and height.
fn two_line_box(body: impl FnOnce(Rect) -> BodyLayout, m: &WidgetMetrics) -> WindowLayout {
    let (width, pad_h) = rows_frame(&DetailLevel::Medium, &WidthScale::Full, m);
    let area = Rect {
        x: pad_h,
        y: TOP_PADDING * m.pad,
        w: width - pad_h * 2.0,
        h: Dimensions::row_height(&DetailLevel::Medium) * m.text,
    };
    WindowLayout {
        width,
        height: area.y + area.h + 4.0 * m.pad,
        body: body(area),
        row_tier: RowTier::Full,
        model_lines: Vec::new(),
        metrics: *m,
    }
}

/// Vertical layout: one row per entry of `lines`, a row with a model line
/// taller by that line.
fn vertical(
    detail: &DetailLevel,
    width_scale: &WidthScale,
    lines: &[bool],
    m: &WidgetMetrics,
) -> WindowLayout {
    let (width, pad_h) = rows_frame(detail, width_scale, m);
    let row_h = row_height(detail, m);
    let line_h = m.t(MODEL_LINE_H);

    let mut rows = Vec::with_capacity(lines.len());
    let mut model_lines = Vec::with_capacity(lines.len());
    let mut y = TOP_PADDING * m.pad;
    for (i, &line) in lines.iter().enumerate() {
        if i > 0 {
            y += m.gap;
        }
        let extra = if line { line_h } else { 0.0 };
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
            h: line_h,
        }));
        y += row_h + extra;
    }

    // The hint is written for the natural width and the renderer drops it once
    // the rows lose their names, so its height must go with it — otherwise a
    // narrowed widget keeps a band of empty background under the last row.
    let tier = row_tier(detail, width - pad_h * 2.0, m);
    let show_hint = *detail == DetailLevel::Compact && tier == RowTier::Full;
    let hint_h = if show_hint { m.t(HINT_HEIGHT) } else { 0.0 };
    let hint = Rect {
        x: pad_h,
        y,
        w: width - pad_h * 2.0,
        h: hint_h,
    };
    let height = y + hint_h + 4.0 * m.pad;

    WindowLayout {
        width,
        height,
        body: BodyLayout::Vertical { rows, hint },
        row_tier: tier,
        model_lines,
        metrics: *m,
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
fn horizontal(
    detail: &DetailLevel,
    flow: &ColumnFlow,
    lines: &[bool],
    m: &WidgetMetrics,
) -> WindowLayout {
    let n = lines.len();
    // Nothing is drawn in the side margins, so they stay small and identical
    // at every detail level: a bigger detail level buys bigger bars and text,
    // not more empty background around them.
    let pad_h = Dimensions::scale(6.0) * m.pad;
    let (top, bottom) = (TOP_PADDING * m.pad, 6.0 * m.pad);
    // Wide enough for the provider name underneath the bar, and no wider —
    // the leftover width reads as distance between providers. Antigravity is
    // the longest name and sets the floor here. Both follow the text size:
    // the body holds the percent above the bar and the name below it.
    let col_w = Dimensions::scale(match detail {
        DetailLevel::Compact => 46.0,
        DetailLevel::Medium => 54.0,
        DetailLevel::Expanded => 62.0,
    }) * m.text;
    let body_h = Dimensions::scale(match detail {
        DetailLevel::Compact => 42.0,
        DetailLevel::Medium => 68.0,
        DetailLevel::Expanded => 88.0,
    }) * m.text;
    let stacked = *flow == ColumnFlow::Column;
    let flow_gap = if stacked {
        STACK_FLOW_GAP
    } else {
        ROW_FLOW_GAP
    };
    let gap = flow_gap + m.gap;
    let gaps = gap * n.saturating_sub(1) as f32;
    let line_h = m.t(MODEL_COL_LINE_H);
    let extra = |line: bool| if line { line_h } else { 0.0 };
    // Stacked: each column's own line; side by side: one shared line.
    let extras: f32 = if stacked {
        lines.iter().map(|&l| extra(l)).sum()
    } else {
        extra(lines.iter().any(|&l| l))
    };
    let (width, height) = if stacked {
        (
            pad_h * 2.0 + col_w,
            top + body_h * n as f32 + gaps + extras + bottom,
        )
    } else {
        (
            pad_h * 2.0 + col_w * n as f32 + gaps,
            top + body_h + extras + bottom,
        )
    };

    let mut cols = Vec::with_capacity(n);
    let mut model_lines = Vec::with_capacity(n);
    // The lines of the columns above, stacked.
    let mut above = 0.0;
    for (i, &line) in lines.iter().enumerate() {
        let (x, y) = if stacked {
            (pad_h, top + (body_h + gap) * i as f32 + above)
        } else {
            (pad_h + (col_w + gap) * i as f32, top)
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
            h: line_h,
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
        metrics: *m,
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
            extra_only: false,
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

    /// Rows chosen, none to show, providers running: the two-line box that
    /// says so. With rows, while nothing is chosen, or with no provider
    /// shown, the widget stays as it was.
    #[test]
    fn rows_chosen_without_data_get_a_box_of_their_own() {
        let ui = UIConfig::default();
        let unshown = Shape::of(&ui, &[], false, mono).with_unshown(true);
        assert!(unshown.unshown && !unshown.choose);
        let win = compute(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &unshown,
        );
        let BodyLayout::Unshown { area } = win.body else {
            panic!("the unshown box");
        };
        // The same box as the first install's.
        let first = compute(
            &ui.layout,
            &ui.detail,
            &ui.width_scale,
            &ui.column_flow,
            &Shape::of(&ui, &[], true, mono),
        );
        let BodyLayout::Choose { area: choose } = first.body else {
            panic!("the choose box");
        };
        assert_eq!(
            (area.w, area.h, win.width),
            (choose.w, choose.h, first.width)
        );
        assert!(
            !Shape::of(&ui, &[session(false)], false, mono)
                .with_unshown(true)
                .unshown
        );
        assert!(!Shape::of(&ui, &[], true, mono).with_unshown(true).unshown);
        assert!(!Shape::of(&ui, &[], false, mono).with_unshown(false).unshown);
        assert_eq!(
            Shape::of(&ui, &[], false, mono).with_unshown(false),
            Shape::plain(0)
        );
    }

    // ─── [tune.widget]: the sizes ─────────────────────────────────

    fn metrics(text: f32, bar: f32, pad: f32, gap: f32) -> WidgetMetrics {
        WidgetMetrics {
            text,
            bar,
            pad,
            gap,
            ..WidgetMetrics::DEFAULT
        }
    }

    fn tuned(
        m: &WidgetMetrics,
        layout: Layout,
        detail: DetailLevel,
        flow: ColumnFlow,
        n: usize,
    ) -> WindowLayout {
        compute_tuned(
            &layout,
            &detail,
            &WidthScale::Full,
            &flow,
            &Shape::plain(n),
            m,
        )
    }

    fn row_rects(win: &WindowLayout) -> Vec<Rect> {
        match &win.body {
            BodyLayout::Vertical { rows, .. } => rows.clone(),
            BodyLayout::Horizontal { cols } => cols.clone(),
            _ => panic!("rows or columns"),
        }
    }

    #[test]
    fn an_empty_tune_is_todays_metrics() {
        let m = WidgetMetrics::from_tune(&Tune::default());
        assert_eq!(m, WidgetMetrics::DEFAULT);
        assert_eq!(m, WidgetMetrics::default());
        let win = compute(
            &Layout::Vertical,
            &DetailLevel::Medium,
            &WidthScale::Full,
            &ColumnFlow::Row,
            &Shape::plain(2),
        );
        assert_eq!(win.metrics, WidgetMetrics::DEFAULT);
    }

    #[test]
    fn the_text_size_scales_rows_and_columns() {
        let base = WidgetMetrics::DEFAULT;
        let big = metrics(1.5, 1.0, 1.0, 0.0);
        for detail in [
            DetailLevel::Compact,
            DetailLevel::Medium,
            DetailLevel::Expanded,
        ] {
            let (a, b) = (
                tuned(&base, Layout::Vertical, detail.clone(), ColumnFlow::Row, 3),
                tuned(&big, Layout::Vertical, detail.clone(), ColumnFlow::Row, 3),
            );
            let (ra, rb) = (row_rects(&a), row_rects(&b));
            assert!(
                (rb[0].h - ra[0].h * 1.5).abs() < 0.01,
                "{detail:?} row height"
            );
            assert!((b.width - a.width * 1.5).abs() < 0.01, "{detail:?} width");
            assert_eq!(rb[0].x, ra[0].x, "the padding does not follow the text");
            assert_eq!(b.row_tier, a.row_tier, "{detail:?}: the same tier");

            let (a, b) = (
                tuned(
                    &base,
                    Layout::Horizontal,
                    detail.clone(),
                    ColumnFlow::Row,
                    3,
                ),
                tuned(&big, Layout::Horizontal, detail.clone(), ColumnFlow::Row, 3),
            );
            let (ca, cb) = (row_rects(&a), row_rects(&b));
            assert!((cb[0].w - ca[0].w * 1.5).abs() < 0.01, "{detail:?} column");
            assert!((cb[0].h - ca[0].h * 1.5).abs() < 0.01, "{detail:?} column");
        }
    }

    #[test]
    fn the_tier_table_holds_at_every_text_size() {
        for text in [0.8, 1.2, 1.5] {
            let m = metrics(text, 1.0, 1.0, 0.0);
            for detail in [
                DetailLevel::Compact,
                DetailLevel::Medium,
                DetailLevel::Expanded,
            ] {
                for scale in [
                    WidthScale::Full,
                    WidthScale::ThreeQuarters,
                    WidthScale::Half,
                ] {
                    let at = |m: &WidgetMetrics| {
                        compute_tuned(
                            &Layout::Vertical,
                            &detail,
                            &scale,
                            &ColumnFlow::Row,
                            &Shape::plain(2),
                            m,
                        )
                        .row_tier
                    };
                    assert_eq!(
                        at(&m),
                        at(&WidgetMetrics::DEFAULT),
                        "{detail:?} {scale:?} at text {text}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_row_gap_separates_rows_and_columns() {
        let gap = metrics(1.0, 1.0, 1.0, 8.0);
        let base = WidgetMetrics::DEFAULT;
        let rows = |m| {
            row_rects(&tuned(
                m,
                Layout::Vertical,
                DetailLevel::Medium,
                ColumnFlow::Row,
                3,
            ))
        };
        let (a, b) = (rows(&base), rows(&gap));
        assert_eq!(b[0].y, a[0].y, "nothing above the first row");
        assert!((b[1].y - (b[0].y + b[0].h) - 8.0).abs() < 0.01);
        assert!((b[2].y - a[2].y - 16.0).abs() < 0.01);
        for flow in [ColumnFlow::Row, ColumnFlow::Column] {
            let win_a = tuned(&base, Layout::Horizontal, DetailLevel::Medium, flow, 3);
            let win_b = tuned(&gap, Layout::Horizontal, DetailLevel::Medium, flow, 3);
            let (a, b) = (row_rects(&win_a), row_rects(&win_b));
            let between = |c: &[Rect]| match flow {
                ColumnFlow::Row => c[1].x - (c[0].x + c[0].w),
                ColumnFlow::Column => c[1].y - (c[0].y + c[0].h),
            };
            assert!((between(&b) - between(&a) - 8.0).abs() < 0.01, "{flow:?}");
        }
    }

    #[test]
    fn a_thicker_bar_makes_medium_and_expanded_rows_taller_not_compact_ones() {
        let thick = metrics(1.0, 3.0, 1.0, 0.0);
        for (detail, grows) in [
            (DetailLevel::Compact, 0.0),
            (DetailLevel::Medium, 8.0),
            (DetailLevel::Expanded, 10.0),
        ] {
            let row = |m| {
                row_rects(&tuned(
                    m,
                    Layout::Vertical,
                    detail.clone(),
                    ColumnFlow::Row,
                    1,
                ))[0]
            };
            let (a, b) = (row(&WidgetMetrics::DEFAULT), row(&thick));
            assert!(
                (b.h - a.h - grows).abs() < 0.01,
                "{detail:?}: {} vs {}",
                b.h,
                a.h
            );
        }
    }

    #[test]
    fn padding_grows_the_frame_around_the_same_rows() {
        for layout in [Layout::Vertical, Layout::Horizontal] {
            let at = |pad| {
                tuned(
                    &metrics(1.0, 1.0, pad, 0.0),
                    layout.clone(),
                    DetailLevel::Medium,
                    ColumnFlow::Row,
                    2,
                )
            };
            let (none, base, wide) = (at(0.0), at(1.0), at(2.0));
            let (rn, rb, rw) = (row_rects(&none), row_rects(&base), row_rects(&wide));
            assert_eq!(rn[0].x, 0.0, "{layout:?}: no padding at all");
            assert!((rw[0].x - rb[0].x * 2.0).abs() < 0.01);
            assert!(
                (rw[0].w - rb[0].w).abs() < 0.01,
                "{layout:?}: the rows keep their width"
            );
            assert!((wide.width - base.width - rb[0].x * 2.0).abs() < 0.01);
            assert!(wide.height > base.height && base.height > none.height);
        }
    }

    #[test]
    fn a_model_line_follows_the_text_size() {
        let shape = Shape {
            model_lines: vec![true, false],
            choose: false,
            unshown: false,
        };
        let m = metrics(1.5, 1.0, 1.0, 0.0);
        let win = compute_tuned(
            &Layout::Vertical,
            &DetailLevel::Expanded,
            &WidthScale::Full,
            &ColumnFlow::Row,
            &shape,
            &m,
        );
        let line = win.model_lines[0].expect("the first row's line");
        assert!((line.h - MODEL_LINE_H * 1.5).abs() < 0.01);
    }

    /// A longer countdown leaves a Medium model less room beside it: a
    /// session row that shares its meta line at half width in the standard
    /// style gets a line of its own in the long one.
    #[test]
    fn a_longer_countdown_can_give_a_medium_model_its_own_line() {
        let ui = ui(
            Layout::Vertical,
            DetailLevel::Medium,
            WidthScale::Half,
            ColumnFlow::Row,
        );
        let rows = [session(true)];
        let with = |countdown| {
            let m = WidgetMetrics {
                countdown,
                ..WidgetMetrics::DEFAULT
            };
            Shape::of_tuned(&ui, &m, &rows, false, mono).model_lines
        };
        assert_eq!(with(Countdown::Standard), [false]);
        assert_eq!(with(Countdown::Long), [true]);
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
