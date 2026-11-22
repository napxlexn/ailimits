// config/annotate.rs — config.toml as the app writes it.
//
// The settings, with a short note before each table and each key: what it
// does, the values it takes, its default, and the menu item or window that
// sets the same. The four `[tune]` tables are always written, every key not
// set commented out with its default, so taking the `# ` away is all it
// takes to use one. Every write of config.toml goes through `annotate`
// (storage.rs); what it writes reads back as the same config.

use super::schema::Config;
use super::tune;

/// The block at the top of the file.
const HEADER: &[&str] = &[
    "AI Limits settings. The app writes this file, and you may edit it too:",
    "a saved edit applies within about two seconds, with no restart.",
    "A line that does not read is left out and its default used: a notification",
    "names the line, and the rest still applies. So trying things is safe.",
    "A number out of its range is held to the nearest end of it.",
    "To undo a change, delete the line: its default comes back.",
    "Guide: docs/en/TUNING.md and docs/en/CONFIG.md (in Ukrainian: docs/uk/)",
    "at https://github.com/napxlexn/ailimits",
    "Comments you add are not kept: the app writes its own when it saves.",
];

/// A table of the file and the notes written with it.
struct Table {
    /// Its path: "general", "ui.rows", ...
    path: &'static str,
    /// The note before its header.
    about: &'static [&'static str],
    /// Its keys in the order written, each with the note before it. A
    /// table of its own below this one (`ui.rows` under `ui`) is not a key
    /// here but a `Table` of its own.
    keys: &'static [(&'static str, &'static [&'static str])],
}

