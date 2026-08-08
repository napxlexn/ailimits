// platform/win.rs — Windows idle detection, tray icon promotion and
// taskbar embedding for the mini panel.

use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// Where the mini panel sits: an always-on-top overlay OVER the taskbar,
/// left of the notification area, in SCREEN coordinates. A plain child of
/// `Shell_TrayWnd` is invisible on Win11 — the taskbar's XAML composition
/// layer paints over every classic child HWND regardless of z-order — so
/// the panel floats above instead (the approach XMeters/TrafficMonitor
/// converged on for Win11).
pub struct TaskbarSlot {
    /// Screen X of the tray's left edge (the panel goes left of it).
    pub tray_left: i32,
    /// Taskbar top in screen coordinates.
    pub top: i32,
    /// Taskbar height.
    pub height: i32,
    /// False while the auto-hidden taskbar is slid off-screen.
    pub visible: bool,
    /// False when `TrayNotifyWnd` could not be found and `tray_left` is an
    /// estimate. Secondary Win11 taskbars have no notification area window.
    pub tray_found: bool,
}

/// Locate the primary taskbar and its notification area (screen coords).
pub fn taskbar_slot() -> Option<TaskbarSlot> {
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, GetWindowRect};

    unsafe {
        let Ok(taskbar) = FindWindowW(w!("Shell_TrayWnd"), None) else {
            return None;
        };
        let mut bar = RECT::default();
        if GetWindowRect(taskbar, &mut bar).is_err() {
            return None;
        }
        // Auto-hide detection: when slid away, only a sliver of the bar
        // remains on the monitor (bottom taskbar assumed — the Win11 default
        // and the only position Win11 supports).
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let monitor = MonitorFromWindow(taskbar, MONITOR_DEFAULTTONEAREST);
        let visible = if GetMonitorInfoW(monitor, &mut mi).as_bool() {
            (mi.rcMonitor.bottom - bar.top) > (bar.bottom - bar.top) / 2
        } else {
            true
        };
        let (tray_left, tray_found) =
            match FindWindowExW(taskbar, HWND::default(), w!("TrayNotifyWnd"), None) {
                Ok(tray) => {
                    let mut r = RECT::default();
                    if GetWindowRect(tray, &mut r).is_ok() {
                        (r.left, true)
                    } else {
                        (
                            crate::platform::taskbar_geom::estimated_tray_left(
                                bar.right,
                                bar.bottom - bar.top,
                            ),
                            false,
                        )
                    }
                }
                Err(_) => (
                    crate::platform::taskbar_geom::estimated_tray_left(
                        bar.right,
                        bar.bottom - bar.top,
                    ),
                    false,
                ),
            };
        Some(TaskbarSlot {
            tray_left,
            top: bar.top,
            height: bar.bottom - bar.top,
            visible,
            tray_found,
        })
    }
}

/// Present a premultiplied-BGRA image as the ENTIRE content of a per-pixel
/// alpha layered overlay at the given screen rect, topmost, shown without
/// stealing focus. Fully transparent pixels let the taskbar show through, so
/// only the painted digits/bars appear — there is no opaque panel window/box,
/// which is what made the old softbuffer panel look like a pasted rectangle.
/// `bgra` must be premultiplied BGRA, top-down, w*h*4 bytes.
pub fn present_layered(
    hwnd: isize,
    bgra: &[u8],
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<(), u32> {
    use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        BLENDFUNCTION, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, UpdateLayeredWindow, GWL_EXSTYLE,
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, ULW_ALPHA,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    if w <= 0 || h <= 0 || bgra.len() < (w * h * 4) as usize {
        return Err(0);
    }
    unsafe {
        let hwnd = HWND(hwnd as _);
        // Per-pixel alpha + no focus stealing + out of Alt+Tab.
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | WS_EX_LAYERED.0 as isize
                | WS_EX_NOACTIVATE.0 as isize
                | WS_EX_TOOLWINDOW.0 as isize,
        );

        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let result = if let Ok(dib) =
            CreateDIBSection(screen_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        {
            if !bits.is_null() {
                std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, (w * h * 4) as usize);
            }
            let old = SelectObject(mem_dc, HGDIOBJ(dib.0));
            let src = POINT { x: 0, y: 0 };
            let dst = POINT { x, y };
            let size = SIZE { cx: w, cy: h };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let ok = UpdateLayeredWindow(
                hwnd,
                screen_dc,
                Some(&dst),
                Some(&size),
                mem_dc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            let update_result = if ok.is_ok() {
                Ok(())
            } else {
                Err(windows::Win32::Foundation::GetLastError().0)
            };
            SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ(dib.0));
            update_result
        } else {
            Err(windows::Win32::Foundation::GetLastError().0)
        };
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        // Keep it topmost and visible without activating (ULW set geometry).
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );

        result
    }
}

