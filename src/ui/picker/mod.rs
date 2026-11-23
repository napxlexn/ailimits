// ui/picker/mod.rs — the "AI Limits Settings" window: the app's own window,
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
//
// This file holds the types, the text and the state read from the input;
// `Picker`'s work is split by what it does:
// - text.rs: measuring and wrapping text;
// - layout.rs: the window laid out for a state (`Frame`);
// - scene.rs: shapes for the renderer, hit testing, descriptions;
// - input.rs: pointer, keyboard, questions and answers, editing a name;
// - window.rs: `PickerWindow`, placement, the Windows settings followed;
// - tests.rs.

use crate::config::schema::{
    ModelPart, ModelSource, NumbersOn, Reads, Surface, TipPart, TrayStyle, UiModel, UiNumbers,
};
use crate::ui::layout::Rect;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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

/// A read the picker asks about before the first tick that needs it. The
/// welcome's question about the tools found answers with one `SignIn` per
/// tool ticked in it, as ticking each row would.
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
    /// Show or hide the percentage, the reset time or the bar on each row
    /// of the widget, the tooltip or the panel; the last one on there stays
    /// on (`UiNumbers::set`).
    SetTip {
        part: TipPart,
        place: NumbersOn,
        on: bool,
    },
    /// The tray icon's look.
    SetTrayStyle(TrayStyle),
    /// Show or hide a provider ("claude") on the taskbar panel and the tray
    /// icon (the Taskbar column): at most `rows::TASKBAR_MAX` at a time,
    /// which the picker holds to.
    SetTaskbar {
        provider: String,
        on: bool,
    },
    /// Rename a model by the key its name is kept under (`ModelChoice::key`);
    /// None brings the automatic name back. The picker has already held the
    /// name to `crate::rows::name_fits`.
    Rename {
        key: String,
        name: Option<String>,
    },
    /// The user allowed a read in the picker's question. Sent before the tick
    /// that needed it.
    Allow(ReadKind),
    /// Allow all: the sign-in of another detected tool may be read
    /// from now on, so its first tick asks nothing. It is not switched on
    /// and none of its rows is ticked: it runs once a row of it is.
    Consent(String),
    /// Take a provider's sign-in from this source from now on.
    SetSignInSource {
        provider: String,
        source: SourceId,
    },
    /// Paste the chosen source's key or token from the clipboard: checked,
    /// then stored in Credential Manager only if it works.
    PasteSecret(String),
    /// Remove the chosen source's stored key or token.
    RemoveSecret(String),
    /// Sign in to Cursor in the browser; cancel the wait; sign out.
    CursorSignIn,
    CursorCancel,
    CursorSignOut,
    /// Open config.toml (in Notepad), for the fine-tuning that has no place
    /// in the window. The app applies what is saved there.
    OpenSettingsFile,
    /// The welcome or the update notice was answered (a button, Esc, the
    /// cross, Alt+F4 or an answered question); it is not owed again.
    NoticeDone,
    /// Esc, the cross or Alt+F4.
    Close,
}

/// A row the picker offers.
#[derive(Debug, Clone, PartialEq)]
pub struct RowChoice {
    /// What the row counts, shown on hover; empty for none.
    pub hint: &'static str,
    /// "claude", "claude.opus".
    pub id: &'static str,
    /// Provider config id.
    pub provider: &'static str,
    /// "Claude" or "Claude · Opus".
    pub name: &'static str,
    /// The line under the name: a provider's "Today's row" or its own short
    /// status ("token expired") as the widget words it; an extra row's
    /// "Asked for once this row is ticked"; empty for none.
    pub note: std::borrow::Cow<'static, str>,
    /// An extra row's word or two beside its name ("weekly"); empty for a
    /// provider's own row.
    pub short: &'static str,
    /// The provider reports this row's metric right now. A row without it
    /// can still be ticked; it appears when the metric does.
    pub has_data: bool,
    /// The provider answered with data: only then can an extra row be said
    /// to be missing from it (not while it is still loading).
    pub provider_ok: bool,
}

/// One provider's sign-in, as the window shows it under the provider's own
/// row: a line that opens to say where the sign-in comes from, to choose
/// it, and to paste, remove or sign in.
#[derive(Debug, Clone, PartialEq)]
pub struct SignInChoice {
    /// Config id.
    pub provider: &'static str,
    /// The closed line: where the sign-in comes from and how it went
    /// ("Claude Code · works", "Not allowed yet").
    pub summary: String,
    /// The sources to choose from, in order; empty when there is one.
    pub sources: Vec<SignInSource>,
    /// What is read and where its token goes, for the source chosen.
    pub detail: String,
    /// The paste button's label, when the source chosen takes a pasted
    /// secret.
    pub paste: Option<&'static str>,
    /// A pasted secret of the source chosen is stored: Remove can be used.
    pub stored: bool,
    /// What the last action came to ("Checking it...", "Refused, so not
    /// stored"), until the next one.
    pub note: Option<String>,
    /// Cursor's sign-in of the app's own, for Cursor only.
    pub cursor: Option<CursorState>,
}

/// One source a provider's sign-in can come from.
#[derive(Debug, Clone, PartialEq)]
pub struct SignInSource {
    pub id: SourceId,
    pub label: &'static str,
    pub on: bool,
}

/// Where a sign-in comes from: the tool's own, a usage token or a key the
/// user pasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceId {
    Tool,
    UsageToken,
    Key,
}

/// Cursor's sign-in of the app's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorState {
    SignedOut,
    /// The browser was opened; the app waits for the sign-in there.
    Waiting,
    SignedIn,
}

