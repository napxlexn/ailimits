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

/// A side of the anchor: where the box goes, or where the widget grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
    Below,
    Above,
}

/// What the window opens beside, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    /// The widget as it is now (or the tray icon, the cursor): the window
    /// opens on its monitor.
    pub rect: ScreenRect,
    /// `rect` joined with the room the widget may grow into while the
    /// window is open: the box keeps out of it, so a row ticked shows
    /// beside the window, never under it.
    pub reach: ScreenRect,
    /// The side the widget grows to as rows are ticked.
    pub grows: Option<Side>,
}

impl Anchor {
    /// A spot that does not grow: the tray icon, the cursor, a widget with
    /// no row left to tick.
    pub fn fixed(rect: ScreenRect) -> Self {
        Self {
            rect,
            reach: rect,
            grows: None,
        }
    }

    /// The widget at `rect`; `grown` its size once every row the window
    /// offers is ticked for it. It keeps its corner as it grows, pulled
    /// back into `work` where it would cross the edge
    /// (`ui::window::clamp_to_work_area`, as `widget::apply_ui_change`
    /// does), so near the right edge its columns grow to the left, near
    /// the bottom its rows grow upwards.
    pub fn widget(rect: ScreenRect, grown: (i32, i32), work: Option<(i32, i32, i32, i32)>) -> Self {
        let (dw, dh) = (grown.0 - rect.w, grown.1 - rect.h);
        if dw <= 0 && dh <= 0 {
            return Self::fixed(rect);
        }
        let (gx, gy) = match work {
            Some((l, t, r, b)) => (
                rect.x.min(r - grown.0).max(l),
                rect.y.min(b - grown.1).max(t),
            ),
            None => (rect.x, rect.y),
        };
        let (x0, y0) = (rect.x.min(gx), rect.y.min(gy));
        let x1 = (rect.x + rect.w).max(gx + grown.0);
        let y1 = (rect.y + rect.h).max(gy + grown.1);
        let grows = match (dw > dh, gx < rect.x, gy < rect.y) {
            (true, true, _) => Side::Left,
            (true, false, _) => Side::Right,
            (false, _, true) => Side::Above,
            (false, _, false) => Side::Below,
        };
        Self {
            rect,
            reach: ScreenRect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            },
            grows: Some(grows),
        }
    }
}

