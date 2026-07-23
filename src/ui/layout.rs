// ui/layout.rs — layout logic (vertical / horizontal).
//
// The layout hands out zone rectangles; the renderer (the single place
// that draws) does the in-zone arrangement.

use super::theme::Dimensions;
use crate::config::schema::{DetailLevel, Layout};

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

/// Full window layout.
#[derive(Debug, Clone)]
pub struct WindowLayout {
    pub width: f32,
    pub height: f32,
    pub body: BodyLayout,
}

/// Hint area height.
const HINT_HEIGHT: f32 = 20.0 * Dimensions::DENSITY_SCALE;
/// Minimal top padding (there is no header).
const TOP_PADDING: f32 = 4.0;

/// Compute the window layout.
pub fn compute(layout: &Layout, detail: &DetailLevel, provider_count: usize) -> WindowLayout {
    // No visible providers (all unconfigured / disabled): the renderer draws a
    // single guidance line. Force a vertical compact box wide enough for that
    // hint regardless of the user's chosen layout, so it never clips (a
    // horizontal 0-provider window would otherwise be ~60px wide).
    if provider_count == 0 {
        return vertical(&DetailLevel::Compact, 1);
    }
    let n = provider_count.max(1);
    match layout {
        Layout::Vertical => vertical(detail, n),
        Layout::Horizontal => horizontal(detail, n),
        // Legacy config value: the 2x2 mode was removed from the UI.
        Layout::Grid => vertical(detail, n),
    }
}

/// Vertical layout.
fn vertical(detail: &DetailLevel, n: usize) -> WindowLayout {
    let width: f32 = Dimensions::scale(match detail {
        DetailLevel::Compact => 212.0,
        DetailLevel::Medium => 250.0,
        DetailLevel::Expanded => 290.0,
    });
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

    let show_hint = *detail == DetailLevel::Compact;
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
    }
}

/// Horizontal layout: vertical bars with names below.
fn horizontal(detail: &DetailLevel, n: usize) -> WindowLayout {
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
    let width = pad_h * 2.0 + col_w * n as f32;
    let height = TOP_PADDING + body_h + 6.0;

    let mut cols = Vec::with_capacity(n);
    for i in 0..n {
        cols.push(Rect {
            x: pad_h + col_w * i as f32,
            y: TOP_PADDING,
            w: col_w,
            h: body_h,
        });
    }

    WindowLayout {
        width,
        height,
        body: BodyLayout::Horizontal { cols },
    }
}
