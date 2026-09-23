// platform/win.rs — Windows idle detection, tray icon promotion and
// taskbar embedding for the mini panel.

use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// Where the mini panel sits: an always-on-top overlay OVER the taskbar,
/// before the notification area along the bar's axis, in SCREEN coordinates.
/// A plain child of `Shell_TrayWnd` is invisible on Win11 — the taskbar's
/// XAML composition layer paints over every classic child HWND regardless of
/// z-order — so the panel floats above instead (the approach
/// XMeters/TrafficMonitor converged on for Win11).
pub struct TaskbarSlot {
    /// The monitor edge the bar sits on.
    pub edge: Edge,
    /// The bar's window rectangle (left, top, right, bottom).
    pub bar: Rect,
    /// The monitor the bar sits on.
    pub mon: Rect,
    /// Where the notification area starts along the bar's axis: its left edge
    /// on a horizontal bar, its top on a side bar. The panel goes before it.
    pub tray_start: i32,
    /// False while the auto-hidden taskbar is slid off-screen.
    pub visible: bool,
    /// False when `TrayNotifyWnd` could not be found and `tray_start` is an
    /// estimate. Secondary Win11 taskbars have no notification area window.
    pub tray_found: bool,
    /// Where the row of app buttons ends along the axis, read off the bar's
    /// pixels (`band_end`); None when the bar is hidden or the read failed.
    pub band_end: Option<i32>,
}

use crate::platform::taskbar_geom::{Edge, Rect};

/// Locate the target taskbar and its notification area (screen coords).
/// `PanelDisplay::Secondary` falls back to the primary taskbar when the
/// requested display does not exist (see `secondary_taskbars`). `own` is
/// the panel's current footprint along the bar's axis, left out of the
/// band scan so the panel does not count itself as an icon.
pub fn taskbar_slot(
    target: crate::config::schema::PanelDisplay,
    own: Option<(i32, i32)>,
    read_screen: bool,
) -> Option<TaskbarSlot> {
    use crate::platform::taskbar_geom::{bar_visible, edge_of, estimated_tray_start};
    use windows::core::w;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetWindowRect};

    unsafe {
        // Resolved fresh every call — see `resolve_taskbar`. A missing secondary
        // display is expected, not exceptional: it falls back to the primary
        // bar there, because an indicator on the wrong screen is recoverable
        // and a vanished one looks like a crash.
        let taskbar = resolve_taskbar(target)?;
        // Explorer recreates the bars on restart, so the handle we just found
        // may differ from the one the move/auto-hide hook is comparing events
        // against. Re-point it here rather than waiting for the user to switch
        // displays: this is the only code path that runs regularly.
        rearm_if_stale(taskbar);
        let mut r = RECT::default();
        if GetWindowRect(taskbar, &mut r).is_err() {
            return None;
        }
        let bar: Rect = (r.left, r.top, r.right, r.bottom);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let monitor = MonitorFromWindow(taskbar, MONITOR_DEFAULTTONEAREST);
        let mon: Rect = if GetMonitorInfoW(monitor, &mut mi).as_bool() {
            let m = mi.rcMonitor;
            (m.left, m.top, m.right, m.bottom)
        } else {
            // No monitor to judge against: take the bar as lying at the foot
            // of a screen of its own width.
            (bar.0, bar.3 - 1440, bar.2, bar.3)
        };
        let edge = edge_of(bar, mon);
        let visible = bar_visible(edge, bar, mon);
        let tray = FindWindowExW(Some(taskbar), None, w!("TrayNotifyWnd"), None)
            .ok()
            .filter(|h| !h.0.is_null())
            .and_then(|tray| {
                let mut t = RECT::default();
                GetWindowRect(tray, &mut t)
                    .is_ok()
                    .then_some(if edge.vertical() { t.top } else { t.left })
            });
        // With no tray window the scan of the bar's pixels places the tray
        // too (the busy cluster at the far end); the DIP reserve is the last
        // resort, for a hidden bar or an unreadable screen. Only a bar at
        // rest is read, and only when the caller asks (`read_screen`): a
        // screen read costs ~50 ms of the compositor's time, and a read on
        // every re-check saw whatever happened to lie on the bar at that
        // instant - a thumbnail's shadow, a tooltip - and flipped the room
        // verdict back and forth, which showed as the panel blinking.
        let at_rest = crate::platform::taskbar_geom::bar_on_screen(edge, bar, mon)
            >= crate::platform::taskbar_geom::thickness(edge, bar);
        // A bar in flight is never read, but the last read of it is used, so
        // the panel rides the slide at the place it will end up.
        let scan = if visible {
            scan_bar(edge, bar, tray, own, read_screen && at_rest)
        } else {
            BandScan::default()
        };
        let (tray_start, tray_found) = match tray.or(scan.tray_start) {
            Some(start) => (start, tray.is_some()),
            None => (estimated_tray_start(edge, bar), false),
        };
        let band_end = scan.band_end;
        Some(TaskbarSlot {
            edge,
            bar,
            mon,
            tray_start,
            visible,
            tray_found,
            band_end,
        })
    }
}

