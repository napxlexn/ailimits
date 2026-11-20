// platform/menu_hover.rs — what the context menu has highlighted, so the item
// the pointer rests on can be shown on the widget, the panel and the tray icon
// before a click. Windows only.
//
// The menu is Windows' own (muda): it closes after a click and runs a modal
// loop of its own while it is open. Windows tells the menu's owner window what
// is highlighted (WM_MENUSELECT); a subclass of that window
// (SetWindowSubclass, which muda and the window library already use) passes it
// on to one handler. The widget's and the panel's menus are opened through
// `show`, which also knows whether an item was clicked; the tray icon's menu
// is opened by tray-icon itself, so its window is watched for good (`watch`)
// and the end of that menu is told by a message the subclass posts to itself.
// No timer, no hook, and nothing runs while no menu is open beyond one
// comparison per message the watched windows get anyway.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use windows::core::PWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMenuItemInfoW, PeekMessageW, PostMessageW, HMENU, MENUITEMINFOW, MIIM_STRING, MIIM_SUBMENU,
    MSG, PM_NOREMOVE, WM_APP, WM_COMMAND, WM_ENTERMENULOOP, WM_EXITMENULOOP, WM_MENUSELECT,
};

/// What the menu is doing, as the handler hears it.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuHover {
    /// A menu opened.
    Begin,
    /// An item is highlighted: the title of the submenu it is in (None at
    /// the top level) and its label.
    Item { group: Option<String>, text: String },
    /// Nothing that could be shown is highlighted: a submenu's title, a
    /// disabled item, an item of a deeper submenu.
    Nothing,
    /// The menu closed; `chosen` when an item was clicked (its MenuEvent is
    /// on its way, or was sent already).
    End { chosen: bool },
}

/// Ours among the subclasses of a window: "AILM".
const SUBCLASS_ID: usize = 0x4149_4C4D;
/// Posted by the subclass to its own window when a menu tray-icon opened has
/// ended, so it arrives after the WM_COMMAND of a click.
const WM_MENU_ENDED: u32 = WM_APP + 0x51D;
/// WM_MENUSELECT's flags for a submenu's title, a separator, and an item
/// that cannot be chosen.
const MF_POPUP: u32 = 0x10;
const MF_SEPARATOR: u32 = 0x800;
const MF_DISABLED: u32 = 0x2;
const MF_GRAYED: u32 = 0x1;

/// The handler: whether it took the event (it may be busy).
type Handler = Rc<dyn Fn(MenuHover) -> bool>;

thread_local! {
    /// The menu's top level (`hpopupmenu`) and the handler.
    static HANDLER: RefCell<Option<(isize, Handler)>> = const { RefCell::new(None) };
    /// A menu opened through `show` is up: its end is told by `show`.
    static SHOWING: Cell<bool> = const { Cell::new(false) };
    /// A watched menu sent its owner a click.
    static CLICKED: Cell<bool> = const { Cell::new(false) };
}

/// Who hears what the menu does: `root` is the menu's top level
/// (`ContextMenu::hpopupmenu`), `f` returns whether it took the event.
pub fn set_handler(root: isize, f: impl Fn(MenuHover) -> bool + 'static) {
    HANDLER.with(|h| *h.borrow_mut() = Some((root, Rc::new(f))));
}

/// Whether a menu opened through `show` (the widget's or the panel's) is up.
pub fn showing() -> bool {
    SHOWING.get()
}

fn root() -> Option<isize> {
    HANDLER.with(|h| h.borrow().as_ref().map(|(r, _)| *r))
}

/// Tell the handler; the handler is taken out first, so it may open or
/// close anything it likes.
fn emit(ev: MenuHover) -> bool {
    let f = HANDLER.with(|h| h.borrow().as_ref().map(|(_, f)| f.clone()));
    f.is_some_and(|f| f(ev))
}

/// Watch a window's menus for good: the tray icon's, which tray-icon opens.
pub fn watch(hwnd: isize) {
    unsafe {
        let _ = SetWindowSubclass(HWND(hwnd as _), Some(subclass), SUBCLASS_ID, 0);
    }
}

fn unwatch(hwnd: isize) {
    unsafe {
        let _ = RemoveWindowSubclass(HWND(hwnd as _), Some(subclass), SUBCLASS_ID);
    }
}

