// app/widget.rs — the overlay window itself: its glass, showing and
// raising it, and laying it out again after a change.

use crate::{
    config::schema::Config,
    ui::{context_menu::ContextMenu, layout, theme::ComputedTheme, window::WindowState},
};
use std::num::NonZeroU32;
use std::rc::Rc;
use tao::dpi::{PhysicalPosition, PhysicalSize};
use tracing::warn;

#[cfg(target_os = "windows")]
pub(super) fn apply_windows_glass(window: &tao::window::Window) {
    use std::ffi::c_void;
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::HWND as WinHwnd;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
    use windows_sys::Win32::Foundation::{BOOL, HWND};
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

    #[repr(C)]
    struct AccentPolicy {
        accent_state: u32,
        accent_flags: u32,
        gradient_color: u32,
        animation_id: u32,
    }

    #[repr(C)]
    struct WindowCompositionAttribData {
        attrib: u32,
        data: *mut c_void,
        size: usize,
    }

    type SetWindowCompositionAttribute =
        unsafe extern "system" fn(HWND, *mut WindowCompositionAttribData) -> BOOL;

    const WCA_ACCENT_POLICY: u32 = 0x13;
    const ACCENT_DISABLED: u32 = 0;
    // BLURBEHIND (3), NOT ACRYLICBLURBEHIND (4): acrylic has a known bug —
    // DWM drops the blur on drag/monitor change leaving a solid matte tint.
    const ACCENT_ENABLE_BLURBEHIND: u32 = 3;

    // Glass tint — the single dark theme.
    let (tint_r, tint_g, tint_b, tint_a): (u32, u32, u32, u32) = (18, 18, 18, 28);

    // Resolved ONCE. This function runs on every focus change, every scale
    // change and every drag that ends on another monitor; calling LoadLibraryA
    // each time bumps user32's reference count with no matching FreeLibrary and
    // re-resolves an export that cannot move. `None` caches the failure too, so
    // a machine without the export does not retry on every focus.
    static COMPOSITION_PROC: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();

    unsafe {
        let hwnd = window.hwnd() as HWND;
        let proc = *COMPOSITION_PROC.get_or_init(|| {
            let user32 = LoadLibraryA(c"user32.dll".as_ptr().cast());
            if user32.is_null() {
                warn!("acrylic accent unavailable: failed to load user32.dll");
                return None;
            }
            match GetProcAddress(user32, c"SetWindowCompositionAttribute".as_ptr().cast()) {
                Some(p) => Some(p as usize),
                None => {
                    warn!("acrylic accent unavailable: SetWindowCompositionAttribute missing");
                    None
                }
            }
        });
        if let Some(proc) = proc {
            let set_window_composition_attribute: SetWindowCompositionAttribute =
                std::mem::transmute(proc);
            let apply_state = |state: u32, color: u32| {
                let mut accent = AccentPolicy {
                    accent_state: state,
                    accent_flags: 0,
                    // AABBGGRR format.
                    gradient_color: color,
                    animation_id: 0,
                };
                let mut data = WindowCompositionAttribData {
                    attrib: WCA_ACCENT_POLICY,
                    data: &mut accent as *mut _ as _,
                    size: std::mem::size_of_val(&accent),
                };
                set_window_composition_attribute(hwnd, &mut data as *mut _);
            };
            // The off→on cycle forces DWM to restart the blur if it got stuck.
            apply_state(ACCENT_DISABLED, 0);
            apply_state(
                ACCENT_ENABLE_BLURBEHIND,
                tint_r | (tint_g << 8) | (tint_b << 16) | (tint_a << 24),
            );
        }

        // Native Win11 rounded corners: DWMWA_WINDOW_CORNER_PREFERENCE=33, ROUND=2.
        let pref: u32 = 2;
        let _ = DwmSetWindowAttribute(
            WinHwnd(window.hwnd() as _),
            DWMWINDOWATTRIBUTE(33),
            &pref as *const u32 as _,
            std::mem::size_of::<u32>() as u32,
        );

        // Win11 strokes a 1px border around a rounded top-level window
        // (DWMWA_BORDER_COLOR=34) whose default dark-mode color is a faint light
        // grey — a visible hairline. DWMWA_COLOR_NONE (0xFFFFFFFE) removes it
        // entirely; with the undecorated shadow off (client == window, see the
        // builder) there is no 1px frame left to expose, so the edge is just the
        // widget's own translucent fill — seamless on any backdrop, no hairline.
        let border_none: u32 = 0xFFFF_FFFE;
        let _ = DwmSetWindowAttribute(
            WinHwnd(window.hwnd() as _),
            DWMWINDOWATTRIBUTE(34),
            &border_none as *const u32 as _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// Left-click on the tray icon or mini panel: if the overlay is already visible
/// and the front-most window at its own centre, hide it; otherwise (hidden, or
/// covered by another window) show it and raise it to the front. So a single
/// click always brings the widget forward, and a second one tucks it away.
pub(super) fn toggle_or_raise_widget(
    window: &tao::window::Window,
    win_state: &WindowState,
    window_visible: &mut bool,
) {
    #[cfg(target_os = "windows")]
    use tao::platform::windows::WindowExtWindows;
    #[cfg(not(target_os = "windows"))]
    let _ = win_state; // coverage test is Windows-only
    #[cfg(target_os = "windows")]
    let already_front = {
        let hwnd = window.hwnd();
        let cx = (win_state.pos.0 + win_state.size.0 / 2.0) as i32;
        let cy = (win_state.pos.1 + win_state.size.1 / 2.0) as i32;
        *window_visible && crate::platform::point_owner(cx, cy) == hwnd
    };
    #[cfg(not(target_os = "windows"))]
    let already_front = *window_visible;

    if already_front {
        *window_visible = false;
        window.set_visible(false);
    } else {
        show_widget(window, window_visible);
    }
}

/// Show the overlay and raise it to the front of its band, without taking
/// the focus.
pub(super) fn show_widget(window: &tao::window::Window, window_visible: &mut bool) {
    #[cfg(target_os = "windows")]
    use tao::platform::windows::WindowExtWindows;
    *window_visible = true;
    window.set_visible(true);
    #[cfg(target_os = "windows")]
    {
        apply_windows_glass(window);
        crate::platform::bring_to_front(window.hwnd());
    }
    window.request_redraw();
}

/// Apply a live UI change: recompute the layout/theme, resize the window
/// and the buffers.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_ui_change(
    config: &Config,
    win_layout: &mut layout::WindowLayout,
    theme: &mut ComputedTheme,
    pixmap: &mut tiny_skia::Pixmap,
    surface: &mut softbuffer::Surface<Rc<tao::window::Window>, Rc<tao::window::Window>>,
    window: &Rc<tao::window::Window>,
    win_state: &mut WindowState,
    menu: &ContextMenu,
    shape: &layout::Shape,
) {
    *win_layout = layout::compute(
        &config.ui.layout,
        &config.ui.detail,
        &config.ui.width_scale,
        &config.ui.column_flow,
        shape,
    );
    *theme = ComputedTheme::compute(&config.ui);

    let (w, h) = (win_layout.width as u32, win_layout.height as u32);
    window.set_inner_size(PhysicalSize::new(w, h));
    if let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) {
        if let Err(e) = surface.resize(nw, nh) {
            warn!("surface resize failed: {e}");
        }
    }
    if let Some(pm) = tiny_skia::Pixmap::new(w, h) {
        *pixmap = pm;
    }
    win_state.size = (win_layout.width as f64, win_layout.height as f64);

    // Resizing keeps the top-left corner, so a widget parked against the right
    // or bottom edge would grow straight off the screen. Pull it back into the
    // work area of the monitor it sits on.
    if let Ok(wp) = window.outer_position() {
        let pos = (wp.x as f64, wp.y as f64);
        let centre = (
            (pos.0 + win_state.size.0 / 2.0) as i32,
            (pos.1 + win_state.size.1 / 2.0) as i32,
        );
        if let Some((l, t, r, b)) = crate::platform::work_area_at(centre.0, centre.1) {
            let work = (l as f64, t as f64, r as f64, b as f64);
            let (nx, ny) = crate::ui::window::clamp_to_work_area(pos, win_state.size, work);
            if (nx, ny) != pos {
                window.set_outer_position(PhysicalPosition::new(nx as i32, ny as i32));
                win_state.pos = (nx, ny);
            }
        }
    }

    menu.sync(config, win_state.pinned);
    window.request_redraw();
}