/// The target bar's rectangle alone, for callers that only need to know
/// which monitor it is on: no tray lookup, no screen read.
pub fn taskbar_rect(target: crate::config::schema::PanelDisplay) -> Option<Rect> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    unsafe {
        let taskbar = resolve_taskbar(target)?;
        let mut r = RECT::default();
        GetWindowRect(taskbar, &mut r).ok()?;
        Some((r.left, r.top, r.right, r.bottom))
    }
}

/// What a scan of the bar's pixels found: where the notification area
/// starts (only asked for when the bar has no tray window) and where the
/// row of app buttons ends, both along the bar's axis.
#[derive(Clone, Copy, Default)]
struct BandScan {
    tray_start: Option<i32>,
    band_end: Option<i32>,
}

/// Read the bar off the screen: its strip is copied out of the desktop DC,
/// every column along the axis is scored by how far its farthest pixel
/// strays from the strip's median colour (icons high, the acrylic low), and
/// `taskbar_geom` turns the scores into the tray's start — when
/// `tray_window` is None — and the buttons' end. The shell exposes nothing
/// better: the XAML buttons of a secondary bar are not in UI Automation at
/// all, and the classic child windows span the whole bar.
///
/// Cost: one BitBlt of a 48-pixel strip and a pass over it, well under a
/// millisecond; the result is kept for a second so a slide's burst of move
/// events reads it once.
fn scan_bar(
    edge: Edge,
    bar: Rect,
    tray_window: Option<i32>,
    own: Option<(i32, i32)>,
    read_screen: bool,
) -> BandScan {
    use crate::platform::taskbar_geom::{
        band_end_from_scores, bar_scale, thickness, tray_start_from_scores,
    };
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
    };

    /// Below this a column is the bar's own acrylic; icons score in the
    /// hundreds. Measured on dark and light bars over a moving wallpaper.
    const THRESHOLD: u32 = 28;
    /// The last read: the bar it was taken of, when, its column scores, and
    /// where the panel stood when they were taken. The scores are what is
    /// kept, not the answer: the answer depends on the span left out (the
    /// panel's own footprint), and that changes with every placement while
    /// the screen does not. The footprint at capture is left out too: the
    /// panel's ink is in the scores, and once the panel moved on, that ink
    /// read as the tray's start and walked the panel left, twenty pixels a
    /// placement.
    struct Last {
        /// The bar's extent along its axis: a bar sliding in or out keeps
        /// it, so the read taken at rest serves the whole slide and the
        /// panel does not hop sideways when the bar arrives.
        span: (i32, i32),
        at: Instant,
        scores: Vec<u32>,
        own: Option<(i32, i32)>,
    }
    static CACHE: Mutex<Option<Last>> = Mutex::new(None);
    let origin = if edge.vertical() { bar.1 } else { bar.0 };
    let span = if edge.vertical() {
        (bar.1, bar.3)
    } else {
        (bar.0, bar.2)
    };
    // A quiet stretch this long separates the buttons from the tray cluster;
    // the gaps inside the cluster are a third of it.
    let gap = (20.0 * bar_scale(thickness(edge, bar))).round() as usize;
    let answer = |scores: &[u32], skips: [Option<(i32, i32)>; 2]| -> BandScan {
        let tray_start = match tray_window {
            Some(_) => None,
            None => tray_start_from_scores(scores, origin, skips, gap, THRESHOLD),
        };
        let band_end = tray_window
            .or(tray_start)
            .and_then(|t| band_end_from_scores(scores, origin, t, skips, THRESHOLD));
        BandScan {
            tray_start,
            band_end,
        }
    };
    // Without a fresh read the last read of this bar stands, however old:
    // the answer must not move between a placement that read the screen and
    // one that did not, or the panel steps sideways on every re-check.
    if let Ok(c) = CACHE.lock() {
        if let Some(l) = c.as_ref() {
            if l.span == span && (!read_screen || l.at.elapsed() < Duration::from_secs(1)) {
                return answer(&l.scores, [own, l.own]);
            }
        }
    }
    if !read_screen {
        return BandScan::default();
    }
    let (w, h) = (bar.2 - bar.0, bar.3 - bar.1);
    if w <= 0 || h <= 0 || w * h > 4_000_000 {
        return BandScan::default();
    }
    let mut px: Vec<u32> = Vec::new();
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(Some(screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            let old = SelectObject(mem, HGDIOBJ(dib.0));
            if BitBlt(mem, 0, 0, w, h, Some(screen), bar.0, bar.1, SRCCOPY).is_ok()
                && !bits.is_null()
            {
                px = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize).to_vec();
            }
            SelectObject(mem, old);
            let _ = DeleteObject(HGDIOBJ(dib.0));
        }
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
    }
    if px.is_empty() {
        return BandScan::default();
    }
    let scores: Vec<u32> = {
        // One score per column along the axis, over the inner rows (the bar's
        // edge rows carry its own border): the farthest pixel from the median.
        let (len, thick) = if edge.vertical() { (h, w) } else { (w, h) };
        let at = |i: i32, j: i32| -> [i32; 3] {
            let p = if edge.vertical() {
                px[(i * w + j) as usize]
            } else {
                px[(j * w + i) as usize]
            };
            [
                ((p >> 16) & 255) as i32,
                ((p >> 8) & 255) as i32,
                (p & 255) as i32,
            ]
        };
        let inner = 6.min(thick / 4);
        let mut chan: [Vec<i32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for i in (0..len).step_by(4) {
            for j in inner..thick - inner {
                let c = at(i, j);
                for k in 0..3 {
                    chan[k].push(c[k]);
                }
            }
        }
        let median = |v: &mut Vec<i32>| -> i32 {
            if v.is_empty() {
                return 0;
            }
            let m = v.len() / 2;
            *v.select_nth_unstable(m).1
        };
        let bg = [
            median(&mut chan[0]),
            median(&mut chan[1]),
            median(&mut chan[2]),
        ];
        (0..len)
            .map(|i| {
                (inner..thick - inner)
                    .map(|j| {
                        let c = at(i, j);
                        ((c[0] - bg[0]).abs() + (c[1] - bg[1]).abs() + (c[2] - bg[2]).abs()) as u32
                    })
                    .max()
                    .unwrap_or(0)
            })
            .collect()
    };
    let scan = answer(&scores, [own, None]);
    if let Ok(mut c) = CACHE.lock() {
        *c = Some(Last {
            span,
            at: Instant::now(),
            scores,
            own,
        });
    }
    scan
}