/// A model the picker offers to rename, under one tool's heading.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelChoice {
    /// Provider config id: the tool it is listed under.
    pub provider: &'static str,
    /// The id as the tool stores it.
    pub model_id: String,
    /// The key its name is kept under (`choices::model_key`): the id, or
    /// "copilot:auto" for a tool's own router. A model two tools share has
    /// one key, and its field under each of them.
    pub key: String,
    /// The automatic name, shown as the field's placeholder.
    pub automatic: String,
    /// The user's name, if any.
    pub renamed: Option<String>,
    /// The model this tool uses right now.
    pub current: bool,
    /// The other running tools that offer it under the same key (their
    /// names): renaming it renames it there too.
    pub shared_with: Vec<&'static str>,
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
    /// The providers ticked for the taskbar panel and the tray icon
    /// (`rows::taskbar_ticked`: untouched, the first two that run).
    pub taskbar_rows: Vec<String>,
    pub model: UiModel,
    /// What each widget row shows: the percentage, the reset time, the bar.
    pub widget: UiNumbers,
    /// What each tooltip row shows: the percentage, the reset time, or both.
    pub tip: UiNumbers,
    /// The same for each row of the taskbar panel.
    pub panel: UiNumbers,
    /// The tray icon's look.
    pub tray_style: TrayStyle,
    pub reads: Reads,
    /// Providers switched on in the config.
    pub enabled: Vec<&'static str>,
    /// Providers whose tool folder exists on this PC (`consent::found_tools`).
    pub found: Vec<&'static str>,
    /// Those of `found` found only by a related tool's folder (GitHub CLI
    /// for Copilot, `.gemini` for Antigravity): marked, never ticked for
    /// the user, never started by Allow all.
    pub found_weak: Vec<&'static str>,
    /// Models that can be renamed, each listed under the tool offering it.
    pub models: Vec<ModelChoice>,
    /// Each provider's sign-in, under its own row.
    pub sign_in: Vec<SignInChoice>,
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
    PageUp,
    PageDown,
    Backspace,
    Delete,
}

/// Input to the picker. Positions are in the window's logical pixels
/// (physical pixels divided by the scale factor), from its top-left corner.
#[derive(Debug, Clone, PartialEq)]
pub enum PickerEvent {
    PointerMove {
        x: f32,
        y: f32,
    },
    /// The left button went down.
    PointerDown {
        x: f32,
        y: f32,
    },
    /// The left button came up.
    PointerUp {
        x: f32,
        y: f32,
    },
    PointerLeave,
    /// A wheel turn in logical pixels; positive scrolls toward the top, as a
    /// wheel turned away from the user does.
    Wheel {
        dy: f32,
    },
    Key(PickerKey),
    /// Typed text: one character, or a whole IME composition.
    Text(String),
    /// Ctrl+V: the clipboard's text, read by the caller.
    Paste(String),
    /// The window lost the keyboard focus to another window: a click
    /// outside it, or a shell panel (Win+V, Win+.) while a name is typed.
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
    /// Small marks and words that step back.
    Dim,
    /// A provider's dot: it works.
    Good,
    /// A provider's dot: it runs without fresh numbers.
    Warn,
}

/// How strongly an ink is laid on. `PickerTheme` (ui/theme.rs) gives each
/// level its factor on the ink's own alpha, so no colour is decided here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The ink as it is.
    Full,
    /// A hovered name field's wash.
    FieldHover,
    /// A hovered secondary button; a toggle or the cross held down.
    Hovered,
    /// A secondary button held down.
    Pressed,
    /// The primary button hovered.
    AccentHover,
    /// The primary button held down.
    AccentPressed,
    /// A tick or a radio button held down.
    MarkPressed,
    /// A control that cannot be used now, and its label.
    Disabled,
    /// A tick that cannot be used now.
    DisabledMark,
    /// A name field's automatic name.
    Placeholder,
    /// A scroll bar's thumb.
    Thumb,
}

/// One thing to draw, in logical pixels.
#[derive(Debug, Clone)]
pub enum Shape {
    /// A filled rounded rectangle.
    Fill {
        rect: Rect,
        radius: f32,
        ink: Ink,
        alpha: Level,
    },
    /// A rounded outline `width` wide, inside `rect`.
    Stroke {
        rect: Rect,
        radius: f32,
        width: f32,
        ink: Ink,
        alpha: Level,
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
        alpha: Level,
    },
    /// A check box filling `rect`.
    Check { rect: Rect, on: bool, alpha: Level },
    /// A radio button filling `rect`.
    Radio { rect: Rect, on: bool, alpha: Level },
    /// The close cross, centred in `rect`.
    Cross { rect: Rect, ink: Ink },
    /// The soft shadow of a box.
    Shadow { rect: Rect, radius: f32 },
    /// The tray icon in a look, filling `rect`, in the text's ink.
    TrayIcon {
        rect: Rect,
        style: TrayStyle,
        alpha: Level,
    },
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
            | Shape::TrayIcon { rect, .. }
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
/// overlay (scroll bar, the read question's box), then the question's text
/// clipped to `ask_viewport`, then the top (the question's buttons, the
/// outline, a description).
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
    /// The visible part of an open question's text, in window coordinates.
    pub ask_viewport: Option<Rect>,
    /// The question's text, in `ask_viewport` coordinates (already
    /// scrolled); drawn clipped.
    pub ask_body: Vec<Shape>,
    /// Window coordinates, over everything.
    pub top: Vec<Shape>,
}

