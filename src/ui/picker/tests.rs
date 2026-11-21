// ui/picker/tests.rs — the picker's tests: the state machine without a
// window (a fixed-advance font), the real font where the layout depends on
// it, and the pinned look (ui/visual_baseline.rs).

use super::window::max_box_height;
#[cfg(target_os = "windows")]
use super::window::system;
use super::*;
use crate::providers::ProviderId;
use crate::ui::renderer::{PickerGlyphs, Renderer};
use crate::ui::theme::PickerTheme;

/// A fixed-advance stand-in for the font: half the size per character,
/// so 10 ordinary characters fit a model name's narrowest place.
fn mono(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.5
}

const ALL: [&str; 6] = [
    "claude",
    "codex",
    "grok",
    "cursor",
    "antigravity",
    "copilot",
];

/// Every row there is, as the app offers them.
fn rows() -> Vec<RowChoice> {
    let own = [
        ("claude", "Claude", "Today’s row"),
        ("codex", "Codex", "Today’s row"),
        ("grok", "Grok", "Shared with Cursor and Grok Bot"),
        ("cursor", "Cursor", "The same limit as the Grok row"),
        ("antigravity", "Antigravity", "Gemini models, weekly"),
        ("copilot", "Copilot", "Premium requests, monthly"),
    ];
    let mut out = Vec::new();
    for (id, name, note) in own {
        out.push(RowChoice {
            id,
            provider: id,
            name,
            note: note.into(),
            hint: "",
            has_data: true,
            provider_ok: true,
        });
        let pid = ProviderId::from_config_id(id).unwrap();
        for s in crate::rows::sub_rows(&pid) {
            out.push(RowChoice {
                id: s.id,
                provider: id,
                name: s.names[0],
                note: s.note.into(),
                hint: s.hint,
                has_data: true,
                provider_ok: true,
            });
        }
    }
    out
}

fn models() -> Vec<ModelChoice> {
    let m = |provider: &'static str, id: &str, key: &str, auto: &str| ModelChoice {
        provider,
        model_id: id.to_string(),
        key: key.to_string(),
        automatic: auto.to_string(),
        renamed: None,
        current: true,
        shared_with: Vec::new(),
    };
    vec![
        m("claude", "claude-opus-5-5", "claude-opus-5-5", "Opus 5.5"),
        m("codex", "gpt-6.1-sol", "gpt-6.1-sol", "GPT-6.1 Sol"),
        m("grok", "grok-4.7", "grok-4.7", "Grok 4.7"),
        m("copilot", "auto", "copilot:auto", "Auto"),
    ]
}

/// The upgraded user's day: five tools read, the default rows, the
/// Opus week in the tooltip, no model label yet.
fn input() -> PickerInput {
    let base = ["claude", "codex", "grok", "antigravity", "copilot"];
    let mut tooltip: Vec<String> = base.iter().map(|s| s.to_string()).collect();
    tooltip.push("claude.opus".into());
    PickerInput {
        mode: PickerMode::Normal,
        rows: rows(),
        widget_rows: base.iter().map(|s| s.to_string()).collect(),
        tooltip_rows: tooltip,
        // Untouched: the first two that run.
        taskbar_rows: vec!["claude".into(), "codex".into()],
        model: UiModel::default(),
        tip: UiNumbers::default(),
        panel: UiNumbers::PERCENT_ONLY,
        sign_in: Vec::new(),
        reads: Reads {
            sign_in: base.iter().map(|s| s.to_string()).collect(),
            ..Reads::default()
        },
        enabled: ALL.to_vec(),
        found: Vec::new(),
        found_weak: Vec::new(),
        models: models(),
    }
}

/// The settings files allowed and the model shown in the tooltip.
fn with_model() -> PickerInput {
    let mut i = input();
    i.reads.tool_settings = true;
    i.model.tooltip = vec![ModelPart::Model];
    i
}

/// The sign-in of every provider, as the app would describe it on a PC
/// where Claude and Codex run on their tools' sign-ins, Copilot on a pasted
/// token, and Cursor is not signed in.
fn sign_ins() -> Vec<SignInChoice> {
    let src = |id, label, on| SignInSource { id, label, on };
    vec![
        SignInChoice {
            provider: "claude",
            summary: "Claude Code \u{b7} works".into(),
            sources: vec![
                src(SourceId::Tool, "Claude Code's sign-in", true),
                src(SourceId::UsageToken, "A usage token you paste", false),
                src(SourceId::Key, "An API key you paste", false),
            ],
            detail: "Reads %USERPROFILE%\\.claude\\.credentials.json; its token goes only to api.anthropic.com. Claude Code renews it as you use it.".into(),
            paste: None,
            stored: false,
            note: None,
            cursor: None,
        },
        SignInChoice {
            provider: "codex",
            summary: "Codex CLI \u{b7} works".into(),
            sources: vec![
                src(SourceId::Tool, "Codex CLI's sign-in", true),
                src(SourceId::UsageToken, "A usage token you paste", false),
            ],
            detail: "Reads %USERPROFILE%\\.codex\\auth.json (in CODEX_HOME when it is set); its token goes only to chatgpt.com.".into(),
            paste: None,
            stored: false,
            note: None,
            cursor: None,
        },
        SignInChoice {
            provider: "grok",
            summary: "Grok CLI \u{b7} works".into(),
            sources: Vec::new(),
            detail: "Reads %USERPROFILE%\\.grok\\auth.json (in GROK_HOME when it is set); its token goes only to cli-chat-proxy.grok.com.".into(),
            paste: None,
            stored: false,
            note: None,
            cursor: None,
        },
        SignInChoice {
            provider: "cursor",
            summary: "Not signed in".into(),
            sources: Vec::new(),
            detail: "Signs in to Cursor in your browser, as the Cursor editor does, and keeps that session in Windows Credential Manager. Nothing of the editor's own sign-in is read.".into(),
            paste: None,
            stored: false,
            note: None,
            cursor: Some(CursorState::SignedOut),
        },
        SignInChoice {
            provider: "antigravity",
            summary: "Antigravity CLI \u{b7} token expired".into(),
            sources: Vec::new(),
            detail: "Reads the Antigravity CLI's sign-in in Windows Credential Manager (gemini:antigravity); its token goes only to cloudcode-pa.googleapis.com. It lasts about an hour: run agy to renew it.".into(),
            paste: None,
            stored: false,
            note: None,
            cursor: None,
        },
        SignInChoice {
            provider: "copilot",
            summary: "Your token \u{b7} works".into(),
            sources: vec![
                src(SourceId::Tool, "GitHub CLI's sign-in", false),
                src(SourceId::Key, "A fine-grained token you paste", true),
            ],
            detail: "Kept in Windows Credential Manager; it goes only to api.github.com. It needs the Copilot Requests permission.".into(),
            paste: Some("Paste token from clipboard"),
            stored: true,
            note: Some("Checked with GitHub: it works. Stored in Credential Manager.".into()),
            cursor: None,
        },
    ]
}

fn picker(input: PickerInput) -> Picker {
    Picker::new(input, mono)
}

/// A picker with the list of names open, every tool's models showing.
fn opened(input: PickerInput) -> Picker {
    let mut p = picker(input);
    p.names_open = true;
    p.open_names = p
        .input
        .models
        .iter()
        .map(|m| m.provider.to_string())
        .collect();
    p
}

fn row(id: &str, surface: Surface) -> Target {
    Target::Row {
        row: id.to_string(),
        surface,
    }
}

fn part(part: ModelPart, surface: Surface) -> Target {
    Target::Part { part, surface }
}

/// A name field of the fixture's models, by key, under its tool.
fn field(key: &str) -> Target {
    let provider = models()
        .into_iter()
        .find(|m| m.key == key)
        .map_or("claude", |m| m.provider);
    Target::Field {
        key: key.to_string(),
        provider: provider.to_string(),
    }
}

/// A target's area in window coordinates, whether it can be used or not.
fn area(p: &Picker, t: &Target) -> Option<(Layer, Rect)> {
    p.frame().els.iter().find_map(|(layer, el)| {
        let layer = *layer;
        let (et, r) = match el {
            El::Tick { target, cell, .. } => (target.clone(), *cell),
            El::Radio { target, rect, .. }
            | El::Pick { target, rect, .. }
            | El::Field { target, rect, .. }
            | El::Button { target, rect, .. }
            | El::Toggle { target, rect, .. } => (target.clone(), *rect),
            El::Close { rect, .. } => (Target::Close, *rect),
            _ => return None,
        };
        (et == *t).then_some((layer, r))
    })
}