/// Every `Shell_SecondaryTrayWnd`, ordered left to right by monitor.
/// Windows creates one per display when "show my taskbar on all displays" is
/// on; there are none when it is off, which is why callers must tolerate an
/// empty result rather than treating it as an error.
///
/// Found by class with `FindWindowExW`, not `EnumWindows`. The shell lifts an
/// auto-hide bar into a higher z-band after a Start menu cycle that caught it
/// open (it then sits above every ordinary topmost window until the next
/// cycle), and `EnumWindows` only walks the desktop band: in that state the
/// bar was simply not there for us, the Display submenu vanished and a panel
/// set to that bar quietly fell back to the primary one. `FindWindowExW`
/// walks by class across bands and finds it either way.
pub fn secondary_taskbars() -> Vec<isize> {
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetWindowRect};

    let mut found: Vec<(isize, i32)> = Vec::new();
    unsafe {
        let mut prev = HWND::default();
        while let Ok(hwnd) = FindWindowExW(None, Some(prev), w!("Shell_SecondaryTrayWnd"), None) {
            if hwnd.0.is_null() {
                break;
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let left = if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                mi.rcMonitor.left
            } else {
                let mut r = RECT::default();
                let _ = GetWindowRect(hwnd, &mut r);
                r.left
            };
            found.push((hwnd.0 as isize, left));
            prev = hwnd;
        }
    }
    crate::platform::taskbar_geom::order_bars(&mut found);
    found.into_iter().map(|(hwnd, _)| hwnd).collect()
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
        SelectObject, SetWindowRgn, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, UpdateLayeredWindow, GWL_EXSTYLE,
        GWL_STYLE, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, ULW_ALPHA,
        WS_CAPTION, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU,
        WS_THICKFRAME,
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
        // A plain popup, no caption. The window library leaves WS_CAPTION on
        // an undecorated window, and Windows 11 rounds a captioned window's
        // corners with a WINDOW REGION sized at its last framed resize: a
        // layered update that makes the window taller (the stacked panel on a
        // side bar, 48x64 after 119x48) left everything past the old region
        // unpainted and unhittable. The region is dropped as well, in case one
        // was set before the style changed.
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let caption = (WS_CAPTION.0 | WS_THICKFRAME.0 | WS_SYSMENU.0) as isize;
        if style & caption != 0 {
            SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !caption) | WS_POPUP.0 as isize);
            let _ = SetWindowRgn(hwnd, None, false);
            // Keep the overlay on screen while the shell peeks at a window
            // (the cursor resting on a taskbar thumbnail): peek fades every
            // other top-level window out, and took the panel with it.
            // DWMWA_EXCLUDED_FROM_PEEK = 12.
            let keep: u32 = 1;
            let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                hwnd,
                windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(12),
                &keep as *const u32 as _,
                std::mem::size_of::<u32>() as u32,
            );
        }

        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
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
            CreateDIBSection(Some(screen_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
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
                Some(screen_dc),
                Some(&dst),
                Some(&size),
                Some(mem_dc),
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
            Some(HWND_TOPMOST),
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

/// A window's class name and screen rectangle, for the log.
pub fn describe_window(hwnd: isize) -> String {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowRect};
    unsafe {
        let h = HWND(hwnd as _);
        let mut cls = [0u16; 64];
        let n = GetClassNameW(h, &mut cls);
        let mut r = RECT::default();
        let _ = GetWindowRect(h, &mut r);
        format!(
            "{} [{},{} {}x{}]",
            String::from_utf16_lossy(&cls[..n.max(0) as usize]),
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top
        )
    }
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
            Some(HWND_TOPMOST),
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
            Some(HWND_TOP),
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
pub fn foreground_scrim_active(target: crate::config::schema::PanelDisplay) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
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
        let Ok(proc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
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
        if !matches!(
            name,
            "searchhost.exe"
                | "startmenuexperiencehost.exe"
                | "searchapp.exe"
                | "shellexperiencehost.exe"
        ) {
            return false;
        }
        // Same shell process, different screen: the panel is not obstructed.
        let Some(bar) = taskbar_rect(target) else {
            return true;
        };
        let panel_monitor = monitor_at((bar.0 + bar.2) / 2, (bar.1 + bar.3) / 2);
        let scrim_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST).0 as isize;
        panel_monitor == scrim_monitor
    }
}

