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
        ("claude", "Claude", "Current limit"),
        ("codex", "Codex", "Current limit"),
        ("grok", "Grok", "Current limit"),
        ("cursor", "Cursor", "Current limit"),
        ("antigravity", "Antigravity", "Current limit"),
        ("copilot", "Copilot", "Premium requests, monthly"),
    ];
    let mut out = Vec::new();
    for (id, name, note) in own {
        out.push(RowChoice {
            id,
            provider: id,
            name,
            note: note.into(),
            short: "",
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
                note: "".into(),
                short: s.short,
                hint: s.hint,
                has_data: true,
                provider_ok: true,
            });
        }
    }
    out
}

/// The rows of two models Claude added later, as the app makes them from
/// its reply (`rows::offered_rows`): one short name, one cut.
fn made_rows() -> Vec<RowChoice> {
    use crate::providers::{Metric, MetricUnit, MetricWindow, ProviderData, ProviderStatus};
    let week = |label: &str| Metric {
        label: label.to_string(),
        used: 10,
        limit: Some(100),
        unit: MetricUnit::Percent,
        reset_at: None,
        window: MetricWindow::Long,
        unified_pool: false,
        extra_only: true,
    };
    let data = ProviderData {
        id: ProviderId::Claude,
        status: ProviderStatus::Ok,
        metrics: vec![week("Haiku"), week("Mythos Preview Extended")],
        updated_at: crate::utc::now(),
        received_at: None,
    };
    crate::rows::offered_rows(&ProviderId::Claude, Some(&data))
        .into_iter()
        .skip(crate::rows::sub_rows(&ProviderId::Claude).count())
        .map(|s| RowChoice {
            id: s.id,
            provider: "claude",
            name: s.names[0],
            note: "".into(),
            short: s.short,
            hint: s.hint,
            has_data: true,
            provider_ok: true,
        })
        .collect()
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
        widget: UiNumbers::default(),
        tip: UiNumbers::default(),
        panel: UiNumbers::PERCENT_ONLY,
        tray_style: crate::config::schema::TrayStyle::default(),
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
            summary: "Claude Code \u{b7} connected".into(),
            sources: vec![
                src(SourceId::Tool, "Claude Code account", true),
                src(SourceId::UsageToken, "Usage token", false),
                src(SourceId::Key, "API key", false),
            ],
            detail: "Reads %USERPROFILE%\\.claude\\.credentials.json; its token goes only to api.anthropic.com. Claude Code renews it as you use it.".into(),
            paste: None,
            stored: false,
            forget: true,
            note: None,
            cursor: None,
        },
        SignInChoice {
            provider: "codex",
            summary: "Codex CLI \u{b7} connected".into(),
            sources: vec![
                src(SourceId::Tool, "Codex CLI account", true),
                src(SourceId::UsageToken, "Usage token", false),
            ],
            detail: "Reads %USERPROFILE%\\.codex\\auth.json (in CODEX_HOME when it is set); its token goes only to chatgpt.com.".into(),
            paste: None,
            stored: false,
            forget: true,
            note: None,
            cursor: None,
        },
        SignInChoice {
            provider: "grok",
            summary: "Grok CLI \u{b7} connected".into(),
            sources: Vec::new(),
            detail: "Reads %USERPROFILE%\\.grok\\auth.json (in GROK_HOME when it is set); its token goes only to cli-chat-proxy.grok.com.".into(),
            paste: None,
            stored: false,
            forget: true,
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
            forget: false,
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
            forget: true,
            note: None,
            cursor: None,
        },
        SignInChoice {
            provider: "copilot",
            summary: "Fine-grained token \u{b7} connected".into(),
            sources: vec![
                src(SourceId::Tool, "GitHub CLI account", false),
                src(SourceId::Key, "Fine-grained token", true),
            ],
            detail: "Kept in Windows Credential Manager; it goes only to api.github.com. It needs no permissions at all.".into(),
            paste: Some("Paste token from clipboard"),
            stored: true,
            forget: false,
            note: Some("Verified with GitHub and saved to Credential Manager.".into()),
            cursor: None,
        },
    ]
}

fn picker(input: PickerInput) -> Picker {
    Picker::new(input, mono)
}

/// A picker with the list of names open, every tool's models showing.
/// The What shows tab with the list of names and every tool in it open.
fn opened(input: PickerInput) -> Picker {
    let mut p = picker(input);
    on_tab(&mut p, Tab::Shows);
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
            | El::Toggle { target, rect, .. }
            | El::Tab { target, rect, .. }
            | El::Tile { target, rect, .. }
            | El::Link { target, rect, .. } => (target.clone(), *rect),
            El::Close { rect, .. } => (Target::Close, *rect),
            _ => return None,
        };
        (et == *t).then_some((layer, r))
    })
}

/// Show `tab`, as a click on it does.
fn on_tab(p: &mut Picker, tab: Tab) {
    if p.tab != tab {
        p.activate(&Target::Tab(tab));
        p.settle();
    }
}