impl PickerScene {
    /// Every text drawn, in order (tests and diagnostics).
    pub fn texts(&self) -> Vec<&str> {
        self.chrome
            .iter()
            .chain(&self.body)
            .chain(&self.overlay)
            .chain(&self.ask_body)
            .chain(&self.top)
            .filter_map(|s| match s {
                Shape::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
}

// ─── Layout constants (logical pixels) ───────────────────────────

/// The box's width: the mock's `.rpk` (320 to 390 wide), one tick column
/// (Taskbar) wider, so the labels keep their room.
const BOX_W: f32 = 434.0;
/// Room around the box for its shadow. A press there is a click outside.
pub const MARGIN: f32 = 12.0;
/// The tallest the box grows; beyond it the list scrolls.
const MAX_BOX_H: f32 = 560.0;
/// The smallest maximum a short work area can impose.
const MIN_BOX_H: f32 = 200.0;
const HEADER_H: f32 = 42.0;
/// The tabs under the title: the control and the air under it.
const TABS_H: f32 = 38.0;
/// A tray icon look's tile: its icon, then its label.
const TILE_H: f32 = 67.0;
/// A tile's icon, square: the tray icon's own 32 px drawing.
const TILE_ICON: f32 = 32.0;
/// How long the pointer rests on something before its description shows,
/// as Windows' own tooltips wait; one shown, the next follows sooner.
const HINT_DELAY: std::time::Duration = std::time::Duration::from_millis(600);
const HINT_RESHOW: std::time::Duration = std::time::Duration::from_millis(100);
/// A provider's dot before its name, the room after it, and its top
/// below the top of the name's line.
const DOT: f32 = 7.0;
const DOT_GAP: f32 = 6.0;
const DOT_TOP: f32 = 5.0;
/// A sign-in's chip: its height, the room inside its ends and between
/// what it says.
const CHIP_H: f32 = 22.0;
const CHIP_PAD: f32 = 9.0;
const CHIP_GAP: f32 = 6.0;
/// The chip's arrow: closed, open (arrowheads the UI font has).
const CHIP_CLOSED: &str = "\u{203A}";
const CHIP_OPEN: &str = "\u{02C5}";
/// The notice's button row.
const FOOTER_H: f32 = 50.0;
const PAD_X: f32 = 14.0;
/// The Widget, Tooltip and Taskbar columns.
const COL_W: f32 = 54.0;
const COL_TASKBAR: f32 = BOX_W - PAD_X - COL_W / 2.0;
const COL_TOOLTIP: f32 = COL_TASKBAR - COL_W;
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
/// Inside the question box: above its text, between the text and the
/// buttons, below the buttons.
const ASK_TOP: f32 = 12.0;
const ASK_GAP: f32 = 10.0;
const ASK_BOTTOM: f32 = 12.0;
/// A tool's check box in the welcome's question.
const PICK_BOX: f32 = 18.0;
/// The widest a model id is shown above its field: a longer one (a cloud
/// ARN) ends in an ellipsis.
const ID_CHARS: usize = 80;
/// A scroll bar's grip: wider than the thumb it moves.
const GRIP_W: f32 = 12.0;

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

const TITLE_ROWS: &str = "AI Limits Settings";
const TAB_ROWS: &str = "Providers";
const TAB_SHOWS: &str = "Display";
const SECT_NUMBERS: &str = "VALUES";
const SECT_MODEL: &str = "MODEL";
const SECT_TRAY: &str = "TRAY ICON STYLE";
const PANEL: &str = "Taskbar";
const DASH: &str = "\u{2013}";
const TRAY_NOTE: &str = "The icon shows with Indicator set to Tray icon in the menu, and stands in for the panel while the Start menu covers it.";

/// The tray icon's looks: the tile's label and what it says on hover.
const TRAY_STYLES: [(TrayStyle, &str, &str); 4] = [
    (
        TrayStyle::Rings,
        "Rings",
        "Two rings: the busier provider outside, the other inside.",
    ),
    (
        TrayStyle::Bars,
        "Bars",
        "Two upright bars filling from the bottom, the busier on the left, like a level meter.",
    ),
    (
        TrayStyle::Number,
        "Number",
        "The busier provider's percentage in digits; a limit reached shows !. The other is in the tooltip.",
    ),
    (
        TrayStyle::Sector,
        "Pie",
        "The busier provider's share as a slice of a disc, like a clock face. The other is in the tooltip.",
    ),
];

/// Why the last tick on there cannot be taken off.
fn lock_hint(place: NumbersOn) -> &'static str {
    match place {
        NumbersOn::Widget => "At least one value stays enabled, so the widget is never empty.",
        NumbersOn::Tooltip => "At least one value stays enabled, so the tooltip is never empty.",
        NumbersOn::Panel => {
            "At least one value stays enabled, so the taskbar panel is never empty."
        }
    }
}
const TITLE_WELCOME: &str = "Welcome to AI Limits";
const TITLE_NOTICE: &str = "Data access";
const FOUND: &str = "detected";
const ONE_LIMIT: &str = "Single limit for all models";
const OFF_IN_MENU: &str = "Disabled. Select a row to enable";
const NOT_REPORTED: &str = "Not reported by the provider yet";
const READ_NOTE: &str =
    "Read from each running tool's own settings file; asked before the first read.";
const MODEL_SHOWN: &str = "Model source";
const SOURCE_SET: &str = "Tool settings";
const SOURCE_LAST: &str = "Last session";
const NAMES: &str = "Model aliases";
const SIGN_IN: &str = "Sign-in";
const SIGN_IN_HINT: &str = "Where AI Limits gets this provider's sign-in from, and how it went. Open it to choose, to paste a key or token of your own, or to remove one.";
const REMOVE: &str = "Remove";
const CURSOR_SIGN_IN: &str = "Sign in to Cursor...";
const CURSOR_CANCEL: &str = "Cancel";
const CURSOR_SIGN_OUT: &str = "Sign out";
const CURSOR_WAITING: &str =
    "Waiting for the sign-in in your browser. It is cancelled after 5 minutes.";
const NAMES_OFF: &str = "Once a tool runs, its models are listed here to rename.";
const NAMES_ON: &str =
    "Made from the id the tool stores. Click a field and type to rename; clear it to get the automatic name back. A model two tools share has one name.";
/// Under the names while Model is not ticked anywhere: a name can be
/// given already, and this says where it will show.
const NAMES_WHERE: &str =
    "Your names show where Model is ticked: tick Model above, in the Widget or Tooltip column, to see them.";
const NAMES_HINT: &str = "Your own name for a model, shown everywhere instead of the automatic one. Listed are the models each running tool is known to offer and the one it uses now, marked active.";
const IN_USE: &str = "active";
const SOURCE_SET_HINT: &str =
    "The model chosen in each tool's settings, whatever a session is using.";
const SOURCE_LAST_HINT: &str = "The model of the newest Claude Code or Codex CLI session, read from the end of its file. The other tools show the model as set.";
/// The link at the list's end, and what it says on hover.
const SETTINGS_FILE: &str = "Edit config.toml...";
const SETTINGS_FILE_HINT: &str = "Fine-tuning that has no place in this window: sizes, colours, thresholds. Opens config.toml in Notepad; changes apply when you save.";
const ALLOW_ALL: &str = "Allow all";
const ALLOW_ALL_LEAD: &str =
    "Allow all also allows the other detected tools, so ticking them later asks nothing. Their rows stay unticked:";
const FOUND_TITLE: &str = "Show the detected tools?";
const FOUND_LEAD: &str = "Tick the tools to show. For each one ticked, AI Limits reads:";
const FOUND_OUTRO: &str = "Each sign-in stays on this PC and goes only to the service named with it, to ask for your usage. A tool's own token is never changed or refreshed. Cancel to tick the tools yourself; untick a tool's rows to stop.";
/// Ordinary text for the "room left" estimate: what typing would add.
const ROOM_SAMPLE: &str = "Sonnet 4.6 Opus Max GPT 5.5 Haiku Grok Fast ";

/// "1 model", "4 models".
fn count_of(n: usize, one: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {one}s")
    }
}

/// What each row of the widget, the tooltip and the taskbar panel can show
/// besides its name, with a sample and what it says on hover. The tooltip
/// has no bar.
const TIP_PARTS: [(TipPart, &str, &str, &str); 3] = [
    (
        TipPart::Percent,
        "Percentage",
        "25% · 48%",
        "How much of the limit is used, as a number.",
    ),
    (
        TipPart::Reset,
        "Time to reset",
        "2h 14min · 4d 3h",
        "How long until the limit the row shows resets. On the panel it follows the bar, or stands in the percentage's place when that is left out.",
    ),
    (
        TipPart::Bar,
        "Progress bar",
        "under or beside the values",
        "The bar of each row in the widget and on the taskbar panel; the tooltip has none. Without it each widget row is one line and the panel takes less of the taskbar. The Vertical layout is made of its bars and keeps them.",
    ),
];

/// What each model part shows, on hover.
fn part_hint(part: ModelPart) -> &'static str {
    match part {
        ModelPart::Model => {
            "The model the tool uses, by name: Opus, GPT, Grok. Shown on the tool's own row."
        }
        ModelPart::Version => {
            "The model's version beside its name: Opus 5.5, GPT-6.1 Sol. Needs Model."
        }
        ModelPart::Effort => {
            "How hard the model thinks, as the tool calls it. It can show without the model."
        }
    }
}

