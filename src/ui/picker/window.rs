// ui/picker/window.rs — the picker's own window (tao + softbuffer), where it
// goes on the screen, and the Windows settings it follows (apps mode, High
// contrast, Text size).

use super::*;
use crate::ui::renderer::{PickerGlyphs, Renderer};
use crate::ui::theme::PickerTheme;
use std::num::NonZeroU32;
use tao::dpi::{PhysicalPosition, PhysicalSize};
use tao::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use tao::event_loop::EventLoopWindowTarget;
use tao::keyboard::{Key, KeyCode, ModifiersState};
use tao::window::{CursorIcon, Theme, Window, WindowBuilder, WindowId};

// ─── Placement ───────────────────────────────────────────────────

/// A rectangle on the screen, in physical pixels: the widget, or the tray
/// icon or taskbar panel when the widget is hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where the window goes, all in physical pixels: its box beside the anchor
/// (right of it, else left, else below, else above), `gap` away, then held
/// inside the work area (left, top, right, bottom). `size` is the window's,
/// `margin` its transparent edge on every side. Returns the window's
/// top-left corner.
pub fn place(
    anchor: ScreenRect,
    size: (i32, i32),
    margin: i32,
    gap: i32,
    work: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (l, t, r, b) = work;
    let (bw, bh) = (size.0 - 2 * margin, size.1 - 2 * margin);
    let right = (anchor.x + anchor.w + gap, anchor.y);
    let left = (anchor.x - gap - bw, anchor.y);
    let below = (anchor.x, anchor.y + anchor.h + gap);
    let above = (anchor.x, anchor.y - gap - bh);
    // A side fits when the box clears the work area's edge on that side;
    // sliding along the anchor is the clamp's job.
    let fits = [
        right.0 + bw <= r,
        left.0 >= l,
        below.1 + bh <= b,
        above.1 >= t,
    ];
    let cands = [right, left, below, above];
    let (x, y) = match fits.iter().position(|f| *f) {
        Some(i) => cands[i],
        // No side has room: the wider one, held in below.
        None if r - (anchor.x + anchor.w) >= anchor.x - l => right,
        None => left,
    };
    let x = x.min(r - bw).max(l);
    let y = y.min(b - bh).max(t);
    (x - margin, y - margin)
}

// ─── Window ──────────────────────────────────────────────────────

/// Logical pixels per wheel notch: three lines of 16.
const WHEEL_NOTCH: f32 = 48.0;
/// Between the anchor and the box, logical pixels.
const GAP: f32 = 8.0;

/// The picker's own window: borderless, transparent, on top, out of the
/// taskbar, presented through softbuffer exactly like the widget. It exists
/// only while open and repaints only on input.
///
/// The app keeps it in an `Option`, routes this window's events to
/// `handle_window_event`, applies the actions (then `set_input`), calls
/// `redraw` on its `RedrawRequested`, and drops it after `Close`.
///
/// It follows Windows' apps mode (light or dark), High contrast (the
/// colours it sets) and Text size (the whole picker drawn larger).
pub struct PickerWindow {
    window: Rc<Window>,
    /// Kept alive for the surface.
    _context: softbuffer::Context<Rc<Window>>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    renderer: Rc<Renderer>,
    picker: Picker,
    pixmap: tiny_skia::Pixmap,
    /// Rasterized glyphs, kept while the window is open.
    glyphs: PickerGlyphs,
    /// The monitor's scale factor.
    scale: f64,
    /// Windows' Text size (1 to 2.25): the picker is drawn that much larger,
    /// its layout the same.
    zoom: f64,
    theme: PickerTheme,
    work: Option<(i32, i32, i32, i32)>,
    pos: (i32, i32),
    size: (u32, u32),
    /// The cursor in the window, physical pixels, once it moved there.
    cursor: Option<(f64, f64)>,
    modifiers: ModifiersState,
    /// A focus loss counts as a click outside only after the focus came.
    had_focus: bool,
    text_cursor: bool,
    /// Let go behind other windows while a name is typed from a shell
    /// panel (Win+V, Win+.); on top again with the focus back.
    behind: bool,
}

