// platform/mod.rs — small OS-specific helpers.

#[cfg(target_os = "windows")]
mod win;
#[cfg(target_os = "windows")]
mod win_uia;

pub mod taskbar_geom;

// Taskbar embedding for the mini panel (Windows-only by nature; the
// callers in ui/taskbar_panel.rs are cfg-gated accordingly).
#[cfg(target_os = "windows")]
pub use win::{
    bring_to_front, create_tooltip_window, describe_window, destroy_window, ensure_on_screen,
    foreground_scrim_active, fullscreen_foreground_active, fullscreen_hold_remaining, hide_window,
    install_taskbar_watch, mouse_hover_time_ms, place_below, point_owner, popup_menu_over,
    present_layered, process_cpu_time, raise_panel_topmost, register_fullscreen_watch,
    secondary_taskbars, taskbar_drawn_at, taskbar_slot, wake_in, watch_taskbar,
    window_is_popup_menu, TaskbarSlot,
};

/// Whether this copy runs inside an MSIX package — the Microsoft Store
/// build. Such a copy is installed, started at login and updated by the
/// Store, and it must not reach into the user's registry, so the paths that
/// do those things for the installer build stand down. Always false off
/// Windows.
pub fn is_packaged() -> bool {
    #[cfg(target_os = "windows")]
    {
        win::is_packaged()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// The identity toasts are shown under: the package's own inside the Store
/// build, the registered `napxlexn.AILimits` everywhere else. Empty off
/// Windows, where toasts are only logged.
pub fn toast_app_id() -> String {
    #[cfg(target_os = "windows")]
    {
        win::toast_app_id()
    }
    #[cfg(not(target_os = "windows"))]
    {
        String::new()
    }
}

/// Register the unpackaged identity's display name and icon with the
/// notification platform, so a toast is attributed to AI Limits. No-op in a
/// package (the Store registered it) and off Windows.
pub fn register_toast_identity() {
    #[cfg(target_os = "windows")]
    {
        win::register_toast_identity()
    }
}

/// Whether menus are drawn at 200% or more (a 16 px menu icon would be
/// upscaled there; the 32 px one is used instead). False off Windows.
pub fn menu_scale_is_200() -> bool {
    #[cfg(target_os = "windows")]
    {
        win::menu_scale_is_200()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Hand a URL to the default browser. Logged only off Windows.
pub fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    {
        win::open_url(url)
    }
    #[cfg(not(target_os = "windows"))]
    {
        tracing::info!("open: {url}");
    }
}

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