const PARTS: [(ModelPart, &str, &str); 3] = [
    (ModelPart::Model, "Model", "Opus · GPT · Grok · Auto"),
    (ModelPart::Version, "Version", "5.5 · 6.1 Sol · 4.7"),
    (
        ModelPart::Effort,
        "Reasoning effort",
        "low · medium · high · xhigh · max",
    ),
];

const SURFACES: [(Surface, f32); 2] = [
    (Surface::Widget, COL_WIDGET),
    (Surface::Tooltip, COL_TOOLTIP),
];

/// The Taskbar column: which providers the taskbar panel and the tray icon
/// show. Its head, and what its ticks say on hover.
const TASKBAR: &str = "Taskbar";
const TASKBAR_HINT: &str = "Show on the taskbar panel and the tray icon. They have room for two; untouched, the first two shown in the widget.";
const TASKBAR_FULL: &str = "The taskbar panel has room for two: untick one first.";
const TASKBAR_LAST: &str = "At least one shows on the taskbar. To hide the panel or the icon, choose Off under Indicator in the menu.";

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
    /// A tab under the title.
    Tab(Tab),
    /// A tray icon look's tile.
    TrayStyle(TrayStyle),
    Row {
        row: String,
        surface: Surface,
    },
    Part {
        part: ModelPart,
        surface: Surface,
    },
    Source(ModelSource),
    /// A Percent or Reset time tick: in the Tooltip or the Taskbar column.
    Tip(TipPart, NumbersOn),
    /// A name field: the key its name is kept under, and the tool it is
    /// listed under (a shared model has a field under each tool).
    Field {
        key: String,
        provider: String,
    },
    /// A tool's tick in the welcome's question about the tools found.
    Pick(String),
    Allow,
    /// The sign-in question's second answer: this tool and every other
    /// tool found on this PC that is not running yet.
    AllowAll,
    Cancel,
    NoticeOk,
    NoticeReview,
    /// Open or close the list of model names.
    Names,
    /// Open or close one tool's models in that list (config id).
    NamesOf(String),
    /// A provider's tick in the Taskbar column (config id).
    Taskbar(String),
    /// Open or close a provider's sign-in (config id).
    SignIn(String),
    /// A sign-in source's radio.
    SignInSource(String, SourceId),
    /// Paste or remove the chosen source's secret.
    Paste(String),
    Remove(String),
    CursorSignIn,
    CursorCancel,
    CursorSignOut,
    /// The "Edit config.toml..." link.
    SettingsFile,
}

/// Keyboard grid position: (row, column).
type Nav = (u32, u32);

/// The window's two tabs: which rows show where, and what each shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Rows,
    Shows,
}