/// The monitor handle containing a screen point, as an isize for comparison.
fn monitor_at(x: i32, y: i32) -> isize {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST).0 as isize }
}

/// True while a fullscreen app (a game, a video, an F11 browser) owns the
/// screen — the same shell state that suspends notification toasts. The
/// taskbar sits UNDER such a window without moving, so the panel's geometric
/// visibility check cannot see it; without this the topmost overlay floats
/// over the game (and the foreground-raise would even re-assert it there).
/// While true the indicator hides the panel, exactly like the taskbar; the
/// next foreground change (alt-tab back to the desktop) restores it.
pub fn fullscreen_foreground_active(target: crate::config::schema::PanelDisplay) -> bool {
    // The shell is asked, not guessed at. It already decides when to put its
    // own taskbar away for a fullscreen app, and it says so: an appbar gets
    // ABN_FULLSCREENAPP when one opens and again when it goes (see
    // `register_fullscreen_watch`). Everything tried before this read some
    // consequence of that decision and got it wrong in both directions:
    //
    // - SHQueryUserNotificationState stays "busy" for as long as a game is
    //   RUNNING, even with the user back on the desktop (2026-07-22);
    // - the FOREGROUND window's geometry misses a game that is covering the
    //   screen without focus - alt-tab into Anno 1800 and the panel was
    //   drawn over it while focus sat elsewhere;
    // - the window's style says nothing: a terminal sized to the screen
    //   looks exactly like a game;
    // - the z-order over the bar's own pixels flickers - Anno lets the bar
    //   through for a frame every second or two, and the panel came back
    //   over the game each time ("taskbar free again: the bar itself is on
    //   top at a sample point").
    //
    // The notification is machine-wide, so it is paired with the one thing
    // geometry answers reliably: is anything actually covering the monitor
    // the panel lives on? A game on the other display then leaves this one
    // alone, which is the scoping the panel gained in 0.6.1.
    // Two ways to know, and either will do, because each has a blind spot:
    // the shell dips its own signal for a second or more while a game is
    // plainly still there, and the bar's z-order flickers under a game that
    // flips its stacking. What they agree on is the monitor being covered,
    // which is asked first and answered by geometry alone.
    let raw = monitor_is_covered(target)
        && (FULLSCREEN_APP.lock().map(|s| s.on).unwrap_or(false) || taskbar_is_buried(target));
    // A trailing hold shorter than the re-check that follows it, so the
    // panel is back on the first look after the game lets go rather than a
    // beat later. It only has to outlast a moment where BOTH witnesses dip
    // at once, which neither the shell's word (dips of a second, covered by
    // the bar being buried) nor the z-order (a frame, covered by the shell's
    // word) does on its own.
    const HOLD: std::time::Duration = std::time::Duration::from_millis(120);
    static UNTIL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let Ok(mut until) = UNTIL.lock() else {
        return raw;
    };
    if raw {
        *until = Some(std::time::Instant::now() + HOLD);
        return true;
    }
    match *until {
        Some(t) if std::time::Instant::now() < t => true,
        Some(_) => {
            *until = None;
            false
        }
        None => false,
    }
}