impl PickerWindow {
    /// Open beside `anchor` (the widget's screen rect, or the tray icon or
    /// panel when the widget is hidden), on that monitor, at its scale,
    /// with the keyboard focus: opened from the menu or by a click.
    /// `renderer` is the widget's: its font measures and draws the picker.
    pub fn open<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        renderer: Rc<Renderer>,
        input: PickerInput,
        anchor: ScreenRect,
    ) -> anyhow::Result<Self> {
        Self::open_with(target, renderer, input, anchor, true)
    }

    /// Open as `open` does, but leave the keyboard focus with the window in
    /// use: for the welcome or the update notice opening by itself (at
    /// start-up, after an update), so it never takes a keystroke meant for
    /// another window. A click on it, or "Rows and models..." from the menu
    /// (`focus`), gives it the focus.
    pub fn open_quietly<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        renderer: Rc<Renderer>,
        input: PickerInput,
        anchor: ScreenRect,
    ) -> anyhow::Result<Self> {
        Self::open_with(target, renderer, input, anchor, false)
    }

    fn open_with<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        renderer: Rc<Renderer>,
        input: PickerInput,
        anchor: ScreenRect,
        activate: bool,
    ) -> anyhow::Result<Self> {
        use anyhow::Context as _;
        let (cx, cy) = (anchor.x + anchor.w / 2, anchor.y + anchor.h / 2);
        let mut scale = target
            .monitor_from_point(cx as f64, cy as f64)
            .map_or(1.0, |m| m.scale_factor());
        let zoom = system::text_scale();
        // An auto-hidden taskbar's band is kept clear too: the bar slides
        // in over whatever sits there.
        let work = crate::platform::safe_area_at(cx, cy);
        let measure = renderer.clone();
        let mut picker = Picker::new(input, move |t, s| measure.text_width(t, s));
        if let Some(w) = work {
            picker.set_max_height(max_box_height(w, scale * zoom));
        }
        let mut size = physical(picker.window_size(), scale * zoom);
        let mut pos = place_window(anchor, size, scale * zoom, work);

        let mut builder = WindowBuilder::new()
            // What a screen reader says for the window.
            .with_title(picker.title())
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(true)
            .with_resizable(false)
            .with_visible(false)
            .with_focused(activate)
            .with_min_inner_size(PhysicalSize::new(40u32, 40u32))
            .with_inner_size(PhysicalSize::new(size.0, size.1))
            .with_position(PhysicalPosition::new(pos.0, pos.1));
        #[cfg(target_os = "windows")]
        {
            use tao::platform::windows::WindowBuilderExtWindows;
            // Out of the taskbar and Alt+Tab; no DWM frame seam (see app.rs).
            builder = builder
                .with_skip_taskbar(true)
                .with_undecorated_shadow(false);
        }
        let window = Rc::new(builder.build(target).context("picker window")?);

        // The monitor the window landed on decides its scale.
        let actual = window.scale_factor();
        if (actual - scale).abs() > 0.01 {
            scale = actual;
            if let Some(w) = work {
                picker.set_max_height(max_box_height(w, scale * zoom));
            }
            size = physical(picker.window_size(), scale * zoom);
            pos = place_window(anchor, size, scale * zoom, work);
            window.set_outer_position(PhysicalPosition::new(pos.0, pos.1));
        }
        // Windows clamps the size at creation; set it again (as app.rs does).
        window.set_inner_size(PhysicalSize::new(size.0, size.1));

        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| anyhow::anyhow!("picker softbuffer context: {e}"))?;
        let mut surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow::anyhow!("picker softbuffer surface: {e}"))?;
        surface
            .resize(
                NonZeroU32::new(size.0).context("zero width")?,
                NonZeroU32::new(size.1).context("zero height")?,
            )
            .map_err(|e| anyhow::anyhow!("picker surface resize: {e}"))?;
        let pixmap = tiny_skia::Pixmap::new(size.0, size.1).context("picker pixmap")?;

        let theme = picker_theme(&window);
        let mut me = Self {
            window,
            _context: context,
            surface,
            renderer,
            picker,
            pixmap,
            glyphs: PickerGlyphs::default(),
            scale,
            zoom,
            theme,
            work,
            pos,
            size,
            cursor: None,
            modifiers: ModifiersState::empty(),
            had_focus: false,
            text_cursor: false,
            behind: false,
        };
        me.redraw();
        me.window.set_visible(true);
        Ok(me)
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    /// The scale the picker is drawn at: the monitor's, times Text size.
    fn draw_scale(&self) -> f64 {
        self.scale * self.zoom
    }

    /// One of this window's events; returns the actions to apply, in order.
    pub fn handle_window_event(&mut self, event: &WindowEvent<'_>) -> Vec<PickerAction> {
        let s = self.draw_scale() as f32;
        let at = |p: (f64, f64)| (p.0 as f32 / s, p.1 as f32 / s);
        let ev = match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                let (x, y) = at((position.x, position.y));
                Some(PickerEvent::PointerMove { x, y })
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                Some(PickerEvent::PointerLeave)
            }
            // A press before any move (the window appeared under a still
            // cursor) has no position: it is not taken for a click outside.
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.cursor.map(|c| {
                let (x, y) = at(c);
                match state {
                    ElementState::Pressed => PickerEvent::PointerDown { x, y },
                    _ => PickerEvent::PointerUp { x, y },
                }
            }),
            WindowEvent::MouseWheel { delta, .. } => Some(PickerEvent::Wheel {
                dy: match delta {
                    MouseScrollDelta::LineDelta(_, lines) => lines * WHEEL_NOTCH,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / s,
                    _ => 0.0,
                },
            }),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => self.key_event(event),
            // A finished IME composition arrives whole.
            WindowEvent::ReceivedImeText(text) => Some(PickerEvent::Text(text.clone())),
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = *m;
                None
            }
            WindowEvent::Focused(true) => {
                self.had_focus = true;
                if self.behind {
                    self.behind = false;
                    self.set_on_top(true);
                }
                None
            }
            WindowEvent::Focused(false) => {
                // Typing a name, the focus may have gone to a shell panel
                // that types into the field: the picker stays (see
                // `Picker::focus_lost`), behind whatever took the focus.
                if self.had_focus && self.picker.editing() {
                    self.behind = true;
                    self.set_on_top(false);
                }
                self.had_focus.then_some(PickerEvent::FocusLost)
            }
            WindowEvent::CloseRequested => Some(PickerEvent::CloseRequested),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // The size follows in the next redraw; glyphs come anew.
                self.scale = *scale_factor;
                self.glyphs = PickerGlyphs::default();
                if let Some(w) = self.work {
                    self.picker
                        .set_max_height(max_box_height(w, self.draw_scale()));
                }
                self.window.request_redraw();
                None
            }
            WindowEvent::ThemeChanged(_) => {
                self.theme = picker_theme(&self.window);
                self.window.request_redraw();
                None
            }
            WindowEvent::Resized(s) => {
                // Sized by someone else (Windows after a DPI change): the
                // next redraw sets the size the picker needs.
                if (s.width, s.height) != self.size {
                    self.size = (0, 0);
                }
                self.window.request_redraw();
                None
            }
            _ => None,
        };
        let actions = ev.map(|e| self.picker.handle(e)).unwrap_or_default();
        if self.picker.take_dirty() {
            self.window.request_redraw();
            // The IME's candidates open beside the caret of the field typed in.
            if let Some((x, y)) = self.picker.caret_spot() {
                let s = self.zoom;
                self.window
                    .set_ime_position(tao::dpi::LogicalPosition::new(x as f64 * s, y as f64 * s));
            }
        }
        let text = self.picker.wants_text_cursor();
        if text != self.text_cursor {
            self.text_cursor = text;
            self.window.set_cursor_icon(if text {
                CursorIcon::Text
            } else {
                CursorIcon::Default
            });
        }
        actions
    }

    /// Hold the window above others, or let it go behind them, without
    /// activating it. (tao's own setter shows the window again, which
    /// would take the focus back from the panel the user is typing in.)
    fn set_on_top(&self, on: bool) {
        #[cfg(target_os = "windows")]
        {
            use tao::platform::windows::WindowExtWindows;
            /// SetWindowPos's HWND_TOPMOST and HWND_NOTOPMOST.
            const TOPMOST: isize = -1;
            const NOT_TOPMOST: isize = -2;
            crate::platform::place_below(
                self.window.hwnd(),
                if on { TOPMOST } else { NOT_TOPMOST },
            );
        }
        #[cfg(not(target_os = "windows"))]
        let _ = on;
    }

    /// Ask about a provider's sign-in at once (`Picker::ask_sign_in`), and
    /// bring the window forward.
    pub fn ask_sign_in(&mut self, provider: &str) {
        self.picker.ask_sign_in(provider);
        self.picker.take_dirty();
        self.window.set_focus();
        self.window.request_redraw();
    }

    /// Bring the window forward (opened again from the menu).
    pub fn focus(&self) {
        self.window.set_focus();
    }

    /// The app applied an action: show the new state.
    pub fn set_input(&mut self, input: PickerInput) {
        self.picker.set_input(input);
        self.picker.take_dirty();
        self.window.request_redraw();
    }

    /// Paint and present (on `RedrawRequested` for this window).
    pub fn redraw(&mut self) {
        let scale = self.draw_scale();
        let want = physical(self.picker.window_size(), scale);
        if want != self.size {
            self.resize(want);
        }
        let scene = self.picker.scene();
        self.renderer.draw_picker(
            &mut self.pixmap,
            &scene,
            &self.theme,
            scale as f32,
            &mut self.glyphs,
        );
        // Copy the pixmap into softbuffer: RGBA to 0xAARRGGBB, as app.rs does.
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            for (dst, src) in buffer.iter_mut().zip(self.pixmap.pixels().iter()) {
                let c = src.demultiply();
                *dst = ((c.alpha() as u32) << 24)
                    | ((c.red() as u32) << 16)
                    | ((c.green() as u32) << 8)
                    | (c.blue() as u32);
            }
            if let Err(e) = buffer.present() {
                tracing::warn!("picker present failed: {e}");
            }
        }
    }

    /// Hide and destroy the window.
    pub fn close(self) {
        self.window.set_visible(false);
    }

    /// Grow or shrink in place; a box that grew past the work area's bottom
    /// or right edge is pulled back in.
    fn resize(&mut self, want: (u32, u32)) {
        self.size = want;
        self.window
            .set_inner_size(PhysicalSize::new(want.0, want.1));
        if let (Some(w), Some(h)) = (NonZeroU32::new(want.0), NonZeroU32::new(want.1)) {
            if let Err(e) = self.surface.resize(w, h) {
                tracing::warn!("picker surface resize failed: {e}");
            }
        }
        if let Some(pm) = tiny_skia::Pixmap::new(want.0, want.1) {
            self.pixmap = pm;
        }
        if let Some((l, t, r, b)) = self.work {
            let m = (MARGIN as f64 * self.draw_scale()).round() as i32;
            let x = self.pos.0.min(r - want.0 as i32 + m).max(l - m);
            let y = self.pos.1.min(b - want.1 as i32 + m).max(t - m);
            if (x, y) != self.pos {
                self.pos = (x, y);
                self.window.set_outer_position(PhysicalPosition::new(x, y));
            }
        }
    }

    fn key_event(&self, ev: &KeyEvent) -> Option<PickerEvent> {
        if ev.state != ElementState::Pressed {
            return None;
        }
        let (shift, ctrl, alt) = (
            self.modifiers.shift_key(),
            self.modifiers.control_key(),
            self.modifiers.alt_key(),
        );
        // A held Enter or Space must not answer a question it just opened:
        // the keys that act are taken once per press (a held Space still
        // types spaces into a name field).
        let acts = matches!(ev.logical_key, Key::Enter | Key::Space)
            || matches!(&ev.logical_key, Key::Character(c) if *c == " ");
        if ev.repeat && acts && !(self.picker.editing() && !matches!(ev.logical_key, Key::Enter)) {
            return None;
        }
        let key = match &ev.logical_key {
            Key::Tab if shift => Some(PickerKey::BackTab),
            Key::Tab => Some(PickerKey::Tab),
            Key::ArrowUp => Some(PickerKey::Up),
            Key::ArrowDown => Some(PickerKey::Down),
            Key::ArrowLeft => Some(PickerKey::Left),
            Key::ArrowRight => Some(PickerKey::Right),
            Key::Space => Some(PickerKey::Space),
            Key::Character(c) if *c == " " && !ctrl => Some(PickerKey::Space),
            Key::Enter => Some(PickerKey::Enter),
            Key::Escape => Some(PickerKey::Escape),
            Key::Home => Some(PickerKey::Home),
            Key::End => Some(PickerKey::End),
            Key::PageUp => Some(PickerKey::PageUp),
            Key::PageDown => Some(PickerKey::PageDown),
            Key::Backspace => Some(PickerKey::Backspace),
            Key::Delete => Some(PickerKey::Delete),
            _ => None,
        };
        if let Some(k) = key {
            return Some(PickerEvent::Key(k));
        }
        // Ctrl+V by the key's position (any layout), or Shift+Insert. The
        // clipboard is read only into a name field being edited.
        let paste = (ctrl && !alt && ev.physical_key == KeyCode::KeyV)
            || (shift && ev.logical_key == Key::Insert);
        if paste {
            return self
                .picker
                .editing()
                .then(|| PickerEvent::Paste(clipboard_text()));
        }
        // A Ctrl shortcut types nothing; AltGr (Ctrl+Alt) does.
        if ctrl && !alt {
            return None;
        }
        ev.text.map(|t| PickerEvent::Text(t.to_string()))
    }
}

