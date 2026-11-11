// ui/picker.rs — the "Rows and models" window: the app's own layered window,
// drawn by the renderer, that chooses rows, model labels and model names, and
// asks before each read of another tool's files.
//
// The picker never reads or writes config.toml or any other file. It shows a
// `PickerInput` the app gives it and answers with `PickerAction`s; the app
// applies each one at once (widget, tooltip, panel, tray) and gives it a new
// input.
//
// Three parts:
// - `Picker`, a pure state machine: input events in, actions out, plus a
//   `PickerScene` (shapes in logical pixels) for the renderer. All the logic
//   lives here and is unit-tested without a window.
// - `PickerWindow`, thin glue: a tao window presented through softbuffer
//   like the widget, which turns window events into `PickerEvent`s. It
//   repaints only on input; it has no timers.
// - The drawing itself is `Renderer::draw_picker` (ui/renderer.rs), the
//   colours `PickerTheme` (ui/theme.rs).

use crate::config::schema::{ModelPart, ModelSource, Reads, Surface, UiModel};
use crate::ui::layout::Rect;
use std::cell::RefCell;
use std::collections::HashMap;

/// What the picker opens as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    /// From the menu.
    Normal,
    /// By itself on a fresh install: "Welcome to AI Limits", the tools found
    /// on this PC marked.
    Welcome,
    /// Once, on the first start after the update that added `[reads]`: every
    /// file read and every host contacted, with OK and Review.
    UpdateNotice,
}

/// A read the picker asks about before the first tick that needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadKind {
    /// A provider's sign-in (config id).
    SignIn(String),
    /// The tools' settings files (model and effort).
    ToolSettings,
    /// The ends of the newest session files (model last used).
    LastSession,
}

/// What the picker asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerAction {
    /// Show or hide a row ("claude", "claude.opus") on a surface.
    SetRow {
        surface: Surface,
        row: String,
        on: bool,
    },
    /// Show or hide a part of the model label on a surface.
    SetPart {
        surface: Surface,
        part: ModelPart,
        on: bool,
    },
    SetSource(ModelSource),
    /// Rename a model by its id as the tool stores it; None brings the
    /// automatic name back. The picker has already held the name to
    /// `crate::rows::name_fits`.
    Rename {
        model_id: String,
        name: Option<String>,
    },
    /// The user allowed a read in the picker's question. Sent before the tick
    /// that needed it.
    Allow(ReadKind),
    /// The welcome or the update notice was answered; it is not owed again.
    NoticeDone,
    /// Esc, the cross, or a click outside.
    Close,
}

/// A row the picker offers.
#[derive(Debug, Clone, PartialEq)]
pub struct RowChoice {
    /// "claude", "claude.opus".
    pub id: &'static str,
    /// Provider config id.
    pub provider: &'static str,
    /// "Claude" or "Claude · Opus".
    pub name: &'static str,
    /// "Today's row", "Weekly, a limit of its own".
    pub note: &'static str,
    /// The provider reports this row's metric right now. A row without it
    /// can still be ticked; it appears when the metric does.
    pub has_data: bool,
}

/// A model the picker offers to rename.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelChoice {
    /// Provider config id.
    pub provider: &'static str,
    /// The id as the tool stores it.
    pub model_id: String,
    /// The automatic name, shown as the field's placeholder.
    pub automatic: String,
    /// The user's name, if any.
    pub renamed: Option<String>,
}

/// Everything the picker shows. A snapshot: the app sends a new one after
/// every change it applies.
#[derive(Debug, Clone)]
pub struct PickerInput {
    pub mode: PickerMode,
    /// Every row there is, providers in config order, each provider's own
    /// row first.
    pub rows: Vec<RowChoice>,
    /// The rows ticked for the widget and for the tooltip (resolved: the
    /// default is spelled out).
    pub widget_rows: Vec<String>,
    pub tooltip_rows: Vec<String>,
    pub model: UiModel,
    pub reads: Reads,
    /// Providers switched on in the config (the Providers menu).
    pub enabled: Vec<&'static str>,
    /// Providers whose tool folder exists on this PC (`consent::found_tools`).
    pub found: Vec<&'static str>,
    /// Models that can be renamed (known from the tools' settings).
    pub models: Vec<ModelChoice>,
}

// ─── Events ──────────────────────────────────────────────────────

/// A key the picker acts on. Typed characters arrive as `PickerEvent::Text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKey {
    Tab,
    /// Shift+Tab.
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Space,
    Enter,
    Escape,
    Home,
    End,
    Backspace,
    Delete,
}

/// Input to the picker. Positions are in the window's logical pixels
/// (physical pixels divided by the scale factor), from its top-left corner.
#[derive(Debug, Clone, PartialEq)]
pub enum PickerEvent {
    PointerMove { x: f32, y: f32 },
    /// The left button went down.
    PointerDown { x: f32, y: f32 },
    /// The left button came up.
    PointerUp { x: f32, y: f32 },
    PointerLeave,
    /// A wheel turn in logical pixels; positive scrolls toward the top, as a
    /// wheel turned away from the user does.
    Wheel { dy: f32 },
    Key(PickerKey),
    /// Typed text: one character, or a whole IME composition.
    Text(String),
    /// Ctrl+V: the clipboard's text, read by the caller.
    Paste(String),
    /// The window lost the keyboard focus: a click outside it.
    FocusLost,
    /// The window was asked to close (Alt+F4).
    CloseRequested,
}

// ─── Scene: what the renderer draws ──────────────────────────────

/// What a shape is painted with. `PickerTheme` (ui/theme.rs) gives each one
/// its light and dark colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// The box.
    Surface,
    /// Hairlines: the box, buttons, fields.
    Border,
    Text,
    /// Notes, hints, column heads.
    Muted,
    Accent,
    /// Text and marks on the accent.
    OnAccent,
    /// The translucent wash of a hovered row or a field.
    Hover,
    Separator,
}

/// One thing to draw, in logical pixels.
#[derive(Debug, Clone)]
pub enum Shape {
    /// A filled rounded rectangle. `alpha` scales the ink's own alpha.
    Fill {
        rect: Rect,
        radius: f32,
        ink: Ink,
        alpha: f32,
    },
    /// A rounded outline `width` wide, inside `rect`.
    Stroke {
        rect: Rect,
        radius: f32,
        width: f32,
        ink: Ink,
        alpha: f32,
    },
    /// One line of text; `y` is the top of its line box.
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        ink: Ink,
        bold: bool,
        italic: bool,
        alpha: f32,
    },
    /// A check box filling `rect`.
    Check { rect: Rect, on: bool, alpha: f32 },
    /// A radio button filling `rect`.
    Radio { rect: Rect, on: bool, alpha: f32 },
    /// The close cross, centred in `rect`.
    Cross { rect: Rect, ink: Ink },
    /// The soft shadow of a box.
    Shadow { rect: Rect, radius: f32 },
}

impl Shape {
    fn moved(&self, dx: f32, dy: f32) -> Shape {
        let m = |r: &Rect| Rect {
            x: r.x + dx,
            y: r.y + dy,
            ..*r
        };
        let mut s = self.clone();
        match &mut s {
            Shape::Fill { rect, .. }
            | Shape::Stroke { rect, .. }
            | Shape::Check { rect, .. }
            | Shape::Radio { rect, .. }
            | Shape::Cross { rect, .. }
            | Shape::Shadow { rect, .. } => *rect = m(rect),
            Shape::Text { x, y, .. } => {
                *x += dx;
                *y += dy;
            }
        }
        s
    }
}

/// The picker as shapes, in logical pixels. The window draws it at its scale
/// factor: chrome first, then the list clipped to `viewport`, then the
/// overlay (scroll bar, the read question, the outline).
#[derive(Debug, Clone)]
pub struct PickerScene {
    /// The whole window, shadow margin included.
    pub width: f32,
    pub height: f32,
    /// Window coordinates.
    pub chrome: Vec<Shape>,
    /// The scrolling list's visible part, in window coordinates.
    pub viewport: Rect,
    /// The list, in viewport coordinates (already scrolled); drawn clipped.
    pub body: Vec<Shape>,
    /// Window coordinates.
    pub overlay: Vec<Shape>,
}