/// Whether something is drawn over the taskbar itself: three points along
/// the bar, clear of the panel's own end, all answering with the same window
/// that is neither the bar nor ours. A browser gone fullscreen over a video
/// is caught this way even when the shell says nothing.
fn taskbar_is_buried(target: crate::config::schema::PanelDisplay) -> bool {
    use crate::platform::taskbar_geom::{bar_visible, edge_of};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetClassNameW, GetWindowThreadProcessId, WindowFromPoint, GA_ROOT,
    };
    unsafe {
        let Some(bar) = taskbar_rect(target) else {
            return false;
        };
        let mid = POINT {
            x: (bar.0 + bar.2) / 2,
            y: (bar.1 + bar.3) / 2,
        };
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(MonitorFromPoint(mid, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        let mon = (m.left, m.top, m.right, m.bottom);
        let edge = edge_of(bar, mon);
        if !bar_visible(edge, bar, mon) {
            return false;
        }
        let ours = GetCurrentProcessId();
        let mut over: Option<isize> = None;
        for frac in [20, 50, 80] {
            let p = if edge.vertical() {
                POINT {
                    x: (bar.0 + bar.2) / 2,
                    y: bar.1 + (bar.3 - bar.1) * frac / 100,
                }
            } else {
                POINT {
                    x: bar.0 + (bar.2 - bar.0) * frac / 100,
                    y: (bar.1 + bar.3) / 2,
                }
            };
            let hwnd = WindowFromPoint(p);
            if hwnd.0.is_null() {
                return false;
            }
            let root = GetAncestor(hwnd, GA_ROOT);
            let mut pid = 0u32;
            GetWindowThreadProcessId(root, Some(&mut pid));
            if pid == ours {
                return false;
            }
            let mut cls = [0u16; 48];
            let n = GetClassNameW(root, &mut cls);
            let cls = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
            if cls == "Shell_TrayWnd" || cls == "Shell_SecondaryTrayWnd" {
                return false;
            }
            match over {
                None => over = Some(root.0 as isize),
                Some(seen) if seen == root.0 as isize => {}
                Some(_) => return false,
            }
        }
        over.is_some()
    }
}

/// Set from the shell's ABN_FULLSCREENAPP notifications.
/// What the shell has said about fullscreen apps: the state, when it last
/// said the state had ended, and which process was in front when it began.
struct FullscreenSignal {
    on: bool,
}

static FULLSCREEN_APP: std::sync::Mutex<FullscreenSignal> =
    std::sync::Mutex::new(FullscreenSignal { on: false });

/// What the shell has said about fullscreen apps. Taken as it comes: its
/// dips - "gone" for a second or so every few seconds while a game is
/// plainly still there (measured with Anno 1800: up, 3 s, gone, 1.2 s, up,
/// over and over) - are covered by the other witness, the bar being buried,
/// and the pair is held for a moment by the caller.
/// Register a hidden window as an appbar so the shell tells us when a
/// fullscreen app takes the screen and when it lets go. No space is
/// reserved: that would need ABM_SETPOS, which is never sent. Called once,
/// from the same thread as the rest of the window work.
pub fn register_fullscreen_watch() {
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_NEW, APPBARDATA};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, RegisterClassW, HWND_MESSAGE, WNDCLASSW, WS_POPUP,
    };

    /// The shell's callback message; any WM_APP value will do.
    const WM_APPBAR: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;
    /// wParam of the callback for the notification we care about.
    const ABN_FULLSCREENAPP: usize = 0x0000_0002;

    unsafe extern "system" fn appbar_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        if msg == WM_APPBAR && wparam.0 == ABN_FULLSCREENAPP {
            // lParam: TRUE when a fullscreen app opens, FALSE when it goes.
            let on = lparam.0 != 0;
            if let Ok(mut s) = FULLSCREEN_APP.lock() {
                s.on = on;
            }
            // Act on it now, not at whatever event happens next: the shell
            // said "up" and the panel was still over the game for another
            // 475 ms while nothing woke the loop.
            if let Some(proxy) = PROXY.get() {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(crate::app::UserEvent::PanelRaise);
                }
            }
            tracing::debug!(
                "shell says a fullscreen app is {}",
                if on { "up" } else { "gone" }
            );
            return windows::Win32::Foundation::LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    unsafe {
        let Ok(hinst) = GetModuleHandleW(None) else {
            return;
        };
        let class = w!("AiLimitsAppBar");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(appbar_wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            Default::default(),
            class,
            w!("AI Limits appbar"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst.into()),
            None,
        ) else {
            tracing::warn!(
                "fullscreen watch not registered; the panel will not stand down for a game"
            );
            return;
        };
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: WM_APPBAR,
            ..Default::default()
        };
        if SHAppBarMessage(ABM_NEW, &mut abd) == 0 {
            tracing::warn!(
                "the shell refused the appbar; the panel will not stand down for a game"
            );
        }
    }
}

/// Whether anything covers the monitor the panel's bar lives on: a visible
/// window, not ours and not one of the shell's own surfaces, whose rectangle
/// contains the monitor. Geometry only - no z-order, no focus - so it is
/// steady while a game flips its own stacking about.
fn monitor_is_covered(target: crate::config::schema::PanelDisplay) -> bool {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct Search {
        mon: RECT,
        ours: u32,
        found: Option<isize>,
    }

    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let s = &mut *(lparam.0 as *mut Search);
        if s.found.is_some() || !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == s.ours {
            return BOOL(1);
        }
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return BOOL(1);
        }
        if r.left > s.mon.left
            || r.top > s.mon.top
            || r.right < s.mon.right
            || r.bottom < s.mon.bottom
        {
            return BOOL(1);
        }
        let mut cls = [0u16; 48];
        let n = GetClassNameW(hwnd, &mut cls);
        let cls = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
        // The shell's own monitor-sized surfaces: the desktop, the taskbar,
        // the XAML island that hosts thumbnails and Task View.
        if matches!(
            cls.as_str(),
            "Progman"
                | "WorkerW"
                | "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "XamlExplorerHostIslandWindow"
                | "ForegroundStaging"
                | "MultitaskingViewFrame"
                | "Windows.UI.Core.CoreWindow"
        ) {
            return BOOL(1);
        }
        s.found = Some(hwnd.0 as isize);
        BOOL(0)
    }

    unsafe {
        let Some(bar) = taskbar_rect(target) else {
            return false;
        };
        let mid = POINT {
            x: (bar.0 + bar.2) / 2,
            y: (bar.1 + bar.3) / 2,
        };
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(MonitorFromPoint(mid, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
            return false;
        }
        let mut search = Search {
            mon: mi.rcMonitor,
            ours: GetCurrentProcessId(),
            found: None,
        };
        let _ = EnumWindows(Some(cb), LPARAM(&mut search as *mut _ as isize));
        if let Some(h) = search.found {
            tracing::trace!("monitor covered by {}", describe_window(h));
        }
        search.found.is_some()
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
    use windows::Win32::Foundation::HINSTANCE;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
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
            None,
            None,
            Some(hinst.into()),
            None,
        ) else {
            return 0;
        };
        hwnd.0 as isize
    }
}