/// Every table but `[tune]`, in the order written. The tests hold the keys
/// to the fields of the structs in schema.rs.
const TABLES: &[Table] = &[
    Table {
        path: "general",
        about: &["How often the app updates, and the taskbar indicator."],
        keys: &[
            (
                "update_interval_secs",
                &[
                    "Seconds between updates: 60 or more (less reads as 60); default 60.",
                    "Menu: Update interval (1, 5, 15, 30 min); other values work here too.",
                ],
            ),
            (
                "indicator",
                &[
                    "On the taskbar: \"tray\" (rings icon, default), \"panel_rows\" (panel),",
                    "\"off\", \"bars\" (older icon), \"panel_grid\" (as panel_rows). Menu: Indicator.",
                ],
            ),
            (
                "auto_update",
                &[
                    "Install new versions by itself: true (default) or false.",
                    "Menu: Automatic updates (in a copy from the installer only).",
                ],
            ),
            (
                "panel_offset_x",
                &[
                    "Moves the taskbar panel right (+) or left (-): -200 to 200 pixels;",
                    "default 0. For taskbars where it sits wrong. No menu item.",
                ],
            ),
            (
                "panel_offset_y",
                &["Moves the taskbar panel down (+) or up (-): -200 to 200 pixels; default 0."],
            ),
            (
                "panel_display",
                &[
                    "Taskbar of the panel: \"primary\" (default) or { secondary = N }:",
                    "N = 0 is Display 2, 1 is Display 3... Menu: Indicator > Display.",
                ],
            ),
        ],
    },
    Table {
        path: "window",
        about: &["The floating widget."],
        keys: &[
            (
                "pos_x",
                &[
                    "Left edge on the screen, pixels; default 50. Dragging the widget sets",
                    "it; a place on no monitor comes back onto one.",
                ],
            ),
            (
                "pos_y",
                &["Top edge on the screen, pixels; default 50."],
            ),
            (
                "opacity",
                &[
                    "Background opacity: 0.10 to 0.85; default 0.45.",
                    "Menu: Background opacity (15% to 85%); any value between works here.",
                ],
            ),
            (
                "pinned",
                &["Always on top: true or false (default). Menu: Always on top."],
            ),
            (
                "locked",
                &["No dragging: true or false (default). Menu: Lock position."],
            ),
        ],
    },
    Table {
        path: "ui",
        about: &[
            "How the widget looks. [ui.rows], [ui.model], [ui.widget], [ui.tooltip],",
            "[ui.panel] and [ui.tray] appear below once set in Rows and models.",
        ],
        keys: &[
            (
                "palette",
                &[
                    "Colours: \"default\" (default), \"ocean\", \"sunset\", \"forest\", \"neon\",",
                    "\"ice\", \"rose\" or \"slate\". Menu: Palette.",
                ],
            ),
            (
                "saturation",
                &[
                    "Colour strength, percent: 0 (grey) to 100; default 55.",
                    "Menu: Saturation (0% to 100% in steps of 25); any value between works.",
                ],
            ),
            (
                "brightness",
                &[
                    "Text and bar brightness, percent: 20 to 100; default 100.",
                    "Menu: Brightness (40%, 60%, 80%, 100%); any value between works.",
                ],
            ),
            (
                "monochrome",
                &["Greys instead of the palette: true or false (default). Menu: Palette."],
            ),
            (
                "layout",
                &[
                    "\"vertical\" (default): providers stacked, bars lying (menu: Horizontal);",
                    "\"horizontal\": providers side by side, bars standing (menu: Vertical).",
                ],
            ),
            (
                "detail",
                &[
                    "How much a row says: \"compact\" (default), \"medium\" or \"expanded\".",
                    "Menu: Compact, Medium, Expanded.",
                ],
            ),
            (
                "width_scale",
                &[
                    "Width in the Horizontal layout: \"full\" (default), \"three_quarters\" or",
                    "\"half\". Menu: Width (100%, 75%, 50%).",
                ],
            ),
            (
                "column_flow",
                &[
                    "In the Vertical layout: \"row\" (side by side, default) or \"column\"",
                    "(stacked). Menu: Arrangement (In a row, In a column).",
                ],
            ),
            (
                "show_forecast",
                &[
                    "Time to the limit (\"~1h 30min\") while usage climbs and no reset is",
                    "known: true or false (default). Menu: Forecast.",
                ],
            ),
        ],
    },
    Table {
        path: "ui.rows",
        about: &[
            "Rows shown, by id: a provider (claude, codex, copilot, antigravity, grok,",
            "cursor) or an extra row. Window: Rows and models.",
        ],
        keys: &[
            (
                "widget",
                &[
                    "The widget's rows. Extra rows: claude.opus, claude.sonnet, claude.fable,",
                    "claude.<new model>, antigravity.cg, grok.build, cursor.bot. Unset: one each.",
                ],
            ),
            (
                "tooltip",
                &["The rows of the panel's and the tray icon's tooltip, as above."],
            ),
            (
                "taskbar",
                &[
                    "Providers on the taskbar panel and the tray icon, at most 2. Without",
                    "it: both show the first two the widget shows.",
                ],
            ),
        ],
    },
    Table {
        path: "ui.model",
        about: &[
            "Model labels (\"Opus 5.5 xhigh\"), read from the tools' settings files",
            "([reads] tool_settings). Window: Rows and models.",
        ],
        keys: &[
            (
                "widget",
                &["Label parts in the widget: \"model\", \"version\" (with \"model\"), \"effort\"."],
            ),
            (
                "tooltip",
                &["Label parts in the taskbar panel's tooltip, as above; [] for none."],
            ),
            (
                "source",
                &[
                    "\"configured\" (default): the model set in the tool; \"last_used\": the",
                    "newest Claude Code or Codex CLI session ([reads] last_session).",
                ],
            ),
        ],
    },
    Table {
        path: "ui.model.names",
        about: &[
            "Your names for models, by the id the tool keeps: \"id\" = \"name\", about",
            "10 characters at most. Delete a line to get the automatic name back.",
        ],
        keys: &[],
    },
    Table {
        path: "ui.widget",
        about: &[
            "What each widget row shows besides its name; keep one true. Window:",
            "Rows and models, What shows, Widget column.",
        ],
        keys: &[
            (
                "percent",
                &["The percentage used: true (default) or false."],
            ),
            (
                "reset",
                &["The time left until the reset: true (default) or false."],
            ),
            (
                "bar",
                &[
                    "The progress bar: true (default) or false; without it each row is",
                    "one line, each column only its texts, and the widget smaller.",
                ],
            ),
        ],
    },
    Table {
        path: "ui.tooltip",
        about: &[
            "What each tooltip row (panel and tray icon) shows besides its name;",
            "keep one true. Window: Rows and models, What shows, Tooltip column.",
        ],
        keys: &[
            (
                "percent",
                &["The percentage used: true (default) or false."],
            ),
            (
                "reset",
                &["The time left until the reset: true (default) or false."],
            ),
            ("bar", &["Not used in the tooltip."]),
        ],
    },
    Table {
        path: "ui.panel",
        about: &[
            "What each row of the taskbar panel shows; keep one true. Window: Rows",
            "and models, What shows, Panel column.",
        ],
        keys: &[
            (
                "percent",
                &["The percentage used: true (default) or false."],
            ),
            (
                "reset",
                &["The time left until the reset: true or false (default)."],
            ),
            (
                "bar",
                &["The progress bar: true (default) or false, for a narrower panel."],
            ),
        ],
    },
    Table {
        path: "ui.tray",
        about: &["The tray icon's look. Window: Rows and models, What shows, Tray icon."],
        keys: &[(
            "style",
            &[
                "\"rings\" (default), \"bars\" (two upright bars), \"number\" (the busier's",
                "percentage) or \"sector\" (its slice of a disc).",
            ],
        )],
    },
    Table {
        path: "providers",
        about: &[
            "One table per provider: claude, codex, copilot, antigravity, grok and",
            "cursor. A provider runs when it is enabled and allowed in [reads].",
        ],
        keys: &[
            ("id", &["Which provider the table is for; leave it as it is."]),
            (
                "enabled",
                &[
                    "Switched on: true or false. Ticking a row of it in Rows and models",
                    "switches it on, unticking its last row switches it off.",
                ],
            ),
            (
                "auth_method",
                &[
                    "Sign-in used: \"subscription\" (the tool's own), \"api_key\" (claude,",
                    "copilot) or \"usage_token\" (claude, codex). Rows and models: its Sign-in.",
                ],
            ),
            (
                "credential_label",
                &["Set by the app (the name of the stored key); leave it as it is."],
            ),
            (
                "alert_threshold",
                &[
                    "Notify when usage reaches this percent: 0 to 100 (over 100: never);",
                    "default 80, copilot 85. No menu item.",
                ],
            ),
        ],
    },
    Table {
        path: "notifications",
        about: &["Usage alerts, as Windows notifications."],
        keys: &[
            (
                "enabled",
                &[
                    "Show the alerts: true (default) or false. No menu item. The",
                    "on_threshold hook runs either way.",
                ],
            ),
            (
                "cooldown_minutes",
                &[
                    "Least time between two alerts of one provider, in minutes: 1 or more",
                    "(0 reads as 1); default 15. No menu item.",
                ],
            ),
        ],
    },
    Table {
        path: "hooks",
        about: &[
            "Commands run on usage events through cmd.exe, hidden, stopped after 30 s;",
            "\"\" (default) runs nothing. Never put a password or a key in one.",
        ],
        keys: &[
            (
                "on_threshold",
                &[
                    "Runs with the alert, once per limit window. It gets AILIMITS_EVENT,",
                    "AILIMITS_PROVIDER, AILIMITS_PERCENT and AILIMITS_RESET_AT.",
                ],
            ),
            (
                "on_reset",
                &[
                    "Runs when usage at or over the threshold drops by more than 30 points",
                    "(the limit reset). The same variables.",
                ],
            ),
            (
                "on_startup",
                &["Runs once, after the first successful update. The same variables."],
            ),
        ],
    },
    Table {
        path: "reads",
        about: &[
            "What the app may read of other tools' files. Editing this counts as your",
            "consent, as a tick in Rows and models does.",
        ],
        keys: &[
            (
                "sign_in",
                &["Providers whose tool's sign-in may be read, by id: [\"claude\", \"codex\"]."],
            ),
            (
                "app_sign_in",
                &["Providers set up with a key, token or sign-in of the app's own, by id."],
            ),
            (
                "tool_settings",
                &["Read the tools' settings files for the model labels: true or false (default)."],
            ),
            (
                "last_session",
                &[
                    "Read the newest Claude Code or Codex CLI session for the model last",
                    "used: true or false (default).",
                ],
            ),
            (
                "pending",
                &["The app's own (a window it still has to show); leave it as it is."],
            ),
            ("sources_settled", &["The app's own; leave it as it is."]),
        ],
    },
];