/// The middle of a target, scrolled into view first.
fn spot(p: &mut Picker, t: &Target) -> (f32, f32) {
    let (layer, r) = area(p, t).expect("target laid out");
    if layer == Layer::Body {
        let f = p.frame();
        if r.y < f.list_top() || bottom(&r) > bottom(&f.viewport) {
            p.scroll = f.scroll + (r.y - f.list_top()) - 8.0;
            p.settle();
        }
    }
    let (_, r) = area(p, t).unwrap();
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn click(p: &mut Picker, t: &Target) -> Vec<PickerAction> {
    let (x, y) = spot(p, t);
    let mut out = p.handle(PickerEvent::PointerDown { x, y });
    out.extend(p.handle(PickerEvent::PointerUp { x, y }));
    out
}

/// The Providers menu's Enabled tick asks the same question; Allow sends
/// only the allowance, Cancel nothing, and an allowed provider is not
/// asked at all.
#[test]
fn the_menu_can_ask_for_a_sign_in() {
    let mut p = picker(input());
    p.ask_sign_in("claude");
    assert!(p.ask.is_none(), "claude is allowed already");
    p.ask_sign_in("cursor");
    assert!(p.ask.is_some());
    assert_eq!(
        click(&mut p, &Target::Allow),
        [PickerAction::Allow(ReadKind::SignIn("cursor".into()))]
    );
    assert!(p.ask.is_none());
    // The old id of an allowed provider is allowed too.
    p.ask_sign_in("gemini");
    assert!(p.ask.is_none());
    p.ask_sign_in("cursor");
    assert!(click(&mut p, &Target::Cancel).is_empty());
    assert!(p.ask.is_none());
}

fn key(p: &mut Picker, k: PickerKey) -> Vec<PickerAction> {
    p.handle(PickerEvent::Key(k))
}

fn typed(p: &mut Picker, s: &str) -> Vec<PickerAction> {
    p.handle(PickerEvent::Text(s.to_string()))
}

fn set_row(surface: Surface, row: &str, on: bool) -> PickerAction {
    PickerAction::SetRow {
        surface,
        row: row.to_string(),
        on,
    }
}

fn rename(key: &str, name: Option<&str>) -> PickerAction {
    PickerAction::Rename {
        key: key.to_string(),
        name: name.map(String::from),
    }
}

/// Every text laid out, scrolled into sight or not.
fn texts(p: &Picker) -> String {
    p.scene_with(false).texts().join("\n")
}

fn ticked(p: &Picker, t: &Target) -> bool {
    p.frame()
        .els
        .iter()
        .any(|(_, el)| matches!(el, El::Tick { target, on: true, .. } if target == t))
}

fn field_text(p: &Picker, key: &str) -> String {
    let m = p.input.models.iter().find(|m| m.key == key).unwrap();
    p.field_text(m)
}

// ─── Rows ────────────────────────────────────────────────────

#[test]
fn a_tick_shows_or_hides_a_row_on_its_surface() {
    let mut p = picker(input());
    assert!(ticked(&p, &row("claude", Surface::Widget)));
    assert_eq!(
        click(&mut p, &row("claude", Surface::Widget)),
        [set_row(Surface::Widget, "claude", false)]
    );
    assert_eq!(
        click(&mut p, &row("claude.opus", Surface::Widget)),
        [set_row(Surface::Widget, "claude.opus", true)]
    );
    assert_eq!(
        click(&mut p, &row("claude.opus", Surface::Tooltip)),
        [set_row(Surface::Tooltip, "claude.opus", false)]
    );
    // Nothing closes.
    assert!(p.ask.is_none());
}

#[test]
fn a_press_released_elsewhere_does_nothing() {
    let mut p = picker(input());
    let (x, y) = spot(&mut p, &row("claude", Surface::Widget));
    p.handle(PickerEvent::PointerDown { x, y });
    assert!(p
        .handle(PickerEvent::PointerUp { x: x - 100.0, y })
        .is_empty());
}

#[test]
fn a_row_of_a_tool_not_yet_read_asks_first() {
    let mut p = picker(input());
    // Cursor is not in reads.sign_in.
    assert!(!ticked(&p, &row("cursor", Surface::Widget)));
    assert!(click(&mut p, &row("cursor", Surface::Widget)).is_empty());
    let ask = p.ask.clone().expect("a question");
    assert_eq!(
        ask.question,
        Question::Read(ReadKind::SignIn("cursor".into()))
    );
    // The question names the tool, where it signs in, where the
    // session is kept and goes; Cursor's own files are not read for it,
    // and the app's session is renewed, so it says that too.
    let text = texts(&p);
    assert!(text.contains("Show Cursor?"), "{text}");
    let info = crate::consent::read_info("cursor").unwrap();
    let flat = text.replace('\n', " ");
    for f in info.sign_in.iter().chain(info.hosts) {
        let first = f.split(' ').next().unwrap();
        assert!(flat.contains(first), "{f} not named");
    }
    for word in [
        "reads no file of Cursor's",
        "cursor.com",
        "ailimits/cursor_session",
        "renews it",
    ] {
        assert!(flat.contains(word), "{word}: {flat}");
    }
    assert!(!flat.contains("never changed or refreshed"), "{flat}");
    // While it is open, other ticks are inert.
    let (x, y) = {
        let (_, r) = area(&p, &row("claude", Surface::Widget)).unwrap();
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    };
    p.handle(PickerEvent::PointerDown { x, y });
    assert!(p.handle(PickerEvent::PointerUp { x, y }).is_empty());
    // Allow: the read first, then the tick.
    assert_eq!(
        click(&mut p, &Target::Allow),
        [
            PickerAction::Allow(ReadKind::SignIn("cursor".into())),
            set_row(Surface::Widget, "cursor", true),
        ]
    );
    assert!(p.ask.is_none());
    assert_eq!(p.focus, Some(row("cursor", Surface::Widget)));
}

#[test]
fn cancel_reads_nothing_and_leaves_the_tick_off() {
    let mut p = picker(input());
    click(&mut p, &row("cursor", Surface::Tooltip));
    assert!(click(&mut p, &Target::Cancel).is_empty());
    assert!(p.ask.is_none());
    assert!(!ticked(&p, &row("cursor", Surface::Tooltip)));
    // Esc cancels a question without closing the picker.
    click(&mut p, &row("cursor", Surface::Tooltip));
    assert!(key(&mut p, PickerKey::Escape).is_empty());
    assert!(p.ask.is_none());
    // Enter answers with the focused Allow.
    click(&mut p, &row("cursor", Surface::Tooltip));
    assert_eq!(
        key(&mut p, PickerKey::Enter),
        [
            PickerAction::Allow(ReadKind::SignIn("cursor".into())),
            set_row(Surface::Tooltip, "cursor", true),
        ]
    );
}

#[test]
fn rows_group_by_provider_with_notes() {
    let mut i = input();
    // Allowed but switched off in the menu: that note.
    i.reads.sign_in.push("cursor".into());
    i.enabled.retain(|p| *p != "cursor");
    i.rows
        .iter_mut()
        .find(|r| r.id == "claude.sonnet")
        .unwrap()
        .has_data = false;
    let text = texts(&picker(i));
    assert!(text.contains(OFF_IN_MENU));
    assert!(text.contains(NOT_REPORTED));
    // Codex and Copilot have one limit for everything.
    assert_eq!(text.matches(ONE_LIMIT).count(), 2);
    // An extra row shows its own part of the name.
    assert!(text.contains("\nOpus\n"));
}

// ─── Model, version, effort ──────────────────────────────────

#[test]
fn a_model_tick_asks_for_the_settings_files() {
    let mut p = picker(input());
    assert!(click(&mut p, &part(ModelPart::Model, Surface::Tooltip)).is_empty());
    assert_eq!(
        p.ask.as_ref().unwrap().question,
        Question::Read(ReadKind::ToolSettings)
    );
    let text = texts(&p).replace('\n', " ");
    // The files of the tools that may be read; Antigravity has none.
    for id in ["claude", "codex", "grok", "copilot"] {
        let s = crate::consent::read_info(id).unwrap().settings.unwrap();
        assert!(text.contains(s.split(':').next().unwrap()), "{s}");
    }
    assert_eq!(
        click(&mut p, &Target::Allow),
        [
            PickerAction::Allow(ReadKind::ToolSettings),
            PickerAction::SetPart {
                surface: Surface::Tooltip,
                part: ModelPart::Model,
                on: true
            },
        ]
    );
}

#[test]
fn version_needs_the_model_and_effort_may_go_alone() {
    let mut i = input();
    i.reads.tool_settings = true;
    let mut p = picker(i.clone());
    // Version is dimmed and inert until Model is on.
    assert!(click(&mut p, &part(ModelPart::Version, Surface::Widget)).is_empty());
    assert!(p.ask.is_none());
    let f = p.frame();
    assert!(!p
        .focusables(&f)
        .iter()
        .any(|(t, ..)| *t == part(ModelPart::Version, Surface::Widget)));
    assert_eq!(
        click(&mut p, &part(ModelPart::Effort, Surface::Widget)),
        [PickerAction::SetPart {
            surface: Surface::Widget,
            part: ModelPart::Effort,
            on: true
        }]
    );
    // Model off takes the version with it.
    i.model.widget = vec![ModelPart::Model, ModelPart::Version];
    let mut p = picker(i);
    assert!(ticked(&p, &part(ModelPart::Version, Surface::Widget)));
    assert_eq!(
        click(&mut p, &part(ModelPart::Model, Surface::Widget)),
        [
            PickerAction::SetPart {
                surface: Surface::Widget,
                part: ModelPart::Model,
                on: false
            },
            PickerAction::SetPart {
                surface: Surface::Widget,
                part: ModelPart::Version,
                on: false
            },
        ]
    );
}

#[test]
fn parts_show_as_ticked_only_when_the_settings_may_be_read() {
    let mut i = input();
    i.model.widget = vec![ModelPart::Model];
    let p = picker(i);
    assert!(!ticked(&p, &part(ModelPart::Model, Surface::Widget)));
}

#[test]
fn last_used_asks_for_the_session_tail() {
    let mut p = opened(with_model());
    assert!(click(&mut p, &Target::Source(ModelSource::LastUsed)).is_empty());
    assert_eq!(
        p.ask.as_ref().unwrap().question,
        Question::Read(ReadKind::LastSession)
    );
    assert!(texts(&p).contains("1 MB"), "{}", texts(&p));
    assert_eq!(
        click(&mut p, &Target::Allow),
        [
            PickerAction::Allow(ReadKind::LastSession),
            PickerAction::SetSource(ModelSource::LastUsed),
        ]
    );
    // Allowed already: no question; the shown one again: nothing.
    let mut i = with_model();
    i.reads.last_session = true;
    let mut p = picker(i);
    assert_eq!(
        click(&mut p, &Target::Source(ModelSource::LastUsed)),
        [PickerAction::SetSource(ModelSource::LastUsed)]
    );
    assert!(click(&mut p, &Target::Source(ModelSource::Configured)).is_empty());
}

#[test]
fn the_source_waits_for_a_part() {
    let mut p = picker(input());
    assert!(click(&mut p, &Target::Source(ModelSource::LastUsed)).is_empty());
    assert!(p.ask.is_none());
}

// ─── Model names ─────────────────────────────────────────────

/// A model can be named before Model is ticked anywhere: the list is
/// the catalogue's and reads nothing. Until Model shows, a line says
/// where the names will. With no tool running there is nothing to name.
#[test]
fn names_can_be_given_before_the_model_shows() {
    let mut nothing = input();
    nothing.reads = Reads::default();
    let text = texts(&opened(nothing));
    assert!(text.contains(NAMES_OFF));
    assert!(!text.replace('\n', " ").contains(NAMES_WHERE));
    // Effort alone, as many users have it: the fields are there, and the
    // line says to tick Model.
    let mut i = input();
    i.reads.tool_settings = true;
    i.model.widget = vec![ModelPart::Effort];
    let mut p = opened(i);
    let text = texts(&p).replace('\n', " ");
    assert!(text.contains(NAMES_ON), "{text}");
    assert!(text.contains(NAMES_WHERE), "{text}");
    assert!(area(&p, &field("claude-opus-5-5")).is_some());
    click(&mut p, &field("claude-opus-5-5"));
    assert!(p.editing());
    assert_eq!(
        typed(&mut p, "Opus"),
        [rename("claude-opus-5-5", Some("Opus"))]
    );
    // Model ticked: the line goes.
    let p = opened(with_model());
    let text = texts(&p);
    assert!(text.replace('\n', " ").contains(NAMES_ON));
    assert!(!text.replace('\n', " ").contains(NAMES_WHERE));
    // The placeholder is the automatic name; the room left shows for
    // the field being typed in only.
    assert!(text.contains("Opus 5.5  (automatic)"), "{text}");
    assert!(!text.contains("Up to about 10 characters"), "{text}");
    let mut p = p;
    click(&mut p, &field("claude-opus-5-5"));
    assert!(texts(&p).contains("Up to about 10 characters"));
}

#[test]
fn typing_renames_at_once_and_stops_at_the_limit() {
    let mut p = opened(with_model());
    let id = "claude-opus-5-5";
    click(&mut p, &field(id));
    assert!(p.editing());
    assert_eq!(typed(&mut p, "Opus"), [rename(id, Some("Opus"))]);
    assert!(texts(&p).contains("Room for about 6 more characters"));
    // Ten characters fit; the eleventh is refused.
    assert_eq!(typed(&mut p, "Maximum"), [rename(id, Some("OpusMaximu"))]);
    assert_eq!(field_text(&p, id), "OpusMaximu");
    assert!(p.edit.as_ref().unwrap().full);
    assert!(texts(&p).contains("Full: it just fits"));
    assert!(typed(&mut p, "x").is_empty());
    // Editing keys.
    assert_eq!(
        key(&mut p, PickerKey::Backspace),
        [rename(id, Some("OpusMaxim"))]
    );
    key(&mut p, PickerKey::Home);
    assert_eq!(
        key(&mut p, PickerKey::Delete),
        [rename(id, Some("pusMaxim"))]
    );
    key(&mut p, PickerKey::Right);
    assert_eq!(typed(&mut p, "é"), [rename(id, Some("péusMaxim"))]);
    key(&mut p, PickerKey::Left);
    key(&mut p, PickerKey::Left);
    assert_eq!(key(&mut p, PickerKey::Backspace), Vec::new());
    key(&mut p, PickerKey::End);
    // An empty field brings the automatic name back.
    let mut last = Vec::new();
    for _ in 0..12 {
        let out = key(&mut p, PickerKey::Backspace);
        if !out.is_empty() {
            last = out;
        }
    }
    assert_eq!(last, [rename(id, None)]);
}

#[test]
fn spaces_never_lead_double_or_trail() {
    let mut p = opened(with_model());
    let id = "claude-opus-5-5";
    click(&mut p, &field(id));
    assert!(typed(&mut p, " ").is_empty());
    assert_eq!(field_text(&p, id), "");
    assert_eq!(typed(&mut p, "Opus"), [rename(id, Some("Opus"))]);
    // A trailing space is kept while typing but never sent.
    assert!(typed(&mut p, "  ").is_empty());
    assert_eq!(field_text(&p, id), "Opus ");
    assert_eq!(typed(&mut p, "Max"), [rename(id, Some("Opus Max"))]);
    // No room for a space with nothing after it.
    typed(&mut p, "i");
    assert_eq!(field_text(&p, id), "Opus Maxi");
    assert!(typed(&mut p, " ").is_empty());
    assert_eq!(field_text(&p, id), "Opus Maxi");
    assert!(p.edit.as_ref().unwrap().full);
}

#[test]
fn a_paste_is_cut_to_what_fits() {
    let mut p = opened(with_model());
    let id = "gpt-6.1-sol";
    click(&mut p, &field(id));
    let out = p.handle(PickerEvent::Paste("  Claude\nOpus Five  ".into()));
    assert_eq!(out, [rename(id, Some("Claude Opu"))]);
    assert!(p.edit.as_ref().unwrap().full);
    // Into the middle: only what still fits goes in.
    key(&mut p, PickerKey::Home);
    let out = p.handle(PickerEvent::Paste("XYZ".into()));
    assert!(out.is_empty(), "{out:?}");
    let mut p = opened(with_model());
    click(&mut p, &field(id));
    typed(&mut p, "GPT");
    key(&mut p, PickerKey::Home);
    let out = p.handle(PickerEvent::Paste("ABCDEFGHIJ".into()));
    assert_eq!(out, [rename(id, Some("ABCDEFGGPT"))]);
    // A paste with nowhere to go is ignored.
    let mut p = opened(with_model());
    assert!(!p.editing());
    assert!(p.handle(PickerEvent::Paste("x".into())).is_empty());
}

#[test]
fn a_new_input_keeps_the_edit_focus_and_scroll() {
    let mut p = opened(with_model());
    let id = "claude-opus-5-5";
    click(&mut p, &field(id));
    typed(&mut p, "Opus ");
    let scroll = p.scroll;
    assert!(scroll > 0.0);
    // The app applied the rename and sends its new state.
    let mut i = with_model();
    i.models[0].renamed = Some("Opus".into());
    p.set_input(i);
    assert_eq!(field_text(&p, id), "Opus ");
    assert_eq!(p.edit.as_ref().unwrap().caret, 5);
    assert_eq!(p.focus, Some(field(id)));
    assert_eq!(p.scroll, scroll);
    // Leaving the field shows the stored name; Tab goes to the next
    // tool's heading, then its field.
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(Target::NamesOf("codex".into())));
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(field("gpt-6.1-sol")));
    assert_eq!(field_text(&p, id), "Opus");
    // The model it edited is gone: the edit ends.
    click(&mut p, &field(id));
    let mut i = with_model();
    i.models.remove(0);
    p.set_input(i);
    assert!(!p.editing());
    assert_eq!(p.focus, None);
}

