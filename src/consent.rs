// consent.rs — what each provider reads and where its token goes, for the
// questions asked before the first read and the one-time notice after the
// update. Nothing here reads a file: the table is text, and `found_tools`
// only asks whether a tool's folder exists.
//
// Keep in step with src/providers (the files and hosts are the ones the code
// uses; the tests below hold them together) and with docs/en/PROVIDERS.md.

use std::path::{Path, PathBuf};

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
}

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
        settings: Some(r"%USERPROFILE%\.claude\settings.json: model, effortLevel"),
        session: Some(r"%USERPROFILE%\.claude\projects\...\*.jsonl (the end of the newest)"),
    },
    ReadInfo {
        id: "codex",
        tool: "Codex CLI",
        tool_sign_in: true,
        sign_in: &[r"%USERPROFILE%\.codex\auth.json (in CODEX_HOME when it is set)"],
        hosts: &["chatgpt.com"],
        pages: &[],
        settings: Some(
            r"%USERPROFILE%\.codex\config.toml: model, model_reasoning_effort (in CODEX_HOME when it is set)",
        ),
        session: Some(
            r"%USERPROFILE%\.codex\sessions\...\*.jsonl (the end of the newest; in CODEX_HOME when it is set)",
        ),
    },
    ReadInfo {
        id: "copilot",
        tool: "GitHub Copilot",
        tool_sign_in: true,
        sign_in: &[
            "a token you paste (kept in Windows Credential Manager)",
            "otherwise the token GitHub CLI keeps for github.com (gh auth token --hostname github.com)",
        ],
        hosts: &["api.github.com"],
        pages: &[],
        settings: Some(
            r"%USERPROFILE%\.copilot\settings.json: model, effortLevel (in COPILOT_HOME when it is set)",
        ),
        session: None,
    },
    ReadInfo {
        id: "antigravity",
        tool: "Antigravity",
        tool_sign_in: true,
        sign_in: &[
            "its sign-in in Windows Credential Manager (gemini:antigravity)",
            r"otherwise %USERPROFILE%\.gemini\oauth_creds.json (older Gemini CLI)",
        ],
        hosts: &[
            "cloudcode-pa.googleapis.com",
            "daily-cloudcode-pa.googleapis.com",
        ],
        pages: &[],
        settings: Some(r"%USERPROFILE%\.gemini\antigravity-cli\settings.json: model"),
        session: None,
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
    },
    ReadInfo {
        id: "cursor",
        tool: "Cursor",
        tool_sign_in: false,
        sign_in: &[
            "no file of Cursor's: you sign in from the menu (Providers > Cursor > Sign in to Cursor...) on Cursor's own sign-in page, cursor.com",
            "the app signs in as the Cursor editor's client and keeps that session in Windows Credential Manager (ailimits/cursor_session)",
        ],
        hosts: &["api2.cursor.sh"],
        pages: &["cursor.com"],
        settings: Some(
            r"%APPDATA%\Cursor\User\globalStorage\state.vscdb: the model and effort chosen for Agent, nothing else of it",
        ),
        session: None,
    },
];

/// The read info for a config id ("gemini" is the old name of Antigravity).
pub fn read_info(id: &str) -> Option<&'static ReadInfo> {
    let id = if id == "gemini" { "antigravity" } else { id };
    READ_INFO.iter().find(|r| r.id == id)
}