/// Destroy a window we created ourselves (the tooltip). The OS reclaims it at
/// process exit anyway, so this matters only if the owner is ever recreated
/// rather than mutated — at which point the old window would linger, invisible
/// and unowned, for the rest of the session.
pub fn destroy_window(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
    if hwnd == 0 {
        return;
    }
    unsafe {
        let _ = DestroyWindow(HWND(hwnd as _));
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

// Handles the taskbar watch hook compares incoming WinEvents against. Module
// scope because both the hook callback (inside `install_taskbar_watch`) and
// `watch_taskbar` (called at startup and whenever the target display
// changes) need to read/write them.
static TASKBAR: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static TRAY: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
/// The move/auto-hide hook and the Explorer process it is scoped to. The
/// scope is what makes the hook cheap — only Explorer's events reach the
/// callback — and what makes it die with Explorer: a WinEvent hook scoped by
/// process id never fires for the process that replaces it. See
/// `rearm_if_stale`.
static LOC_HOOK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static HOOK_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static PROXY: std::sync::OnceLock<
    std::sync::Mutex<tao::event_loop::EventLoopProxy<crate::app::UserEvent>>,
> = std::sync::OnceLock::new();

/// Last secondary bar we successfully enumerated, and the index it answered.
///
/// Windows drops `Shell_SecondaryTrayWnd` out of `EnumWindows` for as long as
/// the Start menu is up — measured, not assumed. Without this cache the
/// enumeration comes back empty for those few hundred milliseconds, the
/// primary-bar fallback fires, and the panel JUMPS TO THE OTHER DISPLAY every
/// time the user presses the Windows key. It then looks like the panel
/// "disappeared" from the display it was configured for.
static LAST_SECONDARY: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static LAST_SECONDARY_IDX: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Resolve the taskbar a target refers to, right now.
///
/// Deliberately re-queried on every call rather than cached: `Shell_TrayWnd`
/// and `Shell_SecondaryTrayWnd` are DESTROYED AND RECREATED whenever Explorer
/// restarts — which happens on its own, days into a session, with no event the
/// app subscribes to. A handle captured at startup is a handle to a window
/// that no longer exists.
fn resolve_taskbar(
    target: crate::config::schema::PanelDisplay,
) -> Option<windows::Win32::Foundation::HWND> {
    use crate::config::schema::PanelDisplay;
    use windows::core::w;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    unsafe {
        match target {
            PanelDisplay::Primary => FindWindowW(w!("Shell_TrayWnd"), None).ok(),
            PanelDisplay::Secondary(i) => {
                use std::sync::atomic::Ordering::Relaxed;
                use windows::Win32::UI::WindowsAndMessaging::IsWindow;
                if let Some(&h) = secondary_taskbars().get(i as usize) {
                    LAST_SECONDARY.store(h, Relaxed);
                    LAST_SECONDARY_IDX.store(i as i32, Relaxed);
                    return Some(HWND(h as _));
                }
                // Enumeration came back without it. Two very different causes,
                // and telling them apart is the whole point: the Start menu
                // hides the bar from enumeration while leaving the window
                // alive, whereas an unplugged monitor destroys it. Trust a
                // handle that is still a window; only a dead one means the
                // display is really gone.
                let cached = LAST_SECONDARY.load(Relaxed);
                if cached != 0
                    && LAST_SECONDARY_IDX.load(Relaxed) == i as i32
                    && IsWindow(Some(HWND(cached as _))).as_bool()
                {
                    return Some(HWND(cached as _));
                }
                // Genuinely gone: fall back to the primary bar rather than
                // hiding, because a visible indicator on the wrong display is
                // recoverable and a vanished one looks like a crash.
                FindWindowW(w!("Shell_TrayWnd"), None).ok()
            }
        }
    }
}

/// Store the handles the hook compares against.
fn arm_watch(taskbar: windows::Win32::Foundation::HWND) {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowExW;
    unsafe {
        TASKBAR.store(taskbar.0 as isize, std::sync::atomic::Ordering::Relaxed);
        TRAY.store(
            FindWindowExW(Some(taskbar), None, w!("TrayNotifyWnd"), None)
                .map(|t| t.0 as isize)
                .unwrap_or(0),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

/// Re-point the hook if the taskbar it watches is no longer the taskbar we
/// resolve. Cheap: one integer compare on the common path.
///
/// **Why this is not optional.** The hook is armed once at startup and again
/// on an explicit display switch. Explorer restarting between those two
/// moments leaves the hook comparing events against a destroyed window: every
/// auto-hide slide stops being reported, and the panel survives only on the
/// 60-second provider tick — it stops following the bar and looks like it
/// vanished. Measured in the field: after five days of uptime BOTH bar handles
/// had changed.
///
/// **And the hook itself, not only its handles.** The move/auto-hide hook is
/// scoped to Explorer's process id, so `taskkill /f /im explorer.exe && start
/// explorer.exe` — or Explorer crashing and coming back — leaves it bound to a
/// process that no longer exists. Re-pointing the handles is not enough then:
/// no slide is ever reported again, and the panel only moves on the 60-second
/// tick or whenever some unrelated foreground change happens to nudge it,
/// which reads as a laggy, out-of-step panel on an auto-hide bar. A changed
/// bar handle with a changed owning pid means a new Explorer: unhook and hook
/// again for the process that is actually there.
fn rearm_if_stale(taskbar: windows::Win32::Foundation::HWND) {
    use std::sync::atomic::Ordering::Relaxed;
    if TASKBAR.load(Relaxed) == taskbar.0 as isize {
        return;
    }
    tracing::debug!("taskbar handle changed, re-arming the watch");
    arm_watch(taskbar);
    let pid = window_pid(taskbar);
    if pid != 0 && LOC_HOOK.load(Relaxed) != 0 && HOOK_PID.load(Relaxed) != pid {
        tracing::info!(
            "explorer is a new process ({} -> {}); re-scoping the taskbar hook",
            HOOK_PID.load(Relaxed),
            pid
        );
        hook_location_changes(pid);
    }
}

/// The process id owning a window; 0 for a dead handle.
fn window_pid(hwnd: windows::Win32::Foundation::HWND) -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Install (or replace) the move/auto-hide hook, scoped to one Explorer
/// process. Any previous one is unhooked first, so there is exactly one and
/// every event arrives once. Must run on the thread with the message loop.
fn hook_location_changes(pid: u32) {
    use std::sync::atomic::Ordering::Relaxed;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_OBJECT_LOCATIONCHANGE, WINEVENT_OUTOFCONTEXT,
    };
    unsafe {
        let old = LOC_HOOK.swap(0, Relaxed);
        if old != 0 {
            let _ = UnhookWinEvent(HWINEVENTHOOK(old as _));
        }
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
            HOOK_PID.store(0, Relaxed);
        } else {
            LOC_HOOK.store(hook.0 as isize, Relaxed);
            HOOK_PID.store(pid, Relaxed);
        }
    }
}

/// The WinEvent callback shared by the three hooks. Fires on the hooking
/// thread's message loop and forwards a user event; the comparison handles
/// decide which taskbar's events count.
unsafe extern "system" fn on_event(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: windows::Win32::Foundation::HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetDesktopWindow, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_REORDER,
        EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW,
    };
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
            if h == TASKBAR.load(std::sync::atomic::Ordering::Relaxed) || (tray != 0 && h == tray) {
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

/// Point the existing hook at a taskbar. The hook itself is process-wide and
/// installed once; only the handles it compares against change.
pub fn watch_taskbar(target: crate::config::schema::PanelDisplay) {
    let Some(taskbar) = resolve_taskbar(target) else {
        return;
    };
    arm_watch(taskbar);
}

/// Watch the taskbar for moves/slides (auto-hide) and the notification area
/// for width changes (icons pinned/unpinned), event-driven via
/// `SetWinEventHook` — no polling, keeps the 0% idle CPU budget. The callback
/// fires on THIS thread's message loop and forwards a `TaskbarMoved` user
/// event so the panel can reposition. Installs the process-wide hook exactly
/// once, then points it at `target` (see `watch_taskbar`).
pub fn install_taskbar_watch(
    proxy: tao::event_loop::EventLoopProxy<crate::app::UserEvent>,
    target: crate::config::schema::PanelDisplay,
) {
    use std::sync::Mutex;
    use windows::core::w;
    use windows::Win32::UI::Accessibility::SetWinEventHook;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, EVENT_OBJECT_REORDER, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
        WINEVENT_SKIPOWNPROCESS,
    };

    unsafe {
        // Install the hooks ONCE per process. Nothing unhooks the two global
        // ones, and the PROXY OnceLock only makes the proxy idempotent, not the
        // hooks: a second call would add more global hooks while the first
        // keep firing, so every event would arrive twice. Re-pointing the
        // watch at a different bar is `watch_taskbar`, which touches only the
        // comparison handles and is safe to call as often as needed; the
        // Explorer-scoped hook alone is replaced when Explorer is, by
        // `rearm_if_stale`.
        static HOOKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if HOOKED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            tracing::debug!("taskbar watch already installed; re-pointing only");
            watch_taskbar(target);
            return;
        }

        // Any taskbar window works here: the pid is Explorer-process-wide (all
        // taskbars, primary and secondary, live in the same explorer.exe), so
        // scoping the hook via the primary bar is enough regardless of which
        // bar the panel is actually attached to. `watch_taskbar` below is what
        // points the callback's comparison handles at the right one.
        let Ok(taskbar) = FindWindowW(w!("Shell_TrayWnd"), None) else {
            // Nothing was hooked - let a later call try again.
            HOOKED.store(false, std::sync::atomic::Ordering::SeqCst);
            return;
        };
        let _ = PROXY.set(Mutex::new(proxy));
        // Scoped to the Explorer process; only the taskbar hwnd passes the
        // callback filter. Lives until Explorer is replaced (`rearm_if_stale`).
        hook_location_changes(window_pid(taskbar));
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
    watch_taskbar(target);
}

/// Whether this process runs inside an MSIX package (the Microsoft Store
/// build). `GetCurrentPackageFullName` answers with a name for a packaged
/// process and `APPMODEL_ERROR_NO_PACKAGE` for a plain exe; any other
/// outcome is treated as unpackaged, which is the conservative reading —
/// the packaged paths only ever take things away.
pub fn is_packaged() -> bool {
    use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut len = 0u32;
    // A zero-length buffer only asks for the size; the answer is in the code.
    let err = unsafe { GetCurrentPackageFullName(&mut len, None) };
    err != APPMODEL_ERROR_NO_PACKAGE && len > 0
}

/// The AppUserModelID the unpackaged builds notify under. The Store package
/// has its own (see `toast_app_id`); this one is registered per user by
/// `register_toast_identity` and carried by the installer's Start menu
/// shortcut, so a toast shows "AI Limits" and the icon, not the identity of
/// whichever app happened to lend its ID.
pub const APP_USER_MODEL_ID: &str = "napxlexn.AILimits";

/// The identity toasts are shown under. Inside an MSIX package it is the
/// package's own application id, which the Store registered along with the
/// display name and logo; anywhere else it is `APP_USER_MODEL_ID`.
pub fn toast_app_id() -> String {
    use windows::Win32::Storage::Packaging::Appx::GetCurrentApplicationUserModelId;
    if is_packaged() {
        let mut len = 0u32;
        // The first call only sizes the buffer; the second fills it.
        let _ = unsafe { GetCurrentApplicationUserModelId(&mut len, None) };
        if len > 0 {
            let mut buf = vec![0u16; len as usize];
            let err = unsafe {
                GetCurrentApplicationUserModelId(
                    &mut len,
                    Some(windows::core::PWSTR::from_raw(buf.as_mut_ptr())),
                )
            };
            if err.is_ok() {
                let s = String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]);
                if !s.is_empty() {
                    return s;
                }
            }
        }
    }
    APP_USER_MODEL_ID.to_string()
}

/// Whether the menu icon size the system asks for is 32 px or more, i.e.
/// the primary display runs at 200%+ scaling (menus follow the system DPI).
pub fn menu_scale_is_200() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};
    unsafe { GetSystemMetrics(SM_CXSMICON) >= 32 }
}