#[test]
fn a_click_places_the_caret() {
    let mut p = opened(with_model());
    let id = "claude-opus-5-5";
    click(&mut p, &field(id));
    typed(&mut p, "Opus");
    let (_, r) = area(&p, &field(id)).unwrap();
    // mono: 6 px a character at 12 px; the second boundary is at 12.
    let x = r.x + 8.0 + 12.5;
    p.handle(PickerEvent::PointerDown { x, y: r.y + 10.0 });
    p.handle(PickerEvent::PointerUp { x, y: r.y + 10.0 });
    assert_eq!(p.edit.as_ref().unwrap().caret, 2);
    assert!(p.wants_text_cursor());
}

// ─── Closing ─────────────────────────────────────────────────

#[test]
fn escape_the_cross_and_a_click_outside_close() {
    let mut p = picker(input());
    assert_eq!(key(&mut p, PickerKey::Escape), [PickerAction::Close]);
    let mut p = picker(input());
    assert_eq!(click(&mut p, &Target::Close), [PickerAction::Close]);
    let mut p = picker(input());
    // The shadow margin is outside the box.
    assert_eq!(
        p.handle(PickerEvent::PointerDown { x: 3.0, y: 3.0 }),
        [PickerAction::Close]
    );
    let mut p = picker(input());
    assert_eq!(p.handle(PickerEvent::FocusLost), [PickerAction::Close]);
    // An open question goes with the window.
    let mut p = picker(input());
    click(&mut p, &row("cursor", Surface::Widget));
    assert_eq!(click(&mut p, &Target::Close), [PickerAction::Close]);
}

// ─── Welcome and the update notice ───────────────────────────

/// The ticks on, by target.
/// What is ticked of what can be chosen to show: not the tooltip's
/// Percent, on by default as a setting of every tooltip row.
fn ticked_targets(p: &Picker) -> Vec<Target> {
    p.frame()
        .els
        .iter()
        .filter_map(|(_, el)| match el {
            El::Tick {
                on: true, target, ..
            } if !matches!(target, Target::Tip(..)) => Some(target.clone()),
            _ => None,
        })
        .collect()
}

fn welcome(found: Vec<&'static str>) -> PickerInput {
    let mut i = input();
    i.mode = PickerMode::Welcome;
    i.reads = Reads::default();
    // A fresh install: every provider switched off, none allowed
    // (`storage::settle_reads`).
    i.enabled = Vec::new();
    i.widget_rows = Vec::new();
    i.tooltip_rows = Vec::new();
    i.taskbar_rows = Vec::new();
    i.found = found;
    i
}

/// The tools found start ticked behind one question; Allow starts each
/// of them as a ticked one would, and nothing else.
#[test]
fn the_welcome_ticks_the_tools_found_and_asks_once_for_all() {
    let mut p = picker(welcome(vec!["claude", "codex", "grok"]));
    let text = texts(&p);
    assert!(text.starts_with(TITLE_WELCOME));
    assert!(text.contains("Show the tools found on this PC?"));
    for id in ["claude", "codex", "grok"] {
        let info = crate::consent::read_info(id).unwrap();
        let flat = text.replace('\n', " ");
        assert!(flat.contains(info.tool), "{id}");
        assert!(flat.contains(info.hosts[0]), "{id}");
    }
    // Their own rows, on both surfaces; no extra row, no other tool.
    let mut on = ticked_targets(&p);
    on.sort_by_key(|t| format!("{t:?}"));
    let mut want: Vec<Target> = ["claude", "codex", "grok"]
        .iter()
        .flat_map(|id| [row(id, Surface::Widget), row(id, Surface::Tooltip)])
        .collect();
    want.sort_by_key(|t| format!("{t:?}"));
    assert_eq!(on, want);
    // It may have opened by itself: a stray Enter answers nothing.
    assert!(key(&mut p, PickerKey::Enter).is_empty());
    assert!(p.ask.is_some());
    // Tab, then Enter allows: one SignIn per tool, in the catalogue's
    // order, then their own rows on both surfaces (shown even where rows
    // were chosen before); the welcome is answered with it.
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(Target::Allow));
    let mut want: Vec<PickerAction> = ["claude", "codex", "grok"]
        .map(|id| PickerAction::Allow(ReadKind::SignIn(id.into())))
        .to_vec();
    for id in ["claude", "codex", "grok"] {
        want.push(set_row(Surface::Widget, id, true));
        want.push(set_row(Surface::Tooltip, id, true));
    }
    want.push(PickerAction::NoticeDone);
    assert_eq!(key(&mut p, PickerKey::Enter), want);
    assert!(p.ask.is_none());
}

/// A tool's question offers Allow all while other tools found on this
/// PC are not running; it names their files and starts them all.
#[test]
fn allow_all_starts_every_tool_found() {
    let mut i = welcome(vec!["claude", "codex", "grok"]);
    i.mode = PickerMode::Normal;
    let mut p = picker(i);
    click(&mut p, &row("codex", Surface::Widget));
    assert_eq!(
        p.ask.as_ref().unwrap().question,
        Question::Read(ReadKind::SignIn("codex".into()))
    );
    let text = texts(&p).replace('\n', " ");
    assert!(text.contains(ALLOW_ALL_LEAD), "{text}");
    for id in ["claude", "grok"] {
        let info = crate::consent::read_info(id).unwrap();
        assert!(text.contains(info.tool), "{id}");
        assert!(text.contains(info.hosts[0]), "{id}");
    }
    // Enter is still Allow, for this tool only.
    assert_eq!(p.focus, Some(Target::Allow));
    let mut want: Vec<PickerAction> = ["codex", "claude", "grok"]
        .map(|id| PickerAction::Allow(ReadKind::SignIn(id.into())))
        .to_vec();
    want.push(set_row(Surface::Widget, "codex", true));
    for id in ["claude", "grok"] {
        want.push(set_row(Surface::Widget, id, true));
        want.push(set_row(Surface::Tooltip, id, true));
    }
    assert_eq!(click(&mut p, &Target::AllowAll), want);
    assert!(p.ask.is_none());
    // Nothing else found: no Allow all.
    let mut alone = welcome(vec!["codex"]);
    alone.mode = PickerMode::Normal;
    let mut p = picker(alone);
    click(&mut p, &row("codex", Surface::Widget));
    assert!(area(&p, &Target::AllowAll).is_none());
    assert!(!texts(&p).contains(ALLOW_ALL_LEAD));
    assert!(p.ask.is_some());
}

/// The list of names opens and closes, and each tool's models with it.
#[test]
fn the_names_open_by_tool() {
    let mut p = picker(with_model());
    assert!(area(&p, &field("claude-opus-5-5")).is_none());
    assert!(area(&p, &Target::NamesOf("claude".into())).is_none());
    assert!(texts(&p).contains("4 models"));
    assert!(click(&mut p, &Target::Names).is_empty());
    assert!(area(&p, &Target::NamesOf("claude".into())).is_some());
    assert!(area(&p, &field("claude-opus-5-5")).is_none());
    click(&mut p, &Target::NamesOf("claude".into()));
    assert!(area(&p, &field("claude-opus-5-5")).is_some());
    assert!(area(&p, &field("gpt-6.1-sol")).is_none());
    assert!(texts(&p).contains(IN_USE));
    // Closed again, a field being edited ends with it.
    click(&mut p, &field("claude-opus-5-5"));
    click(&mut p, &Target::NamesOf("claude".into()));
    assert!(area(&p, &field("claude-opus-5-5")).is_none());
    assert!(p.edit.is_none());
    click(&mut p, &Target::Names);
    assert!(area(&p, &Target::NamesOf("claude".into())).is_none());
}