/// If the point (x, y) lies on no monitor, snap it onto the primary monitor's
/// work area; otherwise return it unchanged. Used to rescue a saved overlay
/// position when the monitor it was on is gone (laptop undock, RDP resize,
/// monitor swap) — a borderless, taskbar-skipped, off-screen window is
/// otherwise un-draggable and looks permanently lost.
pub fn ensure_on_screen(x: i32, y: i32) -> (i32, i32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONULL,
        MONITOR_DEFAULTTOPRIMARY,
    };
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL);
        if !mon.is_invalid() {
            return (x, y); // on some monitor — leave it
        }
        // Off every monitor → drop onto the primary work area (a small inset).
        let primary = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(primary, &mut mi).as_bool() {
            (mi.rcWork.left + 24, mi.rcWork.top + 24)
        } else {
            (24, 24)
        }
    }
}

/// Work area (the monitor minus the taskbar — `rcWork`) of the monitor nearest a
/// screen point, as (left, top, right, bottom). None if the query fails. Drives
/// the Shift-drag edge magnet so the widget docks against the usable edge,
/// never under the taskbar.
pub fn work_area_at(x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            let w = mi.rcWork;
            Some((w.left, w.top, w.right, w.bottom))
        } else {
            None
        }
    }
}

/// Whether either Shift key is currently held. Read at drag time to gate the
/// edge magnet — no event plumbing, no idle cost.
pub fn shift_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};
    unsafe { (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 }
}

/// The window the compositor shows at a screen point (WindowFromPoint), as an
/// isize handle. Used to tell whether the floating overlay is actually visible
/// at its own center, or covered by the cursor-peek "rude topmost" taskbar.
pub fn point_owner(x: i32, y: i32) -> isize {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::WindowFromPoint;
    unsafe { WindowFromPoint(POINT { x, y }).0 as isize }
}

