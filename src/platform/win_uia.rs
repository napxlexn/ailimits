// platform/win_uia.rs — where the taskbar's app buttons end and its
// notification area begins, asked of the shell rather than read off the
// screen.
//
// This used to be a pixel scan: capture the bar's strip, score each column
// against the strip's median colour, call the last busy column the end of the
// buttons and the first quiet stretch before the far end the start of the
// tray. It worked, and it drove itself in circles - the panel standing down
// raised a tray icon, the icon widened the tray, the stretch changed and the
// verdict flipped, twice a second. It also read whatever happened to lie on
// the bar at that instant: a thumbnail's shadow, a tooltip.
//
// Windows 11 draws its bar in XAML, so none of the classic child windows say
// anything useful (`MSTaskListWClass` is still there and still reports the
// geometry of a bar that no longer exists). UI Automation does: every button
// is an element with a bounding rectangle, on the primary bar AND on a
// secondary one, and the notification area's own items are there too, under
// class names starting with "SystemTray.". Measured: 42 elements in 20 ms on
// the primary bar, 33 in 16 ms on a secondary one.

use windows::core::Interface;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, TreeScope_Descendants, UIA_ButtonControlTypeId,
    UIA_ControlTypePropertyId,
};

/// What the bar is made of, along its axis: where the run of buttons ends,
/// and where the notification area starts. Both in screen pixels.
pub(crate) struct BarRoom {
    pub band_end: Option<i32>,
    pub tray_start: Option<i32>,
}

/// Ask the shell. `vertical` picks the axis; None when UI Automation cannot
/// answer, which leaves the caller on its estimates.
pub(crate) fn bar_room(bar: isize, vertical: bool) -> Option<BarRoom> {
    unsafe {
        // The event loop's thread is already an apartment (the tray and menu
        // libraries see to that); this is for any other caller, and an
        // "already initialised" answer is not an error.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        // Cross-process calls with the shell on the other end: bounded, so a
        // busy Explorer cannot hold the widget's loop. The default is two
        // seconds, which is a freeze the user would see.
        if let Ok(timed) = automation.cast::<IUIAutomation2>() {
            let _ = timed.SetConnectionTimeout(500);
            let _ = timed.SetTransactionTimeout(500);
        }
        let root = automation.ElementFromHandle(HWND(bar as _)).ok()?;
        let buttons = automation
            .CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_ButtonControlTypeId.0),
            )
            .ok()?;
        let found = root.FindAll(TreeScope_Descendants, &buttons).ok()?;

        let mut band_end: Option<i32> = None;
        let mut tray_start: Option<i32> = None;
        for i in 0..found.Length().ok()? {
            let Ok(el) = found.GetElement(i) else {
                continue;
            };
            let Ok(r) = el.CurrentBoundingRectangle() else {
                continue;
            };
            // Elements that are not on the screen come back as an empty
            // rectangle - a pinned app in the overflow, for one.
            if r.right <= r.left || r.bottom <= r.top {
                continue;
            }
            let (start, end) = if vertical {
                (r.top, r.bottom)
            } else {
                (r.left, r.right)
            };
            let tray = el
                .CurrentClassName()
                .map(|c| c.to_string().starts_with("SystemTray."))
                .unwrap_or(false);
            if tray {
                tray_start = Some(tray_start.map_or(start, |t: i32| t.min(start)));
            } else {
                band_end = Some(band_end.map_or(end, |e: i32| e.max(end)));
            }
        }
        (band_end.is_some() || tray_start.is_some()).then_some(BarRoom {
            band_end,
            tray_start,
        })
    }
}