impl Tab {
    fn other(self) -> Self {
        match self {
            Tab::Rows => Tab::Shows,
            Tab::Shows => Tab::Rows,
        }
    }
}

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
    /// A tool's tick in a question: the box at `check`, the whole entry
    /// (box, name and what is read) answering to the pointer.
    Pick {
        target: Target,
        rect: Rect,
        check: Rect,
        on: bool,
        nav: Nav,
    },
    Button {
        target: Target,
        rect: Rect,
        label: String,
        primary: bool,
        /// Allow with no tool ticked cannot be used.
        enabled: bool,
        nav: Nav,
    },
    Close {
        rect: Rect,
        nav: Nav,
    },
    /// A heading that opens and closes what is under it: "+ Model names",
    /// or with `arrow` "▸ Sign-in" (a "+" there would read as one more
    /// extra row).
    Toggle {
        target: Target,
        rect: Rect,
        label: String,
        count: String,
        open: bool,
        bold: bool,
        arrow: bool,
        nav: Nav,
    },
    /// A tab under the title, the chosen one raised.
    Tab {
        target: Target,
        rect: Rect,
        label: String,
        on: bool,
        nav: Nav,
    },
    /// A tray icon look: its icon over its label, the chosen one outlined.
    Tile {
        target: Target,
        rect: Rect,
        label: String,
        style: TrayStyle,
        on: bool,
        nav: Nav,
    },
    /// What a part of the window is, shown while the pointer rests on it.
    Hint {
        rect: Rect,
        text: String,
    },
    /// A line of text that acts when clicked, in the accent: underlined
    /// under the pointer. `rect` answers to the pointer; the text sits
    /// `pad` in from its left.
    Link {
        target: Target,
        rect: Rect,
        label: String,
        pad: f32,
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
            El::Pick { rect, check, .. } => {
                *rect = m(rect);
                *check = m(check);
            }
            El::Radio { rect, .. }
            | El::Field { rect, .. }
            | El::Button { rect, .. }
            | El::Close { rect, .. }
            | El::Toggle { rect, .. }
            | El::Hint { rect, .. }
            | El::Tab { rect, .. }
            | El::Tile { rect, .. }
            | El::Link { rect, .. } => *rect = m(rect),
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
            | El::Button {
                target,
                rect,
                enabled: true,
                nav,
                ..
            }
            | El::Field {
                target, rect, nav, ..
            }
            | El::Pick {
                target, rect, nav, ..
            }
            | El::Toggle {
                target, rect, nav, ..
            }
            | El::Link {
                target, rect, nav, ..
            }
            | El::Tab {
                target, rect, nav, ..
            }
            | El::Tile {
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
    /// The scrolling list, clipped to the viewport.
    Body,
    Overlay,
    /// An open question's text, clipped to its own viewport.
    AskBody,
    /// Over everything: the question's buttons, the outline.
    Top,
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
            // Row 0 is the header's close button, row 1 the tabs.
            nav: 2,
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
        alpha: Level::Full,
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
        alpha: Level::Full,
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
    sticky: Option<Sticky>,
    /// The list's scroll bar, while the list scrolls.
    bar: Option<Bar>,
    /// An open question's box and its scrolling text.
    ask: Option<AskFrame>,
}

/// The column heads, drawn over the list (window coordinates).
#[derive(Debug, Clone)]
struct Sticky {
    rect: Rect,
    shapes: Vec<Shape>,
    /// Held at the list's top, over rows scrolled under it.
    pinned: bool,
}

/// A scroll bar (window coordinates): the strip that answers to the
/// pointer, and the thumb drawn in it.
#[derive(Debug, Clone, Copy)]
struct Bar {
    grip: Rect,
    thumb: Rect,
}

impl Bar {
    /// The bar for `viewport` showing `content_h` scrolled to `scroll`, its
    /// thumb `x` from the left; None when everything shows.
    fn of(viewport: &Rect, content_h: f32, scroll: f32, x: f32) -> Option<Bar> {
        let max = (content_h - viewport.h).max(0.0);
        if max <= 0.0 {
            return None;
        }
        let th = (viewport.h * viewport.h / content_h).max(24.0);
        let ty = viewport.y + 2.0 + (viewport.h - 4.0 - th) * scroll / max;
        Some(Bar {
            grip: rect(x + 3.0 - GRIP_W, viewport.y, GRIP_W, viewport.h),
            thumb: rect(x, ty, 3.0, th),
        })
    }

    /// The scroll a thumb dragged `dy` from where it was at `from` gives.
    fn scrolled(&self, viewport: &Rect, content_h: f32, from: f32, dy: f32) -> f32 {
        let max = (content_h - viewport.h).max(0.0);
        let run = (viewport.h - 4.0 - self.thumb.h).max(1.0);
        (from + dy * max / run).clamp(0.0, max)
    }
}

/// An open question laid out (window coordinates).
#[derive(Debug, Clone, Copy)]
struct AskFrame {
    /// The question's box (the tests hold its buttons to it).
    #[cfg_attr(not(test), allow(dead_code))]
    rect: Rect,
    /// Where its text shows, above its buttons.
    viewport: Rect,
    /// The text's whole height.
    content_h: f32,
    /// How far the text is scrolled, held to it.
    scroll: f32,
    /// Its scroll bar, while the text is taller than the room for it.
    bar: Option<Bar>,
}

impl AskFrame {
    fn max_scroll(&self) -> f32 {
        (self.content_h - self.viewport.h).max(0.0)
    }
}

/// A question laid out in its box's own coordinates.
struct AskLayout {
    /// Where its text ends, from the box's top.
    text_h: f32,
    text: Vec<El>,
    /// Its buttons, their top at 0.
    buttons: Vec<El>,
}

impl AskLayout {
    /// The box's height with all of its text showing.
    fn height(&self) -> f32 {
        (self.text_h + ASK_GAP + BUTTON_H + ASK_BOTTOM).ceil()
    }
}

/// A scroll bar's thumb.
fn thumb_shape(b: &Bar) -> Shape {
    Shape::Fill {
        rect: b.thumb,
        radius: 1.5,
        ink: Ink::Muted,
        alpha: Level::Thumb,
    }
}

/// At most `max` characters of `s`, an ellipsis ending a longer one.
fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

impl Frame {
    fn max_scroll(&self) -> f32 {
        (self.content_h - self.viewport.h).max(0.0)
    }

    /// Where the list shows rows: below the column heads while they are
    /// pinned.
    fn list_top(&self) -> f32 {
        match &self.sticky {
            Some(s) if s.pinned => bottom(&s.rect),
            _ => self.viewport.y,
        }
    }

    /// Whether a point is on the list's visible rows.
    fn in_list(&self, x: f32, y: f32) -> bool {
        contains(&self.viewport, x, y)
            && y >= self.list_top()
            && !self
                .sticky
                .as_ref()
                .is_some_and(|s| contains(&s.rect, x, y))
    }

    /// Whether a point is on an open question's visible text.
    fn in_ask_text(&self, x: f32, y: f32) -> bool {
        self.ask.is_some_and(|a| contains(&a.viewport, x, y))
    }
}

// ─── State machine ───────────────────────────────────────────────

/// What the picker shows, from the mode it opened as and what has been
/// answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Rows { welcome: bool },
    Notice,
}

/// What an open question asks.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Question {
    /// One read: for a tick or a sign-in choice that needs it.
    Read(ReadKind),
    /// The welcome's: the sign-ins of the tools found on this PC, each
    /// with a tick of its own (`Ask::tools`, `Ask::picked`).
    Found,
}

/// A question open inside the picker.
#[derive(Debug, Clone)]
struct Ask {
    question: Question,
    /// The tick's own actions, sent after `Allow`.
    then: Vec<PickerAction>,
    /// The tick that asked; it gets the focus back. None for the question
    /// the welcome opens with by itself.
    from: Option<Target>,
    /// A `Found` question's tools (config ids) in order, and those ticked.
    tools: Vec<String>,
    picked: Vec<String>,
    /// How far its text is scrolled.
    scroll: f32,
}

