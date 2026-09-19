// platform/taskbar_geom.rs — placement decisions for the taskbar panel.
//
// Deliberately free of Windows API calls: win.rs gathers handles and
// rectangles, this module decides what they mean. That split is what makes
// the rules testable at all — the shell windows they describe cannot be
// created in a test.

/// Width to keep clear on the right when the notification area cannot be
/// found. Secondary Win11 taskbars have no `TrayNotifyWnd` at all, yet still
/// show a clock; without a reserve the panel is drawn straight over it.
/// 88 DIP is the figure TrafficMonitor uses for the same case.
pub const RIGHT_RESERVE_DIP: f32 = 88.0;

/// Largest manual nudge accepted from the config, in either direction.
pub const MAX_OFFSET: i32 = 200;

/// The taskbar's own scale factor, derived from its height rather than from a
/// DPI query: the bar is the thing we have to match, and a 100% bottom bar is
/// 48px. Clamped so a bogus height cannot explode every derived measurement.
pub fn bar_scale(bar_height: i32) -> f32 {
    if bar_height <= 0 {
        return 1.0;
    }
    (bar_height as f32 / 48.0).clamp(1.0, 3.0)
}

/// Where to pretend the notification area starts when it does not exist.
pub fn estimated_tray_left(bar_right: i32, bar_height: i32) -> i32 {
    bar_right - (RIGHT_RESERVE_DIP * bar_scale(bar_height)).round() as i32
}

/// Keep a hand-edited offset from throwing the panel off the desktop.
pub fn clamp_offset(v: i32) -> i32 {
    v.clamp(-MAX_OFFSET, MAX_OFFSET)
}

/// A rectangle as (left, top, right, bottom), screen coordinates.
pub type Rect = (i32, i32, i32, i32);

/// The monitor edge a taskbar sits on. Windows 11 grew the choice back
/// (Settings > Taskbar behaviors > position, rolling out in 2026); a side bar
/// is a tall strip the width a bottom bar is high.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

impl Edge {
    /// A side bar: its axis runs down the screen.
    pub fn vertical(self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }
}

/// Which edge a bar hugs. Shape first — a side bar is taller than it is wide,
/// however far it has slid — then the nearer of the two monitor edges. Works
/// for an auto-hidden bar too: slid off the bottom it sits mostly below the
/// monitor, and "below" is still nearer the bottom than the top.
pub fn edge_of(bar: Rect, mon: Rect) -> Edge {
    let (l, t, r, b) = bar;
    if b - t > r - l {
        if (l - mon.0).abs() <= (mon.2 - r).abs() {
            Edge::Left
        } else {
            Edge::Right
        }
    } else if (t - mon.1).abs() <= (mon.3 - b).abs() {
        Edge::Top
    } else {
        Edge::Bottom
    }
}

/// The bar's thickness: its height lying down, its width standing up.
pub fn thickness(edge: Edge, bar: Rect) -> i32 {
    if edge.vertical() {
        bar.2 - bar.0
    } else {
        bar.3 - bar.1
    }
}

/// Whether the bar is on screen. Auto-hide slides it off its own edge and
/// leaves a sliver; it counts as visible while more than half its thickness
/// is still on the monitor.
pub fn bar_visible(edge: Edge, bar: Rect, mon: Rect) -> bool {
    let on = match edge {
        Edge::Bottom => mon.3 - bar.1,
        Edge::Top => bar.3 - mon.1,
        Edge::Left => bar.2 - mon.0,
        Edge::Right => mon.2 - bar.0,
    };
    on > thickness(edge, bar) / 2
}

/// Where the notification area is taken to start when it cannot be found:
/// a reserve back from the bar's far end, along its axis. A side bar stacks
/// the clock's two lines over the icons, so it needs more of its length.
pub fn estimated_tray_start(edge: Edge, bar: Rect) -> i32 {
    let scale = bar_scale(thickness(edge, bar));
    if edge.vertical() {
        bar.3 - (SIDE_RESERVE_DIP * scale).round() as i32
    } else {
        bar.2 - (RIGHT_RESERVE_DIP * scale).round() as i32
    }
}

/// Length to keep clear at the foot of a side bar without a `TrayNotifyWnd`:
/// the stacked clock, the bell and the icons above them.
pub const SIDE_RESERVE_DIP: f32 = 150.0;