/// A description shows while the pointer rests on what it describes,
/// never over an open question.
#[test]
fn hover_shows_a_description() {
    let mut p = picker(with_model());
    let effort = |p: &Picker| {
        p.frame()
            .els
            .iter()
            .find_map(|(_, el)| match el {
                El::Hint { rect, text } if text == part_hint(ModelPart::Effort) => {
                    Some((*rect, text.clone()))
                }
                _ => None,
            })
            .expect("the Effort row has a description")
    };
    // Scrolled into view first.
    let (r, _) = effort(&p);
    let f = p.frame();
    p.scroll = (f.scroll + (r.y - f.list_top()) - 8.0).max(0.0);
    p.settle();
    let (r, text) = effort(&p);
    let f = p.frame();
    let first_word = text.split(' ').next().unwrap().to_string();
    let shown = |p: &Picker| {
        p.scene()
            .top
            .iter()
            .any(|s| matches!(s, Shape::Text { text, .. } if text.starts_with(&first_word)))
    };
    assert!(!shown(&p));
    p.handle(PickerEvent::PointerMove {
        x: r.x + 4.0,
        y: r.y + r.h / 2.0,
    });
    assert!(p.take_dirty());
    assert!(shown(&p));
    p.handle(PickerEvent::PointerLeave);
    assert!(!shown(&p));
    // Every extra row describes itself.
    let hints: Vec<String> = f
        .els
        .iter()
        .filter_map(|(_, el)| match el {
            El::Hint { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    for sub in crate::rows::SUB_ROWS {
        assert!(hints.iter().any(|h| h == sub.hint), "{}", sub.id);
    }
}

/// Cancel: nothing read, nothing ticked; each tool then asks on its own.
#[test]
fn the_welcome_starts_with_nothing_ticked_and_marks_found_tools() {
    let mut p = picker(welcome(vec!["claude", "codex", "grok"]));
    // Cancel reads nothing; it answers the welcome, which is not owed
    // again.
    assert_eq!(click(&mut p, &Target::Cancel), [PickerAction::NoticeDone]);
    let text = texts(&p);
    assert!(text.starts_with(TITLE_WELCOME));
    assert!(text.contains("Tick the tools you use."));
    assert_eq!(text.lines().filter(|l| *l == FOUND).count(), 3);
    assert!(ticked_targets(&p).is_empty());
    // No tool found: no question at all.
    let none = picker(welcome(Vec::new()));
    assert!(none.ask.is_none());
    assert!(ticked_targets(&none).is_empty());
    // Outside the welcome the tools found are never asked about.
    let mut normal = welcome(vec!["claude"]);
    normal.mode = PickerMode::Normal;
    assert!(picker(normal).ask.is_none());
    // The first tick asks for that tool's sign-in.
    click(&mut p, &row("codex", Surface::Widget));
    assert_eq!(
        p.ask.as_ref().unwrap().question,
        Question::Read(ReadKind::SignIn("codex".into()))
    );
    assert!(click(&mut p, &Target::Cancel).is_empty());
    // Answered already: closing owes nothing more.
    assert_eq!(key(&mut p, PickerKey::Escape), [PickerAction::Close]);
    // Closed without an answer, the welcome is answered by that, once.
    let mut p = picker(welcome(Vec::new()));
    assert_eq!(
        key(&mut p, PickerKey::Escape),
        [PickerAction::NoticeDone, PickerAction::Close]
    );
    assert_eq!(key(&mut p, PickerKey::Escape), [PickerAction::Close]);
}

#[test]
fn the_welcome_stays_the_welcome_after_the_first_allow() {
    // The intro and the marks stay while it is open, so the rows do not
    // jump under the cursor.
    let mut i = input();
    i.mode = PickerMode::Welcome;
    i.reads.sign_in = vec!["codex".into()];
    assert!(texts(&picker(i)).starts_with(TITLE_WELCOME));
}

fn notice() -> PickerInput {
    let mut i = input();
    i.mode = PickerMode::UpdateNotice;
    i.reads.sign_in = vec!["claude".into(), "copilot".into(), "gemini".into()];
    i
}

#[test]
fn the_update_notice_names_every_file_and_host() {
    let p = picker(notice());
    let text = texts(&p).replace('\n', " ");
    assert!(text.starts_with(TITLE_NOTICE));
    for id in ["claude", "copilot", "antigravity"] {
        let info = crate::consent::read_info(id).unwrap();
        assert!(text.contains(info.tool), "{}", info.tool);
        for s in info.sign_in.iter().chain(info.hosts) {
            for word in s.split(' ').filter(|w| w.len() > 3) {
                assert!(text.contains(word.trim_end_matches(')')), "{word} of {s}");
            }
        }
    }
    // Only what is read.
    assert!(!text.contains("Codex CLI"));
}

#[test]
fn ok_closes_the_notice_and_review_opens_the_picker() {
    let mut p = picker(notice());
    assert!(key(&mut p, PickerKey::Enter).is_empty());
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(Target::NoticeOk));
    assert_eq!(
        key(&mut p, PickerKey::Enter),
        [PickerAction::NoticeDone, PickerAction::Close]
    );
    let mut p = picker(notice());
    assert_eq!(
        click(&mut p, &Target::NoticeReview),
        [PickerAction::NoticeDone]
    );
    assert!(texts(&p).starts_with(TITLE_ROWS));
    // A late snapshot still saying UpdateNotice keeps the picker.
    p.set_input(notice());
    assert!(texts(&p).starts_with(TITLE_ROWS));
    assert_eq!(key(&mut p, PickerKey::Escape), [PickerAction::Close]);
    // The cross on the notice answers it too.
    let mut p = picker(notice());
    assert_eq!(
        click(&mut p, &Target::Close),
        [PickerAction::NoticeDone, PickerAction::Close]
    );
}

// ─── Keyboard and scrolling ──────────────────────────────────

#[test]
fn the_keyboard_walks_the_grid() {
    let mut p = picker(input());
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(Target::Close));
    assert!(p.focus_visible);
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(row("claude", Surface::Widget)));
    key(&mut p, PickerKey::Right);
    assert_eq!(p.focus, Some(row("claude", Surface::Tooltip)));
    key(&mut p, PickerKey::Down);
    assert_eq!(p.focus, Some(row("claude.opus", Surface::Tooltip)));
    assert_eq!(
        key(&mut p, PickerKey::Space),
        [set_row(Surface::Tooltip, "claude.opus", false)]
    );
    key(&mut p, PickerKey::Left);
    key(&mut p, PickerKey::Up);
    assert_eq!(p.focus, Some(row("claude", Surface::Widget)));
    key(&mut p, PickerKey::BackTab);
    assert_eq!(p.focus, Some(Target::Close));
    // End goes to the last control, scrolled into view: with no model
    // part on, the source waits and the list of names is closed, so the
    // last is its heading.
    key(&mut p, PickerKey::End);
    assert_eq!(p.focus, Some(Target::Names));
    assert!(p.scroll > 0.0);
}

#[test]
fn the_list_scrolls_within_the_work_area() {
    let mut p = opened(with_model());
    p.set_max_height(300.0);
    let (_, h) = p.window_size();
    assert_eq!(h, 300.0 + MARGIN * 2.0);
    p.handle(PickerEvent::Wheel { dy: -48.0 });
    assert_eq!(p.scroll, 48.0);
    p.handle(PickerEvent::Wheel { dy: 480.0 });
    assert_eq!(p.scroll, 0.0);
    p.handle(PickerEvent::Wheel { dy: -100_000.0 });
    let f = p.frame();
    assert_eq!(p.scroll, f.max_scroll());
    assert!(f.max_scroll() > 0.0);
    // The column heads stay at the top.
    let s = f.sticky.clone().expect("pinned heads");
    assert!(s.pinned);
    assert_eq!(s.rect.y, f.viewport.y);
    // Tab from the top reaches a field far below and reveals it.
    p.scroll = 0.0;
    p.focus = None;
    while p.focus != Some(field("claude-opus-5-5")) {
        key(&mut p, PickerKey::Tab);
    }
    let f = p.frame();
    let (_, r) = area(&p, &field("claude-opus-5-5")).unwrap();
    assert!(
        r.y >= f.list_top() && bottom(&r) <= bottom(&f.viewport),
        "{r:?}"
    );
}

#[test]
fn hover_repaints_only_when_it_changes() {
    let mut p = picker(input());
    p.take_dirty();
    let (x, y) = spot(&mut p, &row("claude", Surface::Widget));
    p.take_dirty();
    p.handle(PickerEvent::PointerMove { x, y });
    assert!(p.take_dirty());
    p.handle(PickerEvent::PointerMove { x: x + 1.0, y });
    assert!(!p.take_dirty());
    p.handle(PickerEvent::PointerLeave);
    assert!(p.take_dirty());
}

// ─── Placement ───────────────────────────────────────────────

const WORK: (i32, i32, i32, i32) = (0, 0, 1920, 1032);

fn at(x: i32, y: i32, w: i32, h: i32) -> ScreenRect {
    ScreenRect { x, y, w, h }
}

#[test]
fn the_box_goes_beside_the_anchor_inside_the_work_area() {
    let size = (404, 584);
    // Right of a widget on the left, top-aligned; the margin is outside.
    assert_eq!(
        place(at(100, 100, 278, 200), size, 12, 8, WORK),
        (100 + 278 + 8 - 12, 88)
    );
    // Against the right edge: to its left.
    assert_eq!(
        place(at(1600, 100, 278, 200), size, 12, 8, WORK),
        (1600 - 8 - 380 - 12, 88)
    );
    // Low on the screen: slid up into the work area.
    let (_, y) = place(at(100, 900, 278, 100), size, 12, 8, WORK);
    assert_eq!(y + 12 + 560, 1032);
    // The tray icon in the corner: left of it, bottom held in.
    let (x, y) = place(at(1840, 1040, 24, 40), size, 12, 8, WORK);
    assert_eq!((x + 12 + 380, y + 12 + 560), (1840 - 8, 1032));
    // A widget as wide as the screen: below it.
    assert_eq!(
        place(at(0, 50, 1920, 100), size, 12, 8, WORK),
        (-12, 50 + 100 + 8 - 12)
    );
    // No room anywhere: held inside all the same.
    let (x, y) = place(at(10, 10, 1900, 1000), size, 12, 8, WORK);
    assert!(x + 12 >= 0 && x + 12 + 380 <= 1920 && y + 12 >= 0 && y + 12 + 560 <= 1032);
}

#[test]
fn room_text_reads_well() {
    assert_eq!(
        room_text(1, false),
        "Fits every size. Room for about 1 more character."
    );
    assert_eq!(
        room_text(3, false),
        "Fits every size. Room for about 3 more characters."
    );
    assert!(room_text(0, false).starts_with("Full"));
    assert!(room_text(10, true).starts_with("Up to about 10 characters"));
}

#[test]
fn wrapping_breaks_long_paths() {
    let p = picker(input());
    let lines = p.wrap(
        &[span(
            r"%USERPROFILE%\.claude\projects\...\*.jsonl (the end of the newest)",
            SIZE_NOTE,
            Ink::Text,
            false,
        )],
        150.0,
    );
    assert!(lines.len() > 1);
    for l in &lines {
        let end = l.last().map_or(0.0, |r| r.x + mono(&r.text, r.size));
        assert!(
            end <= 150.0 + 0.01,
            "{:?}",
            l.iter().map(|r| &r.text).collect::<Vec<_>>()
        );
    }
    let joined: String = lines
        .iter()
        .flatten()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    assert!(joined.contains(r"\.claude\"));
}

/// What input costs with the real font: a pointer move (hover, cursor
/// shape) and a repaint at 150%. `cargo test --release picker_input_cost
/// -- --ignored --nocapture`.
#[test]
#[ignore]
fn picker_input_cost() {
    let renderer = Rc::new(Renderer::new().expect("a system font"));
    let r = renderer.clone();
    let mut p = Picker::new(with_model(), move |t, s| r.text_width(t, s));
    let n = 200;
    let start = std::time::Instant::now();
    for i in 0..n {
        p.handle(PickerEvent::PointerMove {
            x: 100.0 + (i % 50) as f32,
            y: 100.0 + (i % 300) as f32,
        });
        p.wants_text_cursor();
    }
    let per_move = start.elapsed() / n;
    let (w, h) = p.window_size();
    let mut pm = tiny_skia::Pixmap::new((w * 1.5).ceil() as u32, (h * 1.5).ceil() as u32).unwrap();
    let mut glyphs = PickerGlyphs::default();
    let start = std::time::Instant::now();
    renderer.draw_picker(&mut pm, &p.scene(), &PickerTheme::dark(), 1.5, &mut glyphs);
    let first = start.elapsed();
    let start = std::time::Instant::now();
    for _ in 0..20 {
        renderer.draw_picker(&mut pm, &p.scene(), &PickerTheme::dark(), 1.5, &mut glyphs);
    }
    let per_paint = start.elapsed() / 20;
    println!("pointer move {per_move:?}; at 150%: first paint {first:?}, repaint {per_paint:?}");
}

// ─── The welcome's question, answers and the window ──────────

fn welcome_weak(found: Vec<&'static str>, weak: Vec<&'static str>) -> PickerInput {
    let mut i = welcome(found);
    i.found_weak = weak;
    i
}

fn pick(id: &str) -> Target {
    Target::Pick(id.to_string())
}

fn allow(id: &str) -> PickerAction {
    PickerAction::Allow(ReadKind::SignIn(id.to_string()))
}

/// Each tool found has a tick of its own in the welcome's question,
/// from the mouse and from the keyboard: Allow starts only the tools
/// ticked, and with none ticked it cannot be used.
#[test]
fn the_welcome_question_has_a_tick_per_tool() {
    let mut p = picker(welcome(vec!["claude", "codex", "grok"]));
    for id in ["claude", "codex", "grok"] {
        assert!(p.picked(id), "{id}");
        assert!(area(&p, &pick(id)).is_some(), "{id}");
    }
    // The keyboard: Tab to Allow, Shift+Tab up into the ticks, Space.
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(Target::Allow));
    key(&mut p, PickerKey::BackTab);
    assert_eq!(p.focus, Some(pick("grok")));
    assert!(key(&mut p, PickerKey::Space).is_empty());
    assert!(!p.picked("grok"));
    // The mouse: the whole entry, its row behind it following.
    assert!(click(&mut p, &pick("codex")).is_empty());
    assert!(!p.picked("codex"));
    assert!(!ticked(&p, &row("codex", Surface::Widget)));
    assert!(ticked(&p, &row("claude", Surface::Widget)));
    // Allow: Claude alone, and the welcome is answered.
    assert_eq!(
        click(&mut p, &Target::Allow),
        [
            allow("claude"),
            set_row(Surface::Widget, "claude", true),
            set_row(Surface::Tooltip, "claude", true),
            PickerAction::NoticeDone,
        ]
    );
    // Nothing ticked: Allow cannot be used, nor focused.
    let mut p = picker(welcome(vec!["claude"]));
    click(&mut p, &pick("claude"));
    assert!(click(&mut p, &Target::Allow).is_empty());
    assert!(p.ask.is_some());
    let f = p.frame();
    assert!(!p.focusables(&f).iter().any(|(t, ..)| *t == Target::Allow));
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(Target::Cancel));
}