/// Open `menu` at the cursor, owned by `hwnd` (the widget or the panel),
/// telling the handler what is highlighted and how it ended. Returns whether
/// an item was clicked.
///
/// # Safety
///
/// `hwnd` must be a valid window of this thread.
pub unsafe fn show(menu: &muda::Menu, hwnd: isize) -> bool {
    use muda::ContextMenu as _;
    SHOWING.set(true);
    watch(hwnd);
    emit(MenuHover::Begin);
    let chosen = unsafe { menu.show_context_menu_for_hwnd(hwnd, None) };
    unwatch(hwnd);
    SHOWING.set(false);
    emit(MenuHover::End { chosen });
    chosen
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_MENUSELECT => describe(wparam.0, lparam.0).is_some_and(emit),
        // A menu tray-icon opened: it ends with WM_EXITMENULOOP, and a click
        // is posted to this window as WM_COMMAND around then.
        WM_ENTERMENULOOP if !SHOWING.get() => {
            CLICKED.set(false);
            emit(MenuHover::Begin)
        }
        WM_COMMAND if !SHOWING.get() && lparam.0 == 0 && (wparam.0 >> 16) & 0xFFFF == 0 => {
            CLICKED.set(true);
            false
        }
        WM_EXITMENULOOP if !SHOWING.get() => {
            let _ = unsafe { PostMessageW(Some(hwnd), WM_MENU_ENDED, WPARAM(0), LPARAM(0)) };
            false
        }
        WM_MENU_ENDED => {
            // A click still waiting in the queue counts as one too.
            let mut queued = MSG::default();
            let pending = unsafe {
                PeekMessageW(&mut queued, Some(hwnd), WM_COMMAND, WM_COMMAND, PM_NOREMOVE)
            }
            .as_bool();
            let chosen = CLICKED.get() || pending;
            if !emit(MenuHover::End { chosen }) {
                // The app was busy: try again once it is not.
                let _ = unsafe { PostMessageW(Some(hwnd), WM_MENU_ENDED, WPARAM(0), LPARAM(0)) };
            }
            return LRESULT(0);
        }
        _ => false,
    };
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// What a WM_MENUSELECT says is highlighted. None as the menu closes: its
/// end is told by `show` or WM_EXITMENULOOP, with whether an item was
/// clicked, and a look shown must stay on screen until then.
fn describe(wparam: usize, lparam: isize) -> Option<MenuHover> {
    let item = (wparam & 0xFFFF) as u32;
    let flags = ((wparam >> 16) & 0xFFFF) as u32;
    if flags == 0xFFFF && lparam == 0 {
        return None;
    }
    if flags & (MF_POPUP | MF_SEPARATOR | MF_DISABLED | MF_GRAYED) != 0 {
        return Some(MenuHover::Nothing);
    }
    let Some(group) = root().and_then(|root| group_of(root, lparam)) else {
        return Some(MenuHover::Nothing);
    };
    Some(match item_text(lparam, item, false) {
        Some(text) => MenuHover::Item { group, text },
        None => MenuHover::Nothing,
    })
}

/// Which part of the menu `hmenu` is: the top level (Some(None)), or one
/// of its submenus, by its title. None for a deeper submenu.
fn group_of(root: isize, hmenu: isize) -> Option<Option<String>> {
    if hmenu == root {
        return Some(None);
    }
    // Positions run until the menu has no more.
    for pos in 0..256u32 {
        let mut buf = [0u16; 128];
        let mut mii = MENUITEMINFOW {
            cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_SUBMENU | MIIM_STRING,
            dwTypeData: PWSTR(buf.as_mut_ptr()),
            cch: buf.len() as u32 - 1,
            ..Default::default()
        };
        if unsafe { GetMenuItemInfoW(HMENU(root as _), pos, true, &mut mii) }.is_err() {
            return None;
        }
        if mii.hSubMenu.0 as isize == hmenu {
            let n = (mii.cch as usize).min(buf.len() - 1);
            return Some(Some(String::from_utf16_lossy(&buf[..n])));
        }
    }
    None
}

