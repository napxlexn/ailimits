// consent.rs — what each provider reads and where its token goes, for the
// questions asked before the first read and the one-time notice after the
// update. Nothing here reads a file: the table is text, and `found_tools`
// only asks whether a tool's folder exists.
//
// Keep in step with src/providers (the files and hosts are the ones the code
// uses; the tests below hold them together) and with docs/en/PROVIDERS.md.

use crate::config::paths::{ToolPaths, ToolSign};
use std::path::Path;

/// What one provider reads, in the words the picker shows.
#[derive(Debug, Clone, Copy)]
pub struct ReadInfo {
    /// Config id: "claude".
    pub id: &'static str,
    /// The tool whose files these are: "Claude Code".
    pub tool: &'static str,
    /// Whether the sign-in comes from the tool's own files. False for
    /// Cursor: the app signs in by itself and reads no file of Cursor's for
    /// it, so the question asks to show the tool, not to read its sign-in.
    pub tool_sign_in: bool,
    /// Sources of the sign-in, in the order the provider tries them.
    pub sign_in: &'static [&'static str],
    /// The only hosts the token is sent to.
    pub hosts: &'static [&'static str],
    /// Pages the browser opens for the app's own sign-in (nothing is sent
    /// there by the app itself).
    pub pages: &'static [&'static str],
    /// The settings file and the keys read for the model, if the tool has one.
    pub settings: Option<&'static str>,
    /// The session files read for the model last used, if the tool has them.
    pub session: Option<&'static str>,
    /// What reading this sign-in may cost the user, said wherever its read
    /// is asked for or listed (the question, the welcome, Allow all, the
    /// update notice), if anything.
    pub risk: Option<&'static str>,
}

/// The risk of reading Antigravity's sign-in. Google has suspended accounts
/// whose Antigravity sign-in was used by other apps; the widget sends only
/// the quota requests, but the user decides.
pub const ANTIGRAVITY_RISK: &str = "Google has suspended some accounts for using Antigravity's sign-in outside Antigravity. AI Limits only reads your quota, once per update, but the risk is yours.";

impl ReadInfo {
    /// What the sign-in is called in a sentence: the tool's token, or the
    /// app's own session.
    pub fn secret(&self) -> &'static str {
        if self.tool_sign_in {
            "token"
        } else {
            "session"
        }
    }
}

/// Every provider, in the default config's order.
pub const READ_INFO: &[ReadInfo] = &[
    ReadInfo {
        id: "claude",
        tool: "Claude Code",
        tool_sign_in: true,
        sign_in: &[r"%USERPROFILE%\.claude\.credentials.json"],
        hosts: &["api.anthropic.com"],
        pages: &[],
        settings: Some(
            r"%USERPROFILE%\.claude\settings.json: model, env.ANTHROPIC_MODEL, effortLevel, modelSettings.<model>.effortLevel",
        ),
        session: Some(r"%USERPROFILE%\.claude\projects\...\*.jsonl (the end of the newest)"),
        risk: None,
    },
    ReadInfo {
        id: "codex",
        tool: "Codex CLI",
        tool_sign_in: true,
        sign_in: &[r"%USERPROFILE%\.codex\auth.json (in CODEX_HOME when it is set)"],
        hosts: &["chatgpt.com"],
        pages: &[],
        settings: Some(
            r"%USERPROFILE%\.codex\config.toml: model, model_reasoning_effort, profile, [profiles.<name>] (in CODEX_HOME when it is set)",
        ),
        session: Some(
            r"%USERPROFILE%\.codex\sessions\...\*.jsonl (the end of the newest; in CODEX_HOME when it is set)",
        ),
        risk: None,
    },
    ReadInfo {
        id: "copilot",
        tool: "GitHub Copilot",
        tool_sign_in: true,
        sign_in: &[
            "the token GitHub CLI keeps for github.com (gh auth token --hostname github.com)",
        ],
        hosts: &["api.github.com"],
        pages: &[],
        settings: Some(
            r"%USERPROFILE%\.copilot\settings.json: model, effortLevel (in COPILOT_HOME when it is set)",
        ),
        session: None,
        risk: None,
    },
    ReadInfo {
        id: "antigravity",
        tool: "Antigravity",
        tool_sign_in: true,
        sign_in: &["the Antigravity CLI's sign-in in Windows Credential Manager (gemini:antigravity). It lasts about an hour and is renewed only when you run agy; the Antigravity editor's own sign-in is never read"],
        hosts: &[
            "cloudcode-pa.googleapis.com",
            "daily-cloudcode-pa.googleapis.com",
        ],
        pages: &[],
        settings: Some(r"%USERPROFILE%\.gemini\antigravity-cli\settings.json: model"),
        session: None,
        risk: Some(ANTIGRAVITY_RISK),
    },
    ReadInfo {
        id: "grok",
        tool: "Grok CLI",
        tool_sign_in: true,
        sign_in: &[r"%USERPROFILE%\.grok\auth.json (in GROK_HOME when it is set)"],
        hosts: &["cli-chat-proxy.grok.com"],
        pages: &[],
        settings: Some(
            r"%USERPROFILE%\.grok\config.toml: [models] default, default_reasoning_effort (in GROK_HOME when it is set)",
        ),
        session: None,
        risk: None,
    },
    ReadInfo {
        id: "cursor",
        tool: "Cursor",
        tool_sign_in: false,
        sign_in: &[
            "no file of Cursor's: you sign in from Rows and models (Cursor, Sign-in, Sign in to Cursor...) on Cursor's own sign-in page, cursor.com",
            "the app signs in as the Cursor editor's client and keeps that session in Windows Credential Manager (ailimits/cursor_session)",
        ],
        hosts: &["api2.cursor.sh"],
        pages: &["cursor.com"],
        settings: Some(
            r"%APPDATA%\Cursor\User\globalStorage\state.vscdb: the model and effort chosen for Agent, nothing else of it",
        ),
        session: None,
        risk: None,
    },
];