/// A name field being edited.
#[derive(Debug, Clone)]
struct Edit {
    /// The key the name is kept under (`ModelChoice::key`).
    key: String,
    text: String,
    /// Byte index into `text`, on a char boundary.
    caret: usize,
    /// The last edit was stopped or cut at the width limit.
    full: bool,
    /// The name last sent (or the input's, before the first send).
    sent: Option<String>,
    /// The name when the edit began: Esc brings it back.
    original: Option<String>,
}

/// A scroll bar's thumb held down: which bar, the pointer's y and the
/// scroll when it was pressed.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    ask: bool,
    from_y: f32,
    from_scroll: f32,
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
    /// Whether the list of model names is open (closed on every opening).
    names_open: bool,
    /// The tools whose models are open in that list (config ids).
    open_names: Vec<String>,
    /// The providers whose sign-in is open (config ids).
    open_sign_in: Vec<String>,
    /// The update notice was answered with Review: the normal picker shows.
    reviewed: bool,
    /// `NoticeDone` was sent.
    notice_done: bool,
    /// What the window opened as. The welcome keeps its look while open,
    /// also once the app counts it answered and sends `Normal`.
    opened_as: PickerMode,
    /// The welcome's question about the tools found was answered: what it
    /// left unticked was left on purpose, so Allow all is not offered
    /// while the window stays open.
    found_answered: bool,
    /// A scroll bar's thumb held down.
    drag: Option<Drag>,
    /// The tab shown (Rows on every opening).
    tab: Tab,
    /// The description the pointer rests on (its rect) and since when:
    /// shown once it has rested `hint_delay`.
    hint_rest: Option<(Rect, std::time::Instant)>,
    hint_delay: std::time::Duration,
    dirty: bool,
    /// Bumped by every new input; part of the layout's cache key.
    input_gen: u64,
    /// The last layout and the state it was made from: hover, focus and
    /// presses do not move anything, so a pointer move reuses it.
    layout: RefCell<Option<(LayoutKey, Rc<Frame>)>>,
}

/// Everything a layout depends on besides the input.
#[derive(Debug, Clone, PartialEq)]
struct LayoutKey {
    input: u64,
    scroll: u32,
    max_h: u32,
    /// The open question, its ticks and its scroll.
    ask: Option<(Question, Vec<String>, u32)>,
    found_answered: bool,
    reviewed: bool,
    /// The edit's key, text and "full" flag: the field's hint follows it.
    edit: Option<(String, String, bool)>,
    /// The focused name field: its room-left hint shows under it.
    field: Option<Target>,
    /// What is open in the list of names.
    names: (bool, Vec<String>),
    /// The providers whose sign-in is open.
    sign_in: Vec<String>,
    tab: Tab,
}

/// What a point is over.
#[derive(Debug, Clone, PartialEq)]
enum Hit {
    /// Outside the box: the shadow margin.
    Outside,
    Nothing,
    Target(Target),
    /// A scroll bar: the list's, or the open question's.
    Bar {
        ask: bool,
    },
}

