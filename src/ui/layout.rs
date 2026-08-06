// ui/layout.rs — layout logic (vertical / horizontal).
//
// The layout hands out zone rectangles; the renderer (the single place
// that draws) does the in-zone arrangement.

use super::renderer::{NAME_WIDTH, PCT_WIDTH, ROW_GAP};
use super::theme::Dimensions;
use crate::config::schema::{ColumnFlow, DetailLevel, Layout, WidthScale};

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
}

/// Hint area height.
const HINT_HEIGHT: f32 = 20.0 * Dimensions::DENSITY_SCALE;
/// Minimal top padding (there is no header).
const TOP_PADDING: f32 = 4.0;
/// Below this a bar carries no information worth the pixels.
const MIN_BAR: f32 = 24.0;
/// Space between provider columns. Stacked, without it one provider's name
/// sits directly against the next one's percentage and the eye cannot tell
/// where a provider ends.
const COLUMN_GAP: f32 = 10.0 * Dimensions::DENSITY_SCALE;

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
            // Name (left) and percent (right) share the top line. Both are
            // drawn with the row's full width as their box, so once they stop
            // fitting side by side they overlap instead of truncating.
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

/// Compute the window layout.
pub fn compute(
    layout: &Layout,
    detail: &DetailLevel,
    width_scale: &WidthScale,
    column_flow: &ColumnFlow,
    provider_count: usize,
) -> WindowLayout {
    // No visible providers (all unconfigured / disabled): the renderer draws a
    // single guidance line. Force a vertical compact box wide enough for that
    // hint regardless of the user's chosen layout, so it never clips (a
    // horizontal 0-provider window would otherwise be ~60px wide).
    if provider_count == 0 {
        return vertical(&DetailLevel::Compact, &WidthScale::Full, 1);
    }
    let n = provider_count.max(1);
    match layout {
        Layout::Vertical => vertical(detail, width_scale, n),
        Layout::Horizontal => horizontal(detail, column_flow, n),
        // Legacy config value: the 2x2 mode was removed from the UI.
        Layout::Grid => vertical(detail, width_scale, n),
    }
}

/// Vertical layout.
fn vertical(detail: &DetailLevel, width_scale: &WidthScale, n: usize) -> WindowLayout {
    let width: f32 = Dimensions::scale(match detail {
        DetailLevel::Compact => 212.0,
        DetailLevel::Medium => 250.0,
        DetailLevel::Expanded => 290.0,
    }) * width_scale.fraction();
    let row_h = Dimensions::row_height(detail);
    let pad_h = Dimensions::PADDING_H;

    let mut rows = Vec::with_capacity(n);
    let mut y = TOP_PADDING;
    for _ in 0..n {
        rows.push(Rect {
            x: pad_h,
            y,
            w: width - pad_h * 2.0,
            h: row_h,
        });
        y += row_h;
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
    }
}

/// Horizontal layout: vertical bars with names below, arranged either side by
/// side or stacked downwards. Only the geometry differs between the two — the
/// column body and the renderer's per-column drawing are shared, because a
/// column is drawn relative to its own rectangle wherever that sits.
fn horizontal(detail: &DetailLevel, flow: &ColumnFlow, n: usize) -> WindowLayout {
    // Compact is tighter horizontally as well.
    let pad_h = match detail {
        DetailLevel::Compact => Dimensions::scale(6.0),
        _ => Dimensions::PADDING_H,
    };
    // Compact has no percent text — narrower columns, bars sit closer together.
    let col_w = Dimensions::scale(match detail {
        DetailLevel::Compact => 46.0,
        DetailLevel::Medium => 66.0,
        DetailLevel::Expanded => 82.0,
    });
    let body_h = Dimensions::scale(match detail {
        DetailLevel::Compact => 42.0,
        DetailLevel::Medium => 68.0,
        DetailLevel::Expanded => 88.0,
    });
    let stacked = *flow == ColumnFlow::Column;
    let gaps = COLUMN_GAP * n.saturating_sub(1) as f32;
    let (width, height) = if stacked {
        (
            pad_h * 2.0 + col_w,
            TOP_PADDING + body_h * n as f32 + gaps + 6.0,
        )
    } else {
        (
            pad_h * 2.0 + col_w * n as f32 + gaps,
            TOP_PADDING + body_h + 6.0,
        )
    };

    let mut cols = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y) = if stacked {
            (pad_h, TOP_PADDING + (body_h + COLUMN_GAP) * i as f32)
        } else {
            (pad_h + (col_w + COLUMN_GAP) * i as f32, TOP_PADDING)
        };
        cols.push(Rect {
            x,
            y,
            w: col_w,
            h: body_h,
        });
    }

    WindowLayout {
        width,
        height,
        body: BodyLayout::Horizontal { cols },
        row_tier: RowTier::Full,
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
            2,
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
            let got = compute(&Layout::Vertical, &detail, &scale, &ColumnFlow::Row, 2).row_tier;
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
            2,
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

    /// Providers must not touch. Stacked, one provider's name would otherwise
    /// sit directly against the next one's percentage with nothing between
    /// them to say where one ends.
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
                    gap >= 8.0,
                    "{flow:?}: providers are {gap} px apart, too close to tell apart"
                );
            }
        }
    }

    fn compute_columns(detail: &DetailLevel, flow: &ColumnFlow, n: usize) -> WindowLayout {
        compute(&Layout::Horizontal, detail, &WidthScale::Full, flow, n)
    }

    #[test]
    fn three_quarters_narrows_the_rows_window_to_three_quarters() {
        let full = full_width(&DetailLevel::Compact);
        let scaled = compute(
            &Layout::Vertical,
            &DetailLevel::Compact,
            &WidthScale::ThreeQuarters,
            &ColumnFlow::Row,
            2,
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
