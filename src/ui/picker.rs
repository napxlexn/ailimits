// ui/picker.rs — the "Rows and models" window: the app's own layered window,
// drawn by the renderer, that chooses rows, model labels and model names, and
// asks before each read of another tool's files.
//
// The picker never reads or writes config.toml or any other file. It shows a
// `PickerInput` the app gives it and answers with `PickerAction`s; the app
// applies each one at once (widget, tooltip, panel, tray) and gives it a new
// input.

use crate::config::schema::{ModelPart, ModelSource, Reads, Surface, UiModel};

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
