// platform/mod.rs — small OS-specific helpers.

#[cfg(target_os = "windows")]
mod win;

#[cfg(target_os = "windows")]
pub mod taskbar_geom;

// Taskbar embedding for the mini panel (Windows-only by nature; the
// callers in ui/taskbar_panel.rs are cfg-gated accordingly).
#[cfg(target_os = "windows")]
pub use win::{
    bring_to_front, create_tooltip_window, ensure_on_screen, foreground_scrim_active,
    fullscreen_foreground_active, hide_window, install_taskbar_watch, mouse_hover_time_ms,
    point_owner, present_layered, raise_panel_topmost, secondary_taskbars, taskbar_slot,
    watch_taskbar, TaskbarSlot,
};

/// Pin this exe's tray icons to the visible taskbar corner (Windows 11
/// hides new ones in the overflow). Returns how many icons were promoted.
pub fn promote_tray_icons() -> u32 {
    #[cfg(target_os = "windows")]
    {
        win::promote_tray_icons()
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

/// Whether the OS is using a light taskbar/tray theme (the mini panel paints
/// its foreground to contrast with it). Dark (false) elsewhere / on error.
pub fn system_uses_light_theme() -> bool {
    #[cfg(target_os = "windows")]
    {
        win::system_uses_light_theme()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Work area (monitor minus taskbar) of the monitor nearest a screen point, as
/// (left, top, right, bottom). None on non-Windows / query failure. Used by the
/// Shift-drag edge magnet so the widget docks to the usable edge.
pub fn work_area_at(x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
    #[cfg(target_os = "windows")]
    {
        win::work_area_at(x, y)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (x, y);
        None
    }
}

/// Whether a Shift key is held (gates the edge magnet). False on non-Windows.
pub fn shift_held() -> bool {
    #[cfg(target_os = "windows")]
    {
        win::shift_held()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Seconds since the last keyboard/mouse input (0 if unknown / unsupported).
/// When the workstation is locked there is no input, so this grows past the
/// idle threshold within a few minutes — covering both idle and lock with a
/// single, dependency-light check.
pub fn user_idle_secs() -> u64 {
    #[cfg(target_os = "windows")]
    {
        win::user_idle_secs()
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}