impl Picker {
    /// A picker showing `input`. `measure(text, size)` is the width of text
    /// in the app's font, `Renderer::text_width`: it lays the picker out and
    /// holds a new model name to `rows::name_fits`.
    ///
    /// Nothing has the keyboard focus at first. The welcome and the update
    /// notice may open by themselves while the user types elsewhere, so a
    /// stray Enter answers nothing: the default button takes the focus at
    /// the first Tab or arrow key, or a click.
    pub fn new(input: PickerInput, measure: impl Fn(&str, f32) -> f32 + 'static) -> Self {
        let opened_as = input.mode;
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
            names_open: false,
            open_names: Vec::new(),
            open_sign_in: Vec::new(),
            reviewed: false,
            notice_done: false,
            opened_as,
            found_answered: false,
            drag: None,
            tab: Tab::Rows,
            hint_rest: None,
            // At once in the tests, which look at hints right after a move.
            hint_delay: if cfg!(test) {
                std::time::Duration::ZERO
            } else {
                HINT_DELAY
            },
            dirty: true,
            input_gen: 0,
            layout: RefCell::new(None),
        };
        p.ask_found_tools();
        p
    }

    /// The welcome starts from the tools found on this PC: one question
    /// lists each with a tick of its own, ticked when the tool itself was
    /// found (a folder of its own), unticked when only a related tool was
    /// (GitHub CLI for Copilot, `.gemini` for Antigravity). Their own rows
    /// show ticked behind it as its ticks go. Allow starts the tools ticked;
    /// Cancel leaves nothing ticked and nothing read, and the user ticks
    /// what they use. Nothing is read before the answer. A tool allowed
    /// already is not asked about (one whose rows the user unticked keeps
    /// its consent, so it is left alone too); a fresh install has every
    /// provider switched off and is asked all the same.
    /// With nothing found surely, nothing is asked and the rows are only
    /// marked.
    fn ask_found_tools(&mut self) {
        if self.input.mode != PickerMode::Welcome {
            return;
        }
        let tools: Vec<String> = crate::consent::READ_INFO
            .iter()
            .filter(|r| self.found(r.id) && !self.allowed(r.id))
            .filter(|r| self.input.rows.iter().any(|row| row.id == r.id))
            .map(|r| r.id.to_string())
            .collect();
        let picked: Vec<String> = tools
            .iter()
            .filter(|id| !self.found_weakly(id))
            .cloned()
            .collect();
        if picked.is_empty() {
            return;
        }
        self.ask = Some(Ask {
            question: Question::Found,
            then: Vec::new(),
            from: None,
            tools,
            picked,
            scroll: 0.0,
        });
        self.dirty = true;
    }

    /// The other tools found on this PC by a folder of their own that are
    /// not allowed yet: what Allow all allows besides `provider` (without
    /// switching them on or ticking a row). Never allowed is also how a provider switched off in the
    /// menu looks on a fresh install or after an update, so that is not a
    /// reason to leave one out; one the user switched off after allowing it
    /// keeps its consent and is left alone. Nothing once the welcome's
    /// question was answered.
    fn others_found(&self, provider: &str) -> Vec<String> {
        if self.found_answered {
            return Vec::new();
        }
        let p = canonical(provider);
        crate::consent::READ_INFO
            .iter()
            .filter(|r| {
                r.id != p && self.found(r.id) && !self.found_weakly(r.id) && !self.allowed(r.id)
            })
            .filter(|r| self.input.rows.iter().any(|row| row.id == r.id))
            .map(|r| r.id.to_string())
            .collect()
    }

    /// A tool's own row on both surfaces: what a tool started from a
    /// question shows.
    fn own_rows_on(provider: &str) -> [PickerAction; 2] {
        [Surface::Widget, Surface::Tooltip].map(|surface| PickerAction::SetRow {
            surface,
            row: provider.to_string(),
            on: true,
        })
    }

    /// Whether the open question is the welcome's and has `provider` ticked.
    fn picked(&self, provider: &str) -> bool {
        let p = canonical(provider);
        matches!(
            &self.ask,
            Some(a) if a.question == Question::Found && a.picked.iter().any(|i| i == p)
        )
    }

    /// A new snapshot, after the app applied an action. Focus, hover, scroll,
    /// an open question and an edit in progress stay.
    pub fn set_input(&mut self, input: PickerInput) {
        self.input = input;
        self.input_gen += 1;
        if let Some(e) = &self.edit {
            if !self.input.models.iter().any(|m| m.key == e.key) {
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

    /// What the window is called (screen readers say it): the welcome, the
    /// notice or "AI Limits Settings", as it opened.
    pub fn title(&self) -> &'static str {
        match self.view() {
            View::Rows { welcome: true } => TITLE_WELCOME,
            View::Rows { welcome: false } => TITLE_ROWS,
            View::Notice => TITLE_NOTICE,
        }
    }

    /// Whether anything visible changed since the last call.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Whether a name field is being edited (a paste has somewhere to go).
    pub fn editing(&self) -> bool {
        self.edit.is_some() && matches!(self.focus, Some(Target::Field { .. }))
    }

    /// Where the caret of the field being edited is, in logical pixels: the
    /// IME's candidate window opens there.
    pub fn caret_spot(&self) -> Option<(f32, f32)> {
        if !self.editing() {
            return None;
        }
        let f = self.frame();
        let t = self.focus.as_ref()?;
        let (r, model) = f.els.iter().find_map(|(_, el)| match el {
            El::Field {
                target,
                rect,
                model,
                ..
            } if target == t => Some((*rect, *model)),
            _ => None,
        })?;
        let text = self.field_text(&self.input.models[model]);
        let caret = self
            .edit
            .as_ref()
            .map_or(text.len(), |e| e.caret.min(text.len()));
        Some((
            (r.x + 8.0 + self.adv(&text[..caret], SIZE_TEXT)).round(),
            bottom(&r),
        ))
    }

    /// Whether the pointer is over a name field (the window shows the text
    /// cursor there).
    pub fn wants_text_cursor(&self) -> bool {
        let Some((x, y)) = self.pointer else {
            return false;
        };
        matches!(
            self.hit(&self.frame(), x, y),
            Hit::Target(Target::Field { .. })
        )
    }

    /// Feed one event; returns what the app should do, in order.
    pub fn handle(&mut self, event: PickerEvent) -> Vec<PickerAction> {
        // Only a pointer that moves leaves the layout as it was.
        let relayout = !matches!(
            event,
            PickerEvent::PointerMove { .. } | PickerEvent::PointerLeave
        );
        let out = match event {
            PickerEvent::PointerMove { x, y } => {
                if let Some(d) = self.drag {
                    self.drag_to(d, y);
                }
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
                self.scroll_by(-dy);
                Vec::new()
            }
            PickerEvent::Key(k) => self.key(k),
            PickerEvent::Text(t) => self.type_text(&t),
            PickerEvent::Paste(t) => self.paste(&t),
            PickerEvent::FocusLost => self.focus_lost(),
            PickerEvent::CloseRequested => self.close(),
        };
        if relayout {
            self.settle();
        }
        self.track_hint();
        out
    }

    /// The rect of the description under the pointer, if any.
    fn hint_under(&self) -> Option<Rect> {
        let (x, y) = self.pointer?;
        let f = self.frame();
        match &f.els[self.hint_at(&f, x, y)?].1 {
            El::Hint { rect, .. } => Some(*rect),
            _ => None,
        }
    }

    /// Follow the pointer's rest on a description: a new one starts its
    /// wait, straight after one that showed a short one.
    fn track_hint(&mut self) {
        let same = |a: &Rect, b: &Rect| (a.x, a.y, a.w, a.h) == (b.x, b.y, b.w, b.h);
        let Some(u) = self.hint_under() else {
            self.hint_rest = None;
            return;
        };
        let now = std::time::Instant::now();
        match self.hint_rest {
            Some((r, _)) if same(&r, &u) => {}
            prev => {
                let warm = prev.is_some_and(|(_, since)| now >= since + self.hint_delay);
                let since = if warm {
                    now.checked_sub(self.hint_delay.saturating_sub(HINT_RESHOW))
                        .unwrap_or(now)
                } else {
                    now
                };
                self.hint_rest = Some((u, since));
            }
        }
    }

    /// Whether the description at `i` in `f` has been rested on long enough.
    fn hint_ready(&self, f: &Frame, i: usize) -> bool {
        let El::Hint { rect: r, .. } = &f.els[i].1 else {
            return false;
        };
        self.hint_rest.is_some_and(|(at, since)| {
            (at.x, at.y, at.w, at.h) == (r.x, r.y, r.w, r.h)
                && std::time::Instant::now() >= since + self.hint_delay
        })
    }

    /// When the description the pointer rests on is due: the window is
    /// drawn again then. None when none is waiting.
    pub fn next_wake(&self) -> Option<std::time::Instant> {
        let (_, since) = self.hint_rest?;
        let due = since + self.hint_delay;
        (due > std::time::Instant::now()).then_some(due)
    }

    // ─── Reading the input ───────────────────────────────────────

    fn view(&self) -> View {
        match self.opened_as {
            PickerMode::UpdateNotice if !self.reviewed => View::Notice,
            // The welcome keeps its intro and marks while it is open, also
            // once it counts as answered: the rows do not jump under the
            // cursor after the first Allow.
            PickerMode::Welcome => View::Rows { welcome: true },
            _ => View::Rows { welcome: false },
        }
    }

    /// Whether the provider may run: its sign-in allowed, or set up with the
    /// app's own (a pasted key, the menu's Cursor sign-in), which needs no
    /// question.
    fn allowed(&self, provider: &str) -> bool {
        let p = canonical(provider);
        let reads = &self.input.reads;
        reads
            .sign_in
            .iter()
            .chain(reads.app_sign_in.iter())
            .any(|s| canonical(s) == p)
    }

    fn enabled(&self, provider: &str) -> bool {
        let p = canonical(provider);
        self.input.enabled.iter().any(|e| canonical(e) == p)
    }

    /// A provider's dot: `Good` while it brings numbers, `Warn` while it
    /// runs without them (loading, retrying, not signed in), `Dim` (an
    /// outline) while it is off or not allowed.
    fn dot_of(&self, row: &RowChoice) -> Ink {
        if !(self.allowed(row.provider) && self.enabled(row.provider)) {
            Ink::Dim
        } else if row.provider_ok {
            Ink::Good
        } else {
            Ink::Warn
        }
    }

    /// A sign-in summary, split for its chip: the source in bold, then how
    /// it went ("Claude Code", "· works").
    fn chip_parts(summary: &str) -> (&str, Option<String>) {
        match summary.split_once(" \u{b7} ") {
            Some((source, how)) => (source, Some(format!("\u{b7} {how}"))),
            None => (summary, None),
        }
    }

    /// How wide a sign-in's chip is: "Sign-in", the source, how it went and
    /// the arrow, `CHIP_GAP` apart, `CHIP_PAD` in from its ends.
    fn chip_w(&self, label: &str, summary: &str) -> f32 {
        let (source, how) = Self::chip_parts(summary);
        let mut w = CHIP_PAD * 2.0
            + self.adv(label, SIZE_NOTE)
            + CHIP_GAP
            + self.adv_styled(source, SIZE_NOTE, true)
            + CHIP_GAP
            + self
                .adv(CHIP_CLOSED, SIZE_NOTE)
                .max(self.adv(CHIP_OPEN, SIZE_NOTE));
        if let Some(how) = how {
            w += self.adv(&how, SIZE_NOTE) + CHIP_GAP;
        }
        w.ceil()
    }

    /// Whether a provider is ticked in the Taskbar column.
    fn taskbar_on(&self, provider: &str) -> bool {
        let p = canonical(provider);
        self.input.taskbar_rows.iter().any(|t| canonical(t) == p)
    }

    /// Whether its Taskbar tick can be used: not the last one ticked, and
    /// not one too many (the panel has room for `rows::TASKBAR_MAX`).
    fn taskbar_enabled(&self, provider: &str) -> bool {
        let n = self.input.taskbar_rows.len();
        if self.taskbar_on(provider) {
            n > 1
        } else {
            n < crate::rows::TASKBAR_MAX
        }
    }

    fn found(&self, provider: &str) -> bool {
        let p = canonical(provider);
        self.input.found.iter().any(|e| canonical(e) == p)
    }

    /// Found only by a related tool's folder: marked, never ticked for the
    /// user.
    fn found_weakly(&self, provider: &str) -> bool {
        let p = canonical(provider);
        self.input.found_weak.iter().any(|e| canonical(e) == p)
    }

    /// A row shows as ticked only when it is chosen AND its provider may be
    /// read: what is not read cannot show. While the welcome asks about the
    /// tools found, the own rows of those ticked in it show ticked: what
    /// Allow will start.
    fn row_on(&self, surface: Surface, row: &RowChoice) -> bool {
        if row.id == canonical(row.provider) && self.picked(row.provider) {
            return true;
        }
        let list = match surface {
            Surface::Widget => &self.input.widget_rows,
            Surface::Tooltip => &self.input.tooltip_rows,
        };
        list.iter().any(|r| r == row.id) && self.allowed(row.provider)
    }

    /// A part shows as ticked only with the settings files allowed; the
    /// version only with the model.
    fn numbers(&self, place: NumbersOn) -> UiNumbers {
        match place {
            NumbersOn::Widget => self.input.widget,
            NumbersOn::Tooltip => self.input.tip,
            NumbersOn::Panel => self.input.panel,
        }
    }

    /// Whether the tooltip or the panel shows `part`.
    fn tip_on(&self, part: TipPart, place: NumbersOn) -> bool {
        self.numbers(place).shows(part)
    }

    /// Whether a `part` tick can be used: not the last one on there.
    fn tip_enabled(&self, part: TipPart, place: NumbersOn) -> bool {
        let now = self.numbers(place);
        let mut next = now;
        next.set(part, !now.shows(part), place);
        next != now
    }

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
    /// The models that can be named: every model of each tool that runs.
    /// Naming reads nothing (the list is the catalogue's, with the one in
    /// use when the settings are read), so it does not wait for Model to be
    /// ticked; `NAMES_WHERE` says where the names show until it is.
    fn renamable(&self) -> Vec<usize> {
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

    /// What a name field shows: the edit in progress (under every tool the
    /// model is listed under), or the input's name.
    fn field_text(&self, model: &ModelChoice) -> String {
        match &self.edit {
            Some(e) if e.key == model.key => e.text.clone(),
            _ => model.renamed.clone().unwrap_or_default(),
        }
    }

    /// One tool in the welcome's question or in Allow all's list: its files
    /// and where its token goes, the settings and session files already
    /// allowed for every tool, which apply to it too, and what reading its
    /// sign-in may cost (`ReadInfo::risk`).
    fn found_entry(&self, i: &crate::consent::ReadInfo) -> String {
        let mut s = format!(
            "{}; its {} goes only to {}",
            i.sign_in.join("; "),
            i.secret(),
            i.hosts.join(" and ")
        );
        for file in self.allowed_model_files(i) {
            s.push_str("; ");
            s.push_str(file);
        }
        if let Some(risk) = i.risk {
            s.push_str(". ");
            s.push_str(risk);
        }
        s
    }

    /// The settings and session files a newly allowed tool is read for,
    /// because those reads are allowed for every tool already.
    fn allowed_model_files(&self, i: &crate::consent::ReadInfo) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.input.reads.tool_settings {
            out.extend(i.settings);
        }
        if self.input.reads.last_session && self.source() == ModelSource::LastUsed {
            out.extend(i.session);
        }
        out
    }
}

mod input;
mod layout;
mod scene;
mod text;
mod window;

pub use window::{place, Anchor, PickerWindow, ScreenRect, Side};

#[cfg(test)]
mod tests;
