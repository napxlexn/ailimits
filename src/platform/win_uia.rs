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
    AutomationElementMode_None, CUIAutomation, IUIAutomation, IUIAutomation2,
    IUIAutomationCacheRequest, IUIAutomationCondition, TreeScope_Descendants,
    UIA_BoundingRectanglePropertyId, UIA_ButtonControlTypeId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId,
};

/// The automation client and the one question it asks, kept for the life of
/// the thread. Both are apartment-bound, which is why they live in a
/// thread-local rather than a static: making them per call cost more than
/// the query itself (0.16% of a core against a 0.005% budget, and four
/// extra threads, when the panel was standing down and asking often).
struct Asker {
    automation: IUIAutomation,
    buttons: IUIAutomationCondition,
    /// The two properties the answer needs, fetched WITH the elements.
    /// Without this every element costs two more cross-process calls to read
    /// its rectangle and its class - 42 elements became some ninety round
    /// trips, and the widget's idle CPU went from 0.005% of a core to 0.26%.
    cache: IUIAutomationCacheRequest,
}

impl Asker {
    fn new() -> Option<Self> {
        unsafe {
            // An "already initialised" answer is not an error: the event
            // loop's thread is an apartment before this runs (the tray and
            // menu libraries see to that).
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            // Cross-process calls with the shell on the other end: bounded,
            // so a busy Explorer cannot hold the widget's loop. The default
            // is two seconds, which is a freeze the user would see.
            if let Ok(timed) = automation.cast::<IUIAutomation2>() {
                let _ = timed.SetConnectionTimeout(500);
                let _ = timed.SetTransactionTimeout(500);
            }
            let buttons = automation
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_ButtonControlTypeId.0),
                )
                .ok()?;
            let cache = automation.CreateCacheRequest().ok()?;
            let _ = cache.AddProperty(UIA_BoundingRectanglePropertyId);
            let _ = cache.AddProperty(UIA_ClassNamePropertyId);
            // Nothing is asked of these elements afterwards, so no live
            // references are needed - only what the cache carries.
            let _ = cache.SetAutomationElementMode(AutomationElementMode_None);
            Some(Self {
                automation,
                buttons,
                cache,
            })
        }
    }
}

thread_local! {
    static ASKER: std::cell::RefCell<Option<Asker>> = const { std::cell::RefCell::new(None) };
}

/// Drop the client, so the next question builds a fresh one. Called when a
/// query fails: Explorer restarting is the likeliest reason.
///
/// It buys no memory back. Measured both ways with the panel on the second
/// monitor: kept for the life of the process, private bytes go 24 -> 45 MB
/// at the first query; dropped after every query, 24 -> 45 MB just the same.
/// The twenty megabytes are UI Automation itself loading into the process,
/// not the client object, and rebuilding the client costs about a
/// millisecond (17.5 ms a query against 16.5 ms kept). So the client is kept
/// and this exists for the failure path alone.
pub(crate) fn forget_client() {
    ASKER.with(|a| {
        if let Ok(mut a) = a.try_borrow_mut() {
            *a = None;
        }
    });
}

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
        let found = ASKER.with(|a| {
            let mut a = a.borrow_mut();
            let asker = match a.as_ref() {
                Some(asker) => asker,
                None => a.insert(Asker::new()?),
            };
            let root = asker.automation.ElementFromHandle(HWND(bar as _)).ok()?;
            root.FindAllBuildCache(TreeScope_Descendants, &asker.buttons, &asker.cache)
                .ok()
        })?;

        let mut band_end: Option<i32> = None;
        let mut tray_start: Option<i32> = None;
        for i in 0..found.Length().ok()? {
            let Ok(el) = found.GetElement(i) else {
                continue;
            };
            let Ok(r) = el.CachedBoundingRectangle() else {
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
                .CachedClassName()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A dead bar is what an Explorer restart leaves behind: the handle the
    /// panel was following is gone, and the client may be talking to a
    /// process that no longer exists. The query must fail rather than hang
    /// or lie, and the next good one must still work - `forget_client` is
    /// what stands between those two.
    /// Run: `cargo test uia_survives_a_dead_bar -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn uia_survives_a_dead_bar() {
        use windows::core::w;
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
        let bar = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.unwrap();
        assert!(
            bar_room(bar.0 as isize, false).is_some(),
            "the live bar should answer"
        );
        let at = std::time::Instant::now();
        assert!(
            bar_room(0xDEAD_BEEF, false).is_none(),
            "a dead handle must not produce an answer"
        );
        println!("a dead bar failed in {:?}", at.elapsed());
        forget_client();
        assert!(
            bar_room(bar.0 as isize, false).is_some(),
            "the live bar should answer again after the client is dropped"
        );
    }

    /// What one question costs, away from whatever else the machine is
    /// doing: the load audit cannot tell our work from a browser's. Prints
    /// the wall and CPU time of 50 queries against the primary taskbar.
    /// Run: `cargo test uia_query_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn uia_query_cost() {
        use windows::core::w;
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, SetCursorPos};
        // The SECOND monitor's bar, and held open first: a bar slid away for
        // auto-hide has no elements to give - every rectangle comes back
        // empty, which is a `None` answer, not a slow one.
        let bar = unsafe { FindWindowExW(None, None, w!("Shell_SecondaryTrayWnd"), None) }
            .ok()
            .filter(|h| !h.0.is_null())
            .unwrap_or_else(|| unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.unwrap());
        unsafe {
            let _ = SetCursorPos(5700, 1439);
            std::thread::sleep(std::time::Duration::from_millis(400));
            let _ = SetCursorPos(5701, 1438);
        }
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let vertical = {
            use windows::Win32::Foundation::RECT;
            use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
            let mut r = RECT::default();
            unsafe { GetWindowRect(bar, &mut r) }.unwrap();
            (r.bottom - r.top) > (r.right - r.left)
        };
        // One warm-up: the first call builds the automation client.
        let first = std::time::Instant::now();
        let room = bar_room(bar.0 as isize, vertical);
        println!("first call (builds the client): {:?}", first.elapsed());
        assert!(room.is_some(), "the shell said nothing about its own bar");

        let run = |label: &str, drop_each: bool| {
            let cpu_before = crate::platform::process_cpu_time();
            let at = std::time::Instant::now();
            for _ in 0..50 {
                let _ = bar_room(bar.0 as isize, vertical);
                if drop_each {
                    forget_client();
                }
            }
            let wall = at.elapsed();
            let cpu = crate::platform::process_cpu_time() - cpu_before;
            println!(
                "{label}: {:.1} ms wall ({:.2} ms each), {:.1} ms CPU ({:.2} ms each)",
                wall.as_secs_f64() * 1000.0,
                wall.as_secs_f64() * 1000.0 / 50.0,
                cpu.as_secs_f64() * 1000.0,
                cpu.as_secs_f64() * 1000.0 / 50.0
            );
        };
        run("50 queries, client kept   ", false);
        run("50 queries, client dropped", true);
    }
}