/// The system mouse-hover time in ms — how long Windows waits before showing a
/// tray-icon tooltip on hover (SPI_GETMOUSEHOVERTIME). Defaults to 400 ms.
pub fn mouse_hover_time_ms() -> u64 {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETMOUSEHOVERTIME, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    unsafe {
        let mut t: u32 = 0;
        let ok = SystemParametersInfoW(
            SPI_GETMOUSEHOVERTIME,
            0,
            Some(&mut t as *mut u32 as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok();
        if ok && t > 0 {
            t as u64
        } else {
            400
        }
    }
}

/// Re-assert the panel overlay's topmost z-order WITHOUT moving, resizing,
/// activating or repainting it. Cheap recovery for when another topmost window
/// (the tray overflow flyout, the Start menu, a momentarily-topmost app) covers
/// the overlay: the panel only re-asserts topmost on a present, which does not
/// fire on those interactions, so it would stay hidden behind them.
pub fn raise_panel_topmost(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd as _),
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Raise a window to the top of its z-order band and show it, without
/// activating it — so a no-activate overlay comes to the front on a tray/panel
/// click without stealing focus. Uses HWND_TOP (not HWND_TOPMOST), so it does
/// not force the always-on-top band: it respects the window's pin state.
pub fn bring_to_front(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd as _),
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// True when an immersive shell surface that occludes ordinary topmost
/// overlays — the Start menu or Search — is the foreground window. The Start
/// scrim lives in a protected z-band no overlay can beat (proven: even forcing
/// HWND_TOPMOST does not uncover the panel), so the indicator falls back to a
/// tray icon, which the shell keeps visible, while this is true. Matched by the
/// foreground window's host process (Start/Search are served by these on
/// Win11; the exact host varies by build, so several are accepted).
pub fn foreground_scrim_active() -> bool {
    use windows::Win32::Foundation::{CloseHandle, FALSE};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return false;
        }
        let Ok(proc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) else {
            return false;
        };
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            proc,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(proc);
        if !ok {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
        let name = path.rsplit(['\\', '/']).next().unwrap_or(path.as_str());
        matches!(
            name,
            "searchhost.exe"
                | "startmenuexperiencehost.exe"
                | "searchapp.exe"
                | "shellexperiencehost.exe"
        )
    }
}

/// True while a fullscreen app (a game, a video, an F11 browser) owns the
/// screen — the same shell state that suspends notification toasts. The
/// taskbar sits UNDER such a window without moving, so the panel's geometric
/// visibility check cannot see it; without this the topmost overlay floats
/// over the game (and the foreground-raise would even re-assert it there).
/// While true the indicator hides the panel, exactly like the taskbar; the
/// next foreground change (alt-tab back to the desktop) restores it.
pub fn fullscreen_foreground_active() -> bool {
    // Deliberately NOT SHQueryUserNotificationState: that reports a
    // machine-wide state which stays "busy" for as long as a fullscreen game
    // is RUNNING, even while the user is back on the desktop with another
    // window focused — the panel would then stay hidden exactly when it
    // should be back (verified 2026-07-22: QUNS_BUSY with a maximized browser
    // in the foreground). What matters is whether a fullscreen window is in
    // FRONT of the taskbar right now, which the geometric check answers
    // precisely, with no shell-update latency.
    foreground_covers_taskbar_monitor()
}

/// Whether the current foreground window covers the ENTIRE monitor that hosts
/// the taskbar (rcMonitor). The desktop itself (Progman/WorkerW), this
/// process's own windows, and MAXIMIZED windows do not count: with an
/// auto-hide taskbar the work area is the whole monitor, so a maximized
/// window's rect (frame borders included) contains rcMonitor too — but it is
/// an ordinary window the bar slides over, not a fullscreen app. Real games
/// run as popup windows without WS_MAXIMIZE; F11 browser fullscreen is caught
/// by the shell state either way.
fn foreground_covers_taskbar_monitor() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
        GetWindowThreadProcessId, GWL_STYLE, WS_MAXIMIZE,
    };
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return false;
        }
        let style = GetWindowLongPtrW(fg, GWL_STYLE);
        if style as u32 & WS_MAXIMIZE.0 != 0 {
            return false;
        }
        let mut cls = [0u16; 32];
        let n = GetClassNameW(fg, &mut cls);
        if n > 0 {
            let cls = String::from_utf16_lossy(&cls[..n as usize]);
            // The desktop is monitor-sized by definition — not a fullscreen app.
            if cls == "Progman" || cls == "WorkerW" {
                return false;
            }
        }
        let Ok(taskbar) = FindWindowW(w!("Shell_TrayWnd"), None) else {
            return false;
        };
        let bar_monitor = MonitorFromWindow(taskbar, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(bar_monitor, &mut mi).as_bool() {
            return false;
        }
        let mut wr = RECT::default();
        if GetWindowRect(fg, &mut wr).is_err() {
            return false;
        }
        let m = mi.rcMonitor;
        wr.left <= m.left && wr.top <= m.top && wr.right >= m.right && wr.bottom >= m.bottom
    }
}

/// Minimal window procedure for the tooltip window — everything defaults.
unsafe extern "system" fn tip_wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    windows::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Create a raw, click-through, layered, top-level window to host our own hover
/// tooltip. We paint it with `present_layered` (dark, fully rounded, BORDERLESS)
/// to match the shell's tooltip exactly — a native comctl tooltip can't, it
/// always draws a classic theme border. WS_EX_TRANSPARENT lets the cursor fall
/// through to the overlay beneath, so the tip never steals the hover. The tip's
/// content/position is set on each show. Returns 0 on failure.
pub fn create_tooltip_window() -> isize {
    use windows::core::w;
    use windows::Win32::Foundation::{HINSTANCE, HWND};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, HMENU, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let class = w!("AiLimitsTooltip");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(tip_wndproc),
            hInstance: HINSTANCE(hinst.0),
            lpszClassName: class,
            ..Default::default()
        };
        // Best-effort: re-registering returns 0 (already registered), ignored.
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!(""),
            WS_POPUP,
            0,
            0,
            10,
            10,
            HWND::default(),
            HMENU::default(),
            hinst,
            None,
        ) else {
            return 0;
        };
        hwnd.0 as isize
    }
}