/// The picker's colours: High contrast's own when it is on, else light or
/// dark as Windows' apps mode is (tao reads AppsUseLightTheme for the window).
fn picker_theme(window: &Window) -> PickerTheme {
    match system::high_contrast() {
        Some(colours) => PickerTheme::high_contrast(&colours),
        None => PickerTheme::for_system(window.theme() == Theme::Light),
    }
}

/// Windows' accessibility settings the picker follows, read when it opens.
/// Only calls the app already makes elsewhere (SystemParametersInfoW and the
/// registry readers), so the exe imports nothing new for them.
#[cfg(target_os = "windows")]
pub(super) mod system {
    use crate::ui::theme::ContrastColors;
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    };

    /// Whether High contrast is on, and if so the colours it sets. They are
    /// read where Windows keeps the current scheme, `HKCU\Control
    /// Panel\Colors`; a value missing there takes the usual contrast
    /// scheme's.
    pub fn high_contrast() -> Option<ContrastColors> {
        use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
        use windows::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        };
        let mut hc = HIGHCONTRASTW {
            cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        let ok = unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                hc.cbSize,
                Some(&mut hc as *mut HIGHCONTRASTW as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        if ok.is_err() || (hc.dwFlags & HCF_HIGHCONTRASTON).0 == 0 {
            return None;
        }
        let fallback = ContrastColors::default();
        let key = Key::open(w!("Control Panel\\Colors"));
        let get = |name: PCWSTR, or: [u8; 3]| {
            key.as_ref()
                .and_then(|k| k.string(name))
                .and_then(|s| parse_rgb(&s))
                .unwrap_or(or)
        };
        Some(ContrastColors {
            window: get(w!("Window"), fallback.window),
            window_text: get(w!("WindowText"), fallback.window_text),
            highlight: get(w!("Hilight"), fallback.highlight),
            highlight_text: get(w!("HilightText"), fallback.highlight_text),
            gray_text: get(w!("GrayText"), fallback.gray_text),
        })
    }

    /// Windows' Text size (Settings, Accessibility), 1.0 to 2.25.
    pub fn text_scale() -> f64 {
        Key::open(w!("Software\\Microsoft\\Accessibility"))
            .and_then(|k| k.dword(w!("TextScaleFactor")))
            .map_or(1.0, |v| (v as f64 / 100.0).clamp(1.0, 2.25))
    }

    /// "R G B", as the registry keeps a colour.
    pub(in crate::ui::picker) fn parse_rgb(s: &str) -> Option<[u8; 3]> {
        let mut it = s.split_whitespace().map(|p| p.parse::<u8>().ok());
        let rgb = [it.next()??, it.next()??, it.next()??];
        it.next().is_none().then_some(rgb)
    }

    /// An open key under HKEY_CURRENT_USER, closed when dropped.
    struct Key(HKEY);

    impl Key {
        fn open(path: PCWSTR) -> Option<Key> {
            let mut key = HKEY::default();
            let ok =
                unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path, None, KEY_QUERY_VALUE, &mut key) };
            ok.is_ok().then_some(Key(key))
        }

        fn dword(&self, name: PCWSTR) -> Option<u32> {
            let mut data: u32 = 0;
            let mut len = std::mem::size_of::<u32>() as u32;
            let ok = unsafe {
                RegQueryValueExW(
                    self.0,
                    name,
                    None,
                    None,
                    Some(&mut data as *mut u32 as *mut u8),
                    Some(&mut len),
                )
            };
            (ok.is_ok() && len == 4).then_some(data)
        }

        fn string(&self, name: PCWSTR) -> Option<String> {
            let mut buf = [0u16; 64];
            let mut len = std::mem::size_of_val(&buf) as u32;
            let ok = unsafe {
                RegQueryValueExW(
                    self.0,
                    name,
                    None,
                    None,
                    Some(buf.as_mut_ptr() as *mut u8),
                    Some(&mut len),
                )
            };
            if ok.is_err() {
                return None;
            }
            let chars = (len as usize / 2).min(buf.len());
            let end = buf[..chars].iter().position(|&c| c == 0).unwrap_or(chars);
            Some(String::from_utf16_lossy(&buf[..end]))
        }
    }

    impl Drop for Key {
        fn drop(&mut self) {
            let _ = unsafe { RegCloseKey(self.0) };
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod system {
    pub fn high_contrast() -> Option<crate::ui::theme::ContrastColors> {
        None
    }

    pub fn text_scale() -> f64 {
        1.0
    }
}

/// The clipboard's text, or nothing.
fn clipboard_text() -> String {
    arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .unwrap_or_default()
}

fn physical(size: (f32, f32), scale: f64) -> (u32, u32) {
    (
        ((size.0 as f64) * scale).ceil().max(1.0) as u32,
        ((size.1 as f64) * scale).ceil().max(1.0) as u32,
    )
}

/// The tallest box the work area allows, logical pixels.
pub(super) fn max_box_height(work: (i32, i32, i32, i32), scale: f64) -> f32 {
    ((work.3 - work.1) as f64 / scale) as f32 - MARGIN * 2.0 - GAP * 2.0
}

fn place_window(
    anchor: ScreenRect,
    size: (u32, u32),
    scale: f64,
    work: Option<(i32, i32, i32, i32)>,
) -> (i32, i32) {
    let margin = (MARGIN as f64 * scale).round() as i32;
    let gap = (GAP as f64 * scale).round() as i32;
    // Without a work area (the query failed) nothing holds it in.
    let work = work.unwrap_or((i32::MIN / 4, i32::MIN / 4, i32::MAX / 4, i32::MAX / 4));
    place(anchor, (size.0 as i32, size.1 as i32), margin, gap, work)
}