/// The read info for a config id ("gemini" is the old name of Antigravity).
pub fn read_info(id: &str) -> Option<&'static ReadInfo> {
    let id = if id == "gemini" { "antigravity" } else { id };
    READ_INFO.iter().find(|r| r.id == id)
}

/// Providers whose tool is installed on this PC, judged only by whether a
/// folder exists: nothing inside is opened. Used to mark them in the welcome.
/// The folders are the ones config/paths.rs resolves (`ToolPaths::tool_signs`):
/// absolute paths only, so without a home folder nothing is found.
pub fn found_tools() -> Vec<&'static str> {
    match ToolPaths::from_env() {
        Some(paths) => detect(&paths, |p| p.is_dir())
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
        None => Vec::new(),
    }
}

/// Whether a tool was found by a folder of its own (the tool itself is
/// installed), not only by a related one: GitHub CLI's folder says nothing
/// of Copilot, `.gemini` nothing of Antigravity. Only such tools start
/// ticked in the welcome.
pub fn found_surely(id: &str) -> bool {
    ToolPaths::from_env().is_some_and(|paths| {
        paths
            .tool_signs(id)
            .iter()
            .any(|sign| !sign.weak && sign.path.is_dir())
    })
}

/// What the "found on this PC" mark says on hover.
pub fn found_hint(id: &str, sure: bool) -> &'static str {
    match (id, sure) {
        ("copilot", false) => {
            "Found by GitHub CLI's folder only, so Copilot may not be on this account. Nothing inside it was opened."
        }
        ("antigravity", false) => {
            "Found by the Gemini CLI's folder (.gemini) only, so Antigravity itself may not be installed. Nothing inside it was opened."
        }
        _ => "Found by its folder on this PC; nothing inside it was opened.",
    }
}

/// What the welcome's question says under a tool found only by a related
/// tool's folder, which it leaves unticked.
pub fn weak_note(id: &str) -> &'static str {
    match id {
        "copilot" => "Only GitHub CLI was found: Copilot may not be on this account.",
        "antigravity" => {
            "Only the Gemini CLI's folder was found: Antigravity may not be installed."
        }
        _ => "Only a related tool's folder was found.",
    }
}