/// The middle of a target, scrolled into view first; on the other tab, that
/// tab is shown first.
fn spot(p: &mut Picker, t: &Target) -> (f32, f32) {
    if area(p, t).is_none() {
        let other = p.tab.other();
        on_tab(p, other);
    }
    let (layer, r) = area(p, t).expect("target laid out");
    if layer == Layer::Body {
        let f = p.frame();
        // Scrolled, the heads are pinned over the list's top.
        let top = f.viewport.y + COLS_H;
        if r.y < f.list_top() || bottom(&r) > bottom(&f.viewport) {
            p.scroll = f.scroll + (r.y - top) - 8.0;
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

/// "Settings file..." at the list's end: a click, or Enter on it, asks the
/// app to open config.toml; on hover and on keyboard focus it says what the
/// file is for. The welcome, which is about what to read, has none.
#[test]
fn the_settings_file_link_opens_config_toml() {
    let mut p = picker(input());
    on_tab(&mut p, Tab::Shows);
    let t = Target::SettingsFile;
    let (_, r) = area(&p, &t).expect("the link");
    // The last thing in the list.
    let f = p.frame();
    let last = f
        .els
        .iter()
        .filter(|(l, _)| *l == Layer::Body)
        .filter_map(|(_, el)| super::scene::el_rect(el))
        .map(|r| bottom(&r))
        .fold(0.0, f32::max);
    assert_eq!(bottom(&r), last);
    assert!(texts(&p).contains(SETTINGS_FILE));
    assert_eq!(click(&mut p, &t), [PickerAction::OpenSettingsFile]);
    // Its description under the pointer.
    let (x, y) = spot(&mut p, &t);
    p.handle(PickerEvent::PointerMove { x, y });
    let shown = p.scene().texts().join(" ");
    assert!(shown.contains("Opens config.toml in Notepad"), "{shown}");
    // The keyboard reaches it, and Enter opens it.
    let mut p = picker(input());
    on_tab(&mut p, Tab::Shows);
    p.handle(PickerEvent::Key(PickerKey::End));
    for _ in 0..200 {
        if p.focus == Some(t.clone()) {
            break;
        }
        key(&mut p, PickerKey::Tab);
    }
    assert_eq!(p.focus, Some(t.clone()));
    assert_eq!(
        key(&mut p, PickerKey::Enter),
        [PickerAction::OpenSettingsFile]
    );
    // Not in the welcome.
    let mut welcome = input();
    welcome.mode = PickerMode::Welcome;
    let mut p = picker(welcome);
    on_tab(&mut p, Tab::Shows);
    assert!(area(&p, &t).is_none());
    assert!(!texts(&p).contains(SETTINGS_FILE));
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
    on_tab(&mut p, Tab::Shows);
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
    on_tab(&mut p, Tab::Shows);
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

/// Forget permission under a provider's Sign-in, where its tool's sign-in
/// is the source and may be read; under the model label's note once the
/// settings files may be read.
#[test]
fn forget_permission_takes_a_permission_back() {
    let mut i = input();
    i.sign_in = sign_ins();
    let mut p = picker(i.clone());
    p.open_sign_in = vec!["claude".into(), "copilot".into()];
    assert_eq!(
        click(&mut p, &Target::Forget("claude".into())),
        [PickerAction::ForgetSignIn("claude".into())]
    );
    // A pasted token is the source: Remove, no Forget permission.
    assert!(area(&p, &Target::Forget("copilot".into())).is_none());
    // The model label: the link only once its reads are allowed.
    let mut p = picker(input());
    on_tab(&mut p, Tab::Shows);
    assert!(area(&p, &Target::ForgetModelReads).is_none());
    let mut p = picker(with_model());
    on_tab(&mut p, Tab::Shows);
    assert_eq!(
        click(&mut p, &Target::ForgetModelReads),
        [PickerAction::ForgetModelReads]
    );
}

/// The window closes as a dialog does: by Esc, the cross or Alt+F4. A
/// click beside the box and another window taking the focus leave it open.
#[test]
fn only_esc_the_cross_and_alt_f4_close() {
    let mut p = picker(input());
    assert_eq!(key(&mut p, PickerKey::Escape), [PickerAction::Close]);
    let mut p = picker(input());
    assert_eq!(click(&mut p, &Target::Close), [PickerAction::Close]);
    let mut p = picker(input());
    assert_eq!(p.handle(PickerEvent::CloseRequested), [PickerAction::Close]);
    let mut p = picker(input());
    // The shadow margin is outside the box.
    assert!(p
        .handle(PickerEvent::PointerDown { x: 3.0, y: 3.0 })
        .is_empty());
    assert!(p
        .handle(PickerEvent::PointerUp { x: 3.0, y: 3.0 })
        .is_empty());
    assert!(p.handle(PickerEvent::FocusLost).is_empty());
    // An open question stays with the window, and goes with it.
    let mut p = picker(input());
    click(&mut p, &row("cursor", Surface::Widget));
    assert!(p.handle(PickerEvent::FocusLost).is_empty());
    assert!(p.ask.is_some());
    assert_eq!(click(&mut p, &Target::Close), [PickerAction::Close]);
}

/// A press on the title moves the window; on the cross, on the tabs, in
/// the list or beside the box it does not.
#[test]
fn the_title_moves_the_window() {
    let p = picker(input());
    let b = p.frame().boxr;
    assert!(p.drags_at(b.x + 20.0, b.y + 10.0));
    assert!(p.drags_at(b.x + b.w / 2.0, b.y + HEADER_H - 1.0));
    let (_, cross) = area(&p, &Target::Close).unwrap();
    assert!(!p.drags_at(cross.x + cross.w / 2.0, cross.y + cross.h / 2.0));
    let (_, tab) = area(&p, &Target::Tab(Tab::Shows)).unwrap();
    assert!(!p.drags_at(tab.x + tab.w / 2.0, tab.y + tab.h / 2.0));
    assert!(!p.drags_at(b.x + 20.0, b.y + HEADER_H + TABS_H + 30.0));
    assert!(!p.drags_at(3.0, 3.0));
    // The notice's title too.
    let n = picker(notice());
    let nb = n.frame().boxr;
    assert!(n.drags_at(nb.x + 20.0, nb.y + 10.0));
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
    assert!(text.contains("Show the detected tools?"));
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
/// PC are not allowed; it names their files and allows them all.
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
    // Allow all: this tool and its tick; the others only allowed, none of
    // their rows ticked.
    let want = vec![
        PickerAction::Allow(ReadKind::SignIn("codex".into())),
        set_row(Surface::Widget, "codex", true),
        PickerAction::Consent("claude".into()),
        PickerAction::Consent("grok".into()),
    ];
    assert_eq!(click(&mut p, &Target::AllowAll), want);
    assert!(p.ask.is_none());
    // Nothing else found: no Allow all.
    let mut alone = welcome(vec!["codex"]);
    alone.mode = PickerMode::Normal;
    let mut p = picker(alone);
    click(&mut p, &row("codex", Surface::Widget));
    assert!(area(&p, &Target::AllowAll).is_none());
    assert!(!texts(&p).replace('\n', " ").contains(ALLOW_ALL_LEAD));
    assert!(p.ask.is_some());
}

/// The list of names opens and closes, and each tool's models with it.
#[test]
fn the_names_open_by_tool() {
    let mut p = picker(with_model());
    on_tab(&mut p, Tab::Shows);
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
    on_tab(&mut p, Tab::Shows);
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
    on_tab(&mut p, Tab::Rows);
    let f = p.frame();
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
    assert!(text.contains("Select the tools you use."));
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
    assert_eq!(click(&mut p, &Target::Close), [PickerAction::Close]);
    // Closed without an answer, the welcome is answered by that, once.
    let mut p = picker(welcome(Vec::new()));
    assert_eq!(
        key(&mut p, PickerKey::Escape),
        [PickerAction::NoticeDone, PickerAction::Close]
    );
    assert_eq!(click(&mut p, &Target::Close), [PickerAction::Close]);
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
    assert_eq!(click(&mut p, &Target::Close), [PickerAction::Close]);
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
    assert_eq!(p.focus, Some(Target::Tab(Tab::Rows)));
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(Target::Tab(Tab::Shows)));
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
    assert_eq!(p.focus, Some(Target::Tab(Tab::Shows)));
    // Enter on a tab shows it; the focus stays on it.
    assert!(key(&mut p, PickerKey::Enter).is_empty());
    assert_eq!(p.tab, Tab::Shows);
    assert_eq!(p.focus, Some(Target::Tab(Tab::Shows)));
    key(&mut p, PickerKey::BackTab);
    key(&mut p, PickerKey::BackTab);
    assert_eq!(p.focus, Some(Target::Close));
    // End goes to the last control, scrolled into view: the settings
    // file's link; above it the names' heading, above that the tray
    // icon's looks.
    key(&mut p, PickerKey::End);
    assert_eq!(p.focus, Some(Target::SettingsFile));
    assert!(p.scroll > 0.0);
    key(&mut p, PickerKey::Up);
    assert_eq!(p.focus, Some(Target::Names));
    key(&mut p, PickerKey::Up);
    assert_eq!(p.focus, Some(Target::TrayStyle(TrayStyle::Rings)));
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
        place(at(100, 100, 278, 200), None, size, 12, 8, WORK),
        (100 + 278 + 8 - 12, 88)
    );
    // Against the right edge: to its left.
    assert_eq!(
        place(at(1600, 100, 278, 200), None, size, 12, 8, WORK),
        (1600 - 8 - 380 - 12, 88)
    );
    // Low on the screen: slid up into the work area.
    let (_, y) = place(at(100, 900, 278, 100), None, size, 12, 8, WORK);
    assert_eq!(y + 12 + 560, 1032);
    // The tray icon in the corner: left of it, bottom held in.
    let (x, y) = place(at(1840, 1040, 24, 40), None, size, 12, 8, WORK);
    assert_eq!((x + 12 + 380, y + 12 + 560), (1840 - 8, 1032));
    // A widget as wide as the screen: below it.
    assert_eq!(
        place(at(0, 50, 1920, 100), None, size, 12, 8, WORK),
        (-12, 50 + 100 + 8 - 12)
    );
    // No room anywhere: held inside all the same.
    let (x, y) = place(at(10, 10, 1900, 1000), None, size, 12, 8, WORK);
    assert!(x + 12 >= 0 && x + 12 + 380 <= 1920 && y + 12 >= 0 && y + 12 + 560 <= 1032);
}

/// Beside the widget, the side with more room comes first: away from the
/// nearer edge of the screen, though the other side would fit too.
#[test]
fn the_box_goes_to_the_roomier_side() {
    let size = (404, 584);
    // Right of the middle: left of it.
    assert_eq!(
        place(at(1100, 100, 278, 200), None, size, 12, 8, WORK),
        (1100 - 8 - 380 - 12, 88)
    );
    // Left of the middle: right of it.
    assert_eq!(
        place(at(500, 100, 278, 200), None, size, 12, 8, WORK),
        (500 + 278 + 8 - 12, 88)
    );
}

/// The widget grows while rows are ticked: the box keeps off that side and
/// out of the room it grows into, so a row ticked comes into sight.
#[test]
fn the_box_keeps_off_the_side_the_widget_grows_to() {
    let size = (404, 584);
    // Columns side by side grow to the right: the box goes left of them,
    // though the right has more room.
    let widget = at(700, 800, 300, 120);
    let a = Anchor::widget(widget, (700, 120), Some(WORK));
    assert_eq!(a.grows, Some(Side::Right));
    assert_eq!(a.reach, at(700, 800, 700, 120));
    let (x, _) = place(a.reach, a.grows, size, 12, 8, WORK);
    assert_eq!(x + 12 + 380, 700 - 8);
    // Near the right edge they are pulled back and grow to the left, and
    // the right has no room: above the room they take.
    let widget = at(1500, 800, 300, 120);
    let a = Anchor::widget(widget, (700, 120), Some(WORK));
    assert_eq!(a.grows, Some(Side::Left));
    assert_eq!(a.reach, at(1220, 800, 700, 120));
    let (x, y) = place(a.reach, a.grows, size, 12, 8, WORK);
    assert_eq!((x + 12, y + 12 + 560), (1220, 800 - 8));
    // Against the left edge with no room on the left either: above them
    // (more room than below), never on the right where they grow.
    let widget = at(0, 800, 300, 120);
    let a = Anchor::widget(widget, (1700, 120), Some(WORK));
    let (x, y) = place(a.reach, a.grows, size, 12, 8, WORK);
    assert_eq!((x + 12, y + 12 + 560), (0, 800 - 8));
    // Rows grow downwards: beside them as before, the roomier side first;
    // never below.
    let widget = at(1300, 100, 300, 200);
    let a = Anchor::widget(widget, (300, 500), Some(WORK));
    assert_eq!(a.grows, Some(Side::Below));
    let (x, y) = place(a.reach, a.grows, size, 12, 8, WORK);
    assert_eq!((x + 12 + 380, y + 12), (1300 - 8, 100));
    // Near the bottom they grow upwards: the reach goes up with them.
    let widget = at(100, 900, 300, 100);
    let a = Anchor::widget(widget, (300, 400), Some(WORK));
    assert_eq!(a.grows, Some(Side::Above));
    assert_eq!(a.reach, at(100, 632, 300, 400));
    // A widget as wide as the screen whose rows grow down: above it, the
    // only side not in their way.
    let widget = at(0, 700, 1920, 100);
    let a = Anchor::widget(widget, (1920, 300), Some(WORK));
    let (_, y) = place(a.reach, a.grows, size, 12, 8, WORK);
    assert_eq!(y + 12 + 560, 700 - 8);
    // No row left to tick: it does not grow, nothing is kept clear.
    let widget = at(700, 800, 300, 120);
    assert_eq!(
        Anchor::widget(widget, (300, 120), Some(WORK)),
        Anchor::fixed(widget)
    );
}

/// From the taskbar panel or the tray icon the box opens off the bar, as
/// Windows' flyouts do: above a bottom bar and lined up with the anchor's
/// end nearer the corner; beside a side bar; below a top one.
#[test]
fn the_box_opens_off_the_taskbar() {
    let size = (404, 584);
    let monitor = (0, 0, 1920, 1080);
    // The tray icon at the bottom right: above it, its right edge with the
    // icon's, held in the work area.
    let icon = at(1840, 1040, 24, 40);
    assert_eq!(Anchor::bar_edge(icon, monitor), Side::Below);
    let (x, y) = place_off_bar(icon, Side::Below, size, 12, 8, WORK);
    assert_eq!((x + 12 + 380, y + 12 + 560), (1864, 1040 - 8));
    // The panel at the bottom left: above it, its left edge with the panel's.
    let panel = at(300, 1036, 120, 44);
    let (x, y) = place_off_bar(panel, Side::Below, size, 12, 8, WORK);
    assert_eq!((x + 12, y + 12 + 560), (300, 1036 - 8));
    // A bar on the left: to the right of the icon, its bottom with the icon's.
    let left = (60, 0, 1920, 1080);
    let icon = at(10, 1000, 40, 24);
    assert_eq!(Anchor::bar_edge(icon, monitor), Side::Left);
    let (x, y) = place_off_bar(icon, Side::Left, size, 12, 8, left);
    assert_eq!((x + 12, y + 12 + 560), (60, 1024));
    // A bar on top: below the icon, held in on the right.
    let top = (0, 48, 1920, 1080);
    let icon = at(1880, 4, 24, 40);
    assert_eq!(Anchor::bar_edge(icon, monitor), Side::Above);
    let (x, y) = place_off_bar(icon, Side::Above, size, 12, 8, top);
    assert_eq!((x + 12 + 380, y + 12), (1904, 52));
}

/// Opened from the tray icon in the corner, or from a widget against the
/// screen's edge, the box keeps 16 px (at 100%; 24 at 150%) from the work
/// area's edges, the taskbar's among them.
#[test]
fn the_box_keeps_clear_of_the_screens_edges() {
    use super::window::place_window;
    let size = (404, 584);
    // The tray icon in the corner: left of it, and off the taskbar.
    let (x, y) = place_window(Anchor::fixed(at(1840, 1040, 24, 40)), size, 1.0, Some(WORK));
    assert_eq!((x + 12 + 380, y + 12 + 560), (1840 - 8, 1032 - 16));
    // A widget flush with the top-left corner: beside it, off the top.
    let (x, y) = place_window(Anchor::fixed(at(0, 0, 278, 200)), size, 1.0, Some(WORK));
    assert_eq!((x + 12, y + 12), (278 + 8, 16));
    // A widget as wide as the screen: below it, off the left edge.
    let (x, _) = place_window(Anchor::fixed(at(0, 50, 1920, 100)), size, 1.0, Some(WORK));
    assert_eq!(x + 12, 16);
    // At 150% the room scales.
    let (x, y) = place_window(
        Anchor::fixed(at(1840, 1040, 24, 40)),
        (606, 876),
        1.5,
        Some(WORK),
    );
    assert_eq!((x + 18 + 570, y + 18 + 840), (1840 - 12, 1032 - 24));
}

/// As far from the top as from the taskbar at the bottom, an auto-hidden
/// one's band counted, and the other way round; on every side alike. A
/// monitor with no bar (a second one) keeps all of itself.
#[test]
fn the_room_is_even_on_every_side() {
    use super::window::even_area;
    let monitor = (0, 0, 1920, 1080);
    // A bottom taskbar, kept or auto-hidden: 48 px off every side.
    assert_eq!(even_area(monitor, (0, 0, 1920, 1032)), (48, 48, 1872, 1032));
    // At the top.
    assert_eq!(
        even_area(monitor, (0, 48, 1920, 1080)),
        (48, 48, 1872, 1032)
    );
    // On the left, 62 px.
    assert_eq!(
        even_area(monitor, (62, 0, 1920, 1080)),
        (62, 62, 1858, 1018)
    );
    // No bar on this monitor.
    assert_eq!(even_area(monitor, monitor), monitor);
    // A second monitor to the right, its bar at the bottom.
    let second = (3440, 0, 6000, 1440);
    assert_eq!(
        even_area(second, (3440, 0, 6000, 1392)),
        (3488, 48, 5952, 1392)
    );
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
#[ignore = "a timing, for --release by hand"]
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

/// Allow all allows every other tool found that was never allowed, also
/// one switched off (a fresh install and an update leave every unused
/// provider off), without switching it on; a tool the user allowed and
/// then switched off keeps its consent and is left alone.
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
    assert!(texts(&p).replace('\n', " ").contains(ALLOW_ALL_LEAD));
    let out = click(&mut p, &Target::AllowAll);
    assert!(out.contains(&allow("cursor")));
    assert!(out.contains(&PickerAction::Consent("grok".into())));
    assert!(!out.contains(&allow("claude")) && !out.contains(&allow("codex")));
    assert!(!out.contains(&allow("grok")));
    assert!(!out.contains(&set_row(Surface::Widget, "grok", true)));
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
    assert!(!texts(&p).replace('\n', " ").contains(ALLOW_ALL_LEAD));
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

/// Percent, Reset time and Progress bar have a tick for the widget, the
/// tooltip (no bar there: a dash) and the panel, on the What shows tab.
/// The widget and the tooltip start with everything, the panel with the
/// percentage and its bar; the last one on in a column cannot be unticked
/// and says why on hover.
#[test]
fn the_widget_the_tooltip_and_the_panel_choose_what_rows_show() {
    use crate::config::schema::{NumbersOn, TipPart, UiNumbers};
    let widget = |part| Target::Tip(part, NumbersOn::Widget);
    let tip = |part| Target::Tip(part, NumbersOn::Tooltip);
    let panel = |part| Target::Tip(part, NumbersOn::Panel);
    let set = |part, place, on| PickerAction::SetTip { part, place, on };
    let mut p = picker(input());
    on_tab(&mut p, Tab::Shows);
    let text = texts(&p);
    assert!(
        text.contains("Percentage")
            && text.contains("Time to reset")
            && text.contains("Progress bar"),
        "{text}"
    );
    for part in [TipPart::Percent, TipPart::Reset, TipPart::Bar] {
        assert!(ticked(&p, &widget(part)) && usable(&p, &widget(part)));
    }
    assert!(ticked(&p, &tip(TipPart::Percent)) && ticked(&p, &tip(TipPart::Reset)));
    assert!(area(&p, &tip(TipPart::Bar)).is_none());
    assert!(ticked(&p, &panel(TipPart::Percent)) && !ticked(&p, &panel(TipPart::Reset)));
    // With its bar on, the panel's percentage may go.
    assert!(usable(&p, &panel(TipPart::Percent)));
    assert_eq!(
        click(&mut p, &widget(TipPart::Bar)),
        [set(TipPart::Bar, NumbersOn::Widget, false)]
    );
    assert_eq!(
        click(&mut p, &tip(TipPart::Reset)),
        [set(TipPart::Reset, NumbersOn::Tooltip, false)]
    );
    assert_eq!(
        click(&mut p, &panel(TipPart::Reset)),
        [set(TipPart::Reset, NumbersOn::Panel, true)]
    );
    // The last one on stays, and its description says why.
    let mut i = input();
    i.tip = UiNumbers::PERCENT_ONLY;
    i.widget = UiNumbers {
        percent: false,
        reset: true,
        bar: false,
    };
    let mut p = picker(i);
    on_tab(&mut p, Tab::Shows);
    assert!(!usable(&p, &tip(TipPart::Percent)));
    assert!(click(&mut p, &tip(TipPart::Percent)).is_empty());
    assert!(!usable(&p, &widget(TipPart::Reset)));
    assert!(click(&mut p, &widget(TipPart::Reset)).is_empty());
    let shown = texts(&p).replace('\n', " ");
    assert!(shown.contains(lock_hint(NumbersOn::Widget)), "{shown}");
    // The ticks sit in their columns.
    let (_, t) = area(&p, &tip(TipPart::Reset)).unwrap();
    let (_, effort) = area(&p, &part(ModelPart::Effort, Surface::Tooltip)).unwrap();
    assert_eq!(t.x, effort.x);
    let (_, w) = area(&p, &widget(TipPart::Reset)).unwrap();
    let (_, k) = area(&p, &panel(TipPart::Reset)).unwrap();
    on_tab(&mut p, Tab::Rows);
    let (_, claude) = area(&p, &Target::Taskbar("claude".into())).unwrap();
    let (_, own) = area(&p, &row("claude", Surface::Widget)).unwrap();
    assert_eq!((k.x, w.x), (claude.x, own.x));
}

/// The tray icon's look: a tile each, the chosen one outlined; a click on
/// another asks for it, on the chosen one nothing.
#[test]
fn the_tray_icon_look_is_a_tile() {
    let mut p = picker(input());
    assert!(area(&p, &Target::TrayStyle(TrayStyle::Bars)).is_none());
    on_tab(&mut p, Tab::Shows);
    let tiles = |p: &Picker| {
        p.frame()
            .els
            .iter()
            .filter_map(|(_, el)| match el {
                El::Tile { style, on, .. } => Some((*style, *on)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tiles(&p),
        [
            (TrayStyle::Rings, true),
            (TrayStyle::Bars, false),
            (TrayStyle::Number, false),
            (TrayStyle::Sector, false)
        ]
    );
    assert!(click(&mut p, &Target::TrayStyle(TrayStyle::Rings)).is_empty());
    assert_eq!(
        click(&mut p, &Target::TrayStyle(TrayStyle::Sector)),
        [PickerAction::SetTrayStyle(TrayStyle::Sector)]
    );
    let mut i = input();
    i.tray_style = TrayStyle::Number;
    p.set_input(i);
    assert!(tiles(&p).contains(&(TrayStyle::Number, true)));
}

/// The tabs: Rows on opening; a click shows the other at its top; the
/// window keeps one height for both.
#[test]
fn the_tabs_switch_the_list_and_keep_the_height() {
    let mut p = picker(with_model());
    assert_eq!(p.tab, Tab::Rows);
    assert!(area(&p, &row("claude", Surface::Widget)).is_some());
    assert!(area(&p, &Target::Names).is_none());
    let size = p.window_size();
    p.handle(PickerEvent::Wheel { dy: -48.0 });
    assert!(click(&mut p, &Target::Tab(Tab::Shows)).is_empty());
    assert_eq!((p.tab, p.scroll), (Tab::Shows, 0.0));
    assert!(area(&p, &Target::Names).is_some());
    assert!(area(&p, &row("claude", Surface::Widget)).is_none());
    assert_eq!(p.window_size(), size);
    // Shown in the header, above the list.
    let (layer, r) = area(&p, &Target::Tab(Tab::Rows)).unwrap();
    assert_eq!(layer, Layer::Chrome);
    assert!(bottom(&r) <= p.frame().viewport.y);
}

/// A description waits for the pointer to rest on what it describes,
/// and the next one, straight after, follows sooner.
#[test]
fn a_description_waits_for_the_pointer_to_rest() {
    let mut p = picker(input());
    p.hint_delay = std::time::Duration::from_millis(300);
    let (_, y) = spot(&mut p, &row("claude.opus", Surface::Widget));
    // The outline is always on top; a description adds its box.
    let base = p.scene().top.len();
    let hint = |p: &Picker| p.scene().top.len() > base;
    // On the row's label, left of its ticks.
    p.handle(PickerEvent::PointerMove { x: PAD_X + 30.0, y });
    assert!(!hint(&p));
    let due = p.next_wake().expect("a description is due");
    assert!(due > std::time::Instant::now());
    std::thread::sleep(std::time::Duration::from_millis(320));
    assert!(hint(&p));
    assert!(p.next_wake().is_none());
    p.handle(PickerEvent::PointerLeave);
    assert!(!hint(&p));
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

/// Losing the focus to another window, or a click beside the box, keeps
/// the window and answers nothing: the welcome and the notice are owed
/// still. Esc, the cross, Alt+F4 and a button are answers.
#[test]
fn losing_the_focus_keeps_the_window_and_answers_nothing() {
    for input in [welcome(vec!["claude"]), welcome(Vec::new()), notice()] {
        let mut p = picker(input);
        assert!(p.handle(PickerEvent::FocusLost).is_empty());
        assert!(p
            .handle(PickerEvent::PointerDown { x: 3.0, y: 3.0 })
            .is_empty());
        assert!(!p.notice_done);
        assert_eq!(
            p.handle(PickerEvent::CloseRequested),
            [PickerAction::NoticeDone, PickerAction::Close]
        );
    }
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
    // Not typing: the window stays all the same.
    key(&mut p, PickerKey::Tab);
    assert!(!p.editing());
    assert!(p.handle(PickerEvent::FocusLost).is_empty());
}

/// Esc in a name field takes the edit back to the name it had; a second
/// Esc, with nothing to take back, closes the window.
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
    let mut one = picker(i);
    on_tab(&mut one, Tab::Shows);
    let text = texts(&one);
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
    // Drawn where they are: Allow's face is the accent (the pointer gone,
    // so nothing is hovered).
    if let Some((renderer, scale)) = draw {
        p.handle(PickerEvent::PointerLeave);
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
        on_tab(p, Tab::Shows);
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
    on_tab(&mut closed, Tab::Shows);
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
    on_tab(&mut catalogue, Tab::Shows);
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

    // Asked once: Forget permission under Claude's Sign-in, its
    // description under the pointer; and under the model label's note.
    let mut forget = make(i.clone());
    forget.open_sign_in = vec!["claude".into()];
    let (x, y) = spot(&mut forget, &Target::Forget("claude".into()));
    forget.handle(PickerEvent::PointerMove { x, y });
    items.push((
        "signin-forget-hint-dark".to_string(),
        draw(&forget, false, 1.0),
    ));
    let mut allowed = make(with_model());
    on_tab(&mut allowed, Tab::Shows);
    items.push(("shows-allowed-dark".to_string(), draw(&allowed, false, 1.0)));
    let (x, y) = spot(&mut allowed, &Target::ForgetModelReads);
    allowed.handle(PickerEvent::PointerMove { x, y });
    items.push((
        "shows-forget-hint-dark".to_string(),
        draw(&allowed, false, 1.0),
    ));

    // Model names with Effort alone shown: the fields are there, and a
    // line says where the names show.
    let mut i = input();
    i.reads.tool_settings = true;
    i.model.widget = vec![ModelPart::Effort];
    let mut effort_only = make(i);
    on_tab(&mut effort_only, Tab::Shows);
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

    // "Settings file..." under the pointer: underlined, its description
    // above it; and with the keyboard's focus ring.
    for (light, name) in [(false, "settings-file-dark"), (true, "settings-file-light")] {
        let mut link = make(input());
        let (x, y) = spot(&mut link, &Target::SettingsFile);
        link.handle(PickerEvent::PointerMove { x, y });
        items.push((name.to_string(), draw(&link, light, 1.0)));
    }
    let mut link = make(input());
    on_tab(&mut link, Tab::Shows);
    key(&mut link, PickerKey::End);
    items.push((
        "settings-file-focus-dark".to_string(),
        draw(&link, false, 1.0),
    ));

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
    on_tab(&mut parts, Tab::Shows);
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

    // Weeks of models Claude added later, rows of their own after Fable's;
    // a long name ends in "…" short of the tick columns.
    let mut added = input();
    let made = made_rows();
    let at = added
        .rows
        .iter()
        .position(|r| r.id == "claude.fable")
        .unwrap()
        + 1;
    added.rows.splice(at..at, made);
    added.widget_rows.push("claude.haiku".into());
    items.push((
        "rows-new-models-dark".to_string(),
        draw(&make(added.clone()), false, 1.0),
    ));
    items.push((
        "rows-new-models-light".to_string(),
        draw(&make(added), true, 1.0),
    ));

    // The What shows tab as it opens; the keyboard's ring on its tab.
    let mut shows = make(input());
    on_tab(&mut shows, Tab::Shows);
    items.push(("shows-dark".to_string(), draw(&shows, false, 1.0)));
    items.push(("shows-light".to_string(), draw(&shows, true, 1.0)));
    items.push(("shows-dark-150".to_string(), draw(&shows, false, 1.5)));
    shows.focus = Some(Target::Tab(Tab::Shows));
    shows.focus_visible = true;
    items.push(("tab-focus-dark".to_string(), draw(&shows, false, 1.0)));

    // The widget down to its reset time: that tick stays, and says why
    // under the pointer.
    let mut i = input();
    i.widget = UiNumbers {
        percent: false,
        reset: true,
        bar: false,
    };
    let mut lock = make(i);
    let (x, y) = spot(&mut lock, &Target::Tip(TipPart::Reset, NumbersOn::Widget));
    lock.handle(PickerEvent::PointerMove { x, y });
    items.push(("shows-lock-dark".to_string(), draw(&lock, false, 1.0)));
    items.push(("shows-lock-light".to_string(), draw(&lock, true, 1.0)));

    // The tray icon's looks: Sector chosen, Bars under the pointer, the
    // ring on Number; dark, light and high contrast.
    let mut i = input();
    i.tray_style = TrayStyle::Sector;
    let mut tiles = make(i);
    spot(&mut tiles, &Target::TrayStyle(TrayStyle::Number));
    tiles.focus = Some(Target::TrayStyle(TrayStyle::Number));
    tiles.focus_visible = true;
    let (x, y) = spot(&mut tiles, &Target::TrayStyle(TrayStyle::Bars));
    tiles.handle(PickerEvent::PointerMove { x, y });
    items.push(("tray-tiles-dark".to_string(), draw(&tiles, false, 1.0)));
    items.push(("tray-tiles-light".to_string(), draw(&tiles, true, 1.0)));
    items.push(("tray-tiles-dark-150".to_string(), draw(&tiles, false, 1.5)));
    items.push((
        "tray-tiles-contrast-white".to_string(),
        draw_with(&tiles, &white, 1.0),
    ));

    vb::check("picker", items);
}

/// The keyboard on a provider's own row: the Taskbar tick's description
/// shows for that tick alone, never for the Widget or Tooltip tick beside
/// it (it used to, being the one hint on the row; E2E 2026-11-22).
#[test]
fn a_focused_tick_shows_its_own_description_only() {
    let mut p = picker(with_model());
    let shown = |p: &Picker| {
        let f = p.frame();
        p.focus_hint(&f).and_then(|i| match &f.els[i].1 {
            El::Hint { text, .. } => Some(text.clone()),
            _ => None,
        })
    };
    let widget = row("claude", Surface::Widget);
    while p.focus != Some(widget.clone()) {
        key(&mut p, PickerKey::Tab);
    }
    assert_ne!(shown(&p).as_deref(), Some(TASKBAR_HINT), "Widget tick");
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(row("claude", Surface::Tooltip)));
    assert_ne!(shown(&p).as_deref(), Some(TASKBAR_HINT), "Tooltip tick");
    key(&mut p, PickerKey::Tab);
    assert_eq!(p.focus, Some(Target::Taskbar("claude".into())));
    assert_eq!(shown(&p).as_deref(), Some(TASKBAR_HINT), "Taskbar tick");
    // An extra row keeps its row's description on every tick.
    let fable = row("claude.fable", Surface::Widget);
    while p.focus != Some(fable.clone()) {
        key(&mut p, PickerKey::Tab);
    }
    let fable_hint = crate::rows::SUB_ROWS
        .iter()
        .find(|r| r.id == "claude.fable")
        .unwrap()
        .hint;
    assert_eq!(shown(&p).as_deref(), Some(fable_hint));
}

/// Frames for the README's Settings GIFs, drawn by the app itself: the
/// window and, beside it, the widget its ticks make. One folder a scene
/// under `GIF_FRAMES`, a picker and a widget PNG a step, and `steps.json`
/// with where the pointer goes (window pixels) and whether it clicks.
/// design/gifgen/settings_gif.py lays them out and encodes. Run on demand:
/// `GIF_FRAMES=<dir> cargo test --lib gif_frames_settings -- --ignored`.
#[test]
#[ignore = "writes GIF frames on demand"]
fn gif_frames_settings() {
    use crate::ui::renderer::tests::{rows_fixture, rows_widget, widget_rows};
    let Ok(dir) = std::env::var("GIF_FRAMES") else {
        return;
    };
    let renderer = Rc::new(Renderer::new().expect("a system font"));
    // As the window draws its text, grayscale: ClearType is tuned to one
    // screen's subpixels and fringes in a picture seen on another.
    renderer.set_picker_text(crate::ui::renderer::PickerText::Gdi(
        crate::ui::gdi_text::Smoothing::Gray,
    ));
    let fixture = rows_fixture();
    let make = |input: PickerInput| {
        let (r, b) = (renderer.clone(), renderer.clone());
        Picker::new(input, move |t, s| r.picker_width(t, s, false, 1.0))
            .with_bold_measure(move |t, s| b.picker_width(t, s, true, 1.0))
    };
    let draw = |p: &Picker| {
        let (w, h) = p.window_size();
        let mut pm = tiny_skia::Pixmap::new(w.ceil() as u32, h.ceil() as u32).unwrap();
        renderer.draw_picker(
            &mut pm,
            &p.scene(),
            &PickerTheme::for_system(false),
            1.0,
            &mut PickerGlyphs::default(),
        );
        pm
    };
    let widget = |ids: &[String], numbers: UiNumbers| {
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let rows = widget_rows(&fixture, Some(&ids), None);
        let ui = crate::config::schema::UIConfig {
            widget: numbers,
            ..Default::default()
        };
        rows_widget(&renderer, &rows, &ui, false, None)
    };
    struct Scene {
        name: &'static str,
        steps: Vec<String>,
        n: usize,
    }
    let save =
        |sc: &mut Scene, p: &Picker, w: &tiny_skia::Pixmap, at: (f32, f32), clicked: bool| {
            let d = std::path::Path::new(&dir).join(sc.name);
            std::fs::create_dir_all(&d).unwrap();
            draw(p)
                .save_png(d.join(format!("{:02}-picker.png", sc.n)))
                .unwrap();
            w.save_png(d.join(format!("{:02}-widget.png", sc.n)))
                .unwrap();
            sc.steps.push(format!(
                "{{\"i\": {}, \"x\": {}, \"y\": {}, \"click\": {}}}",
                sc.n, at.0, at.1, clicked
            ));
            sc.n += 1;
        };
    let finish = |sc: Scene| {
        let d = std::path::Path::new(&dir).join(sc.name);
        std::fs::write(d.join("steps.json"), format!("[{}]", sc.steps.join(","))).unwrap();
    };

    // Providers: a row ticked shows at once; a provider's Sign-in opens.
    let mut sc = Scene {
        name: "settings-providers",
        steps: Vec::new(),
        n: 0,
    };
    let mut i = input();
    i.sign_in = sign_ins();
    let mut ids: Vec<String> = vec!["claude".into(), "codex".into()];
    i.widget_rows = ids.clone();
    let mut p = make(i.clone());
    let numbers = UiNumbers::default();
    save(&mut sc, &p, &widget(&ids, numbers), (300.0, 520.0), false);
    let t = row("claude.opus", Surface::Widget);
    let at = spot(&mut p, &t);
    save(&mut sc, &p, &widget(&ids, numbers), at, false);
    click(&mut p, &t);
    p.handle(PickerEvent::PointerLeave);
    ids = vec!["claude".into(), "claude.opus".into(), "codex".into()];
    i.widget_rows = ids.clone();
    p.set_input(i.clone());
    save(&mut sc, &p, &widget(&ids, numbers), at, true);
    let t = Target::SignIn("claude".into());
    let at = spot(&mut p, &t);
    save(&mut sc, &p, &widget(&ids, numbers), at, false);
    click(&mut p, &t);
    p.handle(PickerEvent::PointerLeave);
    save(&mut sc, &p, &widget(&ids, numbers), at, true);
    // The pointer goes back to the widget: the description goes.
    p.handle(PickerEvent::PointerLeave);
    save(&mut sc, &p, &widget(&ids, numbers), (-70.0, 150.0), false);
    finish(sc);

    // Display: what each row shows; without the bar the widget is smaller.
    let mut sc = Scene {
        name: "settings-display",
        steps: Vec::new(),
        n: 0,
    };
    let mut i = input();
    let ids: Vec<String> = vec!["claude".into(), "codex".into(), "grok".into()];
    i.widget_rows = ids.clone();
    let mut numbers = UiNumbers::default();
    let mut p = make(i.clone());
    on_tab(&mut p, Tab::Shows);
    save(&mut sc, &p, &widget(&ids, numbers), (300.0, 520.0), false);
    for (part, on) in [
        (TipPart::Bar, false),
        (TipPart::Reset, false),
        (TipPart::Reset, true),
        (TipPart::Bar, true),
    ] {
        let t = Target::Tip(part, NumbersOn::Widget);
        let at = spot(&mut p, &t);
        save(&mut sc, &p, &widget(&ids, numbers), at, false);
        click(&mut p, &t);
        p.handle(PickerEvent::PointerLeave);
        match part {
            TipPart::Bar => numbers.bar = on,
            TipPart::Reset => numbers.reset = on,
            TipPart::Percent => numbers.percent = on,
        }
        i.widget = numbers;
        p.set_input(i.clone());
        save(&mut sc, &p, &widget(&ids, numbers), at, true);
    }
    finish(sc);
}

/// The note after a paste, as it reads now and with the clipboard hint, for
/// the owner to compare. Run on demand:
/// `MOCK_FRAMES=<dir> cargo test --lib clipboard_hint_mockup -- --ignored`.
#[test]
#[ignore = "writes mockups on demand"]
fn clipboard_hint_mockup() {
    use crate::ui::gdi_text::Smoothing;
    use crate::ui::renderer::PickerText;
    let Ok(dir) = std::env::var("MOCK_FRAMES") else {
        return;
    };
    let renderer = Rc::new(Renderer::new().expect("a system font"));
    renderer.set_picker_text(PickerText::Gdi(Smoothing::Gray));
    const HINT: &str = " It is still on the clipboard: copy something else, and if clipboard history is on, remove it there (Win+V).";
    let notes = [
        (
            "key",
            "Stored in Windows Credential Manager. The row says whether it works.",
        ),
        ("token", "Verified and saved to Windows Credential Manager."),
    ];
    for (kind, now) in notes {
        for (name, text) in [("now", now.to_string()), ("hint", format!("{now}{HINT}"))] {
            let mut i = input();
            i.sign_in = sign_ins();
            for s in i.sign_in.iter_mut() {
                if s.provider == "copilot" {
                    s.note = Some(text.clone());
                }
            }
            let (r, b) = (renderer.clone(), renderer.clone());
            let mut p = Picker::new(i, move |t, s| r.picker_width(t, s, false, 1.0))
                .with_bold_measure(move |t, s| b.picker_width(t, s, true, 1.0));
            p.open_sign_in = vec!["copilot".into()];
            p.settle();
            p.scroll = p.frame().max_scroll();
            p.settle();
            for light in [false, true] {
                let (w, h) = p.window_size();
                let mut pm = tiny_skia::Pixmap::new(w.ceil() as u32, h.ceil() as u32).unwrap();
                renderer.draw_picker(
                    &mut pm,
                    &p.scene(),
                    &PickerTheme::for_system(light),
                    1.0,
                    &mut PickerGlyphs::default(),
                );
                let theme = if light { "light" } else { "dark" };
                pm.save_png(std::path::Path::new(&dir).join(format!("{kind}-{name}-{theme}.png")))
                    .unwrap();
            }
        }
    }
}

/// The window drawn three ways for the owner to compare: fontdue as now,
/// Windows grayscale and Windows ClearType (`gdi_text`), at 100% and 150%.
/// Run on demand: `TEXT_FRAMES=<dir> cargo test --lib picker_text_engines -- --ignored`.
#[test]
#[ignore = "writes comparison renders on demand"]
fn picker_text_engines() {
    use crate::ui::gdi_text::Smoothing;
    use crate::ui::renderer::PickerText;
    let Ok(dir) = std::env::var("TEXT_FRAMES") else {
        return;
    };
    let renderer = Rc::new(Renderer::new().expect("a system font"));
    let make = |input: PickerInput| {
        let r = renderer.clone();
        Picker::new(input, move |t, s| r.text_width(t, s))
    };
    let mut normal = make(input());
    let mut i = input();
    i.sign_in = sign_ins();
    let mut signin = make(i);
    signin.open_sign_in = vec!["claude".into()];
    let mut shows = make(with_model());
    on_tab(&mut shows, Tab::Shows);
    let _ = (&mut normal, &mut signin);
    for (how, name) in [
        (PickerText::Fontdue, "now"),
        (PickerText::Gdi(Smoothing::Gray), "gray"),
        (PickerText::Gdi(Smoothing::ClearType), "cleartype"),
    ] {
        renderer.set_picker_text(how);
        for (p, state) in [
            (&normal, "providers"),
            (&signin, "signin"),
            (&shows, "display"),
        ] {
            for (scale, light) in [(1.0f32, false), (1.5, false), (1.0, true)] {
                let (w, h) = p.window_size();
                let mut pm =
                    tiny_skia::Pixmap::new((w * scale).ceil() as u32, (h * scale).ceil() as u32)
                        .unwrap();
                renderer.draw_picker(
                    &mut pm,
                    &p.scene(),
                    &PickerTheme::for_system(light),
                    scale,
                    &mut PickerGlyphs::default(),
                );
                let theme = if light { "-light" } else { "" };
                let path = std::path::Path::new(&dir).join(format!(
                    "{state}-{}{theme}-{name}.png",
                    (scale * 100.0) as u32
                ));
                pm.save_png(path).unwrap();
            }
        }
    }
    renderer.set_picker_text(PickerText::Fontdue);
}