/// An item's label: by its command id, or by its position.
fn item_text(hmenu: isize, item: u32, by_position: bool) -> Option<String> {
    let mut buf = [0u16; 128];
    let mut mii = MENUITEMINFOW {
        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
        fMask: MIIM_STRING,
        dwTypeData: PWSTR(buf.as_mut_ptr()),
        cch: buf.len() as u32 - 1,
        ..Default::default()
    };
    unsafe { GetMenuItemInfoW(HMENU(hmenu as _), item, by_position, &mut mii) }.ok()?;
    let n = (mii.cch as usize).min(buf.len() - 1);
    Some(String::from_utf16_lossy(&buf[..n]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Config;
    use crate::ui::context_menu::{ContextMenu, MenuAction};
    use windows::Win32::UI::WindowsAndMessaging::MIIM_ID;

    /// Every item of the real menu, as WM_MENUSELECT would describe it
    /// when highlighted: walked by position, then described by command id
    /// the way the subclass does.
    fn highlights(menu: &ContextMenu) -> Vec<MenuHover> {
        use muda::ContextMenu as _;
        let root = menu.menu.hpopupmenu();
        set_handler(root, |_| true);
        let mut out = Vec::new();
        let mut walk = |hmenu: isize| {
            for pos in 0..256u32 {
                let mut mii = MENUITEMINFOW {
                    cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_ID | MIIM_SUBMENU,
                    ..Default::default()
                };
                if unsafe { GetMenuItemInfoW(HMENU(hmenu as _), pos, true, &mut mii) }.is_err() {
                    break;
                }
                if mii.hSubMenu.0.is_null() && mii.wID != 0 {
                    out.extend(describe(mii.wID as usize, hmenu));
                }
            }
        };
        walk(root);
        for pos in 0..256u32 {
            let mut mii = MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_SUBMENU,
                ..Default::default()
            };
            if unsafe { GetMenuItemInfoW(HMENU(root as _), pos, true, &mut mii) }.is_err() {
                break;
            }
            if !mii.hSubMenu.0.is_null() {
                walk(mii.hSubMenu.0 as isize);
            }
        }
        out
    }

    /// Through the real menu's handles and labels, each look item is
    /// recognised as its own action, and the items that are not looks
    /// are recognised as none.
    #[test]
    fn the_real_menu_tells_each_look_item_apart() {
        let menu = ContextMenu::new(&Config::default(), false).unwrap();
        let actions: Vec<MenuAction> = highlights(&menu)
            .into_iter()
            .filter_map(|h| match h {
                MenuHover::Item { group, text } => menu.look_action(group.as_deref(), &text),
                _ => None,
            })
            .collect();
        // Detail 3, layout 2, forecast, width 3, arrangement 2, monochrome,
        // palettes 8, opacity 6, brightness 4, saturation 5.
        assert_eq!(actions.len(), 35, "{actions:?}");
        assert!(actions.contains(&MenuAction::SetOpacity(50)));
        assert!(actions.contains(&MenuAction::SetSaturation(50)));
        assert!(actions.contains(&MenuAction::ToggleMonochrome));
        let mut unique = actions.clone();
        unique.dedup();
        assert_eq!(unique.len(), actions.len());
    }

    /// A submenu's title shows nothing; the menu closing says nothing at
    /// all, so a look shown stays until the end tells whether it was
    /// clicked.
    #[test]
    fn a_title_shows_nothing_and_the_close_says_nothing() {
        assert_eq!(describe(0xFFFF << 16, 0), None);
        assert_eq!(
            describe(((MF_POPUP as usize) << 16) | 3, 1),
            Some(MenuHover::Nothing)
        );
    }

    /// Live, on the second monitor: a real menu opened the way tray-icon
    /// opens its own (TrackPopupMenu without TPM_RETURNCMD, the click
    /// posted as WM_COMMAND), driven by keys posted to the test's own
    /// window. Highlighting an item tells it; Enter ends the menu as
    /// clicked, Esc as not. `cargo test --lib menu_hover_live -- --ignored`
    #[test]
    #[ignore = "opens a menu on the second monitor"]
    fn menu_hover_live_tells_the_highlight_and_the_click() {
        use crate::ui::context_menu::ContextMenu;
        use muda::ContextMenu as _;
        use windows::core::w;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_RETURN};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, RegisterClassW,
            TrackPopupMenu, TranslateMessage, PM_REMOVE, TPM_LEFTALIGN, WM_KEYDOWN, WNDCLASSW,
            WS_EX_TOOLWINDOW, WS_POPUP,
        };

        unsafe extern "system" fn plain(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(h, m, w, l) }
        }
        let menu = ContextMenu::new(&Config::default(), false).unwrap();
        let root = menu.menu.hpopupmenu();
        let heard: Rc<RefCell<Vec<MenuHover>>> = Rc::default();
        let log = heard.clone();
        set_handler(root, move |ev| {
            log.borrow_mut().push(ev);
            true
        });
        unsafe {
            let inst = GetModuleHandleW(None).unwrap();
            let class = w!("AiLimitsMenuHoverTest");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(plain),
                hInstance: inst.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class,
                w!(""),
                WS_POPUP,
                4000,
                400,
                1,
                1,
                None,
                None,
                Some(inst.into()),
                None,
            )
            .unwrap();
            watch(hwnd.0 as isize);
            let run = |keys: &[u16]| {
                for k in keys {
                    let _ = PostMessageW(Some(hwnd), WM_KEYDOWN, WPARAM(*k as usize), LPARAM(0));
                }
                let _ =
                    TrackPopupMenu(HMENU(root as _), TPM_LEFTALIGN, 4000, 400, None, hwnd, None);
                // The loop after the menu: the end and the click arrive here.
                let mut msg = MSG::default();
                for _ in 0..50 {
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    if heard
                        .borrow()
                        .iter()
                        .any(|e| matches!(e, MenuHover::End { .. }))
                    {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                std::mem::take(&mut *heard.borrow_mut())
            };
            // Down, Enter: the first item ("Compact") highlighted and clicked.
            let clicked = run(&[VK_DOWN.0, VK_RETURN.0]);
            // Down, Esc: highlighted, then left.
            let left = run(&[VK_DOWN.0, VK_ESCAPE.0]);
            let _ = DestroyWindow(hwnd);
            eprintln!("clicked: {clicked:?}");
            eprintln!("left: {left:?}");
            let compact = MenuHover::Item {
                group: None,
                text: "Compact".into(),
            };
            // Nothing between the highlight and the end: a look shown stays.
            assert_eq!(
                clicked,
                [
                    MenuHover::Begin,
                    compact.clone(),
                    MenuHover::End { chosen: true }
                ]
            );
            assert_eq!(
                left,
                [MenuHover::Begin, compact, MenuHover::End { chosen: false }]
            );
        }
    }
}