/// Hide the panel window (indicator switched away / taskbar slid away).
pub fn hide_window(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    unsafe {
        let _ = ShowWindow(HWND(hwnd as _), SW_HIDE);
    }
}

/// Watch the taskbar for moves/slides (auto-hide) and the notification area
/// for width changes (icons pinned/unpinned), event-driven via
/// `SetWinEventHook` — no polling, keeps the 0% idle CPU budget. The callback
/// fires on THIS thread's message loop and forwards a `TaskbarMoved` user
/// event so the panel can reposition.
pub fn install_taskbar_watch(proxy: tao::event_loop::EventLoopProxy<crate::app::UserEvent>) {
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use windows::core::w;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowExW, FindWindowW, GetDesktopWindow, GetWindowThreadProcessId,
        EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_REORDER, EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW,
        WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
    };

    static PROXY: OnceLock<Mutex<tao::event_loop::EventLoopProxy<crate::app::UserEvent>>> =
        OnceLock::new();
    static TASKBAR: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    static TRAY: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

    unsafe extern "system" fn on_event(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        id_object: i32,
        _id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        let ev = match event {
            // The taskbar itself moved/slid (auto-hide, resolution change), OR
            // the notification area changed width (an icon was pinned/unpinned
            // — the bar stays put, only `TrayNotifyWnd`'s left edge shifts) →
            // reposition the panel. Win11 animates the tray resize, so a pin
            // emits a burst of these; each reposition is a cheap re-present,
            // and following the burst keeps the panel gliding with the icons.
            EVENT_OBJECT_LOCATIONCHANGE if id_object == OBJID_WINDOW.0 => {
                let h = hwnd.0 as isize;
                let tray = TRAY.load(std::sync::atomic::Ordering::Relaxed);
                if h == TASKBAR.load(std::sync::atomic::Ordering::Relaxed)
                    || (tray != 0 && h == tray)
                {
                    crate::app::UserEvent::TaskbarMoved
                } else {
                    return;
                }
            }
            // Some window came to the foreground (the tray overflow flyout, the
            // Start menu, an app) — it may have covered the overlay, which only
            // re-asserts topmost on a present. Re-raise it (cheap, no repaint).
            EVENT_SYSTEM_FOREGROUND => crate::app::UserEvent::PanelRaise,
            // The shell re-stacked top-level z-order (reported on the taskbar or
            // the DESKTOP). The auto-hide bar peeking back can front itself above
            // the floating overlay as a pure z change — no move/foreground event —
            // which a topmost overlay cannot beat. Re-check whether the overlay is
            // now covered so the indicator can fall back to a tray icon.
            EVENT_OBJECT_REORDER => {
                let tb = TASKBAR.load(std::sync::atomic::Ordering::Relaxed);
                let h = hwnd.0 as isize;
                if h == tb || h == GetDesktopWindow().0 as isize {
                    crate::app::UserEvent::PanelRecheck
                } else {
                    return;
                }
            }
            _ => return,
        };
        if let Some(proxy) = PROXY.get() {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(ev);
            }
        }
    }

    unsafe {
        let Ok(taskbar) = FindWindowW(w!("Shell_TrayWnd"), None) else {
            return;
        };
        TASKBAR.store(taskbar.0 as isize, std::sync::atomic::Ordering::Relaxed);
        // The notification-area child: its LOCATIONCHANGE is how we learn that
        // icons were pinned/unpinned (the bar itself does not move then).
        // Resolved once, like the bar — an Explorer restart invalidates both
        // hwnds and the pid-scoped hook alike.
        if let Ok(tray) = FindWindowExW(taskbar, HWND::default(), w!("TrayNotifyWnd"), None) {
            TRAY.store(tray.0 as isize, std::sync::atomic::Ordering::Relaxed);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(taskbar, Some(&mut pid));
        let _ = PROXY.set(Mutex::new(proxy));
        // Scoped to the Explorer process; only the taskbar hwnd passes the
        // callback filter. The hook lives for the process lifetime.
        let hook = SetWinEventHook(
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(on_event),
            pid,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if hook.is_invalid() {
            tracing::warn!("taskbar watch hook failed — panel won't track auto-hide");
        }
        // A second, GLOBAL hook for foreground changes so the panel can re-raise
        // itself above whatever just covered it (the tray overflow flyout etc.).
        let fg_hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if fg_hook.is_invalid() {
            tracing::warn!(
                "foreground watch hook failed — panel may stay covered by the tray overflow"
            );
        }
        // A third, GLOBAL hook for top-level z-order RE-STACKS. The auto-hide bar
        // peeking back fronts itself above the floating overlay as a pure z
        // change (no move/foreground), which a topmost overlay cannot beat — so
        // re-check coverage and fall back to a tray icon. SKIPOWNPROCESS so our
        // own window ops do not echo back into the hook.
        let reorder_hook = SetWinEventHook(
            EVENT_OBJECT_REORDER,
            EVENT_OBJECT_REORDER,
            None,
            Some(on_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if reorder_hook.is_invalid() {
            tracing::warn!(
                "reorder watch hook failed — panel may not fall back to a tray icon when covered"
            );
        }
    }
}

/// Promote this exe's notification icons onto the always-visible taskbar
/// corner. Windows 11 hides new tray icons behind the overflow chevron;
/// the per-icon "always show" toggle is just `IsPromoted=1` under the
/// per-user key `HKCU\Control Panel\NotifyIconSettings\<uid>`, so the
/// widget can flip it for its own icons. Explorer watches the key and
/// applies the change live. A no-op on Windows 10 (no such key) and on
/// any registry error — promotion is best-effort, never fatal.
pub fn promote_tray_icons() -> u32 {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD,
    };

    let Ok(exe) = std::env::current_exe() else {
        return 0;
    };
    let exe = exe.to_string_lossy().to_lowercase();
    let mut promoted = 0u32;

    unsafe {
        let mut root = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Control Panel\\NotifyIconSettings"),
            0,
            KEY_ENUMERATE_SUB_KEYS,
            &mut root,
        )
        .is_err()
        {
            return 0;
        }

        let mut index = 0u32;
        loop {
            let mut name = [0u16; 64];
            let mut name_len = name.len() as u32;
            if RegEnumKeyExW(
                root,
                index,
                windows::core::PWSTR(name.as_mut_ptr()),
                &mut name_len,
                None,
                windows::core::PWSTR::null(),
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            index += 1;

            let mut sub = HKEY::default();
            if RegOpenKeyExW(
                root,
                windows::core::PCWSTR(name.as_ptr()),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                &mut sub,
            )
            .is_err()
            {
                continue;
            }

            // ExecutablePath (REG_SZ) → compare with our exe, case-insensitive.
            let mut buf = [0u8; 1040];
            let mut len = buf.len() as u32;
            let matches = RegQueryValueExW(
                sub,
                w!("ExecutablePath"),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut len),
            )
            .is_ok()
                && {
                    let u16s: Vec<u16> = buf[..len as usize]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .take_while(|&c| c != 0)
                        .collect();
                    String::from_utf16_lossy(&u16s).to_lowercase() == exe
                };

            if matches {
                let one = 1u32.to_le_bytes();
                if RegSetValueExW(sub, w!("IsPromoted"), 0, REG_DWORD, Some(&one)).is_ok() {
                    promoted += 1;
                }
            }
            let _ = RegCloseKey(sub);
        }
        let _ = RegCloseKey(root);
    }
    promoted
}

/// Whether Windows is using the LIGHT system theme — which is what the
/// taskbar and tray follow (distinct from the *app* theme). Reads the
/// per-user `SystemUsesLightTheme` DWORD (1 = light). Defaults to dark
/// (false) on any error, matching the Win11 out-of-box taskbar.
pub fn system_uses_light_theme() -> bool {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    };
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
        .is_err()
        {
            return false;
        }
        let mut data: u32 = 0;
        let mut len: u32 = std::mem::size_of::<u32>() as u32;
        let ok = RegQueryValueExW(
            key,
            w!("SystemUsesLightTheme"),
            None,
            None,
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut len),
        )
        .is_ok();
        let _ = RegCloseKey(key);
        ok && data == 1
    }
}

pub fn user_idle_secs() -> u64 {
    unsafe {
        let mut lii = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut lii).as_bool() {
            // GetTickCount wraps every ~49.7 days; wrapping_sub stays correct.
            let now = GetTickCount();
            (now.wrapping_sub(lii.dwTime) as u64) / 1000
        } else {
            0
        }
    }
}