/// Where the window goes, all in physical pixels: its box beside `anchor`
/// (the widget's reach), `gap` away, then held inside the work area (left,
/// top, right, bottom). Beside it first, else below or above; of each pair
/// the side with more room first, away from the nearer edge of the screen;
/// the side the widget `grows` to last of all, so the rows ticked in the
/// window come into sight beside it. `size` is the window's, `margin` its
/// transparent edge on every side. Returns the window's top-left corner.
pub fn place(
    anchor: ScreenRect,
    grows: Option<Side>,
    size: (i32, i32),
    margin: i32,
    gap: i32,
    work: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (l, t, r, b) = work;
    let (bw, bh) = (size.0 - 2 * margin, size.1 - 2 * margin);
    let room = |s: Side| match s {
        Side::Right => r - (anchor.x + anchor.w),
        Side::Left => anchor.x - l,
        Side::Below => b - (anchor.y + anchor.h),
        Side::Above => anchor.y - t,
    };
    let spot = |s: Side| match s {
        Side::Right => (anchor.x + anchor.w + gap, anchor.y),
        Side::Left => (anchor.x - gap - bw, anchor.y),
        Side::Below => (anchor.x, anchor.y + anchor.h + gap),
        Side::Above => (anchor.x, anchor.y - gap - bh),
    };
    // A side fits when the box clears the work area's edge on that side;
    // sliding along the anchor is the clamp's job.
    let fits = |s: Side| {
        let (x, y) = spot(s);
        match s {
            Side::Right => x + bw <= r,
            Side::Left => x >= l,
            Side::Below => y + bh <= b,
            Side::Above => y >= t,
        }
    };
    // The roomier of two sides first; a tie keeps the first.
    let roomier = |a: Side, z: Side| if room(z) > room(a) { [z, a] } else { [a, z] };
    let beside = roomier(Side::Right, Side::Left);
    let mut order = beside
        .into_iter()
        .chain(roomier(Side::Below, Side::Above))
        .filter(|s| Some(*s) != grows)
        .chain(grows);
    let (x, y) = match order.find(|s| fits(*s)) {
        Some(s) => spot(s),
        // No side has room: the roomier one beside it that the widget
        // does not grow to, held in below.
        None => spot(
            beside
                .into_iter()
                .find(|s| Some(*s) != grows)
                .unwrap_or(beside[0]),
        ),
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
/// Between the box and the edges of the work area (the screen's, or the
/// taskbar's), logical pixels: never flush against them, a little wider
/// than the 12 px the shell's own flyouts keep.
const EDGE: f32 = 16.0;

/// The picker's own window: borderless, transparent, presented through
/// softbuffer exactly like the widget. It behaves as any program's window:
/// in the taskbar and Alt+Tab, moved by its title, above other windows
/// while in use and behind them once another one is, and closed only on
/// purpose (Esc, the cross, Alt+F4). It exists only while open and repaints
/// only on input.
///
/// The app keeps it in an `Option`, routes this window's events to
/// `handle_window_event`, applies the actions (then `set_input`), calls
/// `redraw` on its `RedrawRequested`, and drops it after `Close`, keeping
/// where it was moved to (`close`) for the next opening.
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
    text_cursor: bool,
    /// Let go behind other windows while another one is in use; on top
    /// again with the focus back.
    behind: bool,
    /// Pressed on its title: a move that follows is the user's.
    dragging: bool,
    /// Where it is was chosen by the user: moved by its title, or opened
    /// where it was left.
    placed: bool,
    /// Moved since the work area was found: found again at the next redraw,
    /// as the window may be on another monitor now.
    moved: bool,
    /// When a description the pointer rests on is due: drawn then.
    hint_due: Option<std::time::Instant>,
}

impl PickerWindow {
    /// Open beside `anchor` (the widget and the room it may grow into, or
    /// the cursor when the widget is hidden), on that monitor, at its scale,
    /// with the keyboard focus: opened from the menu or by a click. `at`:
    /// where the user left it the last time (`close`), used while a monitor
    /// is still there. `renderer` is the widget's: its font measures and
    /// draws the picker.
    pub fn open<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        renderer: Rc<Renderer>,
        input: PickerInput,
        anchor: Anchor,
        at: Option<(i32, i32)>,
    ) -> anyhow::Result<Self> {
        Self::open_with(target, renderer, input, anchor, at, true)
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
        anchor: Anchor,
        at: Option<(i32, i32)>,
    ) -> anyhow::Result<Self> {
        Self::open_with(target, renderer, input, anchor, at, false)
    }

    fn open_with<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        renderer: Rc<Renderer>,
        input: PickerInput,
        anchor: Anchor,
        at: Option<(i32, i32)>,
        activate: bool,
    ) -> anyhow::Result<Self> {
        use anyhow::Context as _;
        // Where it was left, while a monitor still shows its title there.
        let at = at.filter(|&(x, y)| {
            crate::platform::safe_area_at(x + TITLE_SPOT.0, y + TITLE_SPOT.1).is_some()
        });
        let (cx, cy) = match at {
            Some((x, y)) => (x + TITLE_SPOT.0, y + TITLE_SPOT.1),
            None => (
                anchor.rect.x + anchor.rect.w / 2,
                anchor.rect.y + anchor.rect.h / 2,
            ),
        };
        let mut scale = target
            .monitor_from_point(cx as f64, cy as f64)
            .map_or(1.0, |m| m.scale_factor());
        let zoom = system::text_scale();
        // An auto-hidden taskbar's band is kept clear too: the bar slides
        // in over whatever sits there.
        let work = crate::platform::safe_area_at(cx, cy).map(|a| even_area(a.monitor, a.safe));
        let measure = renderer.clone();
        let mut picker = Picker::new(input, move |t, s| measure.text_width(t, s));
        if let Some(w) = work {
            picker.set_max_height(max_box_height(w, scale * zoom));
        }
        let mut size = physical(picker.window_size(), scale * zoom);
        let spot = |size, scale| match at {
            Some(p) => hold_in(p, size, scale, work),
            None => place_window(anchor, size, scale, work),
        };
        let mut pos = spot(size, scale * zoom);

        let mut builder = WindowBuilder::new()
            // What a screen reader says for the window.
            .with_title(picker.title())
            .with_window_icon(app_icon())
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(true)
            .with_resizable(false)
            .with_minimizable(false)
            .with_maximizable(false)
            .with_visible(false)
            .with_focused(activate)
            .with_min_inner_size(PhysicalSize::new(40u32, 40u32))
            .with_inner_size(PhysicalSize::new(size.0, size.1))
            .with_position(PhysicalPosition::new(pos.0, pos.1));
        #[cfg(target_os = "windows")]
        {
            use tao::platform::windows::WindowBuilderExtWindows;
            // In the taskbar and Alt+Tab, as a program's window is, so it can
            // be found behind others; no DWM frame seam (see app.rs).
            builder = builder.with_undecorated_shadow(false);
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
            pos = spot(size, scale * zoom);
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
            text_cursor: false,
            behind: false,
            dragging: false,
            placed: at.is_some(),
            moved: false,
            hint_due: None,
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
            // Pressed on its title: moved as any window is, by Windows.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if self.cursor.is_some_and(|c| {
                let (x, y) = at(c);
                self.picker.drags_at(x, y)
            }) =>
            {
                self.dragging = true;
                if let Err(e) = self.window.drag_window() {
                    tracing::debug!("the picker could not be moved: {e}");
                }
                None
            }
            // A press before any move (the window appeared under a still
            // cursor) has no position: it is not taken for a click.
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.cursor.map(|c| {
                if *state == ElementState::Pressed {
                    self.dragging = false;
                }
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
                if self.behind {
                    self.behind = false;
                    self.set_on_top(true);
                }
                None
            }
            // Another window in use: the picker stays open, behind it, as a
            // program's window does (a name typed from a shell panel, Win+V
            // or Win+., goes on when the focus comes back).
            WindowEvent::Focused(false) => {
                self.behind = true;
                self.set_on_top(false);
                Some(PickerEvent::FocusLost)
            }
            WindowEvent::CloseRequested => Some(PickerEvent::CloseRequested),
            WindowEvent::Moved(p) => {
                if (p.x, p.y) != self.pos {
                    self.pos = (p.x, p.y);
                    self.moved = true;
                    self.placed |= self.dragging;
                }
                None
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // The size follows in the next redraw, for the monitor it
                // is on now; glyphs come anew.
                self.scale = *scale_factor;
                self.glyphs = PickerGlyphs::default();
                self.moved = true;
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
        self.hint_due = self.picker.next_wake();
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
        if self.moved {
            self.find_work();
        }
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

    /// When the description the pointer rests on is due (`Picker::next_wake`).
    pub fn next_wake(&self) -> Option<std::time::Instant> {
        self.picker.next_wake()
    }

    /// A timer came due: draw again if a description is due now, so it
    /// shows after its wait.
    pub fn on_timer(&mut self, now: std::time::Instant) {
        if self.hint_due.is_some_and(|t| now >= t) {
            self.hint_due = None;
            self.window.request_redraw();
        }
        self.hint_due = self.hint_due.or_else(|| self.picker.next_wake());
    }

    /// Hide and destroy the window. Returns where it is when the user chose
    /// that (moved it by its title, or it opened where they left it), for
    /// the next opening.
    pub fn close(self) -> Option<(i32, i32)> {
        self.window.set_visible(false);
        self.placed.then_some(self.pos)
    }

    /// The work area of the monitor the window is on now, and the tallest
    /// box it allows.
    fn find_work(&mut self) {
        self.moved = false;
        let (cx, cy) = (
            self.pos.0 + self.size.0 as i32 / 2,
            self.pos.1 + self.size.1 as i32 / 2,
        );
        if let Some(a) = crate::platform::safe_area_at(cx, cy) {
            let w = even_area(a.monitor, a.safe);
            self.work = Some(w);
            self.picker
                .set_max_height(max_box_height(w, self.draw_scale()));
        }
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
        // Grown or shrunk: held in as when it opened, `EDGE` off the work
        // area's edges.
        if self.work.is_some() {
            let (x, y) = hold_in(self.pos, want, self.draw_scale(), self.work);
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
/// The room the window may use: the safe area, kept as far from every edge
/// of the monitor as it is from the taskbar's (an auto-hidden one's band
/// counted), so the window sits as far from the top as from the bottom
/// wherever the widget and the taskbar are. A monitor with no bar keeps it
/// all.
pub(super) fn even_area(
    monitor: (i32, i32, i32, i32),
    safe: (i32, i32, i32, i32),
) -> (i32, i32, i32, i32) {
    let (ml, mt, mr, mb) = monitor;
    let (sl, st, sr, sb) = safe;
    let bar = (sl - ml).max(st - mt).max(mr - sr).max(mb - sb).max(0);
    let even = (
        sl.max(ml + bar),
        st.max(mt + bar),
        sr.min(mr - bar),
        sb.min(mb - bar),
    );
    // A bar thicker than half the monitor (never seen): the safe area alone.
    if even.2 - even.0 < (sr - sl) / 2 || even.3 - even.1 < (sb - st) / 2 {
        return safe;
    }
    even
}

/// A spot on the title, from the window's corner, physical pixels: where a
/// window left there is looked for when it opens again.
const TITLE_SPOT: (i32, i32) = (60, 30);

/// A window corner `pos` held inside the work area drawn in by `EDGE`, as
/// far as its box fits; without a work area, `pos` as it is.
pub(super) fn hold_in(
    pos: (i32, i32),
    size: (u32, u32),
    scale: f64,
    work: Option<(i32, i32, i32, i32)>,
) -> (i32, i32) {
    let Some((l, t, r, b)) = work else {
        return pos;
    };
    let m = (MARGIN as f64 * scale).round() as i32;
    let e = (EDGE as f64 * scale).round() as i32;
    let (l, t, r, b) = (l + e, t + e, r - e, b - e);
    let x = pos.0.min(r - size.0 as i32 + m).max(l - m);
    let y = pos.1.min(b - size.1 as i32 + m).max(t - m);
    (x, y)
}

/// The app's icon, for the taskbar and Alt+Tab: drawn from raw pixels
/// (`CreateIcon`, as the tray icon is), not loaded from the exe's resource.
fn app_icon() -> Option<tao::window::Icon> {
    const SIZE: u32 = 48;
    const ICON: &[u8] = include_bytes!("../../../assets/icon-48.rgba");
    tao::window::Icon::from_rgba(ICON.to_vec(), SIZE, SIZE).ok()
}

pub(super) fn max_box_height(work: (i32, i32, i32, i32), scale: f64) -> f32 {
    ((work.3 - work.1) as f64 / scale) as f32 - MARGIN * 2.0 - GAP * 2.0 - EDGE * 2.0
}

/// The window's corner: `place` inside the work area drawn in by `EDGE` on
/// every side, so the box never touches the screen's edge or the taskbar.
pub(super) fn place_window(
    anchor: Anchor,
    size: (u32, u32),
    scale: f64,
    work: Option<(i32, i32, i32, i32)>,
) -> (i32, i32) {
    let margin = (MARGIN as f64 * scale).round() as i32;
    let gap = (GAP as f64 * scale).round() as i32;
    let edge = (EDGE as f64 * scale).round() as i32;
    // Without a work area (the query failed) nothing holds it in.
    let work = match work {
        Some((l, t, r, b)) => (l + edge, t + edge, r - edge, b - edge),
        None => (i32::MIN / 4, i32::MIN / 4, i32::MAX / 4, i32::MAX / 4),
    };
    place(
        anchor.reach,
        anchor.grows,
        (size.0 as i32, size.1 as i32),
        margin,
        gap,
        work,
    )
}
