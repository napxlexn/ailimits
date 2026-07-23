// ui/window.rs — floating window state and drag handling.
//
// Drag: hold LMB anywhere in the window and move.

/// Drag operation state.
#[derive(Debug, Clone, Copy, Default)]
pub struct DragState {
    /// Whether a drag is in progress.
    pub active: bool,
    /// Cursor screen position at press time.
    pub start_cursor: (f64, f64),
    /// Window position at press time.
    pub start_window: (f64, f64),
}

/// Window state.
#[derive(Debug, Clone, Default)]
pub struct WindowState {
    /// Current window screen position.
    pub pos: (f64, f64),
    /// Current window size.
    pub size: (f64, f64),
    /// Last known cursor position inside the window.
    pub cursor_in_window: (f64, f64),
    /// LMB pressed but the drag has not started yet: it starts on the FIRST
    /// CursorMoved, because cursor_in_window at press time can be stale
    /// (e.g. right after the context menu) — otherwise the window would
    /// jump to the click point.
    pub drag_pending: bool,
    pub drag: DragState,
    /// Pinned on top of other windows.
    pub pinned: bool,
}

impl WindowState {
    /// Start a drag: remember the starting positions.
    pub fn begin_drag(&mut self, cursor_screen: (f64, f64)) {
        self.drag = DragState {
            active: true,
            start_cursor: cursor_screen,
            start_window: self.pos,
        };
    }

    /// Update the position during a drag; returns the new window position
    /// if a drag is active. With `magnet = Some((work_area, range))` (Shift held)
    /// the position is softly pulled toward the nearest work-area edge.
    pub fn update_drag(
        &mut self,
        cursor_screen: (f64, f64),
        magnet: Option<((f64, f64, f64, f64), f64)>,
    ) -> Option<(i32, i32)> {
        if !self.drag.active {
            return None;
        }
        let mut x = self.drag.start_window.0 + (cursor_screen.0 - self.drag.start_cursor.0);
        let mut y = self.drag.start_window.1 + (cursor_screen.1 - self.drag.start_cursor.1);
        if let Some((work, range)) = magnet {
            let (sx, sy) = magnet_snap((x, y), self.size, work, range);
            x = sx;
            y = sy;
        }
        self.pos = (x, y);
        Some((x.round() as i32, y.round() as i32))
    }

    /// End the drag; returns true if a drag was active (position must be saved).
    pub fn end_drag(&mut self) -> bool {
        let was_active = self.drag.active;
        self.drag.active = false;
        self.drag_pending = false;
        was_active
    }
}

/// Pull range (physical px) of the Shift-drag edge magnet.
pub const MAGNET_RANGE: f64 = 64.0;

/// Soft "gravity-well" snap toward the nearest work-area edge. Within `range` px
/// of an edge the position eases toward flush with it — smoothstep, so the pull
/// is gentle far out, firmer near the edge, and exactly flush at 0 — while
/// beyond `range` the position is left untouched. X and Y are handled
/// independently so a corner snaps on both axes. `pos`/`size` are the window's
/// top-left and size; `work` is (left, top, right, bottom) of the monitor work
/// area. Nothing locks: as the cursor pulls away the distance grows and the pull
/// fades, so the drag stays controllable.
pub fn magnet_snap(
    pos: (f64, f64),
    size: (f64, f64),
    work: (f64, f64, f64, f64),
    range: f64,
) -> (f64, f64) {
    fn axis(v: f64, ext: f64, lo: f64, hi: f64, range: f64) -> f64 {
        if range <= 0.0 {
            return v;
        }
        let d_lo = (v - lo).abs();
        let d_hi = (v + ext - hi).abs();
        let (dist, target) = if d_lo <= d_hi {
            (d_lo, lo)
        } else {
            (d_hi, hi - ext)
        };
        if dist >= range {
            return v;
        }
        let p = 1.0 - dist / range; // 0 at the range edge, 1 flush against it
        let pull = p * p * (3.0 - 2.0 * p); // smoothstep ease-in
        v + (target - v) * pull
    }
    (
        axis(pos.0, size.0, work.0, work.2, range),
        axis(pos.1, size.1, work.1, work.3, range),
    )
}

#[cfg(test)]
mod tests {
    use super::magnet_snap;

    const WORK: (f64, f64, f64, f64) = (0.0, 0.0, 1000.0, 800.0);
    const SIZE: (f64, f64) = (200.0, 100.0);

    #[test]
    fn center_is_untouched() {
        assert_eq!(
            magnet_snap((400.0, 350.0), SIZE, WORK, 64.0),
            (400.0, 350.0)
        );
    }

    #[test]
    fn near_edge_snaps_essentially_flush() {
        let (x, _) = magnet_snap((2.0, 350.0), SIZE, WORK, 64.0);
        assert!(x < 1.0, "x={x} should ease ~flush to the left edge (0)");
    }

    #[test]
    fn pull_is_stronger_closer_to_the_edge() {
        let near = magnet_snap((10.0, 350.0), SIZE, WORK, 64.0).0;
        let far = magnet_snap((50.0, 350.0), SIZE, WORK, 64.0).0;
        assert!(near < 10.0 && far < 50.0, "both pulled toward 0");
        assert!(
            (10.0 - near) / 10.0 > (50.0 - far) / 50.0,
            "the nearer edge pulls a larger fraction"
        );
    }

    #[test]
    fn corner_snaps_on_both_axes_toward_far_edges() {
        // window right = 990 (10px from 1000), bottom = 790 (10px from 800)
        let (x, y) = magnet_snap((790.0, 690.0), SIZE, WORK, 64.0);
        assert!(x > 790.0, "pulled toward the right edge (target 800)");
        assert!(y > 690.0, "pulled toward the bottom edge (target 700)");
    }

    #[test]
    fn beyond_range_is_free() {
        assert_eq!(magnet_snap((200.0, 350.0), SIZE, WORK, 64.0).0, 200.0);
    }
}