/// A tool found only by a related tool's folder is marked but never
/// ticked for the user: in the welcome's question it starts unticked
/// and says why; alone, nothing is asked; Allow all never starts it.
#[test]
fn a_tool_found_by_a_related_folder_starts_unticked() {
    let found = vec!["claude", "copilot", "antigravity"];
    let mut p = picker(welcome_weak(found, vec!["copilot", "antigravity"]));
    assert!(p.picked("claude") && !p.picked("copilot") && !p.picked("antigravity"));
    assert!(!ticked(&p, &row("copilot", Surface::Widget)));
    let text = texts(&p).replace('\n', " ");
    assert!(text.contains("Only GitHub CLI was found"), "{text}");
    assert!(text.contains("Antigravity may not be installed"), "{text}");
    // Ticked by hand, it starts too.
    click(&mut p, &pick("copilot"));
    let out = click(&mut p, &Target::Allow);
    assert!(out.contains(&allow("copilot")));
    assert!(!out.contains(&allow("antigravity")));
    // Only weak finds: no question; the mark's description says why.
    let p = picker(welcome_weak(vec!["copilot"], vec!["copilot"]));
    assert!(p.ask.is_none());
    let hints: Vec<String> = p
        .frame()
        .els
        .iter()
        .filter_map(|(_, el)| match el {
            El::Hint { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(hints
        .iter()
        .any(|h| h == crate::consent::found_hint("copilot", false)));
    // Allow all leaves it out.
    let mut i = welcome_weak(vec!["claude", "copilot"], vec!["copilot"]);
    i.mode = PickerMode::Normal;
    let mut p = picker(i);
    click(&mut p, &row("claude", Surface::Widget));
    assert!(p.ask.is_some());
    assert!(area(&p, &Target::AllowAll).is_none());
}

/// Allow all starts every other tool found that was never allowed, also
/// one switched off in the Providers menu (a fresh install and an update
/// leave every unused provider off), and switches it on; a tool the user
/// allowed and then switched off keeps its consent and is left alone.
/// Once the welcome's question was answered, Allow all is not offered
/// while the window stays open.
#[test]
fn allow_all_starts_the_tools_never_allowed_and_leaves_the_rest() {
    let mut i = welcome(vec!["claude", "codex", "grok"]);
    i.mode = PickerMode::Normal;
    // As after an update: Claude and Codex were in use, the rest is off.
    i.enabled = vec!["claude", "codex"];
    i.reads.sign_in = vec!["claude".into(), "codex".into()];
    // Nothing else found that is not allowed yet: nothing more to offer.
    i.found = vec!["claude", "codex"];
    let mut p = picker(i.clone());
    click(&mut p, &row("cursor", Surface::Widget));
    assert!(p.ask.is_some());
    assert!(area(&p, &Target::AllowAll).is_none());
    i.found = vec!["claude", "codex", "grok", "cursor"];
    let mut p = picker(i.clone());
    click(&mut p, &row("cursor", Surface::Widget));
    assert!(texts(&p).contains(ALLOW_ALL_LEAD));
    let out = click(&mut p, &Target::AllowAll);
    assert!(out.contains(&allow("cursor")));
    assert!(out.contains(&allow("grok")));
    assert!(!out.contains(&allow("claude")) && !out.contains(&allow("codex")));
    assert!(out.contains(&set_row(Surface::Widget, "grok", true)));
    // Allowed and switched off: the menu says so, not Allow all.
    i.reads.sign_in.push("grok".into());
    let mut p = picker(i);
    click(&mut p, &row("cursor", Surface::Widget));
    assert!(area(&p, &Target::AllowAll).is_none());
    // The welcome asks about every tool found that is not allowed, though
    // a fresh install has them all switched off.
    let w = welcome(vec!["claude", "grok"]);
    assert!(w.enabled.is_empty());
    assert_eq!(picker(w).ask.unwrap().tools, ["claude", "grok"]);
    // Cancel in the welcome: no Allow all after it.
    let mut p = picker(welcome(vec!["claude", "codex", "grok"]));
    click(&mut p, &Target::Cancel);
    click(&mut p, &row("codex", Surface::Widget));
    assert!(p.ask.is_some());
    assert!(area(&p, &Target::AllowAll).is_none());
    assert!(!texts(&p).contains(ALLOW_ALL_LEAD));
}

/// The welcome a real fresh install opens with: built from
/// `Config::fresh_install()` as the app builds it, not from a hand-made
/// input, it asks about the tools found.
#[test]
fn a_fresh_install_asks_about_the_tools_found() {
    let config = crate::config::schema::Config::fresh_install();
    let input = crate::choices::picker_input_with(
        &config,
        &[],
        &crate::models::Models::default(),
        PickerMode::Welcome,
        &["claude", "codex", "grok"],
        |id| id != "grok",
    );
    let p = picker(input);
    let ask = p.ask.as_ref().expect("the welcome asks");
    assert_eq!(ask.tools, ["claude", "codex", "grok"]);
    assert_eq!(ask.picked, ["claude", "codex"]);
}

// ─── The tooltip's percent and reset time ────────────────────

/// Percent and Reset time have a tick in the Tooltip column and one in
/// the Taskbar column, none for the widget, which always shows both. The
/// tooltip starts with both, the panel with the percentage alone; the last
/// one on in a column cannot be unticked.
#[test]
fn the_tooltip_and_the_panel_choose_their_percent_and_reset_time() {
    use crate::config::schema::{NumbersOn, TipPart, UiNumbers};
    let tip = |part| Target::Tip(part, NumbersOn::Tooltip);
    let panel = |part| Target::Tip(part, NumbersOn::Panel);
    let set = |part, place, on| PickerAction::SetTip { part, place, on };
    let mut p = picker(input());
    let text = texts(&p);
    assert!(
        text.contains("Percent") && text.contains("Reset time"),
        "{text}"
    );
    assert!(ticked(&p, &tip(TipPart::Percent)) && ticked(&p, &tip(TipPart::Reset)));
    assert!(ticked(&p, &panel(TipPart::Percent)) && !ticked(&p, &panel(TipPart::Reset)));
    assert!(!usable(&p, &panel(TipPart::Percent)));
    assert_eq!(
        click(&mut p, &tip(TipPart::Reset)),
        [set(TipPart::Reset, NumbersOn::Tooltip, false)]
    );
    assert_eq!(
        click(&mut p, &panel(TipPart::Reset)),
        [set(TipPart::Reset, NumbersOn::Panel, true)]
    );
    let mut i = input();
    i.tip = UiNumbers::PERCENT_ONLY;
    let mut p = picker(i);
    assert!(!usable(&p, &tip(TipPart::Percent)));
    assert!(click(&mut p, &tip(TipPart::Percent)).is_empty());
    // The ticks sit in their columns.
    let (_, t) = area(&p, &tip(TipPart::Reset)).unwrap();
    let (_, effort) = area(&p, &part(ModelPart::Effort, Surface::Tooltip)).unwrap();
    assert_eq!(t.x, effort.x);
    let (_, k) = area(&p, &panel(TipPart::Reset)).unwrap();
    let (_, claude) = area(&p, &Target::Taskbar("claude".into())).unwrap();
    assert_eq!(k.x, claude.x);
}

// ─── Taskbar ─────────────────────────────────────────────────

fn taskbar(id: &str) -> Target {
    Target::Taskbar(id.to_string())
}

fn set_taskbar(id: &str, on: bool) -> PickerAction {
    PickerAction::SetTaskbar {
        provider: id.to_string(),
        on,
    }
}

/// Whether a tick is laid out and can be used.
fn usable(p: &Picker, t: &Target) -> bool {
    p.frame()
        .els
        .iter()
        .any(|(_, el)| matches!(el, El::Tick { target, enabled: true, .. } if target == t))
}

/// The Taskbar column: a tick on each provider's own row (none on its
/// extra rows), the first two that run ticked until the user chooses,
/// room for two, and never none.
#[test]
fn the_taskbar_column_holds_two_providers() {
    let mut p = picker(input());
    assert!(texts(&p).contains(TASKBAR));
    assert!(ticked(&p, &taskbar("claude")) && ticked(&p, &taskbar("codex")));
    assert!(area(&p, &taskbar("claude.opus")).is_none());
    // Two ticked: a third waits, and says why on hover.
    assert!(!usable(&p, &taskbar("grok")));
    assert!(click(&mut p, &taskbar("grok")).is_empty());
    let (x, y) = spot(&mut p, &taskbar("grok"));
    p.handle(PickerEvent::PointerMove { x, y });
    let shown = texts(&p).replace('\n', " ");
    assert!(shown.contains(TASKBAR_FULL), "{shown}");
    // Untick one, then the other tool can be ticked.
    assert_eq!(
        click(&mut p, &taskbar("codex")),
        [set_taskbar("codex", false)]
    );
    let mut i = input();
    i.taskbar_rows = vec!["claude".into()];
    let mut p = picker(i);
    assert_eq!(click(&mut p, &taskbar("grok")), [set_taskbar("grok", true)]);
    // The last one stays: nothing would mean the default again.
    assert!(!usable(&p, &taskbar("claude")));
    assert!(click(&mut p, &taskbar("claude")).is_empty());
    // A tool not allowed yet asks first; Allow starts it on the taskbar.
    let mut i = input();
    i.taskbar_rows = vec!["claude".into()];
    let mut p = picker(i);
    assert!(click(&mut p, &taskbar("cursor")).is_empty());
    assert!(p.ask.is_some());
    assert_eq!(
        click(&mut p, &Target::Allow),
        [allow("cursor"), set_taskbar("cursor", true)]
    );
}

/// The welcome and the notice may open by themselves while the user
/// types elsewhere: nothing has the focus, so a stray Enter or Space
/// answers nothing. A question the user's own tick opened has Allow
/// ready.
#[test]
fn a_window_that_opened_by_itself_takes_no_stray_key() {
    for input in [welcome(vec!["claude", "codex"]), notice()] {
        let mut p = picker(input);
        assert_eq!(p.focus, None);
        assert!(key(&mut p, PickerKey::Enter).is_empty());
        assert!(key(&mut p, PickerKey::Space).is_empty());
        assert!(!p.notice_done);
    }
    let mut p = picker(welcome(vec!["claude"]));
    assert!(p.ask.is_some());
    // An arrow key brings the focus to the default button first.
    key(&mut p, PickerKey::Down);
    assert_eq!(p.focus, Some(Target::Allow));
    let mut p = picker(input());
    click(&mut p, &row("cursor", Surface::Widget));
    assert_eq!(p.focus, Some(Target::Allow));
}

/// Losing the focus to another window, or a click beside the box,
/// closes the window but answers nothing: the welcome and the notice
/// are owed still. The cross, Esc, Alt+F4 and a button are answers.
#[test]
fn losing_the_focus_answers_nothing() {
    for input in [welcome(vec!["claude"]), welcome(Vec::new()), notice()] {
        let mut p = picker(input.clone());
        assert_eq!(p.handle(PickerEvent::FocusLost), [PickerAction::Close]);
        let mut p = picker(input.clone());
        assert_eq!(
            p.handle(PickerEvent::PointerDown { x: 3.0, y: 3.0 }),
            [PickerAction::Close]
        );
        let mut p = picker(input);
        assert_eq!(
            p.handle(PickerEvent::CloseRequested),
            [PickerAction::NoticeDone, PickerAction::Close]
        );
    }
    // The Providers menu's question in the welcome: its Allow goes alone
    // (the app shows the provider's rows for it), the welcome's answer
    // with the close.
    let mut p = picker(welcome(Vec::new()));
    p.ask_sign_in("codex");
    assert_eq!(click(&mut p, &Target::Allow), [allow("codex")]);
    assert_eq!(
        p.handle(PickerEvent::FocusLost),
        [PickerAction::NoticeDone, PickerAction::Close]
    );
}

/// Typing a name, the focus may go to a shell panel (Win+V, Win+.)
/// that types into the field: the window stays and the edit goes on.
#[test]
fn a_name_being_typed_keeps_the_window_when_the_focus_goes() {
    let mut p = opened(with_model());
    let id = "claude-opus-5-5";
    click(&mut p, &field(id));
    typed(&mut p, "Op");
    assert!(p.handle(PickerEvent::FocusLost).is_empty());
    assert!(p.editing());
    assert_eq!(typed(&mut p, "us"), [rename(id, Some("Opus"))]);
    // Not typing: the window goes.
    key(&mut p, PickerKey::Tab);
    assert!(!p.editing());
    assert_eq!(p.handle(PickerEvent::FocusLost), [PickerAction::Close]);
}

/// Esc in a name field takes the edit back to the name it had; a second
/// Esc, with nothing to take back, closes.
#[test]
fn escape_in_a_name_field_takes_the_edit_back() {
    let mut i = with_model();
    i.models[0].renamed = Some("Mine".into());
    let mut p = opened(i);
    let id = "claude-opus-5-5";
    click(&mut p, &field(id));
    assert_eq!(typed(&mut p, "X"), [rename(id, Some("MineX"))]);
    assert_eq!(key(&mut p, PickerKey::Escape), [rename(id, Some("Mine"))]);
    assert_eq!(field_text(&p, id), "Mine");
    assert!(p.editing());
    assert_eq!(key(&mut p, PickerKey::Escape), [PickerAction::Close]);
}

/// The page keys and the scroll bar move the list; the thumb drags and
/// a press beside it pages.
#[test]
fn page_keys_and_the_scroll_bar_move_the_list() {
    let mut p = opened(with_model());
    p.set_max_height(300.0);
    key(&mut p, PickerKey::PageDown);
    let f = p.frame();
    assert!(
        (p.scroll - (f.viewport.h - ROW_H)).abs() < 1.0,
        "{}",
        p.scroll
    );
    key(&mut p, PickerKey::PageUp);
    assert_eq!(p.scroll, 0.0);
    let b = p.frame().bar.expect("a scroll bar");
    let (x, y) = (b.thumb.x + 1.0, b.thumb.y + 2.0);
    p.handle(PickerEvent::PointerDown { x, y });
    p.handle(PickerEvent::PointerMove { x, y: y + 10_000.0 });
    p.handle(PickerEvent::PointerUp { x, y: y + 10_000.0 });
    assert_eq!(p.scroll, p.frame().max_scroll());
    let before = p.scroll;
    let b = p.frame().bar.unwrap();
    p.handle(PointerDown {
        x,
        y: b.grip.y + 2.0,
    });
    p.handle(PointerUp {
        x,
        y: b.grip.y + 2.0,
    });
    assert!(p.scroll < before);
    // Nothing was pressed or ticked on the way.
    assert!(p.pressed.is_none());
}

use PickerEvent::{PointerDown, PointerUp};

/// What the pointer shows on hover, the keyboard shows on focus: the
/// description of the row the focus ring is on.
#[test]
fn the_keyboard_shows_a_description_too() {
    let mut p = picker(with_model());
    let fable = row("claude.fable", Surface::Tooltip);
    for _ in 0..40 {
        if p.focus == Some(fable.clone()) {
            break;
        }
        key(&mut p, PickerKey::Tab);
    }
    assert_eq!(p.focus, Some(fable));
    let hint = crate::rows::SUB_ROWS
        .iter()
        .find(|s| s.id == "claude.fable")
        .unwrap()
        .hint;
    let first = hint.split(' ').next().unwrap();
    let shown = |p: &Picker| {
        p.scene()
            .top
            .iter()
            .any(|s| matches!(s, Shape::Text { text, .. } if text.starts_with(first)))
    };
    assert!(shown(&p));
    // A row's description stops short of the tick columns.
    let f = p.frame();
    let right = p
        .scene()
        .top
        .iter()
        .filter_map(|s| match s {
            Shape::Fill { rect, .. } => Some(rect.x + rect.w),
            _ => None,
        })
        .fold(0.0, f32::max);
    assert!(right <= f.boxr.x + LABEL_RIGHT + 6.5, "{right}");
    // The mouse takes over: a click hides the ring and the description.
    let (x, y) = spot(&mut p, &row("claude", Surface::Widget));
    p.handle(PointerDown { x, y });
    p.handle(PointerUp { x, y });
    p.handle(PickerEvent::PointerLeave);
    assert!(!shown(&p));
}

/// An extra row is said to be missing only once its provider answered:
/// not while it loads, so the rows do not grow and shrink after Allow.
#[test]
fn a_row_is_called_missing_only_once_its_provider_answered() {
    let mut i = input();
    let sonnet = |i: &mut PickerInput| {
        i.rows
            .iter_mut()
            .find(|r| r.id == "claude.sonnet")
            .unwrap()
            .clone()
    };
    for r in i.rows.iter_mut().filter(|r| r.id == "claude.sonnet") {
        r.has_data = false;
        r.provider_ok = false;
    }
    assert!(!sonnet(&mut i).provider_ok);
    assert!(!texts(&picker(i.clone())).contains(NOT_REPORTED));
    for r in i.rows.iter_mut().filter(|r| r.id == "claude.sonnet") {
        r.provider_ok = true;
    }
    assert!(texts(&picker(i)).contains(NOT_REPORTED));
}

/// A long automatic name (a cloud id) is cut to its field with an
/// ellipsis, and so is the id above it; one model is "1 model".
#[test]
fn long_names_are_cut_and_counts_read_well() {
    let mut i = with_model();
    i.models[0].automatic =
        "Arn Aws Bedrock Us East 1 Inference Profile Anthropic Claude Opus".into();
    i.models[0].model_id = "arn:aws:bedrock:us-east-1:123456789012:".repeat(5);
    let p = opened(i);
    let (_, r) = area(&p, &field("claude-opus-5-5")).unwrap();
    let placeholder = p
        .scene_with(false)
        .body
        .iter()
        .find_map(|s| match s {
            Shape::Text {
                text, italic: true, ..
            } => Some(text.clone()),
            _ => None,
        })
        .expect("a placeholder");
    assert!(placeholder.ends_with("…  (automatic)"), "{placeholder}");
    assert!(mono(&placeholder, SIZE_TEXT) <= r.w - 16.0, "{placeholder}");
    assert!(texts(&p).contains("…"));
    let mut i = with_model();
    i.models.truncate(1);
    let text = texts(&picker(i));
    assert!(text.lines().any(|l| l == "1 model"), "{text}");
}

/// A tool started from Allow all or the welcome's question is read for
/// the model too when that is allowed for every tool already: its entry
/// names those files as well.
#[test]
fn a_tool_started_later_names_the_reads_already_allowed() {
    let mut i = welcome(vec!["claude", "codex"]);
    i.reads.tool_settings = true;
    let text = texts(&picker(i)).replace('\n', " ");
    assert!(text.contains(r"\.claude\settings.json"), "{text}");
    assert!(text.contains(r"\.codex\config.toml"), "{text}");
    // Not allowed: not named.
    let text = texts(&picker(welcome(vec!["claude"]))).replace('\n', " ");
    assert!(!text.contains("settings.json"), "{text}");
}

/// A model two tools share has its field under each of them: one name,
/// typed under one and shown under the other at once.
#[test]
fn a_shared_model_has_its_field_under_each_tool() {
    let mut i = with_model();
    for (provider, current, other) in [("codex", false, "Copilot"), ("copilot", true, "Codex")] {
        i.models.push(ModelChoice {
            provider,
            model_id: "gpt-5.6-sol".into(),
            key: "gpt-5.6-sol".into(),
            automatic: "GPT-5.6 Sol".into(),
            renamed: None,
            current,
            shared_with: vec![other],
        });
    }
    let mut p = opened(i);
    let under = |p: &str| Target::Field {
        key: "gpt-5.6-sol".into(),
        provider: p.into(),
    };
    assert!(area(&p, &under("codex")).is_some());
    assert!(area(&p, &under("copilot")).is_some());
    assert!(texts(&p).contains("shared with Copilot"));
    click(&mut p, &under("codex"));
    assert_eq!(typed(&mut p, "Sol"), [rename("gpt-5.6-sol", Some("Sol"))]);
    let theirs = p
        .input
        .models
        .iter()
        .find(|m| m.provider == "copilot" && m.key == "gpt-5.6-sol")
        .unwrap();
    assert_eq!(p.field_text(theirs), "Sol");
    // The other field takes the edit over, caret and all.
    click(&mut p, &under("copilot"));
    assert_eq!(p.focus, Some(under("copilot")));
    assert_eq!(p.edit.as_ref().unwrap().text, "Sol");
    assert!(p.caret_spot().is_some());
}

#[test]
fn the_window_is_named_as_it_opened() {
    assert_eq!(picker(input()).title(), TITLE_ROWS);
    assert_eq!(picker(welcome(Vec::new())).title(), TITLE_WELCOME);
    assert_eq!(picker(notice()).title(), TITLE_NOTICE);
    // The welcome keeps its name once answered and sent as Normal.
    let mut p = picker(welcome(vec!["claude"]));
    click(&mut p, &Target::Cancel);
    p.set_input(input());
    assert_eq!(p.title(), TITLE_WELCOME);
}

/// High contrast's colours are used as they are: no shadow, no fading
/// of text, the contrast scheme's own highlight.
#[test]
fn high_contrast_uses_its_own_colours() {
    let c = crate::ui::theme::ContrastColors::default();
    let t = PickerTheme::high_contrast(&c);
    assert_eq!((t.text.r, t.text.a), (255, 255));
    assert_eq!(t.muted, t.text);
    assert_eq!(t.levels.placeholder, 1.0);
    assert!(t.shadow.iter().all(|(_, a)| *a == 0));
    assert_eq!((t.accent.r, t.accent.g, t.accent.b), (26, 235, 255));
    #[cfg(target_os = "windows")]
    {
        assert_eq!(system::parse_rgb("0 255 12"), Some([0, 255, 12]));
        assert_eq!(system::parse_rgb("0 255"), None);
        assert_eq!(system::parse_rgb("0 256 1"), None);
        assert_eq!(system::parse_rgb("1 2 3 4"), None);
    }
}

/// With the real font, the welcome's question and Allow all fit every
/// work area: from 300 to 560 pixels tall, and as tall as the common
/// screens allow at 125 to 200 per cent. The buttons stay inside the
/// question and the box, where they are drawn; the text above them
/// scrolls to its last line.
#[test]
fn the_question_keeps_its_buttons_in_any_height() {
    let renderer = Rc::new(Renderer::new().expect("a system font"));
    let make = |input: PickerInput| {
        let r = renderer.clone();
        Picker::new(input, move |t, s| r.text_width(t, s))
    };
    let tools = [
        "claude",
        "codex",
        "copilot",
        "antigravity",
        "grok",
        "cursor",
    ];
    let mut heights: Vec<(f32, f32)> = [300.0, 350.0, 400.0, 450.0, 500.0, 560.0]
        .iter()
        .map(|h| (*h, 1.0))
        .collect();
    for (w, h) in [
        (1920, 1032),
        (1600, 860),
        (1366, 728),
        (1280, 760),
        (1280, 680),
    ] {
        for scale in [1.25, 1.5, 1.75, 2.0] {
            heights.push((max_box_height((0, 0, w, h), scale), scale as f32));
        }
    }
    for n in 3..=6 {
        let found = tools[..n].to_vec();
        let mut welcome_p = make(welcome(found.clone()));
        let mut i = welcome(found);
        i.mode = PickerMode::Normal;
        let mut allow_all = make(i);
        click(&mut allow_all, &row("codex", Surface::Widget));
        assert!(area(&allow_all, &Target::AllowAll).is_some());
        for (k, &(h, scale)) in heights.iter().enumerate() {
            // Drawn too for the longest questions: at 300 px, and on a
            // 1080p screen at each scale.
            let draw = n == 6 && (k == 0 || (6..10).contains(&k));
            for (p, what) in [(&mut welcome_p, "welcome"), (&mut allow_all, "allow all")] {
                p.set_max_height(h);
                let what = format!("{what}, {n} tools, {h} px");
                check_question(p, draw.then_some((&*renderer, scale)), &what);
            }
        }
    }
}

/// Antigravity's read says what it may cost before it is allowed, wherever
/// it is asked for or listed: its own question, the welcome, Allow all and
/// the update notice; no other tool's question carries it. Its question
/// keeps its buttons and its last line in any height.
#[test]
fn antigravity_says_what_its_read_may_cost() {
    let risk = crate::consent::ANTIGRAVITY_RISK;
    let says = |p: &Picker| {
        let words = texts(p).split_whitespace().collect::<Vec<_>>().join(" ");
        words.contains(risk)
    };
    let mut not_yet = input();
    not_yet.reads.sign_in.retain(|s| s != "antigravity");
    // Its own question, from a tick.
    let mut p = picker(not_yet.clone());
    click(&mut p, &row("antigravity", Surface::Widget));
    assert_eq!(
        p.ask.as_ref().unwrap().question,
        Question::Read(ReadKind::SignIn("antigravity".into()))
    );
    assert!(says(&p), "{}", texts(&p));
    // Another tool's question does not.
    let mut i = input();
    i.reads.sign_in.retain(|s| s != "claude");
    let mut other = picker(i);
    click(&mut other, &row("claude", Surface::Widget));
    assert!(other.ask.is_some());
    assert!(!texts(&other).contains("suspended"));
    // The welcome's entry, Allow all's list and the update notice.
    assert!(says(&picker(welcome(vec!["claude", "antigravity"]))));
    let mut i = welcome(vec!["claude", "codex", "antigravity"]);
    i.mode = PickerMode::Normal;
    let mut all = picker(i);
    click(&mut all, &row("codex", Surface::Widget));
    assert!(area(&all, &Target::AllowAll).is_some());
    assert!(says(&all));
    assert!(says(&picker(notice())));
    // With the real font, in every height the welcome is checked in.
    let renderer = Rc::new(Renderer::new().expect("a system font"));
    let r = renderer.clone();
    let mut p = Picker::new(not_yet, move |t, s| r.text_width(t, s));
    click(&mut p, &row("antigravity", Surface::Widget));
    let mut heights: Vec<f32> = vec![300.0, 350.0, 400.0, 450.0, 500.0, 560.0];
    for scale in [1.25, 1.5, 1.75, 2.0] {
        heights.push(max_box_height((0, 0, 1280, 680), scale));
    }
    for h in heights {
        p.set_max_height(h);
        check_question(&mut p, None, &format!("antigravity, {h} px"));
    }
}

fn check_question(p: &mut Picker, draw: Option<(&Renderer, f32)>, what: &str) {
    let inside = |a: &Rect, b: &Rect| {
        a.x >= b.x - 0.01
            && a.y >= b.y - 0.01
            && a.x + a.w <= b.x + b.w + 0.01
            && a.y + a.h <= b.y + b.h + 0.01
    };
    let f = p.frame();
    let a = f.ask.expect("a question");
    assert!(
        f.boxr.h <= p.max_box_h.ceil() + 0.01,
        "{what}: box too tall"
    );
    assert!(inside(&a.rect, &f.boxr), "{what}: question out of the box");
    let buttons: Vec<Rect> = [Target::Allow, Target::Cancel]
        .iter()
        .map(|t| area(p, t).expect("a button").1)
        .collect();
    for b in &buttons {
        assert!(inside(b, &a.rect), "{what}: {b:?} out of {:?}", a.rect);
        assert!(
            b.y >= bottom(&a.viewport),
            "{what}: a button under the text"
        );
    }
    // Drawn where they are: Allow's face is the accent.
    if let Some((renderer, scale)) = draw {
        let (w, h) = p.window_size();
        let mut pm =
            tiny_skia::Pixmap::new((w * scale).ceil() as u32, (h * scale).ceil() as u32).unwrap();
        let theme = PickerTheme::dark();
        renderer.draw_picker(
            &mut pm,
            &p.scene(),
            &theme,
            scale,
            &mut PickerGlyphs::default(),
        );
        let allow = buttons[0];
        let (px, py) = (
            ((allow.x + 6.0) * scale) as u32,
            ((allow.y + 4.0) * scale) as u32,
        );
        let c = pm.pixel(px, py).unwrap().demultiply();
        assert_eq!(
            (c.red(), c.green(), c.blue()),
            (theme.accent.r, theme.accent.g, theme.accent.b),
            "{what}: Allow not drawn"
        );
    }
    // Scrolled to the end, the last line of the text shows.
    p.handle(PickerEvent::Wheel { dy: -100_000.0 });
    let f = p.frame();
    let a = f.ask.unwrap();
    assert_eq!(a.scroll, a.max_scroll(), "{what}");
    let last = f
        .els
        .iter()
        .filter(|(l, _)| *l == Layer::AskBody)
        .filter_map(|(_, el)| match el {
            El::Shape(Shape::Text { y, size, .. }) => Some(y + lh(*size)),
            _ => None,
        })
        .fold(0.0, f32::max);
    assert!(
        last <= bottom(&a.viewport) + 0.5,
        "{what}: the end does not show ({last} > {})",
        bottom(&a.viewport)
    );
    // And back at the top, the title does.
    p.handle(PickerEvent::Wheel { dy: 100_000.0 });
    let f = p.frame();
    let a = f.ask.unwrap();
    let first = f
        .els
        .iter()
        .filter(|(l, _)| *l == Layer::AskBody)
        .find_map(|(_, el)| match el {
            El::Shape(Shape::Text { y, .. }) => Some(*y),
            _ => None,
        })
        .unwrap();
    assert!(first >= a.viewport.y, "{what}: the title is cut");
}

// ─── Visual baseline ─────────────────────────────────────────

/// The picker, pinned: see ui/visual_baseline.rs. New looks only (the
/// picker did not exist in 0.7.0).
#[test]
fn visual_baseline_picker() {
    use crate::ui::visual_baseline as vb;
    let renderer = Rc::new(Renderer::new().expect("a system font"));
    let draw = |p: &Picker, light: bool, scale: f32| {
        let (w, h) = p.window_size();
        let mut pm =
            tiny_skia::Pixmap::new((w * scale).ceil() as u32, (h * scale).ceil() as u32).unwrap();
        renderer.draw_picker(
            &mut pm,
            &p.scene(),
            &PickerTheme::for_system(light),
            scale,
            &mut PickerGlyphs::default(),
        );
        pm
    };
    let make = |input: PickerInput| {
        let r = renderer.clone();
        Picker::new(input, move |t, s| r.text_width(t, s))
    };
    let mut items = Vec::new();

    let normal = make(input());
    items.push(("normal-dark".to_string(), draw(&normal, false, 1.0)));
    items.push(("normal-light".to_string(), draw(&normal, true, 1.0)));
    items.push(("normal-dark-150".to_string(), draw(&normal, false, 1.5)));

    // Percent, Reset time and the panel's Progress bar, scrolled into view.
    let mut numbers = make(input());
    spot(&mut numbers, &Target::Tip(TipPart::Bar, NumbersOn::Panel));
    items.push(("numbers-dark".to_string(), draw(&numbers, false, 1.0)));
    items.push(("numbers-light".to_string(), draw(&numbers, true, 1.0)));

    let mut welcome = input();
    welcome.mode = PickerMode::Welcome;
    welcome.reads = Reads::default();
    welcome.found = vec!["claude", "codex", "grok"];
    let mut all_found = welcome.clone();
    let welcome = make(welcome);
    items.push(("welcome-dark".to_string(), draw(&welcome, false, 1.0)));
    items.push(("welcome-light".to_string(), draw(&welcome, true, 1.0)));
    // Every tool found: the longest question the welcome asks.
    all_found.found = vec![
        "claude",
        "codex",
        "copilot",
        "antigravity",
        "grok",
        "cursor",
    ];
    let all_found_input = all_found.clone();
    let all_found = make(all_found);
    items.push((
        "welcome-all-found-dark".to_string(),
        draw(&all_found, false, 1.0),
    ));
    // Cancelled: nothing ticked, the tools found still marked.
    let mut cancelled = all_found;
    click(&mut cancelled, &Target::Cancel);
    items.push((
        "welcome-cancelled-dark".to_string(),
        draw(&cancelled, false, 1.0),
    ));

    let notice = make(notice());
    items.push(("notice-dark".to_string(), draw(&notice, false, 1.0)));
    items.push(("notice-light".to_string(), draw(&notice, true, 1.0)));

    // A read question open, with the focus ring on Allow.
    let mut ask = make(input());
    click(&mut ask, &row("cursor", Surface::Widget));
    ask.focus_visible = true;
    items.push(("ask-sign-in-dark".to_string(), draw(&ask, false, 1.0)));
    // Antigravity's question, with what its read may cost.
    let mut ask = make({
        let mut i = input();
        i.reads.sign_in.retain(|s| s != "antigravity");
        i
    });
    click(&mut ask, &row("antigravity", Surface::Widget));
    items.push(("ask-risk-dark".to_string(), draw(&ask, false, 1.0)));
    let mut ask = make(input());
    click(&mut ask, &part(ModelPart::Model, Surface::Widget));
    items.push(("ask-settings-light".to_string(), draw(&ask, true, 1.0)));
    let mut ask = make(with_model());
    click(&mut ask, &Target::Source(ModelSource::LastUsed));
    items.push(("ask-session-dark".to_string(), draw(&ask, false, 1.0)));

    // The names, opened: one typed (caret, room hint), the others
    // placeholders.
    let open_all = |p: &mut Picker| {
        p.names_open = true;
        p.open_names = p
            .input
            .models
            .iter()
            .map(|m| m.provider.to_string())
            .collect();
    };
    let mut names = make(with_model());
    open_all(&mut names);
    click(&mut names, &field("claude-opus-5-5"));
    typed(&mut names, "My Opus");
    items.push(("name-typed-dark".to_string(), draw(&names, false, 1.0)));
    items.push(("name-typed-light".to_string(), draw(&names, true, 1.0)));
    let mut full = make(with_model());
    open_all(&mut full);
    click(&mut full, &field("gpt-6.1-sol"));
    full.handle(PickerEvent::Paste("GPT-6.1 Sol Thinking".into()));
    items.push(("name-full-dark".to_string(), draw(&full, false, 1.0)));
    let mut placeholder = make(with_model());
    open_all(&mut placeholder);
    placeholder.handle(PickerEvent::Wheel { dy: -10_000.0 });
    items.push((
        "name-placeholder-dark".to_string(),
        draw(&placeholder, false, 1.0),
    ));
    // Closed, as it opens: one line at the bottom.
    let mut closed = make(with_model());
    closed.handle(PickerEvent::Wheel { dy: -10_000.0 });
    items.push(("names-closed-dark".to_string(), draw(&closed, false, 1.0)));
    // Every model of each tool: the list open, Claude's models shown,
    // the one in use marked.
    let mut catalogue = with_model();
    let mut more: Vec<ModelChoice> = Vec::new();
    for (provider, ids) in [
        (
            "claude",
            &[
                "claude-opus-5-5",
                "claude-fable-5-2",
                "claude-sonnet-5-5",
                "claude-haiku-5-5",
            ][..],
        ),
        (
            "codex",
            &["gpt-6.1-sol", "gpt-6.1-terra", "gpt-6-astra"][..],
        ),
        ("grok", &["grok-4.7", "grok-4.6"][..]),
        ("copilot", &["auto", "gpt-5.5"][..]),
    ] {
        for (n, id) in ids.iter().enumerate() {
            let pid = ProviderId::from_config_id(provider).unwrap();
            more.push(ModelChoice {
                provider,
                model_id: id.to_string(),
                key: crate::choices::model_key(&pid, id),
                automatic: crate::models::name_parts(id, &pid, None).with_version,
                renamed: None,
                current: n == 0,
                shared_with: Vec::new(),
            });
        }
    }
    catalogue.models = more;
    let mut catalogue = make(catalogue);
    catalogue.names_open = true;
    catalogue.open_names = vec!["claude".into()];
    catalogue.handle(PickerEvent::Wheel { dy: -10_000.0 });
    items.push((
        "names-catalogue-dark".to_string(),
        draw(&catalogue, false, 1.0),
    ));

    // A tool's question with other tools found: Allow all.
    let mut found = super::tests::welcome(vec!["claude", "codex", "grok", "copilot"]);
    found.mode = PickerMode::Normal;
    let mut allow_all = make(found);
    click(&mut allow_all, &row("codex", Surface::Widget));
    items.push((
        "ask-allow-all-dark".to_string(),
        draw(&allow_all, false, 1.0),
    ));

    // The Taskbar column chosen: Claude and Grok, Codex's tick waiting,
    // its description under the pointer.
    let mut chosen = input();
    chosen.taskbar_rows = vec!["claude".into(), "grok".into()];
    let mut chosen = make(chosen);
    let (x, y) = spot(&mut chosen, &Target::Taskbar("codex".into()));
    chosen.handle(PickerEvent::PointerMove { x, y });
    items.push(("taskbar-chosen-dark".to_string(), draw(&chosen, false, 1.0)));

    // Sign-in under each provider: every line closed; Claude open on
    // its tool's sign-in; Copilot open on a pasted token; Cursor waiting
    // for the browser.
    let mut i = input();
    i.sign_in = sign_ins();
    let closed = make(i.clone());
    items.push(("signin-closed-dark".to_string(), draw(&closed, false, 1.0)));
    let mut claude = make(i.clone());
    claude.open_sign_in = vec!["claude".into()];
    items.push(("signin-claude-dark".to_string(), draw(&claude, false, 1.0)));
    items.push(("signin-claude-light".to_string(), draw(&claude, true, 1.0)));
    let mut copilot = make(i.clone());
    copilot.open_sign_in = vec!["copilot".into()];
    let (_, y) = spot(&mut copilot, &Target::SignIn("copilot".into()));
    let _ = y;
    items.push((
        "signin-copilot-dark".to_string(),
        draw(&copilot, false, 1.0),
    ));
    let mut waiting = i.clone();
    if let Some(c) = waiting.sign_in.iter_mut().find(|s| s.provider == "cursor") {
        c.summary = "Waiting for the browser".into();
        c.cursor = Some(CursorState::Waiting);
    }
    let mut cursor = make(waiting);
    cursor.open_sign_in = vec!["cursor".into()];
    let _ = spot(&mut cursor, &Target::CursorCancel);
    items.push((
        "signin-cursor-waiting-dark".to_string(),
        draw(&cursor, false, 1.0),
    ));

    // Model names with Effort alone shown: the fields are there, and a
    // line says where the names show.
    let mut i = input();
    i.reads.tool_settings = true;
    i.model.widget = vec![ModelPart::Effort];
    let mut effort_only = make(i);
    effort_only.names_open = true;
    effort_only.open_names = vec!["claude".into()];
    effort_only.handle(PickerEvent::Wheel { dy: -10_000.0 });
    items.push((
        "names-effort-only-dark".to_string(),
        draw(&effort_only, false, 1.0),
    ));

    // A description under the pointer: the Fable row's.
    let mut hint = make(with_model());
    let (x, y) = spot(&mut hint, &row("claude.fable", Surface::Widget));
    hint.handle(PickerEvent::PointerMove { x: 60.0, y });
    let _ = x;
    items.push(("hint-dark".to_string(), draw(&hint, false, 1.0)));

    // Scrolled half way, the heads pinned, a row hovered.
    let mut scrolled = make(input());
    scrolled.set_max_height(360.0);
    scrolled.handle(PickerEvent::Wheel { dy: -200.0 });
    scrolled.handle(PickerEvent::PointerMove { x: 120.0, y: 160.0 });
    items.push(("scrolled-dark".to_string(), draw(&scrolled, false, 1.0)));

    // Model ticks: Effort alone, Version dimmed; keyboard focus ring.
    let mut i = input();
    i.reads.tool_settings = true;
    i.model.widget = vec![ModelPart::Effort];
    i.model.tooltip = vec![ModelPart::Model, ModelPart::Version];
    let mut parts = make(i);
    parts.focus = Some(part(ModelPart::Effort, Surface::Widget));
    parts.focus_visible = true;
    parts.reveal_focus();
    items.push((
        "version-disabled-dark".to_string(),
        draw(&parts, false, 1.0),
    ));
    items.push((
        "version-disabled-light".to_string(),
        draw(&parts, true, 1.0),
    ));

    // The welcome with every tool found on a 1080p screen at 125 to 200
    // per cent: the question as tall as the work area allows, its text
    // scrolling above buttons that stay in the box.
    for scale in [1.25_f32, 1.5, 1.75, 2.0] {
        let mut p = make(all_found_input.clone());
        p.set_max_height(max_box_height((0, 0, 1920, 1032), scale as f64));
        let pct = (scale * 100.0).round() as u32;
        items.push((format!("welcome-all-found-{pct}"), draw(&p, false, scale)));
        if scale == 2.0 {
            p.handle(PickerEvent::Wheel { dy: -10_000.0 });
            items.push((
                format!("welcome-all-found-{pct}-end"),
                draw(&p, false, scale),
            ));
        }
    }
    // Copilot and Antigravity found only by GitHub CLI's and Gemini
    // CLI's folders: listed, unticked, saying why.
    let mut weak = all_found_input.clone();
    weak.found = vec!["claude", "codex", "copilot", "antigravity"];
    weak.found_weak = vec!["copilot", "antigravity"];
    let weak = make(weak);
    items.push(("welcome-weak-dark".to_string(), draw(&weak, false, 1.0)));
    // Nothing ticked: Allow cannot be used.
    let mut none = make({
        let mut i = all_found_input.clone();
        i.found = vec!["claude", "codex"];
        i
    });
    click(&mut none, &Target::Pick("claude".into()));
    click(&mut none, &Target::Pick("codex".into()));
    items.push((
        "welcome-none-ticked-dark".to_string(),
        draw(&none, false, 1.0),
    ));
    // The keyboard in the question: the focus ring on a tool's tick.
    let mut keys = make({
        let mut i = all_found_input.clone();
        i.found = vec!["claude", "codex", "grok"];
        i
    });
    key(&mut keys, PickerKey::Tab);
    key(&mut keys, PickerKey::BackTab);
    items.push(("welcome-keyboard-light".to_string(), draw(&keys, true, 1.0)));

    // High contrast: its own colours (black and white schemes).
    let draw_with = |p: &Picker, theme: &PickerTheme, scale: f32| {
        let (w, h) = p.window_size();
        let mut pm =
            tiny_skia::Pixmap::new((w * scale).ceil() as u32, (h * scale).ceil() as u32).unwrap();
        renderer.draw_picker(
            &mut pm,
            &p.scene(),
            theme,
            scale,
            &mut PickerGlyphs::default(),
        );
        pm
    };
    let black = PickerTheme::high_contrast(&crate::ui::theme::ContrastColors::default());
    let white = PickerTheme::high_contrast(&crate::ui::theme::ContrastColors {
        window: [255, 255, 255],
        window_text: [0, 0, 0],
        highlight: [55, 0, 110],
        highlight_text: [255, 255, 255],
        gray_text: [96, 0, 0],
    });
    let hc = make(all_found_input.clone());
    items.push((
        "welcome-contrast-black".to_string(),
        draw_with(&hc, &black, 1.0),
    ));
    let mut hc_names = make(with_model());
    open_all(&mut hc_names);
    hc_names.handle(PickerEvent::Wheel { dy: -10_000.0 });
    items.push((
        "names-contrast-white".to_string(),
        draw_with(&hc_names, &white, 1.0),
    ));

    // The keyboard on a row with a description: it shows, short of the
    // tick columns.
    let mut focus = make(with_model());
    let fable = row("claude.fable", Surface::Widget);
    while focus.focus != Some(fable.clone()) {
        key(&mut focus, PickerKey::Tab);
    }
    items.push(("focus-hint-dark".to_string(), draw(&focus, false, 1.0)));

    // A model two tools share, under each; a cloud id cut to its field.
    let mut shared = with_model();
    for (provider, current, other) in [("codex", false, "Copilot"), ("copilot", true, "Codex")] {
        shared.models.push(ModelChoice {
            provider,
            model_id: "gpt-5.6-sol".into(),
            key: "gpt-5.6-sol".into(),
            automatic: "GPT-5.6 Sol".into(),
            renamed: Some("Sol".into()),
            current,
            shared_with: vec![other],
        });
    }
    shared.models[0].model_id = "arn:aws:bedrock:us-east-1:123456789012:".repeat(5);
    shared.models[0].automatic =
        "Arn Aws Bedrock Us East 1 Inference Profile Anthropic Claude Opus".into();
    let mut shared = make(shared);
    open_all(&mut shared);
    shared.handle(PickerEvent::Wheel { dy: -10_000.0 });
    items.push(("names-shared-dark".to_string(), draw(&shared, false, 1.0)));

    vb::check("picker", items);
}