/// The panel's top-left: just before the tray along the bar's axis, centred
/// across its thickness. `tray_start` is the tray's near edge along the axis
/// (its left on a horizontal bar, its top on a side one).
pub fn panel_origin(
    edge: Edge,
    bar: Rect,
    tray_start: i32,
    size: (i32, i32),
    margin: i32,
) -> (i32, i32) {
    let (w, h) = size;
    if edge.vertical() {
        (bar.0 + (bar.2 - bar.0 - w) / 2, tray_start - h - margin)
    } else {
        (tray_start - w - margin, bar.1 + (bar.3 - bar.1 - h) / 2)
    }
}

/// Whether the stretch between the last app button and the tray holds the
/// panel with its margins. An unknown band end is taken as room: the scan is
/// best-effort, and a panel withheld on a guess is worse than one drawn.
pub fn room_for(band_end: Option<i32>, tray_start: i32, len: i32, margin: i32) -> bool {
    match band_end {
        None => true,
        Some(end) => tray_start - end >= len + 2 * margin,
    }
}

/// Where the row of app buttons ends, read off the bar's own pixels. Given
/// one score per pixel along the bar's axis (how far that column strays from
/// the bar's background, icons scoring high and the acrylic low), the last
/// scoring column before `tray_start` is the band's end; `skip` is a span to
/// treat as empty — the panel's own footprint, which would otherwise count
/// as an icon and push itself out. Coordinates are along the axis, with
/// `origin` the bar's start.
pub fn band_end_from_scores(
    scores: &[u32],
    origin: i32,
    tray_start: i32,
    skip: Option<(i32, i32)>,
    threshold: u32,
) -> Option<i32> {
    let stop = (tray_start - origin - 4).clamp(0, scores.len() as i32) as usize;
    (0..stop)
        .rev()
        .find(|&i| {
            let at = origin + i as i32;
            let skipped = skip.is_some_and(|(a, b)| at >= a && at < b);
            !skipped && scores[i] > threshold
        })
        .map(|i| origin + i as i32 + 1)
}

/// Where the notification area starts when the bar has no `TrayNotifyWnd`
/// to ask, read off the same scores: the tray is the cluster of busy columns
/// at the bar's far end (icons, the clock, the bell, separated by a few
/// pixels at most), and it ends at the first quiet stretch of `gap` columns
/// walking back from the end. None when the bar is busy all the way, or
/// empty: nothing to go on, and the caller keeps its estimate.
pub fn tray_start_from_scores(
    scores: &[u32],
    origin: i32,
    skip: Option<(i32, i32)>,
    gap: usize,
    threshold: u32,
) -> Option<i32> {
    let quiet = |i: usize| {
        let at = origin + i as i32;
        skip.is_some_and(|(a, b)| at >= a && at < b) || scores[i] <= threshold
    };
    let n = scores.len();
    // past the far end's own quiet border, into the cluster
    let mut i = n;
    while i > 0 && quiet(i - 1) {
        i -= 1;
    }
    if i == n {
        return None;
    }
    let mut run = 0usize;
    while i > 0 {
        i -= 1;
        if quiet(i) {
            run += 1;
            if run >= gap {
                return Some(origin + (i + gap) as i32);
            }
        } else {
            run = 0;
        }
    }
    None
}

/// Put taskbars in a stable, human-meaningful order: left to right by the
/// monitor they sit on. The shell hands them over in whatever order it
/// happens to enumerate, which would make a saved display index point
/// somewhere else after a reboot.
pub fn order_bars(bars: &mut [(isize, i32)]) {
    bars.sort_by_key(|&(hwnd, left)| (left, hwnd));
}