/// The note before the `[tune]` tables.
const TUNE_ABOUT: &[&str] = &[
    "Fine-tuning with no menu item (guide: docs/en/TUNING.md). To use a key,",
    "delete the \"# \" before it; never add a second table of the same name.",
    "The value shown is the default, unless its note says otherwise.",
    "A factor of 1.0 is the default size.",
];

/// The `[tune]` tables, each with the note before its header.
const TUNE_TABLES: &[(&str, &[&str])] = &[
    (
        "widget",
        &["The floating widget. Colours are \"#rrggbb\"; Monochrome ignores them."],
    ),
    (
        "panel",
        &["The taskbar panel. A panel too big for its place gives way to the tray icon."],
    ),
    (
        "tray",
        &[
            "The tray icon's rings, in pixels of its 32 x 32 drawing (2 are 1 on the",
            "screen at 100%). Twice ring_width plus ring_gap stays at most 14.",
        ],
    ),
    (
        "format",
        &["When the widget's colours change, and how it writes a time left."],
    ),
];

/// Where a note after a `[tune]` key starts.
const NOTE_COLUMN: usize = 26;

/// `config` as config.toml text, with its notes. Reads back as `config`:
/// should the text ever not (a bug here), the plain text is written
/// instead, without the notes, rather than a file that says something else.
pub fn annotate(config: &Config) -> Result<String, String> {
    let annotated = annotated(config)?;
    if reads_back_as(&annotated, config) {
        return Ok(annotated);
    }
    tracing::warn!("config.toml written without its notes: they did not read back the same");
    toml::to_string_pretty(config).map_err(|e| e.to_string())
}