impl PickerScene {
    /// Every text drawn, in order (tests and diagnostics).
    pub fn texts(&self) -> Vec<&str> {
        self.chrome
            .iter()
            .chain(&self.body)
            .chain(&self.overlay)
            .filter_map(|s| match s {
                Shape::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
}

// ─── Layout constants (logical pixels) ───────────────────────────

/// The box's width. The mock's `.rpk` is 320 to 390 wide.
const BOX_W: f32 = 380.0;
/// Room around the box for its shadow. A press there is a click outside.
pub const MARGIN: f32 = 12.0;
/// The tallest the box grows; beyond it the list scrolls.
const MAX_BOX_H: f32 = 560.0;
/// The smallest maximum a short work area can impose.
const MIN_BOX_H: f32 = 200.0;
const HEADER_H: f32 = 42.0;
/// The notice's button row.
const FOOTER_H: f32 = 50.0;
const PAD_X: f32 = 14.0;
/// The Widget and Tooltip columns.
const COL_W: f32 = 54.0;
const COL_TOOLTIP: f32 = BOX_W - PAD_X - COL_W / 2.0;
const COL_WIDGET: f32 = COL_TOOLTIP - COL_W;
/// Where a row's label must end, before the Widget column.
const LABEL_RIGHT: f32 = COL_WIDGET - COL_W / 2.0 - 6.0;
/// An extra row's label, and the notes under a group.
const SUB_X: f32 = 28.0;
const ROW_H: f32 = 36.0;
const COLS_H: f32 = 20.0;
const TICK: f32 = 20.0;
const TICK_RING: f32 = 34.0;
const RADIO: f32 = 18.0;
const RADIO_H: f32 = 26.0;
const FIELD_H: f32 = 32.0;
const BUTTON_H: f32 = 32.0;
const BUTTON_MIN_W: f32 = 88.0;
const SIZE_TITLE: f32 = 13.0;
const SIZE_TEXT: f32 = 12.0;
const SIZE_NOTE: f32 = 11.0;
const SIZE_BADGE: f32 = 10.5;
/// The question box's inset from the picker box.
const ASK_INSET: f32 = 8.0;
const ASK_PAD: f32 = 14.0;

/// The line height for a text size: 16 for 12 px, as the mock's 1.3 rounds.
fn lh(size: f32) -> f32 {
    (size * 4.0 / 3.0).round()
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

fn contains(r: &Rect, x: f32, y: f32) -> bool {
    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
}

fn bottom(r: &Rect) -> f32 {
    r.y + r.h
}

/// The current config id ("gemini" became "antigravity").
fn canonical(id: &str) -> &str {
    if id == "gemini" {
        "antigravity"
    } else {
        id
    }
}

// ─── UI text ─────────────────────────────────────────────────────

const TITLE_ROWS: &str = "Rows and models";
const TITLE_WELCOME: &str = "Welcome to AI Limits";
const TITLE_NOTICE: &str = "What AI Limits reads";
const FOUND: &str = "found on this PC";
const ONE_LIMIT: &str = "One limit for every model";
const OFF_IN_MENU: &str = "Off in the Providers menu";
const NOT_REPORTED: &str = "Not reported right now; shows once it is";
const READ_NOTE: &str = "Read from each tool’s own settings file; asked before the first read.";
const MODEL_SHOWN: &str = "Model shown";
const SOURCE_SET: &str = "As set in the tool";
const SOURCE_LAST: &str = "Last used";
const NAMES: &str = "Model names";
const NAMES_OFF: &str = "Once Model is on, each model can be renamed here.";
const NAMES_ON: &str =
    "Made from the id the tool stores. Type to rename; clear the field to get the automatic name back.";
/// Ordinary text for the "room left" estimate: what typing would add.
const ROOM_SAMPLE: &str = "Sonnet 4.6 Opus Max GPT 5.5 Haiku Grok Fast ";

/// The parts of the model label, as the picker lists them.
const PARTS: [(ModelPart, &str, &str); 3] = [
    (ModelPart::Model, "Model", "Opus · GPT · Grok · Auto"),
    (ModelPart::Version, "Version", "5.5 · 6.1 Sol · 4.7"),
    (ModelPart::Effort, "Effort", "low · medium · high · xhigh · max"),
];

const SURFACES: [(Surface, f32); 2] = [
    (Surface::Widget, COL_WIDGET),
    (Surface::Tooltip, COL_TOOLTIP),
];

/// What the hint under a name field says: the room left for ordinary text.
fn room_text(chars_left: usize, empty: bool) -> String {
    if empty {
        format!("Up to about {chars_left} characters, so it fits every widget size.")
    } else if chars_left > 0 {
        let s = if chars_left == 1 { "" } else { "s" };
        format!("Fits every size. Room for about {chars_left} more character{s}.")
    } else {
        "Full: it just fits the smallest widget size.".to_string()
    }
}

// ─── Text layout ─────────────────────────────────────────────────

/// A piece of text in one style, for wrapping.
#[derive(Debug, Clone)]
struct Span {
    text: String,
    size: f32,
    ink: Ink,
    bold: bool,
}

fn span(text: impl Into<String>, size: f32, ink: Ink, bold: bool) -> Span {
    Span {
        text: text.into(),
        size,
        ink,
        bold,
    }
}

/// A wrapped line's piece: text in one style at `x` from the line's start.
#[derive(Debug, Clone)]
struct Run {
    x: f32,
    text: String,
    size: f32,
    ink: Ink,
    bold: bool,
    /// The span it came from.
    style: usize,
}

// ─── Layout elements ─────────────────────────────────────────────

/// Something the user can act on.
#[derive(Debug, Clone, PartialEq)]
enum Target {
    Close,
    Row { row: String, surface: Surface },
    Part { part: ModelPart, surface: Surface },
    Source(ModelSource),
    /// A name field, by model id.
    Field(String),
    Allow,
    Cancel,
    NoticeOk,
    NoticeReview,
}

/// Keyboard grid position: (row, column).
type Nav = (u32, u32);

/// A laid-out element. Interactive ones become shapes once hover, focus and
/// the edit in progress are known.
#[derive(Debug, Clone)]
enum El {
    Shape(Shape),
    /// A row's hover wash.
    RowBg(Rect),
    /// A check box centred in its cell.
    Tick {
        target: Target,
        cell: Rect,
        on: bool,
        enabled: bool,
        nav: Nav,
    },
    Radio {
        target: Target,
        rect: Rect,
        label: String,
        on: bool,
        enabled: bool,
        nav: Nav,
    },
    /// A name field, `model` indexing `PickerInput::models`.
    Field {
        target: Target,
        rect: Rect,
        model: usize,
        nav: Nav,
    },
    Button {
        target: Target,
        rect: Rect,
        label: String,
        primary: bool,
        nav: Nav,
    },
    Close {
        rect: Rect,
        nav: Nav,
    },
}

impl El {
    fn moved(&self, dx: f32, dy: f32) -> El {
        let m = |r: &Rect| Rect {
            x: r.x + dx,
            y: r.y + dy,
            ..*r
        };
        let mut e = self.clone();
        match &mut e {
            El::Shape(s) => *s = s.moved(dx, dy),
            El::RowBg(r) => *r = m(r),
            El::Tick { cell, .. } => *cell = m(cell),
            El::Radio { rect, .. }
            | El::Field { rect, .. }
            | El::Button { rect, .. }
            | El::Close { rect, .. } => *rect = m(rect),
        }
        e
    }

    /// The target, its hit area and keyboard position, when it can be used.
    fn active(&self) -> Option<(&Target, Rect, Nav)> {
        match self {
            El::Tick {
                target,
                cell,
                enabled: true,
                nav,
                ..
            } => Some((target, *cell, *nav)),
            El::Radio {
                target,
                rect,
                enabled: true,
                nav,
                ..
            }
            | El::Field {
                target, rect, nav, ..
            }
            | El::Button {
                target, rect, nav, ..
            } => Some((target, *rect, *nav)),
            El::Close { rect, nav } => Some((&Target::Close, *rect, *nav)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Chrome,
    Body,
    Overlay,
}

/// The content of the scrolling list as it is laid out, top to bottom.
struct Flow {
    y: f32,
    els: Vec<El>,
    nav: u32,
    /// The column heads: content y, height, shapes in content coordinates.
    sticky: Option<(f32, f32, Vec<Shape>)>,
}

impl Flow {
    fn new() -> Self {
        Self {
            y: 0.0,
            els: Vec::new(),
            // Row 0 is the header's close button.
            nav: 1,
            sticky: None,
        }
    }

    fn shape(&mut self, s: Shape) {
        self.els.push(El::Shape(s));
    }

    /// Wrapped lines at (x, y), `line` apart; returns the height used.
    fn lines(&mut self, lines: &[Vec<Run>], x: f32, y: f32, line: f32) -> f32 {
        for (i, l) in lines.iter().enumerate() {
            for r in l {
                self.shape(run_shape(r, x, y + i as f32 * line, line));
            }
        }
        lines.len() as f32 * line
    }
}

/// A run's text shape on a line `line` tall: a smaller run sits on the same
/// baseline as the line's larger text.
fn run_shape(r: &Run, x: f32, y: f32, line: f32) -> Shape {
    let drop = ((line - lh(r.size)) * 0.75).max(0.0).round();
    Shape::Text {
        x: x + r.x,
        y: y + drop,
        text: r.text.clone(),
        size: r.size,
        ink: r.ink,
        bold: r.bold,
        italic: false,
        alpha: 1.0,
    }
}

fn text_shape(x: f32, y: f32, text: &str, size: f32, ink: Ink, bold: bool) -> Shape {
    Shape::Text {
        x,
        y,
        text: text.to_string(),
        size,
        ink,
        bold,
        italic: false,
        alpha: 1.0,
    }
}

/// Everything laid out for one state, in window coordinates.
struct Frame {
    width: f32,
    height: f32,
    boxr: Rect,
    viewport: Rect,
    content_h: f32,
    /// The scroll position, held to the content.
    scroll: f32,
    els: Vec<(Layer, El)>,
    /// The pinned column heads: what covers the list's top, and its shapes.
    sticky: Option<(Rect, Vec<Shape>)>,
}

impl Frame {
    fn max_scroll(&self) -> f32 {
        (self.content_h - self.viewport.h).max(0.0)
    }

    /// The list's visible part below the pinned column heads.
    fn list_top(&self) -> f32 {
        self.sticky
            .as_ref()
            .map_or(self.viewport.y, |(r, _)| bottom(r))
    }
}

// ─── State machine ───────────────────────────────────────────────

/// What the picker shows, from the mode and what has been answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Rows { welcome: bool },
    Notice,
}

/// A read question open inside the picker.
#[derive(Debug, Clone)]
struct Ask {
    kind: ReadKind,
    /// The tick's own actions, sent after `Allow`.
    then: Vec<PickerAction>,
    /// The tick that asked; it gets the focus back.
    from: Target,
}

/// A name field being edited.
#[derive(Debug, Clone)]
struct Edit {
    model_id: String,
    text: String,
    /// Byte index into `text`, on a char boundary.
    caret: usize,
    /// The last edit was stopped or cut at the width limit.
    full: bool,
    /// The name last sent (or the input's, before the first send).
    sent: Option<String>,
}

/// `measure(text, size)`: the width of text in the app's font.
type Measure = Box<dyn Fn(&str, f32) -> f32>;

/// The picker's state. See the module comment.
pub struct Picker {
    input: PickerInput,
    measure: Measure,
    /// `measure` is a full text layout; the same strings come back often.
    widths: RefCell<HashMap<(String, u32), f32>>,
    max_box_h: f32,
    scroll: f32,
    pointer: Option<(f32, f32)>,
    pressed: Option<Target>,
    focus: Option<Target>,
    /// Show the focus ring: the keyboard moved the focus last.
    focus_visible: bool,
    edit: Option<Edit>,
    ask: Option<Ask>,
    /// The update notice was answered with Review: the normal picker shows.
    reviewed: bool,
    /// `NoticeDone` was sent.
    notice_done: bool,
    dirty: bool,
}

/// What a point is over.
#[derive(Debug, Clone, PartialEq)]
enum Hit {
    /// Outside the box: the shadow margin.
    Outside,
    Nothing,
    Target(Target),
}

impl Picker {
    /// A picker showing `input`. `measure(text, size)` is the width of text
    /// in the app's font, `Renderer::text_width`: it lays the picker out and
    /// holds a new model name to `rows::name_fits`.
    pub fn new(input: PickerInput, measure: impl Fn(&str, f32) -> f32 + 'static) -> Self {
        let mut p = Self {
            input,
            measure: Box::new(measure),
            widths: RefCell::new(HashMap::new()),
            max_box_h: MAX_BOX_H,
            scroll: 0.0,
            pointer: None,
            pressed: None,
            focus: None,
            focus_visible: false,
            edit: None,
            ask: None,
            reviewed: false,
            notice_done: false,
            dirty: true,
        };
        // Enter answers the notice with its default button.
        if p.view() == View::Notice {
            p.focus = Some(Target::NoticeOk);
        }
        p
    }

    /// A new snapshot, after the app applied an action. Focus, hover, scroll,
    /// an open question and an edit in progress stay.
    pub fn set_input(&mut self, input: PickerInput) {
        self.input = input;
        if let Some(e) = &self.edit {
            if !self.input.models.iter().any(|m| m.model_id == e.model_id) {
                self.edit = None;
            }
        }
        self.drop_lost_focus();
        self.settle();
        self.dirty = true;
    }

    /// The tallest the box may be, in logical pixels: the work area's height
    /// less the margins, at most 560.
    pub fn set_max_height(&mut self, h: f32) {
        self.max_box_h = h.clamp(MIN_BOX_H, MAX_BOX_H);
        self.settle();
        self.dirty = true;
    }

    /// The window's size in logical pixels, shadow margin included.
    pub fn window_size(&self) -> (f32, f32) {
        let f = self.frame();
        (f.width, f.height)
    }

    /// Whether anything visible changed since the last call.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Whether a name field is being edited (a paste has somewhere to go).
    pub fn editing(&self) -> bool {
        self.edit.is_some() && matches!(self.focus, Some(Target::Field(_)))
    }

    /// Whether the pointer is over a name field (the window shows the text
    /// cursor there).
    pub fn wants_text_cursor(&self) -> bool {
        let Some((x, y)) = self.pointer else {
            return false;
        };
        matches!(self.hit(&self.frame(), x, y), Hit::Target(Target::Field(_)))
    }

    /// Feed one event; returns what the app should do, in order.
    pub fn handle(&mut self, event: PickerEvent) -> Vec<PickerAction> {
        let out = match event {
            PickerEvent::PointerMove { x, y } => {
                let before = self.hover_state();
                self.pointer = Some((x, y));
                if self.hover_state() != before {
                    self.dirty = true;
                }
                Vec::new()
            }
            PickerEvent::PointerLeave => {
                if self.pointer.take().is_some() {
                    self.dirty = true;
                }
                Vec::new()
            }
            PickerEvent::PointerDown { x, y } => self.pointer_down(x, y),
            PickerEvent::PointerUp { x, y } => self.pointer_up(x, y),
            PickerEvent::Wheel { dy } => {
                self.scroll -= dy;
                self.settle();
                self.dirty = true;
                Vec::new()
            }
            PickerEvent::Key(k) => self.key(k),
            PickerEvent::Text(t) => self.type_text(&t),
            PickerEvent::Paste(t) => self.paste(&t),
            PickerEvent::FocusLost | PickerEvent::CloseRequested => self.close(),
        };
        self.settle();
        out
    }

    /// The picker as shapes, for `Renderer::draw_picker`.
    pub fn scene(&self) -> PickerScene {
        let f = self.frame();
        let (hover, hover_row) = match self.pointer {
            Some((x, y)) => (self.hit(&f, x, y), self.hover_row(&f, x, y)),
            None => (Hit::Nothing, None),
        };
        let hovered = match &hover {
            Hit::Target(t) => Some(t),
            _ => None,
        };
        let mut chrome = Vec::new();
        let mut body = Vec::new();
        let mut overlay = Vec::new();
        let (vx, vy) = (f.viewport.x, f.viewport.y);
        for (i, (layer, el)) in f.els.iter().enumerate() {
            let mut shapes = Vec::new();
            self.el_shapes(el, hovered, hover_row == Some(i), &mut shapes);
            match layer {
                Layer::Chrome => chrome.extend(shapes),
                Layer::Overlay => overlay.extend(shapes),
                Layer::Body => {
                    // Off-screen rows are not drawn.
                    let visible = el_rect(el)
                        .is_none_or(|r| bottom(&r) > vy && r.y < bottom(&f.viewport));
                    if visible {
                        body.extend(shapes.iter().map(|s| s.moved(-vx, -vy)));
                    }
                }
            }
        }
        if let Some((r, shapes)) = &f.sticky {
            body.push(
                Shape::Fill {
                    rect: *r,
                    radius: 0.0,
                    ink: Ink::Surface,
                    alpha: 1.0,
                }
                .moved(-vx, -vy),
            );
            body.extend(shapes.iter().map(|s| s.moved(-vx, -vy)));
        }
        PickerScene {
            width: f.width,
            height: f.height,
            chrome,
            viewport: f.viewport,
            body,
            overlay,
        }
    }

    // ─── Reading the input ───────────────────────────────────────

    fn view(&self) -> View {
        match self.input.mode {
            PickerMode::UpdateNotice if !self.reviewed => View::Notice,
            PickerMode::Welcome if self.input.reads.sign_in.is_empty() => {
                View::Rows { welcome: true }
            }
            _ => View::Rows { welcome: false },
        }
    }

    fn allowed(&self, provider: &str) -> bool {
        let p = canonical(provider);
        self.input
            .reads
            .sign_in
            .iter()
            .any(|s| canonical(s) == p)
    }

    fn enabled(&self, provider: &str) -> bool {
        let p = canonical(provider);
        self.input.enabled.iter().any(|e| canonical(e) == p)
    }

    fn found(&self, provider: &str) -> bool {
        let p = canonical(provider);
        self.input.found.iter().any(|e| canonical(e) == p)
    }

    /// A row shows as ticked only when it is chosen AND its provider may be
    /// read: what is not read cannot show, and a fresh install starts with
    /// nothing ticked.
    fn row_on(&self, surface: Surface, row: &RowChoice) -> bool {
        let list = match surface {
            Surface::Widget => &self.input.widget_rows,
            Surface::Tooltip => &self.input.tooltip_rows,
        };
        list.iter().any(|r| r == row.id) && self.allowed(row.provider)
    }

    /// A part shows as ticked only with the settings files allowed; the
    /// version only with the model.
    fn part_on(&self, surface: Surface, part: ModelPart) -> bool {
        let parts = self.input.model.parts(surface);
        self.input.reads.tool_settings
            && parts.contains(&part)
            && (part != ModelPart::Version || parts.contains(&ModelPart::Model))
    }

    fn part_enabled(&self, surface: Surface, part: ModelPart) -> bool {
        part != ModelPart::Version || self.part_on(surface, ModelPart::Model)
    }

    fn any_part_on(&self) -> bool {
        [Surface::Widget, Surface::Tooltip]
            .iter()
            .any(|&s| PARTS.iter().any(|(p, _, _)| self.part_on(s, *p)))
    }

    fn model_on(&self) -> bool {
        self.part_on(Surface::Widget, ModelPart::Model)
            || self.part_on(Surface::Tooltip, ModelPart::Model)
    }

    fn source(&self) -> ModelSource {
        if self.input.model.source == ModelSource::LastUsed && self.input.reads.last_session {
            ModelSource::LastUsed
        } else {
            ModelSource::Configured
        }
    }

    /// The models offered for renaming: those of providers that may be read.
    fn renamable(&self) -> Vec<usize> {
        if !(self.model_on() && self.input.reads.tool_settings) {
            return Vec::new();
        }
        (0..self.input.models.len())
            .filter(|&i| self.allowed(self.input.models[i].provider))
            .collect()
    }

    /// A provider's name as the picker shows it: its own row's name.
    fn provider_name(&self, provider: &str) -> String {
        let p = canonical(provider);
        self.input
            .rows
            .iter()
            .find(|r| canonical(r.provider) == p && !r.id.contains('.'))
            .map(|r| r.name.to_string())
            .or_else(|| crate::consent::read_info(p).map(|i| i.tool.to_string()))
            .unwrap_or_else(|| p.to_string())
    }

    /// What a name field shows: the edit in progress, or the input's name.
    fn field_text(&self, model: &ModelChoice) -> String {
        match &self.edit {
            Some(e) if e.model_id == model.model_id => e.text.clone(),
            _ => model.renamed.clone().unwrap_or_default(),
        }
    }

    // ─── Measuring ───────────────────────────────────────────────

    fn w(&self, text: &str, size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let key = (text.to_string(), size.to_bits());
        if let Some(w) = self.widths.borrow().get(&key) {
            return *w;
        }
        let w = (self.measure)(text, size);
        self.widths.borrow_mut().insert(key, w);
        w
    }

    /// The pen advance after `text`: where the next character would start
    /// (a trailing space counts, unlike the ink width).
    fn adv(&self, text: &str, size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        (self.w(&format!("{text}|"), size) - self.w("|", size)).max(0.0)
    }

    /// The faux bold the renderer draws is about a pixel wider.
    fn adv_styled(&self, text: &str, size: f32, bold: bool) -> f32 {
        self.adv(text, size) + if bold { 1.0 } else { 0.0 }
    }

    /// Whether a model name fits every place the widget draws one.
    fn name_fits(&self, name: &str) -> bool {
        crate::rows::name_fits(name, |t, s| self.w(t, s))
    }

    /// How many more characters of ordinary text still fit after `text`.
    fn chars_left(&self, text: &str) -> usize {
        let sample: Vec<char> = ROOM_SAMPLE.chars().collect();
        let mut k = 0;
        while k < sample.len() {
            let more: String = sample[..=k].iter().collect();
            if !self.name_fits(&format!("{text}{more}")) {
                break;
            }
            k += 1;
        }
        let fit: String = sample[..k].iter().collect();
        fit.trim_end().chars().count()
    }

    /// Wrap styled text to `width`: words break at spaces, a word too long
    /// for a line (a path) after a separator or, failing that, anywhere.
    fn wrap(&self, spans: &[Span], width: f32) -> Vec<Vec<Run>> {
        struct Tok {
            text: String,
            style: usize,
            space: bool,
        }
        let mut toks: Vec<Tok> = Vec::new();
        let mut space = false;
        for (si, sp) in spans.iter().enumerate() {
            for (i, part) in sp.text.split(' ').enumerate() {
                if i > 0 {
                    space = true;
                }
                if part.is_empty() {
                    continue;
                }
                let first_space = space && !toks.is_empty();
                space = false;
                let s = &spans[si];
                for (k, piece) in self
                    .break_word(part, s.size, s.bold, width)
                    .into_iter()
                    .enumerate()
                {
                    toks.push(Tok {
                        text: piece,
                        style: si,
                        space: k == 0 && first_space,
                    });
                }
            }
        }
        // Greedy fill, then merge neighbours of one style into runs.
        let mut lines: Vec<Vec<(f32, &Tok)>> = vec![Vec::new()];
        let mut x = 0.0;
        for t in &toks {
            let s = &spans[t.style];
            let gap = if t.space && x > 0.0 {
                self.adv(" ", s.size)
            } else {
                0.0
            };
            let w = self.adv_styled(&t.text, s.size, s.bold);
            if x > 0.0 && x + gap + w > width {
                lines.push(Vec::new());
                x = 0.0;
                lines.last_mut().unwrap().push((0.0, t));
                x += w;
            } else {
                lines.last_mut().unwrap().push((x + gap, t));
                x += gap + w;
            }
        }
        lines
            .into_iter()
            .filter(|l| !l.is_empty())
            .map(|l| {
                // Neighbours from one span share a run, drawn in one go.
                let mut runs: Vec<Run> = Vec::new();
                for (tx, t) in l {
                    let s = &spans[t.style];
                    match runs.last_mut() {
                        Some(r) if r.style == t.style => {
                            if t.space {
                                r.text.push(' ');
                            }
                            r.text.push_str(&t.text);
                        }
                        _ => runs.push(Run {
                            x: tx,
                            text: t.text.clone(),
                            size: s.size,
                            ink: s.ink,
                            bold: s.bold,
                            style: t.style,
                        }),
                    }
                }
                runs
            })
            .collect()
    }

    /// Split a word that is wider than `width` into pieces that fit.
    fn break_word(&self, word: &str, size: f32, bold: bool, width: f32) -> Vec<String> {
        if self.adv_styled(word, size, bold) <= width {
            return vec![word.to_string()];
        }
        let mut out = Vec::new();
        let mut rest = word;
        while !rest.is_empty() {
            if self.adv_styled(rest, size, bold) <= width {
                out.push(rest.to_string());
                break;
            }
            let mut end = 0;
            let mut soft = 0;
            for (i, c) in rest.char_indices() {
                let next = i + c.len_utf8();
                if self.adv_styled(&rest[..next], size, bold) > width {
                    break;
                }
                end = next;
                if matches!(c, '\\' | '/' | '.' | '_' | '-' | ',') {
                    soft = next;
                }
            }
            let cut = if soft > 0 { soft } else { end.max(first_char_len(rest)) };
            out.push(rest[..cut].to_string());
            rest = &rest[cut..];
        }
        out
    }

    // ─── Layout ──────────────────────────────────────────────────

    /// Lay out the whole window for the current state.
    fn frame(&self) -> Frame {
        let view = self.view();
        let mut flow = Flow::new();
        match view {
            View::Rows { welcome } => self.flow_rows(&mut flow, welcome),
            View::Notice => self.flow_notice(&mut flow),
        }
        let content_h = flow.y.ceil();
        let footer_h = if view == View::Notice { FOOTER_H } else { 0.0 };
        let ask = self.ask.as_ref().map(|a| self.ask_layout(a));

        let natural = HEADER_H + content_h + footer_h;
        let mut box_h = natural.min(self.max_box_h);
        if let Some((qh, _)) = &ask {
            box_h = box_h.max((HEADER_H + qh + ASK_INSET * 2.0).min(self.max_box_h));
        }
        let box_h = box_h.ceil();
        let boxr = rect(MARGIN, MARGIN, BOX_W, box_h);
        let viewport = rect(
            boxr.x + 1.0,
            boxr.y + HEADER_H,
            BOX_W - 2.0,
            box_h - HEADER_H - footer_h - 1.0,
        );
        let max_scroll = (content_h - viewport.h).max(0.0);
        let scroll = self.scroll.clamp(0.0, max_scroll).round();

        let mut els: Vec<(Layer, El)> = Vec::new();
        // The box and its header.
        els.push((
            Layer::Chrome,
            El::Shape(Shape::Shadow {
                rect: boxr,
                radius: 8.0,
            }),
        ));
        els.push((
            Layer::Chrome,
            El::Shape(Shape::Fill {
                rect: boxr,
                radius: 8.0,
                ink: Ink::Surface,
                alpha: 1.0,
            }),
        ));
        let title = match view {
            View::Rows { welcome: true } => TITLE_WELCOME,
            View::Rows { welcome: false } => TITLE_ROWS,
            View::Notice => TITLE_NOTICE,
        };
        els.push((
            Layer::Chrome,
            El::Shape(text_shape(
                boxr.x + PAD_X,
                boxr.y + 8.0 + ((30.0 - lh(SIZE_TITLE)) / 2.0).round(),
                title,
                SIZE_TITLE,
                Ink::Text,
                true,
            )),
        ));
        els.push((
            Layer::Chrome,
            El::Close {
                rect: rect(boxr.x + BOX_W - 6.0 - 34.0, boxr.y + 8.0, 34.0, 30.0),
                nav: (0, 9),
            },
        ));

        // The list, moved from content to window coordinates.
        let (dx, dy) = (boxr.x, viewport.y - scroll);
        for el in &flow.els {
            els.push((Layer::Body, el.moved(dx, dy)));
        }
        let sticky = flow.sticky.as_ref().and_then(|(cy, ch, shapes)| {
            // Pinned only once the list has scrolled past it; before that it
            // is drawn in place, as part of the list.
            let at = (cy + dy).max(viewport.y);
            let r = rect(viewport.x, at, viewport.w, *ch);
            (bottom(&r) > viewport.y && r.y < bottom(&viewport)).then(|| {
                let shift = at - (cy + dy);
                (
                    r,
                    shapes
                        .iter()
                        .map(|s| s.moved(dx, dy + shift))
                        .collect::<Vec<_>>(),
                )
            })
        });

        // The notice's buttons.
        if view == View::Notice {
            let y = bottom(&boxr) - 14.0 - BUTTON_H;
            let (review, review_w) = ("Review…", self.button_w("Review…"));
            let ok_w = self.button_w("OK");
            let rx = boxr.x + BOX_W - PAD_X - review_w;
            let ox = rx - 8.0 - ok_w;
            els.push((
                Layer::Overlay,
                El::Button {
                    target: Target::NoticeOk,
                    rect: rect(ox, y, ok_w, BUTTON_H),
                    label: "OK".into(),
                    primary: true,
                    nav: (u32::MAX - 1, 0),
                },
            ));
            els.push((
                Layer::Overlay,
                El::Button {
                    target: Target::NoticeReview,
                    rect: rect(rx, y, review_w, BUTTON_H),
                    label: review.into(),
                    primary: false,
                    nav: (u32::MAX - 1, 1),
                },
            ));
        }

        // The scroll bar: a thin thumb, only when the list scrolls.
        if max_scroll > 0.0 {
            let th = (viewport.h * viewport.h / content_h).max(24.0);
            let ty = viewport.y + 2.0 + (viewport.h - 4.0 - th) * scroll / max_scroll;
            els.push((
                Layer::Overlay,
                El::Shape(Shape::Fill {
                    rect: rect(boxr.x + BOX_W - 6.0, ty, 3.0, th),
                    radius: 1.5,
                    ink: Ink::Muted,
                    alpha: 0.45,
                }),
            ));
        }

        // The read question, over the list's bottom.
        if let Some((qh, q)) = ask {
            let qh = qh.min(box_h - HEADER_H - ASK_INSET);
            let qr = rect(
                boxr.x + ASK_INSET,
                bottom(&boxr) - ASK_INSET - qh,
                BOX_W - ASK_INSET * 2.0,
                qh,
            );
            for s in [
                Shape::Shadow {
                    rect: qr,
                    radius: 8.0,
                },
                Shape::Fill {
                    rect: qr,
                    radius: 8.0,
                    ink: Ink::Surface,
                    alpha: 1.0,
                },
                Shape::Stroke {
                    rect: qr,
                    radius: 8.0,
                    width: 1.0,
                    ink: Ink::Border,
                    alpha: 1.0,
                },
            ] {
                els.push((Layer::Overlay, El::Shape(s)));
            }
            for el in q {
                els.push((Layer::Overlay, el.moved(qr.x, qr.y)));
            }
        }

        // The outline, last, over whatever reaches the edge.
        els.push((
            Layer::Overlay,
            El::Shape(Shape::Stroke {
                rect: boxr,
                radius: 8.0,
                width: 1.0,
                ink: Ink::Border,
                alpha: 1.0,
            }),
        ));

        Frame {
            width: BOX_W + MARGIN * 2.0,
            height: box_h + MARGIN * 2.0,
            boxr,
            viewport,
            content_h,
            scroll,
            els,
            sticky,
        }
    }

    fn button_w(&self, label: &str) -> f32 {
        (self.adv(label, SIZE_TEXT) + 24.0).max(BUTTON_MIN_W).round()
    }

    /// The list: rows, model parts, the source, the names.
    fn flow_rows(&self, f: &mut Flow, welcome: bool) {
        let input = &self.input;
        if welcome {
            f.y += 4.0;
            let mut spans = vec![
                span("Tick the tools you use.", SIZE_TEXT, Ink::Text, true),
                span(
                    " Each one asks once before AI Limits reads its sign-in file; nothing is read until then.",
                    SIZE_TEXT,
                    Ink::Text,
                    false,
                ),
            ];
            if !input.found.is_empty() {
                spans.push(span(
                    " Tools found on this PC are marked.",
                    SIZE_TEXT,
                    Ink::Text,
                    false,
                ));
            }
            let lines = self.wrap(&spans, BOX_W - 16.0 - 20.0);
            let h = lines.len() as f32 * lh(SIZE_TEXT) + 16.0;
            f.shape(Shape::Fill {
                rect: rect(8.0, f.y, BOX_W - 16.0, h),
                radius: 6.0,
                ink: Ink::Hover,
                alpha: 1.0,
            });
            f.lines(&lines, 18.0, f.y + 8.0, lh(SIZE_TEXT));
            f.y += h + 8.0;
        }

        // The column heads, pinned to the top while the list scrolls.
        let heads: Vec<Shape> = SURFACES
            .iter()
            .zip(["Widget", "Tooltip"])
            .map(|((_, cx), label)| {
                let w = self.adv(label, SIZE_NOTE);
                text_shape(
                    (cx - w / 2.0).round(),
                    f.y + ((COLS_H - lh(SIZE_NOTE)) / 2.0).round(),
                    label,
                    SIZE_NOTE,
                    Ink::Muted,
                    false,
                )
            })
            .collect();
        for s in &heads {
            f.shape(s.clone());
        }
        f.sticky = Some((f.y, COLS_H, heads.into_iter().map(|s| s.moved(0.0, -f.y)).collect()));
        f.y += COLS_H;

        // Rows, grouped by provider.
        let mut groups: Vec<Vec<&RowChoice>> = Vec::new();
        for row in &input.rows {
            match groups.last_mut() {
                Some(g) if canonical(g[0].provider) == canonical(row.provider) => g.push(row),
                _ => groups.push(vec![row]),
            }
        }
        for (gi, g) in groups.iter().enumerate() {
            if gi > 0 {
                f.y += 6.0;
            }
            for row in g {
                self.flow_row(f, row, welcome);
            }
            // A provider with one limit for everything has no extra rows.
            if g.len() == 1 && !g[0].id.contains('.') {
                f.shape(text_shape(SUB_X, f.y, ONE_LIMIT, SIZE_NOTE, Ink::Muted, false));
                f.y += lh(SIZE_NOTE) + 4.0;
            }
        }

        // Model, version, effort.
        self.flow_sep(f);
        for (part, label, note) in PARTS {
            let notes = self.wrap(
                &[span(note, SIZE_NOTE, Ink::Muted, false)],
                LABEL_RIGHT - PAD_X,
            );
            let block = lh(SIZE_TEXT) + notes.len() as f32 * lh(SIZE_NOTE);
            let h = (block + 8.0).max(ROW_H);
            let y = f.y;
            f.els.push(El::RowBg(rect(6.0, y, BOX_W - 12.0, h)));
            let top = y + ((h - block) / 2.0).round();
            f.shape(text_shape(PAD_X, top, label, SIZE_TEXT, Ink::Text, false));
            f.lines(&notes, PAD_X, top + lh(SIZE_TEXT), lh(SIZE_NOTE));
            for (col, (surface, cx)) in SURFACES.iter().enumerate() {
                f.els.push(El::Tick {
                    target: Target::Part {
                        part,
                        surface: *surface,
                    },
                    cell: rect(cx - COL_W / 2.0, y, COL_W, h),
                    on: self.part_on(*surface, part),
                    enabled: self.part_enabled(*surface, part),
                    nav: (f.nav, col as u32),
                });
            }
            f.nav += 1;
            f.y += h;
        }
        let read_note = self.wrap(
            &[span(READ_NOTE, SIZE_NOTE, Ink::Muted, false)],
            BOX_W - SUB_X - PAD_X,
        );
        f.y += f.lines(&read_note, SUB_X, f.y, lh(SIZE_NOTE)) + 4.0;

        // Where the model comes from.
        let enabled = self.any_part_on();
        f.y += 6.0;
        f.shape(Shape::Text {
            x: PAD_X,
            y: f.y,
            text: MODEL_SHOWN.to_string(),
            size: SIZE_TEXT,
            ink: Ink::Text,
            bold: false,
            italic: false,
            alpha: if enabled { 1.0 } else { 0.4 },
        });
        f.y += lh(SIZE_TEXT) + 4.0;
        let mut x = PAD_X;
        for (col, (source, label)) in [
            (ModelSource::Configured, SOURCE_SET),
            (ModelSource::LastUsed, SOURCE_LAST),
        ]
        .into_iter()
        .enumerate()
        {
            let w = RADIO + 8.0 + self.adv(label, SIZE_TEXT);
            f.els.push(El::Radio {
                target: Target::Source(source),
                rect: rect(x, f.y, w.ceil(), RADIO_H),
                label: label.to_string(),
                on: self.source() == source,
                enabled,
                nav: (f.nav, col as u32),
            });
            x += w.ceil() + 16.0;
        }
        f.nav += 1;
        f.y += RADIO_H;

        // Model names.
        self.flow_sep(f);
        f.y += 8.0;
        f.shape(text_shape(PAD_X, f.y, NAMES, SIZE_TEXT, Ink::Text, true));
        f.y += lh(SIZE_TEXT);
        let models = self.renamable();
        let note = if models.is_empty() { NAMES_OFF } else { NAMES_ON };
        let note = self.wrap(
            &[span(note, SIZE_NOTE, Ink::Muted, false)],
            BOX_W - SUB_X - PAD_X,
        );
        f.y += f.lines(&note, SUB_X, f.y, lh(SIZE_NOTE)) + 4.0;
        for mi in models {
            self.flow_name(f, mi);
        }
        f.y += 10.0;
    }

    /// One row: its name (and note) with a Widget and a Tooltip tick.
    fn flow_row(&self, f: &mut Flow, row: &RowChoice, welcome: bool) {
        let own = !row.id.contains('.');
        let x = if own { PAD_X } else { SUB_X };
        let width = LABEL_RIGHT - x;
        let name = if own {
            row.name
        } else {
            row.name.split_once(" · ").map_or(row.name, |(_, b)| b)
        };
        let mut notes: Vec<String> = Vec::new();
        if !row.note.is_empty() {
            notes.push(row.note.to_string());
        }
        if own && !self.enabled(row.provider) {
            notes.push(OFF_IN_MENU.to_string());
        }
        if !own && !row.has_data && self.allowed(row.provider) {
            notes.push(NOT_REPORTED.to_string());
        }
        let mut note_lines = Vec::new();
        for n in &notes {
            note_lines.extend(self.wrap(&[span(n.as_str(), SIZE_NOTE, Ink::Muted, false)], width));
        }
        let block = lh(SIZE_TEXT) + note_lines.len() as f32 * lh(SIZE_NOTE);
        let h = (block + 8.0).max(ROW_H);
        let y = f.y;
        f.els.push(El::RowBg(rect(6.0, y, BOX_W - 12.0, h)));
        let top = y + ((h - block) / 2.0).round();
        if !own {
            f.shape(text_shape(PAD_X + 1.0, top, "+", SIZE_TEXT, Ink::Muted, false));
        }
        f.shape(text_shape(x, top, name, SIZE_TEXT, Ink::Text, own));
        if own && welcome && self.found(row.provider) {
            let bx = (x + self.adv_styled(name, SIZE_TEXT, true) + 8.0).round();
            let bw = (self.adv_styled(FOUND, SIZE_BADGE, true) + 14.0).round();
            f.shape(Shape::Fill {
                rect: rect(bx, top, bw, lh(SIZE_TEXT)),
                radius: 8.0,
                ink: Ink::Hover,
                alpha: 1.0,
            });
            f.shape(text_shape(
                bx + 7.0,
                top + ((lh(SIZE_TEXT) - lh(SIZE_BADGE)) / 2.0).round(),
                FOUND,
                SIZE_BADGE,
                Ink::Accent,
                true,
            ));
        }
        f.lines(&note_lines, x, top + lh(SIZE_TEXT), lh(SIZE_NOTE));
        for (col, (surface, cx)) in SURFACES.iter().enumerate() {
            f.els.push(El::Tick {
                target: Target::Row {
                    row: row.id.to_string(),
                    surface: *surface,
                },
                cell: rect(cx - COL_W / 2.0, y, COL_W, h),
                on: self.row_on(*surface, row),
                enabled: true,
                nav: (f.nav, col as u32),
            });
        }
        f.nav += 1;
        f.y += h;
    }

    fn flow_sep(&self, f: &mut Flow) {
        f.y += 8.0;
        f.shape(Shape::Fill {
            rect: rect(10.0, f.y, BOX_W - 20.0, 1.0),
            radius: 0.0,
            ink: Ink::Separator,
            alpha: 1.0,
        });
        f.y += 9.0;
    }

    /// One model's name field: who and which id, the field, the room left.
    fn flow_name(&self, f: &mut Flow, mi: usize) {
        let m = &self.input.models[mi];
        let width = BOX_W - PAD_X * 2.0;
        f.y += 6.0;
        let label = self.wrap(
            &[
                span(self.provider_name(m.provider), SIZE_TEXT, Ink::Text, true),
                span(format!("  {}", m.model_id), SIZE_NOTE, Ink::Muted, false),
            ],
            width,
        );
        f.y += f.lines(&label, PAD_X, f.y, lh(SIZE_TEXT)) + 4.0;
        f.els.push(El::Field {
            target: Target::Field(m.model_id.clone()),
            rect: rect(PAD_X, f.y, width, FIELD_H),
            model: mi,
            nav: (f.nav, 0),
        });
        f.nav += 1;
        f.y += FIELD_H + 4.0;
        let text = self.field_text(m);
        let full = matches!(&self.edit, Some(e) if e.model_id == m.model_id && e.full);
        let hint = room_text(self.chars_left(text.trim()), text.trim().is_empty());
        let (ink, bold) = if full {
            (Ink::Accent, true)
        } else {
            (Ink::Muted, false)
        };
        let hint = self.wrap(&[span(hint, SIZE_NOTE, ink, bold)], width);
        f.y += f.lines(&hint, PAD_X, f.y, lh(SIZE_NOTE)) + 6.0;
    }

    /// The update notice: every file read and every host the tokens go to.
    fn flow_notice(&self, f: &mut Flow) {
        let width = BOX_W - PAD_X * 2.0;
        f.y += 2.0;
        self.flow_para(
            f,
            &[span(
                "Since 0.7.1, AI Limits asks before it reads any file. The tools you already use keep working, nothing changes for them:",
                SIZE_TEXT,
                Ink::Text,
                false,
            )],
            width,
        );
        for p in self.allowed_in_order() {
            let Some(info) = crate::consent::read_info(&p) else {
                continue;
            };
            let mut spans = vec![span(format!("{}:", info.tool), SIZE_TEXT, Ink::Text, true)];
            spans.push(span(
                format!(" {}", info.sign_in.join("; ")),
                SIZE_NOTE,
                Ink::Text,
                false,
            ));
            spans.push(span(", sent only to ", SIZE_TEXT, Ink::Text, false));
            spans.push(span(info.hosts.join(" and "), SIZE_NOTE, Ink::Text, false));
            if self.input.reads.tool_settings {
                if let Some(s) = info.settings {
                    spans.push(span(". The model: ", SIZE_TEXT, Ink::Text, false));
                    spans.push(span(s, SIZE_NOTE, Ink::Text, false));
                }
            }
            if self.input.reads.last_session {
                if let Some(s) = info.session {
                    spans.push(span(". Last used: ", SIZE_TEXT, Ink::Text, false));
                    spans.push(span(s, SIZE_NOTE, Ink::Text, false));
                }
            }
            self.flow_item(f, &spans, width);
        }
        f.y += 4.0;
        self.flow_para(
            f,
            &[span(
                "A new tool or option asks once before its first read. Untick a tool in Rows and models to stop reading its file.",
                SIZE_TEXT,
                Ink::Text,
                false,
            )],
            width,
        );
    }

    fn flow_para(&self, f: &mut Flow, spans: &[Span], width: f32) {
        let lines = self.wrap(spans, width);
        f.y += f.lines(&lines, PAD_X, f.y, lh(SIZE_TEXT)) + 8.0;
    }

    fn flow_item(&self, f: &mut Flow, spans: &[Span], width: f32) {
        let lines = self.wrap(spans, width - 18.0);
        f.shape(text_shape(PAD_X + 4.0, f.y, "•", SIZE_TEXT, Ink::Text, false));
        f.y += f.lines(&lines, PAD_X + 18.0, f.y, lh(SIZE_TEXT)) + 4.0;
    }

    /// The providers that may be read, in the order the picker lists them.
    fn allowed_in_order(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let listed = self
            .input
            .rows
            .iter()
            .map(|r| r.provider)
            .chain(self.input.reads.sign_in.iter().map(String::as_str));
        for p in listed {
            let p = canonical(p).to_string();
            if self.allowed(&p) && !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }

    /// The question's title, its lead-in, the files, and what follows.
    fn ask_text(&self, kind: &ReadKind) -> (String, String, Vec<String>, String) {
        // The files a question covers: those of the providers that may be
        // read, or every tool's when none is yet.
        let files = |pick: fn(&crate::consent::ReadInfo) -> Option<&'static str>| {
            let allowed: Vec<String> = self
                .allowed_in_order()
                .iter()
                .filter_map(|p| crate::consent::read_info(p).and_then(pick))
                .map(String::from)
                .collect();
            if allowed.is_empty() {
                crate::consent::READ_INFO
                    .iter()
                    .filter_map(pick)
                    .map(String::from)
                    .collect()
            } else {
                allowed
            }
        };
        match kind {
            ReadKind::SignIn(p) => {
                let info = crate::consent::read_info(p);
                let tool = info.map_or(p.as_str(), |i| i.tool);
                let mut list: Vec<String> = info
                    .map(|i| i.sign_in.iter().map(|s| s.to_string()).collect())
                    .unwrap_or_default();
                // Reads already allowed for every tool apply to this one too.
                if let Some(i) = info {
                    if self.input.reads.tool_settings {
                        list.extend(i.settings.map(String::from));
                    }
                    if self.input.reads.last_session && self.source() == ModelSource::LastUsed {
                        list.extend(i.session.map(String::from));
                    }
                }
                let hosts = info.map_or(String::new(), |i| i.hosts.join(" and "));
                (
                    format!("Read {tool}’s sign-in?"),
                    format!(
                        "To show your {} limits, AI Limits reads:",
                        self.provider_name(p)
                    ),
                    list,
                    format!(
                        "The token stays on this PC and goes only to {hosts}, to ask for your usage. It is never changed or refreshed. Untick to stop; details in Privacy."
                    ),
                )
            }
            ReadKind::ToolSettings => (
                "Read the tools’ settings files?".to_string(),
                "To show the model, its version and effort, AI Limits reads these values from each tool’s own settings file:".to_string(),
                files(|i| i.settings),
                "Only those values are kept, in memory. Nothing is sent anywhere. Untick to stop; details in Privacy.".to_string(),
            ),
            ReadKind::LastSession => (
                "Read the end of your newest session?".to_string(),
                "For Last used, AI Limits reads the last 64 KB of the newest session file:".to_string(),
                files(|i| i.session),
                "Only the model name is kept. Your conversations are not stored or sent anywhere. Switch back to As set in the tool to stop; details in Privacy.".to_string(),
            ),
        }
    }

    /// The question box's height and contents, in its own coordinates.
    fn ask_layout(&self, ask: &Ask) -> (f32, Vec<El>) {
        let (title, lead, files, outro) = self.ask_text(&ask.kind);
        let inner = BOX_W - ASK_INSET * 2.0 - ASK_PAD * 2.0;
        let mut f = Flow::new();
        f.y = 12.0;
        let t = self.wrap(&[span(title, SIZE_TITLE, Ink::Text, true)], inner);
        f.y += f.lines(&t, ASK_PAD, f.y, lh(SIZE_TITLE)) + 6.0;
        let l = self.wrap(&[span(lead, SIZE_TEXT, Ink::Text, false)], inner);
        f.y += f.lines(&l, ASK_PAD, f.y, lh(SIZE_TEXT)) + 4.0;
        for file in files {
            let lines = self.wrap(&[span(file, SIZE_NOTE, Ink::Text, false)], inner - 18.0);
            f.shape(text_shape(ASK_PAD + 4.0, f.y, "•", SIZE_NOTE, Ink::Text, false));
            f.y += f.lines(&lines, ASK_PAD + 18.0, f.y, lh(SIZE_NOTE)) + 2.0;
        }
        f.y += 4.0;
        let o = self.wrap(&[span(outro, SIZE_TEXT, Ink::Text, false)], inner);
        f.y += f.lines(&o, ASK_PAD, f.y, lh(SIZE_TEXT)) + 10.0;
        let (aw, cw) = (self.button_w("Allow"), self.button_w("Cancel"));
        let right = BOX_W - ASK_INSET * 2.0 - ASK_PAD;
        f.els.push(El::Button {
            target: Target::Allow,
            rect: rect(right - cw - 8.0 - aw, f.y, aw, BUTTON_H),
            label: "Allow".into(),
            primary: true,
            nav: (0, 0),
        });
        f.els.push(El::Button {
            target: Target::Cancel,
            rect: rect(right - cw, f.y, cw, BUTTON_H),
            label: "Cancel".into(),
            primary: false,
            nav: (0, 1),
        });
        f.y += BUTTON_H + 12.0;
        (f.y.ceil(), f.els)
    }

    // ─── Shapes with state ───────────────────────────────────────

    fn focused(&self, t: &Target) -> bool {
        self.focus.as_ref() == Some(t)
    }

    fn ring(&self, t: &Target) -> bool {
        self.focus_visible && self.focused(t)
    }

    fn el_shapes(&self, el: &El, hovered: Option<&Target>, row_hover: bool, out: &mut Vec<Shape>) {
        let pressed = |t: &Target| self.pressed.as_ref() == Some(t) && hovered == Some(t);
        match el {
            El::Shape(s) => out.push(s.clone()),
            El::RowBg(r) => {
                if row_hover {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Hover,
                        alpha: 1.0,
                    });
                }
            }
            El::Tick {
                target,
                cell,
                on,
                enabled,
                ..
            } => {
                let (cx, cy) = (cell.x + cell.w / 2.0, (cell.y + cell.h / 2.0).round());
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: rect(cx - TICK_RING / 2.0, cy - TICK_RING / 2.0, TICK_RING, TICK_RING),
                        radius: 6.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: 1.0,
                    });
                }
                out.push(Shape::Check {
                    rect: rect(cx - TICK / 2.0, cy - TICK / 2.0, TICK, TICK),
                    on: *on,
                    alpha: if *enabled {
                        if pressed(target) {
                            0.8
                        } else {
                            1.0
                        }
                    } else {
                        0.35
                    },
                });
            }
            El::Radio {
                target,
                rect: r,
                label,
                on,
                enabled,
                ..
            } => {
                let alpha = if *enabled { 1.0 } else { 0.4 };
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: rect(r.x - 4.0, r.y, r.w + 8.0, r.h),
                        radius: 6.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: 1.0,
                    });
                }
                out.push(Shape::Radio {
                    rect: rect(r.x, r.y + ((r.h - RADIO) / 2.0).round(), RADIO, RADIO),
                    on: *on,
                    alpha: if pressed(target) { alpha * 0.8 } else { alpha },
                });
                out.push(Shape::Text {
                    x: r.x + RADIO + 8.0,
                    y: r.y + ((r.h - lh(SIZE_TEXT)) / 2.0).round(),
                    text: label.clone(),
                    size: SIZE_TEXT,
                    ink: Ink::Text,
                    bold: false,
                    italic: false,
                    alpha,
                });
            }
            El::Field {
                target,
                rect: r,
                model,
                ..
            } => {
                let m = &self.input.models[*model];
                let focused = self.focused(target);
                out.push(Shape::Fill {
                    rect: *r,
                    radius: 4.0,
                    ink: Ink::Hover,
                    alpha: if hovered == Some(target) && !focused {
                        1.6
                    } else {
                        1.0
                    },
                });
                out.push(Shape::Stroke {
                    rect: *r,
                    radius: 4.0,
                    width: 1.0,
                    ink: Ink::Border,
                    alpha: 1.0,
                });
                let (line_h, ink) = if focused {
                    (2.0, Ink::Accent)
                } else {
                    (1.0, Ink::Muted)
                };
                out.push(Shape::Fill {
                    rect: rect(r.x + 2.0, bottom(r) - line_h, r.w - 4.0, line_h),
                    radius: 0.0,
                    ink,
                    alpha: 1.0,
                });
                let tx = r.x + 8.0;
                let ty = r.y + ((r.h - lh(SIZE_TEXT)) / 2.0).round();
                let text = self.field_text(m);
                if text.is_empty() {
                    let auto = if m.automatic.trim().is_empty() {
                        "(automatic)".to_string()
                    } else {
                        format!("{}  (automatic)", m.automatic)
                    };
                    out.push(Shape::Text {
                        x: tx,
                        y: ty,
                        text: auto,
                        size: SIZE_TEXT,
                        ink: Ink::Muted,
                        bold: false,
                        italic: true,
                        alpha: 0.7,
                    });
                } else {
                    out.push(text_shape(tx, ty, &text, SIZE_TEXT, Ink::Text, false));
                }
                if focused {
                    let caret = self
                        .edit
                        .as_ref()
                        .filter(|e| e.model_id == m.model_id)
                        .map_or(text.len(), |e| e.caret.min(text.len()));
                    let cx = (tx + self.adv(&text[..caret], SIZE_TEXT)).round();
                    out.push(Shape::Fill {
                        rect: rect(cx, r.y + 7.0, 1.0, r.h - 14.0),
                        radius: 0.0,
                        ink: Ink::Text,
                        alpha: 1.0,
                    });
                }
            }
            El::Button {
                target,
                rect: r,
                label,
                primary,
                ..
            } => {
                let hover = hovered == Some(target);
                let down = pressed(target);
                if *primary {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Accent,
                        alpha: if down {
                            0.8
                        } else if hover {
                            0.9
                        } else {
                            1.0
                        },
                    });
                } else {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Hover,
                        alpha: if down {
                            3.0
                        } else if hover {
                            2.0
                        } else {
                            1.0
                        },
                    });
                    out.push(Shape::Stroke {
                        rect: *r,
                        radius: 4.0,
                        width: 1.0,
                        ink: Ink::Border,
                        alpha: 1.0,
                    });
                }
                if self.ring(target) {
                    out.push(Shape::Stroke {
                        rect: rect(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0),
                        radius: 6.0,
                        width: 2.0,
                        ink: Ink::Text,
                        alpha: 1.0,
                    });
                }
                let w = self.adv(label, SIZE_TEXT);
                out.push(text_shape(
                    (r.x + (r.w - w) / 2.0).round(),
                    r.y + ((r.h - lh(SIZE_TEXT)) / 2.0).round(),
                    label,
                    SIZE_TEXT,
                    if *primary { Ink::OnAccent } else { Ink::Text },
                    false,
                ));
            }
            El::Close { rect: r, .. } => {
                let t = Target::Close;
                if hovered == Some(&t) || self.ring(&t) {
                    out.push(Shape::Fill {
                        rect: *r,
                        radius: 4.0,
                        ink: Ink::Hover,
                        alpha: if pressed(&t) { 2.0 } else { 1.0 },
                    });
                }
                out.push(Shape::Cross {
                    rect: *r,
                    ink: Ink::Text,
                });
            }
        }
    }

    // ─── Hit testing ─────────────────────────────────────────────

    fn hit(&self, f: &Frame, x: f32, y: f32) -> Hit {
        if !contains(&f.boxr, x, y) {
            return Hit::Outside;
        }
        let in_list = contains(&f.viewport, x, y) && y >= f.list_top();
        for (layer, el) in f.els.iter().rev() {
            let Some((t, r, _)) = el.active() else {
                continue;
            };
            // While a question is open only it and the cross answer.
            if self.ask.is_some() && !matches!(t, Target::Allow | Target::Cancel | Target::Close) {
                continue;
            }
            if *layer == Layer::Body && !in_list {
                continue;
            }
            if contains(&r, x, y) {
                return Hit::Target(t.clone());
            }
        }
        Hit::Nothing
    }

    /// The index in `f.els` of the row wash under the point.
    fn hover_row(&self, f: &Frame, x: f32, y: f32) -> Option<usize> {
        if self.ask.is_some() || !contains(&f.viewport, x, y) || y < f.list_top() {
            return None;
        }
        f.els.iter().position(|(layer, el)| {
            *layer == Layer::Body && matches!(el, El::RowBg(r) if contains(r, x, y))
        })
    }

    fn hover_state(&self) -> (Hit, Option<usize>) {
        match self.pointer {
            Some((x, y)) => {
                let f = self.frame();
                (self.hit(&f, x, y), self.hover_row(&f, x, y))
            }
            None => (Hit::Nothing, None),
        }
    }

    // ─── Pointer ─────────────────────────────────────────────────

    fn pointer_down(&mut self, x: f32, y: f32) -> Vec<PickerAction> {
        self.pointer = Some((x, y));
        self.focus_visible = false;
        self.dirty = true;
        let f = self.frame();
        match self.hit(&f, x, y) {
            Hit::Outside => self.close(),
            Hit::Nothing => {
                self.pressed = None;
                // A press on nothing ends an edit, as a click beside a field does.
                if matches!(self.focus, Some(Target::Field(_))) {
                    self.set_focus(None);
                }
                Vec::new()
            }
            Hit::Target(Target::Field(id)) => {
                self.pressed = None;
                self.set_focus(Some(Target::Field(id.clone())));
                self.place_caret(&f, &id, x);
                Vec::new()
            }
            Hit::Target(t) => {
                self.pressed = Some(t);
                Vec::new()
            }
        }
    }

    fn pointer_up(&mut self, x: f32, y: f32) -> Vec<PickerAction> {
        self.pointer = Some((x, y));
        let Some(t) = self.pressed.take() else {
            return Vec::new();
        };
        self.dirty = true;
        let f = self.frame();
        if self.hit(&f, x, y) != Hit::Target(t.clone()) {
            return Vec::new();
        }
        if !matches!(t, Target::Close | Target::NoticeOk) {
            self.set_focus(Some(t.clone()));
        }
        self.activate(&t)
    }

    /// Put the caret at the character boundary nearest to `x`.
    fn place_caret(&mut self, f: &Frame, id: &str, x: f32) {
        let Some(r) = f.els.iter().find_map(|(_, el)| match el {
            El::Field {
                target: Target::Field(fid),
                rect,
                ..
            } if fid == id => Some(*rect),
            _ => None,
        }) else {
            return;
        };
        let Some(text) = self.edit.as_ref().map(|e| e.text.clone()) else {
            return;
        };
        let local = x - (r.x + 8.0);
        let mut best = (f32::MAX, text.len());
        let bounds = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()));
        for i in bounds {
            let d = (self.adv(&text[..i], SIZE_TEXT) - local).abs();
            if d < best.0 {
                best = (d, i);
            }
        }
        if let Some(e) = self.edit.as_mut() {
            e.caret = best.1;
        }
    }

    // ─── Acting ──────────────────────────────────────────────────

    fn activate(&mut self, t: &Target) -> Vec<PickerAction> {
        self.dirty = true;
        match t {
            Target::Close => self.close(),
            Target::Row { row, surface } => {
                let Some(choice) = self.input.rows.iter().find(|r| r.id == row).cloned() else {
                    return Vec::new();
                };
                let on = self.row_on(*surface, &choice);
                let act = PickerAction::SetRow {
                    surface: *surface,
                    row: row.clone(),
                    on: !on,
                };
                if !on && !self.allowed(choice.provider) {
                    let kind = ReadKind::SignIn(canonical(choice.provider).to_string());
                    self.open_question(kind, vec![act], t.clone());
                    return Vec::new();
                }
                vec![act]
            }
            Target::Part { part, surface } => {
                if !self.part_enabled(*surface, *part) {
                    return Vec::new();
                }
                if self.part_on(*surface, *part) {
                    let mut out = vec![PickerAction::SetPart {
                        surface: *surface,
                        part: *part,
                        on: false,
                    }];
                    // A version without its model means nothing.
                    if *part == ModelPart::Model
                        && self
                            .input
                            .model
                            .parts(*surface)
                            .contains(&ModelPart::Version)
                    {
                        out.push(PickerAction::SetPart {
                            surface: *surface,
                            part: ModelPart::Version,
                            on: false,
                        });
                    }
                    return out;
                }
                let act = PickerAction::SetPart {
                    surface: *surface,
                    part: *part,
                    on: true,
                };
                if !self.input.reads.tool_settings {
                    self.open_question(ReadKind::ToolSettings, vec![act], t.clone());
                    return Vec::new();
                }
                vec![act]
            }
            Target::Source(source) => {
                if !self.any_part_on() || self.source() == *source {
                    return Vec::new();
                }
                let act = PickerAction::SetSource(*source);
                if *source == ModelSource::LastUsed && !self.input.reads.last_session {
                    self.open_question(ReadKind::LastSession, vec![act], t.clone());
                    return Vec::new();
                }
                vec![act]
            }
            Target::Field(_) => {
                self.set_focus(Some(t.clone()));
                Vec::new()
            }
            Target::Allow => {
                let Some(ask) = self.ask.take() else {
                    return Vec::new();
                };
                self.set_focus(Some(ask.from));
                let mut out = vec![PickerAction::Allow(ask.kind)];
                out.extend(ask.then);
                out
            }
            Target::Cancel => {
                if let Some(ask) = self.ask.take() {
                    self.set_focus(Some(ask.from));
                }
                Vec::new()
            }
            Target::NoticeOk => {
                self.notice_done = true;
                vec![PickerAction::NoticeDone, PickerAction::Close]
            }
            Target::NoticeReview => {
                self.notice_done = true;
                self.reviewed = true;
                self.scroll = 0.0;
                self.focus = None;
                vec![PickerAction::NoticeDone]
            }
        }
    }

    fn open_question(&mut self, kind: ReadKind, then: Vec<PickerAction>, from: Target) {
        self.ask = Some(Ask { kind, then, from });
        // Enter allows, as the mock's question focuses its Allow button.
        self.set_focus(Some(Target::Allow));
        self.dirty = true;
    }

    /// Esc, the cross, a click outside, Alt+F4: the welcome and the notice
    /// count as answered once seen.
    fn close(&mut self) -> Vec<PickerAction> {
        let mut out = Vec::new();
        if matches!(self.input.mode, PickerMode::Welcome | PickerMode::UpdateNotice)
            && !self.notice_done
        {
            self.notice_done = true;
            out.push(PickerAction::NoticeDone);
        }
        self.ask = None;
        out.push(PickerAction::Close);
        out
    }

    // ─── Focus ───────────────────────────────────────────────────

    /// Move the focus. Leaving a name field ends its edit; entering one
    /// starts it, caret at the end.
    fn set_focus(&mut self, t: Option<Target>) {
        if self.focus == t {
            return;
        }
        self.focus = t;
        self.dirty = true;
        match &self.focus {
            Some(Target::Field(id)) => {
                if self.edit.as_ref().is_none_or(|e| &e.model_id != id) {
                    let current = self
                        .input
                        .models
                        .iter()
                        .find(|m| &m.model_id == id)
                        .and_then(|m| m.renamed.clone())
                        .unwrap_or_default();
                    let sent = Some(current.trim().to_string()).filter(|s| !s.is_empty());
                    self.edit = Some(Edit {
                        model_id: id.clone(),
                        caret: current.len(),
                        text: current,
                        full: false,
                        sent,
                    });
                }
            }
            _ => self.edit = None,
        }
    }

    /// The focusable targets in tab order with their keyboard positions and
    /// rectangles (window coordinates).
    fn focusables(&self, f: &Frame) -> Vec<(Target, Nav, Rect, Layer)> {
        let mut out: Vec<(Target, Nav, Rect, Layer)> = Vec::new();
        for (layer, el) in &f.els {
            if let Some((t, r, nav)) = el.active() {
                if self.ask.is_some() && !matches!(t, Target::Allow | Target::Cancel) {
                    continue;
                }
                out.push((t.clone(), nav, r, *layer));
            }
        }
        out
    }

    /// Forget a focus whose target is gone or disabled.
    fn drop_lost_focus(&mut self) {
        if let Some(t) = &self.focus {
            let f = self.frame();
            if !self.focusables(&f).iter().any(|(ft, ..)| ft == t) {
                self.set_focus(None);
            }
        }
    }

    /// Scroll the focused target into view.
    fn reveal_focus(&mut self) {
        let Some(t) = self.focus.clone() else {
            return;
        };
        let f = self.frame();
        let Some((_, _, r, Layer::Body)) = self.focusables(&f).into_iter().find(|(ft, ..)| *ft == t)
        else {
            return;
        };
        // A field is revealed with its label above it.
        let top = if matches!(t, Target::Field(_)) {
            r.y - lh(SIZE_TEXT) - 6.0
        } else {
            r.y
        };
        let list_top = f.sticky.as_ref().map_or(f.viewport.y + COLS_H, |(s, _)| {
            bottom(s).max(f.viewport.y + COLS_H)
        });
        if top < list_top {
            self.scroll = f.scroll - (list_top - top);
        } else if bottom(&r) > bottom(&f.viewport) {
            self.scroll = f.scroll + (bottom(&r) - bottom(&f.viewport)) + 4.0;
        }
        self.settle();
    }

    /// Hold the scroll to the content after any change.
    fn settle(&mut self) {
        let f = self.frame();
        self.scroll = self.scroll.clamp(0.0, f.max_scroll());
    }

    // ─── Keyboard ────────────────────────────────────────────────

    fn key(&mut self, k: PickerKey) -> Vec<PickerAction> {
        self.dirty = true;
        if k == PickerKey::Escape {
            if self.ask.is_some() {
                return self.activate(&Target::Cancel);
            }
            return self.close();
        }
        // In a name field the editing keys edit.
        if self.edit.is_some() && matches!(self.focus, Some(Target::Field(_))) {
            match k {
                PickerKey::Backspace => return self.erase(false),
                PickerKey::Delete => return self.erase(true),
                PickerKey::Left | PickerKey::Right | PickerKey::Home | PickerKey::End => {
                    if let Some(e) = self.edit.as_mut() {
                        e.caret = match k {
                            PickerKey::Left => prev_boundary(&e.text, e.caret),
                            PickerKey::Right => next_boundary(&e.text, e.caret),
                            PickerKey::Home => 0,
                            _ => e.text.len(),
                        };
                    }
                    return Vec::new();
                }
                PickerKey::Space => return self.type_text(" "),
                PickerKey::Enter => return Vec::new(),
                _ => {}
            }
        }
        let f = self.frame();
        let items = self.focusables(&f);
        if items.is_empty() {
            return Vec::new();
        }
        let at = self
            .focus
            .as_ref()
            .and_then(|t| items.iter().position(|(ft, ..)| ft == t));
        let next = match k {
            PickerKey::Tab => Some(at.map_or(0, |i| (i + 1) % items.len())),
            PickerKey::BackTab => Some(at.map_or(items.len() - 1, |i| {
                (i + items.len() - 1) % items.len()
            })),
            PickerKey::Home => Some(0),
            PickerKey::End => Some(items.len() - 1),
            PickerKey::Up | PickerKey::Down | PickerKey::Left | PickerKey::Right => {
                Some(at.map_or(0, |i| grid_move(&items, i, k)))
            }
            PickerKey::Space | PickerKey::Enter => {
                self.focus_visible = true;
                return match &self.focus {
                    Some(t) if items.iter().any(|(ft, ..)| ft == t) => {
                        let t = t.clone();
                        self.activate(&t)
                    }
                    _ => Vec::new(),
                };
            }
            _ => None,
        };
        if let Some(i) = next {
            self.focus_visible = true;
            self.set_focus(Some(items[i].0.clone()));
            self.reveal_focus();
        }
        Vec::new()
    }

    // ─── Editing a name ──────────────────────────────────────────

    fn type_text(&mut self, s: &str) -> Vec<PickerAction> {
        if !matches!(self.focus, Some(Target::Field(_))) {
            return Vec::new();
        }
        let Some(mut e) = self.edit.take() else {
            return Vec::new();
        };
        self.dirty = true;
        let mut full = false;
        for c in s.chars() {
            let c = if c.is_whitespace() { ' ' } else { c };
            if c.is_control() {
                continue;
            }
            let (before, after) = e.text.split_at(e.caret);
            if c == ' ' {
                // No leading, double or useless trailing space: a space must
                // leave room for something after it.
                if before.is_empty() || before.ends_with(' ') || after.starts_with(' ') {
                    continue;
                }
                if !self.name_fits(&format!("{before} e{after}")) {
                    full = true;
                    break;
                }
            }
            let candidate = format!("{before}{c}{after}");
            if !self.name_fits(&candidate) {
                full = true;
                break;
            }
            e.caret += c.len_utf8();
            e.text = candidate;
        }
        e.full = full;
        self.edit = Some(e);
        self.renamed()
    }

    /// A paste: one line, single spaces, cut to what fits.
    fn paste(&mut self, s: &str) -> Vec<PickerAction> {
        if !matches!(self.focus, Some(Target::Field(_))) {
            return Vec::new();
        }
        let Some(mut e) = self.edit.take() else {
            return Vec::new();
        };
        self.dirty = true;
        let clean = s
            .chars()
            .filter(|c| !c.is_control() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let (before, after) = e.text.split_at(e.caret);
        let (before, after) = (before.to_string(), after.to_string());
        // `clean` has no space at either end.
        let inserted = if before.is_empty() && after.is_empty() {
            crate::rows::fit_name(&clean, |t, sz| self.w(t, sz))
        } else {
            let mut end = 0;
            for (i, c) in clean.char_indices() {
                let next = i + c.len_utf8();
                if !self.name_fits(&format!("{before}{}{after}", &clean[..next])) {
                    break;
                }
                end = next;
            }
            let piece = &clean[..end];
            if after.is_empty() || after.starts_with(' ') {
                piece.trim_end().to_string()
            } else {
                piece.to_string()
            }
        };
        e.full = inserted.chars().count() < clean.chars().count();
        e.text = format!("{before}{inserted}{after}");
        e.caret = before.len() + inserted.len();
        self.edit = Some(e);
        self.renamed()
    }

    fn erase(&mut self, forward: bool) -> Vec<PickerAction> {
        let Some(e) = self.edit.as_mut() else {
            return Vec::new();
        };
        let (from, to) = if forward {
            (e.caret, next_boundary(&e.text, e.caret))
        } else {
            (prev_boundary(&e.text, e.caret), e.caret)
        };
        if from == to {
            return Vec::new();
        }
        e.text.replace_range(from..to, "");
        e.caret = from;
        e.full = false;
        self.renamed()
    }

    /// The rename an edit makes, when the trimmed name changed.
    fn renamed(&mut self) -> Vec<PickerAction> {
        let Some(e) = self.edit.as_mut() else {
            return Vec::new();
        };
        let name = Some(e.text.trim().to_string()).filter(|s| !s.is_empty());
        if name == e.sent {
            return Vec::new();
        }
        e.sent = name.clone();
        vec![PickerAction::Rename {
            model_id: e.model_id.clone(),
            name,
        }]
    }
}