/// Whether the Panel indicator must hand over to the tray icon.
///
/// Every input is evaluated against the monitor the panel actually occupies.
/// That scoping is the whole point: a Start menu or a fullscreen game on
/// ANOTHER display must not blank a panel that is plainly visible, and a
/// fullscreen game on the panel's own display must not be missed.
pub fn should_fall_back(
    scrim_here: bool,
    covered: bool,
    fullscreen_here: bool,
    unavailable: bool,
) -> bool {
    scrim_here || covered || fullscreen_here || unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reserve has to grow with the bar, or it under-reserves on a
    /// high-DPI taskbar and the panel lands on the clock anyway.
    #[test]
    fn the_estimated_tray_edge_scales_with_the_bar() {
        let at_100 = estimated_tray_left(3440, 48);
        let at_200 = estimated_tray_left(3440, 96);
        assert_eq!(at_100, 3440 - 88);
        assert!(
            3440 - at_200 > 3440 - at_100,
            "a taller bar must reserve more: 100% left {at_100}, 200% left {at_200}"
        );
    }

    /// A stray bar height must not produce a reserve wider than the screen.
    #[test]
    fn the_scale_is_bounded_at_both_ends() {
        assert_eq!(bar_scale(0), 1.0, "a zero-height bar falls back to 1x");
        assert_eq!(bar_scale(480), 3.0, "an absurd bar height is capped");
    }

    #[test]
    fn offsets_are_clamped_to_a_sane_window() {
        assert_eq!(clamp_offset(0), 0);
        assert_eq!(clamp_offset(50), 50);
        assert_eq!(clamp_offset(9999), MAX_OFFSET);
        assert_eq!(clamp_offset(-9999), -MAX_OFFSET);
    }

    /// Index-addressed displays are only meaningful if the order is stable.
    /// Enumeration order from the shell is arbitrary, so sort by geometry.
    #[test]
    fn bars_are_ordered_left_to_right_and_ties_are_broken() {
        let mut bars = vec![(0xBB, 3440), (0xAA, 0), (0xCC, 3440)];
        order_bars(&mut bars);
        assert_eq!(
            bars,
            vec![(0xAA, 0), (0xBB, 3440), (0xCC, 3440)],
            "left edge first, handle as the tie-break"
        );
    }

    /// The panel degrades to a tray icon whenever it cannot be seen. Each
    /// input is already scoped to the panel's own monitor by the caller;
    /// this is the rule that combines them.
    #[test]
    fn any_single_obstruction_forces_the_tray_icon() {
        assert!(
            should_fall_back(true, false, false, false),
            "start menu scrim"
        );
        assert!(
            should_fall_back(false, true, false, false),
            "something covers it"
        );
        assert!(
            should_fall_back(false, false, true, false),
            "fullscreen app"
        );
    }

    #[test]
    fn an_unobstructed_panel_keeps_the_overlay() {
        assert!(!should_fall_back(false, false, false, false));
    }

    /// A panel that never made it onto the screen is not "fine" — it is the
    /// case most in need of a substitute, and the one that used to read as
    /// healthy because an absent rectangle cannot be covered by anything.
    #[test]
    fn a_panel_that_cannot_place_itself_hands_over_to_the_tray() {
        assert!(should_fall_back(false, false, false, true));
    }

    const MON: Rect = (3440, 0, 6000, 1440);

    /// The four positions, each shown and each slid away by auto-hide: the
    /// shape decides the axis, the nearer edge decides the side.
    #[test]
    fn every_edge_is_recognised_shown_or_hidden() {
        assert_eq!(edge_of((3440, 1392, 6000, 1440), MON), Edge::Bottom);
        assert_eq!(
            edge_of((3440, 1438, 6000, 1486), MON),
            Edge::Bottom,
            "slid down"
        );
        assert_eq!(edge_of((3440, 0, 6000, 48), MON), Edge::Top);
        assert_eq!(edge_of((3440, -46, 6000, 2), MON), Edge::Top, "slid up");
        assert_eq!(edge_of((3440, 0, 3488, 1440), MON), Edge::Left);
        assert_eq!(edge_of((3394, 0, 3442, 1440), MON), Edge::Left, "slid left");
        assert_eq!(edge_of((5952, 0, 6000, 1440), MON), Edge::Right);
        assert_eq!(
            edge_of((5998, 0, 6046, 1440), MON),
            Edge::Right,
            "slid right"
        );
    }

    #[test]
    fn a_bar_is_visible_while_most_of_it_is_on_the_monitor() {
        assert!(bar_visible(Edge::Bottom, (3440, 1392, 6000, 1440), MON));
        assert!(!bar_visible(Edge::Bottom, (3440, 1438, 6000, 1486), MON));
        assert!(
            bar_visible(Edge::Bottom, (3440, 1410, 6000, 1458), MON),
            "mid-slide, more than half in"
        );
        assert!(bar_visible(Edge::Top, (3440, 0, 6000, 48), MON));
        assert!(!bar_visible(Edge::Top, (3440, -46, 6000, 2), MON));
        assert!(bar_visible(Edge::Left, (3440, 0, 3488, 1440), MON));
        assert!(!bar_visible(Edge::Left, (3394, 0, 3442, 1440), MON));
        assert!(bar_visible(Edge::Right, (5952, 0, 6000, 1440), MON));
        assert!(!bar_visible(Edge::Right, (5998, 0, 6046, 1440), MON));
    }

    /// Lying down the panel stands before the tray, centred in the bar's
    /// height; standing up it sits above the tray, centred in the bar's width.
    #[test]
    fn the_panel_sits_before_the_tray_along_the_axis() {
        let bottom = (3440, 1392, 6000, 1440);
        assert_eq!(
            panel_origin(Edge::Bottom, bottom, 5740, (119, 48), 10),
            (5740 - 119 - 10, 1392)
        );
        let top = (3440, 0, 6000, 48);
        assert_eq!(
            panel_origin(Edge::Top, top, 5740, (119, 40), 10),
            (5740 - 119 - 10, 4)
        );
        let right = (5952, 0, 6000, 1440);
        assert_eq!(
            panel_origin(Edge::Right, right, 1200, (48, 60), 10),
            (5952, 1200 - 60 - 10)
        );
        let left = (3440, 0, 3488, 1440);
        assert_eq!(
            panel_origin(Edge::Left, left, 1200, (40, 60), 10),
            (3444, 1130)
        );
    }

    #[test]
    fn the_side_reserve_is_longer_than_the_bottom_one() {
        let bottom = estimated_tray_start(Edge::Bottom, (3440, 1392, 6000, 1440));
        let right = estimated_tray_start(Edge::Right, (5952, 0, 6000, 1440));
        assert_eq!(bottom, 6000 - 88);
        assert_eq!(right, 1440 - 150);
    }

    /// No room means no panel; an unknown band end never withholds it.
    #[test]
    fn room_is_the_gap_between_the_buttons_and_the_tray() {
        assert!(room_for(None, 5740, 119, 10));
        assert!(room_for(Some(5392), 5740, 119, 10), "348px for 139");
        assert!(!room_for(Some(5620), 5740, 119, 10), "120px for 139");
        assert!(room_for(Some(5601), 5740, 119, 10), "exactly 139");
    }

    /// A bar with no tray window: the tray is the busy cluster at the far
    /// end, and it starts after the first quiet stretch walking back from
    /// there. Small gaps inside the cluster (icon to clock, clock to bell)
    /// do not split it; the panel's own footprint reads as quiet.
    #[test]
    fn the_tray_starts_after_the_first_quiet_stretch_before_the_far_end() {
        let mut scores = vec![5u32; 600];
        scores[100..300].fill(300); // buttons
        scores[400..470].fill(300); // tray icons
        scores[478..540].fill(300); // the clock, 8px on
        scores[548..580].fill(300); // the bell
        assert_eq!(
            tray_start_from_scores(&scores, 3440, None, 20, 28),
            Some(3440 + 400)
        );
        assert_eq!(
            band_end_from_scores(&scores, 3440, 3440 + 400, None, 28),
            Some(3440 + 300)
        );
        scores[330..380].fill(200); // the panel already drawn in the gap
        assert_eq!(
            tray_start_from_scores(&scores, 3440, None, 20, 28),
            Some(3440 + 400),
            "20px left of the panel"
        );
        assert_eq!(
            tray_start_from_scores(&scores, 3440, Some((3440 + 330, 3440 + 380)), 20, 28),
            Some(3440 + 400)
        );
        assert_eq!(
            tray_start_from_scores(&[5u32; 100], 0, None, 20, 28),
            None,
            "an empty bar"
        );
        assert_eq!(
            tray_start_from_scores(&[300u32; 100], 0, None, 20, 28),
            None,
            "a bar busy end to end"
        );
    }

    /// The scan: icons score high, the acrylic low; the last high column
    /// before the tray is the end, and the panel's own footprint is skipped.
    #[test]
    fn the_band_ends_at_the_last_busy_column_outside_the_panels_footprint() {
        let mut scores = vec![5u32; 400];
        scores[100..180].fill(300); // the buttons
        scores[260..300].fill(200); // the panel, already drawn
        assert_eq!(
            band_end_from_scores(&scores, 1000, 1000 + 360, None, 28),
            Some(1300),
            "the panel counts without a skip"
        );
        assert_eq!(
            band_end_from_scores(&scores, 1000, 1000 + 360, Some((1260, 1300)), 28),
            Some(1180)
        );
        assert_eq!(
            band_end_from_scores(&[3u32; 100], 0, 90, None, 28),
            None,
            "an empty bar has no band"
        );
    }
}