/// Whether `text` reads as `config`. `tool_files` is never saved.
fn reads_back_as(text: &str, config: &Config) -> bool {
    let mut want = config.clone();
    for p in want.providers.iter_mut() {
        p.tool_files = false;
    }
    toml::from_str::<Config>(text).is_ok_and(|back| back == want)
}

/// The notes around the plain text. The plain text is read back as a
/// table first: the numbers then print as written (an f32 as `0.45`, not
/// as its f64), and the order and the layout are this file's.
fn annotated(config: &Config) -> Result<String, String> {
    let plain = toml::to_string(config).map_err(|e| e.to_string())?;
    let root: toml::Table = plain.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let mut out = Out::default();
    out.comment(HEADER);
    // Keys of no table (none today) go before the first header.
    for (key, value) in &root {
        if !is_section(value) {
            out.plain(key, value);
        }
    }
    for table in TABLES.iter().filter(|t| !t.path.contains('.')) {
        match root.get(table.path) {
            Some(toml::Value::Table(t)) => out.table(table.path, t, true),
            Some(toml::Value::Array(items)) if all_tables(items) => {
                for (i, item) in items.iter().enumerate() {
                    if let toml::Value::Table(t) = item {
                        out.array_item(table.path, t, i == 0);
                    }
                }
            }
            _ => {}
        }
    }
    // Tables this file does not know (none today), without notes.
    for (key, value) in &root {
        let known = key == "tune" || TABLES.iter().any(|t| t.path == key);
        if known || !is_section(value) {
            continue;
        }
        match value {
            toml::Value::Table(t) => out.table(&key_text(key), t, false),
            toml::Value::Array(items) => {
                for item in items.iter().filter_map(toml::Value::as_table) {
                    out.array_item(&key_text(key), item, false);
                }
            }
            _ => {}
        }
    }
    out.tune(root.get("tune").and_then(toml::Value::as_table));
    Ok(out.0)
}

/// A value written under a header of its own: a table, or a list of
/// tables (`[[providers]]`).
fn is_section(value: &toml::Value) -> bool {
    match value {
        toml::Value::Table(_) => true,
        toml::Value::Array(items) => all_tables(items),
        _ => false,
    }
}

/// A list of tables, and not an empty one (that is written `key = []`).
fn all_tables(items: &[toml::Value]) -> bool {
    !items.is_empty() && items.iter().all(toml::Value::is_table)
}