/// The rectangle a laid-out element covers, for culling.
fn el_rect(el: &El) -> Option<Rect> {
    match el {
        El::Shape(Shape::Fill { rect, .. })
        | El::Shape(Shape::Stroke { rect, .. })
        | El::Shape(Shape::Check { rect, .. })
        | El::Shape(Shape::Radio { rect, .. })
        | El::Shape(Shape::Cross { rect, .. })
        | El::Shape(Shape::Shadow { rect, .. })
        | El::RowBg(rect)
        | El::Tick { cell: rect, .. }
        | El::Radio { rect, .. }
        | El::Field { rect, .. }
        | El::Button { rect, .. }
        | El::Close { rect, .. } => Some(*rect),
        El::Shape(Shape::Text { x, y, size, .. }) => Some(rect(*x, *y, 1.0, lh(*size))),
    }
}

/// Arrow keys over the grid of rows and columns: up and down to the nearest
/// column of the next row that has one, left and right within a row.
fn grid_move(items: &[(Target, Nav, Rect, Layer)], at: usize, k: PickerKey) -> usize {
    let (row, col) = items[at].1;
    let pick = |cands: Vec<usize>| -> Option<usize> {
        cands
            .into_iter()
            .min_by_key(|&i| (items[i].1 .1 as i64 - col as i64).unsigned_abs())
    };
    let found = match k {
        PickerKey::Up | PickerKey::Down => {
            let target_row = items
                .iter()
                .map(|(_, n, ..)| n.0)
                .filter(|&r| if k == PickerKey::Up { r < row } else { r > row })
                .reduce(|a, b| if k == PickerKey::Up { a.max(b) } else { a.min(b) });
            target_row.and_then(|tr| {
                pick(
                    (0..items.len())
                        .filter(|&i| items[i].1 .0 == tr)
                        .collect(),
                )
            })
        }
        _ => {
            let same: Vec<usize> = (0..items.len()).filter(|&i| items[i].1 .0 == row).collect();
            let want = |c: u32| {
                if k == PickerKey::Left {
                    c < col
                } else {
                    c > col
                }
            };
            same.into_iter()
                .filter(|&i| want(items[i].1 .1))
                .min_by_key(|&i| (items[i].1 .1 as i64 - col as i64).unsigned_abs())
        }
    };
    found.unwrap_or(at)
}

fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i].char_indices().last().map_or(0, |(j, _)| j)
}

fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

fn first_char_len(s: &str) -> usize {
    s.chars().next().map_or(0, char::len_utf8)
}

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
    let fits = [right.0 + bw <= r, left.0 >= l, below.1 + bh <= b, above.1 >= t];
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

use crate::ui::renderer::Renderer;
use crate::ui::theme::PickerTheme;
use std::num::NonZeroU32;
use std::rc::Rc;
use tao::dpi::{PhysicalPosition, PhysicalSize};
use tao::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use tao::event_loop::EventLoopWindowTarget;
use tao::keyboard::{Key, KeyCode, ModifiersState};
use tao::window::{CursorIcon, Window, WindowBuilder, WindowId};

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
pub struct PickerWindow {
    window: Rc<Window>,
    /// Kept alive for the surface.
    _context: softbuffer::Context<Rc<Window>>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    renderer: Rc<Renderer>,
    picker: Picker,
    pixmap: tiny_skia::Pixmap,
    scale: f64,
    theme: PickerTheme,
    work: Option<(i32, i32, i32, i32)>,
    pos: (i32, i32),
    size: (u32, u32),
    /// The cursor in the window, physical pixels.
    cursor: (f64, f64),
    modifiers: ModifiersState,
    /// A focus loss counts as a click outside only after the focus came.
    had_focus: bool,
    text_cursor: bool,
}