/// Hand a URL to the default browser through the shell, the same path a
/// double-clicked .url file takes; no console, no child to wait on.
pub fn open_url(url: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (verb, target) = (wide("open"), wide(url));
    let h = unsafe {
        ShellExecuteW(
            None,
            PCWSTR::from_raw(verb.as_ptr()),
            PCWSTR::from_raw(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute reports failure as a value at or below 32.
    if (h.0 as isize) <= 32 {
        tracing::warn!("could not open {url} (shell code {})", h.0 as isize);
    }
}

/// Tell the notification platform who `APP_USER_MODEL_ID` is: the display
/// name and the icon a toast shows come from
/// `HKCU\Software\Classes\AppUserModelId\<id>`, the registration Windows
/// accepts from a desktop app that has no Start menu shortcut (a portable
/// copy, a Scoop install). The icon is the app's own, written once beside
/// the config since the exe carries it only as a resource. Best-effort and
/// idempotent; a packaged copy never gets here (the Store did this).
pub fn register_toast_identity() {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };

    if is_packaged() {
        return;
    }
    let icon = crate::config::storage::config_path().with_file_name("icon.png");
    const ICON: &[u8] = include_bytes!("../../assets/icon.png");
    let icon_is_current = std::fs::metadata(&icon)
        .map(|m| m.len() == ICON.len() as u64)
        .unwrap_or(false);
    if !icon_is_current {
        if let Some(parent) = icon.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&icon, ICON) {
            tracing::debug!("toast icon not written ({e}); the toast shows without one");
        }
    }

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let set = |key: HKEY, name: windows::core::PCWSTR, value: &str| unsafe {
        let v = wide(value);
        let bytes = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
        RegSetValueExW(key, name, None, REG_SZ, Some(bytes))
    };
    unsafe {
        let mut key = HKEY::default();
        let path = wide(&format!(
            "Software\\Classes\\AppUserModelId\\{APP_USER_MODEL_ID}"
        ));
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR::from_raw(path.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .is_err()
        {
            tracing::debug!("toast identity not registered; toasts fall back to a borrowed one");
            return;
        }
        let _ = set(key, w!("DisplayName"), "AI Limits");
        let _ = set(key, w!("IconUri"), &icon.to_string_lossy());
        let _ = RegCloseKey(key);
    }
}

/// Promote this exe's notification icons onto the always-visible taskbar
/// corner. Windows 11 hides new tray icons behind the overflow chevron;
/// the per-icon "always show" toggle is just `IsPromoted=1` under the
/// per-user key `HKCU\Control Panel\NotifyIconSettings\<uid>`, so the
/// widget can flip it for its own icons. Explorer watches the key and
/// applies the change live. A no-op on Windows 10 (no such key) and on
/// any registry error — promotion is best-effort, never fatal.
///
/// Inside an MSIX package it is skipped outright: HKCU writes from a
/// packaged process land in the package's virtualised hive, which Explorer
/// never reads, so the write could not work — and the Store policy asks
/// that a product not change Windows settings without the user's say.
pub fn promote_tray_icons() -> u32 {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD,
    };

    if is_packaged() {
        return 0;
    }
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
            None,
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
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                None,
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
                None,
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
                if RegSetValueExW(sub, w!("IsPromoted"), None, REG_DWORD, Some(&one)).is_ok() {
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
            None,
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