/// Providers whose tool is installed on this PC, judged only by whether a
/// folder exists: nothing inside is opened. Used to mark them in the welcome.
pub fn found_tools() -> Vec<&'static str> {
    match Places::from_env() {
        Some(places) => detect(&places, |p| p.is_dir())
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
    Places::from_env().is_some_and(|places| {
        tool_dirs(&places, id)
            .iter()
            .any(|(dir, sure)| *sure && dir.is_dir())
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

/// The folders the welcome looks for, resolved the way the tools resolve
/// them. Absolute paths only: without a home folder nothing is looked for.
#[derive(Debug, Clone)]
struct Places {
    user: PathBuf,
    roaming: PathBuf,
    codex: PathBuf,
    copilot: PathBuf,
    grok: PathBuf,
}

impl Places {
    fn from_env() -> Option<Self> {
        let user = dirs::home_dir().filter(|p| p.is_absolute())?;
        let roaming = dirs::config_dir()
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| user.join("AppData").join("Roaming"));
        let tool = |var: &str, dir: &str| env_dir(var).unwrap_or_else(|| user.join(dir));
        Some(Places {
            codex: tool("CODEX_HOME", ".codex"),
            copilot: tool("COPILOT_HOME", ".copilot"),
            grok: tool("GROK_HOME", ".grok"),
            roaming,
            user,
        })
    }
}

/// A folder named by an environment variable, when set to an absolute path.
fn env_dir(var: &str) -> Option<PathBuf> {
    absolute_dir(std::env::var_os(var))
}

/// A folder from a variable's value: never a relative one, which would be
/// taken from the process's working folder.
fn absolute_dir(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// The folders whose existence says a tool is installed, each with whether
/// it is the tool's own (sure) or only a related tool's (weak).
fn tool_dirs(places: &Places, id: &str) -> Vec<(PathBuf, bool)> {
    let p = places;
    match id {
        "claude" => vec![(p.user.join(".claude"), true)],
        "codex" => vec![(p.codex.clone(), true)],
        "copilot" => vec![
            (p.copilot.clone(), true),
            (p.roaming.join("GitHub CLI"), false),
        ],
        "antigravity" => vec![
            (p.roaming.join("Antigravity"), true),
            (p.user.join(".gemini").join("antigravity-cli"), true),
            (p.user.join(".gemini"), false),
        ],
        "grok" => vec![(p.grok.clone(), true)],
        "cursor" => vec![(p.roaming.join("Cursor"), true)],
        _ => Vec::new(),
    }
}

/// The tools found, in READ_INFO's order, with whether each was found
/// surely. `exists` says whether a folder exists.
fn detect(places: &Places, exists: impl Fn(&Path) -> bool) -> Vec<(&'static str, bool)> {
    READ_INFO
        .iter()
        .filter_map(|r| {
            let dirs = tool_dirs(places, r.id);
            let mut found = dirs.iter().filter(|(d, _)| exists(d));
            let first = found.next()?;
            let sure = first.1 || found.any(|(_, s)| *s);
            Some((r.id, sure))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

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
                let src = std::fs::read_to_string(&path).unwrap();
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
            if id == "mod" {
                assert!(
                    hosts.is_empty(),
                    "{} calls {hosts:?} for no provider",
                    path.display()
                );
                continue;
            }
            let info = read_info(&id)
                .unwrap_or_else(|| panic!("{} belongs to no provider", path.display()));
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

    fn places(root: &Path) -> Places {
        Places {
            user: root.to_path_buf(),
            roaming: root.join("AppData").join("Roaming"),
            codex: root.join("codex-home"),
            copilot: root.join(".copilot"),
            grok: root.join(".grok"),
        }
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

    /// A relative tool home is never used: the tool's default under the
    /// user's own folder stands instead.
    #[test]
    fn only_absolute_homes_count() {
        let v = |s: &str| Some(std::ffi::OsString::from(s));
        assert!(absolute_dir(v(r"relative\codex")).is_none());
        assert!(absolute_dir(v("")).is_none());
        assert!(absolute_dir(None).is_none());
        assert_eq!(
            absolute_dir(v(r"C:\abs\codex")),
            Some(PathBuf::from(r"C:\abs\codex"))
        );
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
        // Antigravity names the Credential Manager entry it reads.
        assert!(read_info("antigravity").unwrap().sign_in[0].contains("gemini:antigravity"));
        // Codex follows CODEX_HOME for its sign-in too.
        assert!(read_info("codex").unwrap().sign_in[0].contains("CODEX_HOME"));
    }
}