/// The tools found, in READ_INFO's order, with whether each was found
/// surely. `exists` says whether a folder exists.
fn detect(paths: &ToolPaths, exists: impl Fn(&Path) -> bool) -> Vec<(&'static str, bool)> {
    READ_INFO
        .iter()
        .filter_map(|r| {
            let signs: Vec<ToolSign> = paths.tool_signs(r.id);
            let found: Vec<&ToolSign> = signs.iter().filter(|s| exists(&s.path)).collect();
            if found.is_empty() {
                return None;
            }
            Some((r.id, found.iter().any(|s| !s.weak)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    #[test]
    fn every_default_provider_has_read_info() {
        for p in crate::config::schema::Config::default().providers {
            assert!(read_info(&p.id).is_some(), "no read info for {}", p.id);
        }
        assert_eq!(read_info("gemini").map(|r| r.id), Some("antigravity"));
    }

    /// Every source file under src/providers, read at test time so that a
    /// new file is covered without being listed here, with the provider it
    /// belongs to: `claude.rs` and `cursor/session.rs` alike.
    /// A source with its `#[cfg(test)] mod name { ... }` blocks cut out:
    /// the URLs a test feeds a local server are not hosts the app calls.
    /// `#[cfg(test)] mod name;` declarations stay (their file is walked
    /// on its own and has no app URL).
    fn without_test_modules(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let mut rest = src;
        while let Some(at) = rest.find("#[cfg(test)]") {
            let after = &rest[at + "#[cfg(test)]".len()..];
            let head = after.trim_start();
            let is_block = (head.starts_with("mod ")
                || head.starts_with("pub mod ")
                || head.starts_with("pub(crate) mod "))
                && head
                    .find(['{', ';'])
                    .is_some_and(|i| head.as_bytes()[i] == b'{');
            if !is_block {
                out.push_str(&rest[..at + "#[cfg(test)]".len()]);
                rest = after;
                continue;
            }
            out.push_str(&rest[..at]);
            let open = rest.len() - head.len() + head.find('{').unwrap();
            let mut depth = 0usize;
            let mut end = rest.len();
            for (i, c) in rest[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            rest = &rest[end..];
        }
        out.push_str(rest);
        // Comments describe hosts; only code calls them.
        out.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_modules_are_cut_out_of_the_scanned_sources() {
        let src = "const A: &str = \"https://a.example\";\n#[cfg(test)]\nmod helper;\n#[cfg(test)]\nmod tests {\n    fn f() { let _ = \"https://b.example\"; if true { } }\n}\nconst C: &str = \"https://c.example\";\n";
        let cut = without_test_modules(src);
        assert!(cut.contains("a.example") && cut.contains("c.example"));
        assert!(cut.contains("mod helper;"));
        assert!(!cut.contains("b.example"));
    }

    fn provider_sources() -> Vec<(String, PathBuf, String)> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(dir).expect("src/providers") {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("providers");
        let mut files = Vec::new();
        walk(&root, &mut files);
        files.sort();
        files
            .into_iter()
            .map(|path| {
                let rel = path.strip_prefix(&root).unwrap();
                let first = rel.components().next().unwrap().as_os_str();
                let id = Path::new(first)
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                let src = without_test_modules(&std::fs::read_to_string(&path).unwrap());
                (id, path, src)
            })
            .collect()
    }

    /// The hosts named to the user are exactly the hosts the code calls:
    /// every URL in a provider's sources is named with that provider (as a
    /// host its token goes to, or a page the browser opens for its
    /// sign-in), and nothing is named that the code does not call.
    #[test]
    fn hosts_match_the_provider_urls() {
        let mut called: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut files = 0;
        for (id, path, src) in provider_sources() {
            files += 1;
            let hosts = url_hosts(&src);
            // Shared helpers (mod.rs, proxy.rs, test_server.rs) belong to
            // no provider, so they may call no host at all.
            let Some(info) = read_info(&id) else {
                assert!(
                    hosts.is_empty(),
                    "{} calls {hosts:?} for no provider",
                    path.display()
                );
                continue;
            };
            for host in hosts {
                assert!(
                    info.hosts.contains(&host.as_str()) || info.pages.contains(&host.as_str()),
                    "{} calls {host}, which the consent text does not name",
                    path.display()
                );
                called.entry(id.clone()).or_default().insert(host);
            }
        }
        // claude, codex, copilot, antigravity, grok, cursor, cursor/session, mod.
        assert!(files >= 8, "only {files} provider files found");
        for info in READ_INFO {
            let code = called.get(info.id).cloned().unwrap_or_default();
            for named in info.hosts.iter().chain(info.pages) {
                assert!(
                    code.contains(*named),
                    "{} names {named}, which its code never calls",
                    info.id
                );
            }
            // A page the browser opens is named in the sign-in text itself.
            for page in info.pages {
                assert!(
                    info.sign_in.iter().any(|s| s.contains(page)),
                    "{}: the sign-in text does not name {page}",
                    info.id
                );
            }
        }
    }

    /// Every file and Credential Manager entry a provider's code reads for a
    /// sign-in is named in that provider's consent text.
    #[test]
    fn the_files_read_are_named() {
        for (id, path, src) in provider_sources() {
            let Some(info) = read_info(&id) else {
                continue;
            };
            let text: String = info
                .sign_in
                .iter()
                .copied()
                .chain(info.settings)
                .chain(info.session)
                .collect::<Vec<_>>()
                .join(" ");
            for name in file_literals(&src).into_iter().chain(target_literals(&src)) {
                assert!(
                    text.contains(&name),
                    "{} reads {name}, which the consent text does not name",
                    path.display()
                );
            }
        }
    }

    /// The keys the model labels' readers take from a tool's settings file
    /// (models/settings.rs) are named in its consent text.
    #[test]
    fn the_settings_keys_read_are_named() {
        for (id, keys) in [
            (
                "claude",
                &["model", "ANTHROPIC_MODEL", "effortLevel", "modelSettings"][..],
            ),
            (
                "codex",
                &["model", "model_reasoning_effort", "profile", "[profiles."][..],
            ),
            ("copilot", &["model", "effortLevel"][..]),
            (
                "grok",
                &["[models] default", "default_reasoning_effort"][..],
            ),
        ] {
            let text = read_info(id).unwrap().settings.unwrap();
            for key in keys {
                assert!(
                    text.contains(key),
                    "{id}: the settings text does not name {key}"
                );
            }
        }
    }

    /// String literals that are file names: ".credentials.json", "auth.json".
    fn file_literals(src: &str) -> Vec<String> {
        src.split('"')
            .skip(1)
            .step_by(2)
            .filter(|s| {
                [".json", ".vscdb", ".yml"].iter().any(|x| s.ends_with(x))
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            })
            .map(String::from)
            .collect()
    }

    /// The Credential Manager targets a provider reads that are not the
    /// app's own: `const ..._CREDENTIAL_TARGET: &str = "gemini:antigravity"`,
    /// and the app's own session label.
    fn target_literals(src: &str) -> Vec<String> {
        src.lines()
            .filter_map(|l| {
                let l = l.trim_start().trim_start_matches("pub ");
                let rest = l.strip_prefix("const ")?;
                let (name, value) = rest.split_once(": &str = \"")?;
                let value = value.strip_suffix("\";")?;
                (name.ends_with("CREDENTIAL_TARGET") || name == "SESSION_LABEL")
                    .then(|| value.to_string())
            })
            .collect()
    }

    fn url_hosts(src: &str) -> Vec<String> {
        ["https://", "http://"]
            .iter()
            .flat_map(|scheme| {
                src.match_indices(scheme).map(|(i, _)| {
                    src[i + scheme.len()..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                        .collect::<String>()
                })
            })
            .filter(|h| !h.is_empty() && !h.ends_with(".invalid"))
            .collect()
    }

    /// The folders under `root`, with CODEX_HOME set to `root\codex-home`.
    fn places(root: &Path) -> ToolPaths {
        let codex = root.join("codex-home").into_os_string();
        ToolPaths::resolve(Some(root.to_path_buf()), None, move |var| {
            (var == "CODEX_HOME").then(|| codex.clone())
        })
        .unwrap()
    }

    /// A tool found only by a related tool's folder is found, but not
    /// surely: GitHub CLI says nothing of Copilot, `.gemini` nothing of
    /// Antigravity. The tool's own folder makes it sure.
    #[test]
    fn related_folders_find_a_tool_weakly() {
        let root = Path::new(r"C:\home");
        let p = places(root);
        let have = |dirs: &[PathBuf]| {
            let dirs: Vec<PathBuf> = dirs.to_vec();
            detect(&p, move |d| dirs.iter().any(|x| x == d))
        };
        let roaming = root.join("AppData").join("Roaming");
        assert_eq!(
            have(&[roaming.join("GitHub CLI"), root.join(".gemini")]),
            [("copilot", false), ("antigravity", false)]
        );
        assert_eq!(
            have(&[
                roaming.join("GitHub CLI"),
                root.join(".copilot"),
                root.join(".gemini").join("antigravity-cli"),
                root.join(".gemini"),
            ]),
            [("copilot", true), ("antigravity", true)]
        );
        // CODEX_HOME: the folder it names, not ~/.codex.
        assert!(have(&[root.join(".codex")]).is_empty());
        assert_eq!(have(&[root.join("codex-home")]), [("codex", true)]);
        // Every other tool by its own folder, in the catalogue's order.
        let all = have(&[
            root.join(".claude"),
            root.join(".grok"),
            roaming.join("Cursor"),
            roaming.join("Antigravity"),
        ]);
        assert_eq!(
            all,
            [
                ("claude", true),
                ("antigravity", true),
                ("grok", true),
                ("cursor", true)
            ]
        );
        assert!(found_hint("copilot", false).contains("GitHub CLI"));
        assert!(found_hint("antigravity", false).contains(".gemini"));
        assert!(found_hint("copilot", true).starts_with("Found by its folder"));
    }

    /// Antigravity's question says what its read may cost, plainly; no other
    /// tool carries such a note.
    #[test]
    fn antigravity_carries_its_risk_note() {
        let risk = read_info("antigravity").unwrap().risk.unwrap();
        assert_eq!(risk, ANTIGRAVITY_RISK);
        for words in ["suspended", "outside Antigravity", "the risk is yours"] {
            assert!(risk.contains(words), "{words}");
        }
        assert!(risk.is_ascii(), "English, plain ASCII");
        assert_eq!(read_info("gemini").unwrap().risk, Some(ANTIGRAVITY_RISK));
        assert!(READ_INFO
            .iter()
            .filter(|r| r.id != "antigravity")
            .all(|r| r.risk.is_none()));
    }

    /// A relative tool home is never used, nor the default folder in its
    /// place: that tool is not looked for at all (config/paths.rs).
    #[test]
    fn only_absolute_homes_count() {
        let root = Path::new(r"C:\home");
        let paths = ToolPaths::resolve(Some(root.to_path_buf()), None, |var| {
            (var == "CODEX_HOME").then(|| std::ffi::OsString::from(r"relative\codex"))
        })
        .unwrap();
        let everything = |_: &Path| true;
        let found: Vec<&str> = detect(&paths, everything)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert!(!found.contains(&"codex"), "{found:?}");
        assert!(found.contains(&"claude"));
        // Without an absolute profile folder nothing resolves to look for.
        let relative_home = Some(PathBuf::from(r"relative\home"));
        assert!(ToolPaths::resolve(relative_home, None, |_| None).is_none());
    }

    /// Cursor reads no file of its own for the sign-in: the text says so,
    /// names its sign-in page, the editor client it signs in as, and where
    /// the session is kept; the editor's database is named only for the
    /// model, under the settings question.
    #[test]
    fn cursor_signs_in_by_itself() {
        let c = read_info("cursor").unwrap();
        assert!(!c.tool_sign_in);
        assert_eq!(c.secret(), "session");
        let text = c.sign_in.join(" ");
        for word in [
            "no file of Cursor's",
            "Sign in to Cursor...",
            "cursor.com",
            "editor's client",
            "ailimits/cursor_session",
        ] {
            assert!(text.contains(word), "{word}");
        }
        assert!(!text.contains("state.vscdb"));
        assert!(c.settings.unwrap().contains("state.vscdb"));
        // Every other tool's sign-in is its own token.
        assert!(READ_INFO
            .iter()
            .filter(|r| r.id != "cursor")
            .all(|r| r.tool_sign_in && r.secret() == "token"));
        // Antigravity names the Credential Manager entry it reads, and only
        // that: the older Gemini CLI's file is no longer read.
        let ag = read_info("antigravity").unwrap();
        assert_eq!(ag.sign_in.len(), 1);
        assert!(ag.sign_in[0].contains("gemini:antigravity"));
        assert!(!ag.sign_in.join(" ").contains("oauth_creds"));
        // Codex follows CODEX_HOME for its sign-in too.
        assert!(read_info("codex").unwrap().sign_in[0].contains("CODEX_HOME"));
    }
}