/// A key as TOML writes it: bare when it can be, else in quotes.
fn key_text(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if bare {
        return key.to_string();
    }
    let mut out = String::with_capacity(key.len() + 2);
    out.push('"');
    for c in key.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The text being written.
#[derive(Default)]
struct Out(String);

impl Out {
    fn line(&mut self, text: &str) {
        self.0.push_str(text);
        self.0.push('\n');
    }

    /// An empty line, unless the text starts here or has one already.
    fn blank(&mut self) {
        if !self.0.is_empty() && !self.0.ends_with("\n\n") {
            self.0.push('\n');
        }
    }

    fn comment(&mut self, lines: &[&str]) {
        for l in lines {
            self.line(&format!("# {l}"));
        }
    }

    /// `key = value`, with no note.
    fn plain(&mut self, key: &str, value: &toml::Value) {
        self.line(&format!("{} = {value}", key_text(key)));
    }

    /// A table under `[path]`, its keys and the tables below it.
    fn table(&mut self, path: &str, table: &toml::Table, explain: bool) {
        let doc = TABLES.iter().find(|t| t.path == path);
        self.blank();
        if let Some(doc) = doc.filter(|_| explain) {
            self.comment(doc.about);
        }
        self.line(&format!("[{path}]"));
        self.keys(path, table, doc, explain);
    }

    /// One table of a list of tables, under `[[path]]`: the notes before
    /// the first one only.
    fn array_item(&mut self, path: &str, table: &toml::Table, explain: bool) {
        let doc = TABLES.iter().find(|t| t.path == path);
        self.blank();
        if let Some(doc) = doc.filter(|_| explain) {
            self.comment(doc.about);
        }
        self.line(&format!("[[{path}]]"));
        self.keys(path, table, doc, explain);
    }

    /// The keys of the table at `path`, then the tables below it.
    fn keys(&mut self, path: &str, table: &toml::Table, doc: Option<&Table>, explain: bool) {
        let child = |key: &str, value: &toml::Value| {
            value.is_table() && TABLES.iter().any(|t| t.path == format!("{path}.{key}"))
        };
        let documented = |key: &str| doc.is_some_and(|d| d.keys.iter().any(|(k, _)| *k == key));
        let mut first = true;
        for (key, about) in doc.map_or(&[][..], |d| d.keys) {
            let Some(value) = table.get(*key) else {
                continue;
            };
            if child(key, value) {
                continue;
            }
            if explain {
                if !first {
                    self.blank();
                }
                self.comment(about);
            }
            self.plain(key, value);
            first = false;
        }
        for (key, value) in table {
            if !documented(key) && !child(key, value) {
                self.plain(key, value);
            }
        }
        for sub in TABLES {
            let Some(name) = sub
                .path
                .strip_prefix(path)
                .and_then(|r| r.strip_prefix('.'))
            else {
                continue;
            };
            if name.contains('.') {
                continue;
            }
            if let Some(toml::Value::Table(t)) = table.get(name) {
                self.table(sub.path, t, explain);
            }
        }
    }

    /// The four `[tune]` tables, always: the keys set as they are, every
    /// other key commented out with the value it has unset.
    fn tune(&mut self, tune: Option<&toml::Table>) {
        self.blank();
        self.comment(TUNE_ABOUT);
        let ours = |key: &str| TUNE_TABLES.iter().any(|(name, _)| *name == key);
        // Keys of `[tune]` itself (none today).
        let loose: Vec<(&String, &toml::Value)> = tune
            .into_iter()
            .flatten()
            .filter(|(k, v)| !(ours(k) && v.is_table()))
            .collect();
        if !loose.is_empty() {
            self.line("[tune]");
            for (key, value) in loose {
                self.plain(key, value);
            }
        }
        for (name, about) in TUNE_TABLES {
            let table = tune
                .and_then(|t| t.get(*name))
                .and_then(toml::Value::as_table);
            self.blank();
            self.comment(about);
            self.line(&format!("[tune.{name}]"));
            let docs = tune::KEY_DOCS.iter().filter(|d| d.table == *name);
            for d in docs.clone() {
                match table.and_then(|t| t.get(d.key)) {
                    Some(value) => {
                        let note = if d.default {
                            format!("{}; default {}", d.note, d.shown)
                        } else {
                            d.note.to_string()
                        };
                        self.noted(&format!("{} = {value}", d.key), &note);
                    }
                    None => self.noted(&format!("# {} = {}", d.key, d.shown), d.note),
                }
            }
            for (key, value) in table.into_iter().flatten() {
                if !docs.clone().any(|d| d.key == key) {
                    self.plain(key, value);
                }
            }
        }
    }

    /// `code`, then `# note` from `NOTE_COLUMN` on.
    fn noted(&mut self, code: &str, note: &str) {
        let pad = NOTE_COLUMN.saturating_sub(code.chars().count()).max(2);
        self.line(&format!("{code}{}# {note}", " ".repeat(pad)));
    }
}

/// The names of a struct's fields as the file spells them, for the tests
/// that hold the notes to the structs: serde passes them to
/// `deserialize_struct`, which is all this deserializer answers.
#[cfg(test)]
pub(crate) fn field_names<T: serde::de::DeserializeOwned>() -> &'static [&'static str] {
    struct Probe<'a>(&'a mut Option<&'static [&'static str]>);
    impl<'de> serde::Deserializer<'de> for Probe<'_> {
        type Error = serde::de::value::Error;
        fn deserialize_any<V: serde::de::Visitor<'de>>(
            self,
            _: V,
        ) -> Result<V::Value, Self::Error> {
            Err(serde::de::Error::custom("not a struct"))
        }
        fn deserialize_struct<V: serde::de::Visitor<'de>>(
            self,
            _: &'static str,
            fields: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Self::Error> {
            *self.0 = Some(fields);
            Err(serde::de::Error::custom("probed"))
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map enum identifier ignored_any
        }
    }
    let mut fields = None;
    let _ = T::deserialize(Probe(&mut fields));
    fields.expect("a struct with named fields")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{
        AuthMethod, ColumnFlow, DetailLevel, GeneralConfig, HooksConfig, IndicatorKind, Layout,
        ModelPart, ModelSource, NotificationConfig, Palette, PanelDisplay, ProviderConfig, Reads,
        UIConfig, UiModel, UiNumbers, UiRows, WidthScale, WindowConfig,
    };
    use crate::config::tune::{Corners, Countdown};

    fn read(text: &str) -> Config {
        toml::from_str(text).unwrap_or_else(|e| panic!("{e}\n{text}"))
    }

    /// A config with every optional table set: rows, labels, names, the
    /// tooltip and the panel, a secondary taskbar, hooks, sign-ins of both
    /// kinds and some `[tune]` keys.
    fn everything() -> Config {
        let mut c = Config::fresh_install();
        c.allow_sign_in("claude");
        c.allow_app_sign_in("cursor");
        c.reads.tool_settings = true;
        c.reads.last_session = true;
        c.reads.pending = crate::config::schema::PendingNotice::None;
        for p in c.providers.iter_mut() {
            p.enabled = matches!(p.id.as_str(), "claude" | "cursor" | "copilot");
        }
        c.providers[2].auth_method = AuthMethod::ApiKey;
        c.providers[2].credential_label = "copilot_pat".into();
        c.providers[1].auth_method = AuthMethod::UsageToken;
        c.providers[0].alert_threshold = 93;
        c.general.update_interval_secs = 300;
        c.general.indicator = IndicatorKind::PanelRows;
        c.general.auto_update = false;
        c.general.panel_offset_x = -12;
        c.general.panel_offset_y = 7;
        c.general.panel_display = PanelDisplay::Secondary(1);
        c.window.pos_x = -1900;
        c.window.pos_y = 333;
        c.window.opacity = 0.7;
        c.window.pinned = true;
        c.window.locked = true;
        c.ui.palette = Palette::Forest;
        c.ui.saturation = 30;
        c.ui.brightness = 70;
        c.ui.monochrome = true;
        c.ui.layout = Layout::Horizontal;
        c.ui.detail = DetailLevel::Expanded;
        c.ui.width_scale = WidthScale::Half;
        c.ui.column_flow = ColumnFlow::Column;
        c.ui.show_forecast = true;
        c.ui.rows.widget = Some(vec!["claude".into(), "claude.opus".into()]);
        c.ui.rows.tooltip = Some(Vec::new());
        c.ui.rows.taskbar = Some(vec!["cursor".into()]);
        c.ui.model.widget = vec![ModelPart::Model, ModelPart::Effort];
        c.ui.model.tooltip = vec![ModelPart::Model, ModelPart::Version];
        c.ui.model.source = ModelSource::LastUsed;
        c.ui.model
            .names
            .insert("claude-opus-5-5".into(), "Opus \"5\" é".into());
        c.ui.model
            .names
            .insert("copilot:auto".into(), "Auto".into());
        c.ui.model.names.insert("a\nb\\c".into(), "odd".into());
        c.ui.widget = UiNumbers {
            percent: false,
            reset: true,
            bar: false,
        };
        c.ui.tray.style = crate::config::schema::TrayStyle::Sector;
        c.ui.tooltip.percent = false;
        c.ui.tooltip.bar = false;
        c.ui.panel = UiNumbers {
            percent: true,
            reset: true,
            bar: false,
        };
        c.notifications.enabled = false;
        c.notifications.cooldown_minutes = 45;
        c.hooks.on_threshold = r#""C:\Program Files\x\notify.exe" --flag"#.into();
        c.hooks.on_reset = "echo '''x'''".into();
        c.hooks.on_startup = "line one\nline two".into();
        c.tune.widget.text_scale = Some(1.2);
        c.tune.widget.row_gap = Some(0.1);
        c.tune.widget.corners = Some(Corners::Small);
        c.tune.widget.low = Some("#00ff00".into());
        c.tune
            .widget
            .provider_colors
            .insert("claude".into(), "#d97757".into());
        c.tune.panel.tray_gap = Some(4);
        c.tune.panel.ink_dark = Some("teal".into());
        c.tune.tray.ring_gap = Some(3.3);
        c.tune.format.high_at = Some(90.5);
        c.tune.format.countdown = Some(Countdown::Long);
        c
    }

    /// What the app writes reads back as the config it wrote, whatever is
    /// set; never through the plain fallback.
    #[test]
    fn the_annotated_file_reads_back_as_the_config() {
        for config in [Config::default(), Config::fresh_install(), everything()] {
            let text = annotated(&config).unwrap();
            assert_eq!(read(&text), config, "\n{text}");
            assert_eq!(annotate(&config).unwrap(), text);
        }
    }

    /// The order of the file: the header, then the tables as listed.
    #[test]
    fn the_tables_are_written_in_order_with_their_notes() {
        let text = annotate(&everything()).unwrap();
        assert!(text.starts_with("# AI Limits settings."), "{text}");
        let headers: Vec<&str> = text.lines().filter(|l| l.starts_with('[')).collect();
        assert_eq!(
            headers,
            [
                "[general]",
                "[window]",
                "[ui]",
                "[ui.rows]",
                "[ui.model]",
                "[ui.model.names]",
                "[ui.widget]",
                "[ui.tooltip]",
                "[ui.panel]",
                "[ui.tray]",
                "[[providers]]",
                "[[providers]]",
                "[[providers]]",
                "[[providers]]",
                "[[providers]]",
                "[[providers]]",
                "[notifications]",
                "[hooks]",
                "[reads]",
                "[tune.widget]",
                "[tune.panel]",
                "[tune.tray]",
                "[tune.format]",
            ]
        );
        // Each table and each key has its note right before it; the
        // providers' notes come once, before the first. Not the names (the
        // table's note covers them) nor `[tune]` (its notes follow the key).
        let lines: Vec<&str> = text.lines().collect();
        let mut header = "";
        let mut providers = 0;
        for (i, l) in lines.iter().enumerate() {
            if l.starts_with('[') {
                header = l;
                providers += usize::from(*l == "[[providers]]");
            }
            let is_key = l.contains(" = ") && !l.starts_with(['#', ' ', '"']);
            let quiet = (header == "[[providers]]" && providers > 1)
                || header == "[ui.model.names]"
                || header.starts_with("[tune.");
            let needs = if l.starts_with('[') {
                !(header == "[[providers]]" && providers > 1)
            } else {
                is_key && !quiet
            };
            if needs {
                assert!(
                    i > 0 && lines[i - 1].starts_with("# "),
                    "no note before {l:?}\n{text}"
                );
            }
        }
        assert_eq!(text.matches("# Which provider the table is for").count(), 1);
    }

    /// A note is at most two lines, a comment line at most 80 columns.
    #[test]
    fn the_notes_are_short() {
        for t in TABLES {
            assert!(t.about.len() <= 2, "{}", t.path);
            for (key, about) in t.keys {
                assert!((1..=2).contains(&about.len()), "{}.{key}", t.path);
            }
        }
        for (name, about) in TUNE_TABLES {
            assert!(about.len() <= 2, "{name}");
        }
        for config in [Config::fresh_install(), everything()] {
            let text = annotate(&config).unwrap();
            for l in text.lines().filter(|l| l.starts_with('#')) {
                assert!(
                    l.chars().count() <= 80,
                    "{} columns: {l}",
                    l.chars().count()
                );
            }
        }
    }

    /// Every field of every table has its note, and every note names a
    /// field: a field added without one fails here.
    #[test]
    fn every_field_has_a_note_and_every_note_a_field() {
        fn keys(path: &str) -> Vec<&'static str> {
            let t = TABLES.iter().find(|t| t.path == path).unwrap();
            let mut keys: Vec<&str> = t.keys.iter().map(|(k, _)| *k).collect();
            // The tables below it count as its fields too.
            for sub in TABLES {
                if let Some(name) = sub
                    .path
                    .strip_prefix(path)
                    .and_then(|r| r.strip_prefix('.'))
                {
                    if !name.contains('.') {
                        keys.push(name);
                    }
                }
            }
            keys.sort_unstable();
            keys
        }
        fn fields(names: &[&'static str]) -> Vec<&'static str> {
            let mut v = names.to_vec();
            v.sort_unstable();
            v
        }
        assert_eq!(keys("general"), fields(field_names::<GeneralConfig>()));
        assert_eq!(keys("window"), fields(field_names::<WindowConfig>()));
        assert_eq!(keys("ui"), fields(field_names::<UIConfig>()));
        assert_eq!(keys("ui.rows"), fields(field_names::<UiRows>()));
        assert_eq!(keys("ui.model"), fields(field_names::<UiModel>()));
        assert_eq!(keys("ui.widget"), fields(field_names::<UiNumbers>()));
        assert_eq!(keys("ui.tooltip"), fields(field_names::<UiNumbers>()));
        assert_eq!(keys("ui.panel"), fields(field_names::<UiNumbers>()));
        assert_eq!(
            keys("ui.tray"),
            fields(field_names::<crate::config::schema::UiTray>())
        );
        assert_eq!(keys("providers"), fields(field_names::<ProviderConfig>()));
        assert_eq!(
            keys("notifications"),
            fields(field_names::<NotificationConfig>())
        );
        assert_eq!(keys("hooks"), fields(field_names::<HooksConfig>()));
        assert_eq!(keys("reads"), fields(field_names::<Reads>()));
        // The top: every table but [tune], which is written apart.
        let mut top: Vec<&str> = TABLES
            .iter()
            .map(|t| t.path)
            .filter(|p| !p.contains('.'))
            .chain(["tune"])
            .collect();
        top.sort_unstable();
        assert_eq!(top, fields(field_names::<Config>()));
        let tune: Vec<&str> = TUNE_TABLES.iter().map(|(n, _)| *n).collect();
        assert_eq!(tune, field_names::<crate::config::tune::Tune>());
    }

    /// The `[tune]` tables are always there: a key set as it is, with its
    /// note and default; every other key commented out with its default.
    #[test]
    fn the_tune_tables_are_always_written() {
        let fresh = annotate(&Config::fresh_install()).unwrap();
        for name in ["widget", "panel", "tray", "format"] {
            assert!(fresh.contains(&format!("\n[tune.{name}]\n")), "{fresh}");
        }
        for d in crate::config::tune::KEY_DOCS {
            let line = format!("# {} = {}", d.key, d.shown);
            assert!(
                fresh.lines().any(|l| l.starts_with(&format!("{line} "))),
                "{line}\n{fresh}"
            );
        }
        // Read: the tables written empty are the default.
        assert!(read(&fresh).tune.is_default());
        let text = annotate(&everything()).unwrap();
        assert!(
            text.lines()
                .any(|l| l.starts_with("text_scale = 1.2 ") && l.ends_with("; default 1.0")),
            "{text}"
        );
        assert!(text.contains("\n# bar_scale = 1.0 "), "{text}");
        assert!(
            text.contains("provider_colors = { claude = \"#d97757\" } "),
            "{text}"
        );
        // A float written as it was set, never as its f32 widened.
        assert!(text.contains("row_gap = 0.1 "), "{text}");
        assert!(text.contains("opacity = 0.7\n"), "{text}");
        assert!(!text.contains("0000"), "{text}");
    }

    /// Taking the `# ` away from a commented-out key gives the very line
    /// the app writes for that key set to its default.
    #[test]
    fn uncommenting_a_line_is_all_it_takes() {
        /// The lines of `[tune.<table>]` in `text`, from its header to the
        /// next empty line, and where they start.
        fn table_lines<'t>(text: &'t str, table: &str) -> (usize, Vec<&'t str>) {
            let header = format!("[tune.{table}]");
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.iter().position(|l| *l == header).unwrap();
            let block = lines[start..]
                .iter()
                .take_while(|l| !l.is_empty())
                .copied()
                .collect();
            (start, block)
        }
        let fresh = annotate(&Config::fresh_install()).unwrap();
        for d in crate::config::tune::KEY_DOCS {
            let (start, block) = table_lines(&fresh, d.table);
            let (i, commented) = block
                .iter()
                .enumerate()
                .find(|(_, l)| l.starts_with(&format!("# {} = ", d.key)))
                .unwrap();
            // The file with that one line uncommented.
            let mut lines: Vec<&str> = fresh.lines().collect();
            lines[start + i] = &commented[2..];
            let config = read(&lines.join("\n"));
            assert!(!config.tune.is_default(), "{} did not read", d.key);
            if d.default {
                let again = annotate(&config).unwrap();
                let set = format!("{} = {} ", d.key, d.shown);
                let (_, block) = table_lines(&again, d.table);
                assert!(block.iter().any(|l| l.starts_with(&set)), "{set}\n{again}");
            }
        }
    }

    /// Keys that need quotes get them, with what must be escaped.
    #[test]
    fn keys_are_quoted_where_they_must_be() {
        assert_eq!(key_text("claude-opus-5-5"), "claude-opus-5-5");
        assert_eq!(key_text("copilot:auto"), "\"copilot:auto\"");
        assert_eq!(key_text(""), "\"\"");
        assert_eq!(
            key_text("a \"b\"\\\n\u{1}"),
            "\"a \\\"b\\\"\\\\\\n\\u0001\""
        );
        let mut config = Config::default();
        for key in ["gpt 6", "x.y", "é", "\u{7f}", "'q'", "\"", "\t"] {
            config.ui.model.names.insert(key.into(), "N".into());
        }
        let text = annotated(&config).unwrap();
        assert_eq!(read(&text), config, "{text}");
    }

    /// Hand-written values the menu never sets still read back.
    #[test]
    fn unusual_values_read_back() {
        let mut c = everything();
        c.window.opacity = 0.123_456_7;
        c.ui.saturation = 255;
        c.general.update_interval_secs = u64::from(u32::MAX);
        // An f64 that is an f32 widened: written as itself, not as 1.1.
        c.tune.panel.digit_size = Some(f64::from(1.1f32));
        c.tune.widget.padding_scale = Some(0.0);
        c.tune.format.mid_at = Some(-5.0);
        c.providers.push(ProviderConfig {
            id: "future".into(),
            enabled: true,
            auth_method: AuthMethod::Subscription,
            credential_label: String::new(),
            alert_threshold: 0,
            tool_files: false,
        });
        let text = annotated(&c).unwrap();
        assert_eq!(read(&text), c, "{text}");
    }
}