impl PickerWindow {
    /// Open beside `anchor` (the widget's screen rect, or the tray icon or
    /// panel when the widget is hidden), on that monitor, at its scale.
    /// `renderer` is the widget's: its font measures and draws the picker.
    pub fn open<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        renderer: Rc<Renderer>,
        input: PickerInput,
        anchor: ScreenRect,
    ) -> anyhow::Result<Self> {
        use anyhow::Context as _;
        let (cx, cy) = (anchor.x + anchor.w / 2, anchor.y + anchor.h / 2);
        let mut scale = target
            .monitor_from_point(cx as f64, cy as f64)
            .map_or(1.0, |m| m.scale_factor());
        let work = crate::platform::work_area_at(cx, cy);
        let measure = renderer.clone();
        let mut picker = Picker::new(input, move |t, s| measure.text_width(t, s));
        if let Some(w) = work {
            picker.set_max_height(max_box_height(w, scale));
        }
        let mut size = physical(picker.window_size(), scale);
        let mut pos = place_window(anchor, size, scale, work);

        let mut builder = WindowBuilder::new()
            .with_title(TITLE_ROWS)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(true)
            .with_resizable(false)
            .with_visible(false)
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
                picker.set_max_height(max_box_height(w, scale));
            }
            size = physical(picker.window_size(), scale);
            pos = place_window(anchor, size, scale, work);
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

        let mut me = Self {
            window,
            _context: context,
            surface,
            renderer,
            picker,
            pixmap,
            scale,
            theme: PickerTheme::for_system(crate::platform::system_uses_light_theme()),
            work,
            pos,
            size,
            cursor: (0.0, 0.0),
            modifiers: ModifiersState::empty(),
            had_focus: false,
            text_cursor: false,
        };
        me.redraw();
        me.window.set_visible(true);
        Ok(me)
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    /// One of this window's events; returns the actions to apply, in order.
    pub fn handle_window_event(&mut self, event: &WindowEvent<'_>) -> Vec<PickerAction> {
        let s = self.scale as f32;
        let at = |p: (f64, f64)| (p.0 as f32 / s, p.1 as f32 / s);
        let ev = match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                let (x, y) = at(self.cursor);
                Some(PickerEvent::PointerMove { x, y })
            }
            WindowEvent::CursorLeft { .. } => Some(PickerEvent::PointerLeave),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = at(self.cursor);
                Some(match state {
                    ElementState::Pressed => PickerEvent::PointerDown { x, y },
                    _ => PickerEvent::PointerUp { x, y },
                })
            }
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
                None
            }
            WindowEvent::Focused(false) => self.had_focus.then_some(PickerEvent::FocusLost),
            WindowEvent::CloseRequested => Some(PickerEvent::CloseRequested),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // The size follows in the next redraw.
                self.scale = *scale_factor;
                if let Some(w) = self.work {
                    self.picker.set_max_height(max_box_height(w, self.scale));
                }
                self.window.request_redraw();
                None
            }
            WindowEvent::ThemeChanged(_) => {
                self.theme = PickerTheme::for_system(crate::platform::system_uses_light_theme());
                self.window.request_redraw();
                None
            }
            WindowEvent::Resized(_) => {
                self.window.request_redraw();
                None
            }
            _ => None,
        };
        let actions = ev.map(|e| self.picker.handle(e)).unwrap_or_default();
        if self.picker.take_dirty() {
            self.window.request_redraw();
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

    /// The app applied an action: show the new state.
    pub fn set_input(&mut self, input: PickerInput) {
        self.picker.set_input(input);
        self.picker.take_dirty();
        self.window.request_redraw();
    }

    /// Paint and present (on `RedrawRequested` for this window).
    pub fn redraw(&mut self) {
        let want = physical(self.picker.window_size(), self.scale);
        if want != self.size {
            self.resize(want);
        }
        let scene = self.picker.scene();
        self.renderer
            .draw_picker(&mut self.pixmap, &scene, &self.theme, self.scale as f32);
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
            let m = (MARGIN as f64 * self.scale).round() as i32;
            let x = self.pos.0.min(r - want.0 as i32 + m).max(l - m);
            let y = self.pos.1.min(b - want.1 as i32 + m).max(t - m);
            if (x, y) != self.pos {
                self.pos = (x, y);
                self.window
                    .set_outer_position(PhysicalPosition::new(x, y));
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
            return self.picker.editing().then(|| PickerEvent::Paste(clipboard_text()));
        }
        // A Ctrl shortcut types nothing; AltGr (Ctrl+Alt) does.
        if ctrl && !alt {
            return None;
        }
        ev.text.map(|t| PickerEvent::Text(t.to_string()))
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
fn max_box_height(work: (i32, i32, i32, i32), scale: f64) -> f32 {
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
