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
pub fn should_fall_back(scrim_here: bool, covered: bool, fullscreen_here: bool) -> bool {
    scrim_here || covered || fullscreen_here
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
        assert!(should_fall_back(true, false, false), "start menu scrim");
        assert!(should_fall_back(false, true, false), "something covers it");
        assert!(should_fall_back(false, false, true), "fullscreen app");
    }

    #[test]
    fn an_unobstructed_panel_keeps_the_overlay() {
        assert!(!should_fall_back(false, false, false));
    }
}
