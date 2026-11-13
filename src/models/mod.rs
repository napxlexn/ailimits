// models/mod.rs — the model and effort each tool is set to (or last used),
// and the names shown for them. See docs/en/PROVIDERS.md, "Model labels".
//
// Read only what `[reads]` allows; tolerant of any file shape (an unknown
// shape means no model, never an error); only the model id and the effort
// leave a reader, never other contents of the file.

mod files;
mod name;
mod session;
mod settings;

use crate::config::schema::{ModelSource, Reads};
use crate::providers::ProviderId;
use files::Stamp;
use std::collections::HashMap;
use std::path::PathBuf;

/// A tool's model as it stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    /// The id as the tool stores it: "claude-opus-5-5", "gpt-6.1-sol",
    /// Antigravity's label "Claude Sonnet 4.6 (Thinking)". Empty when the
    /// tool stores an effort but no model (its default model is not
    /// written): the effort then shows alone.
    pub id: String,
    /// The effort as the tool stores it ("xhigh", "high"), if it has one.
    pub effort: Option<String>,
}

/// The model of each provider that has one to show.
pub type Models = HashMap<ProviderId, ModelInfo>;

/// A model name in its two lengths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameParts {
    /// The model without its version: "Opus", "GPT".
    pub model: String,
    /// The model with its version: "Opus 5.5", "GPT-6.1 Sol". Equal to
    /// `model` when the id carries no version.
    pub with_version: String,
}

/// The name shown for a model id: the user's own name when there is one,
/// otherwise the generic rule (no table of names, so a new model needs no
/// update). Either way split into model and version the same way.
pub fn name_parts(id: &str, provider: &ProviderId, rename: Option<&str>) -> NameParts {
    name::name_parts(id, provider.display_name(), rename)
}

/// Reads the model of each tool, only as far as `[reads]` allows, and only
/// again when a file changed (its modification time or length). No
/// watchers, no threads: called from the scheduler's cycle.
#[derive(Debug, Default)]
pub struct ModelReader {
    /// Fixed folders for tests; None: from the environment on every read.
    homes: Option<Homes>,
    /// The settings file last read per provider.
    settings: HashMap<ProviderId, Seen>,
    /// The session file last read per provider.
    sessions: HashMap<ProviderId, Seen>,
}

/// Takes the model out of a settings file's bytes.
type SettingsParser = fn(&[u8]) -> Option<ModelInfo>;
/// Takes the model out of a session file's tail.
type TailParser = fn(&str) -> Option<ModelInfo>;

/// A file as last read, and what it gave.
#[derive(Debug)]
struct Seen {
    path: PathBuf,
    stamp: Stamp,
    info: Option<ModelInfo>,
}

impl ModelReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// The models to show for `ids`. No settings file is opened with
    /// `tool_settings` off; `LastUsed` reads session tails (Claude Code,
    /// Codex CLI) only with `last_session` on, and falls back to the
    /// configured model (when `tool_settings` allows) for the other tools and
    /// whenever a session gives no model.
    pub fn read(&mut self, reads: &Reads, source: ModelSource, ids: &[ProviderId]) -> Models {
        let use_sessions = source == ModelSource::LastUsed && reads.last_session;
        // Nothing read is kept once its read is no longer allowed.
        if !reads.tool_settings {
            self.settings.clear();
        }
        if !use_sessions {
            self.sessions.clear();
        }
        let mut out = Models::new();
        if !reads.tool_settings && !use_sessions {
            return out;
        }
        let Some(homes) = self.homes.clone().or_else(Homes::from_env) else {
            return out;
        };
        for id in ids {
            if out.contains_key(id) {
                continue;
            }
            let last = if use_sessions {
                self.last_used(&homes, id)
            } else {
                None
            };
            let info = last.or_else(|| {
                if reads.tool_settings {
                    self.configured(&homes, id)
                } else {
                    None
                }
            });
            if let Some(info) = info {
                out.insert(id.clone(), info);
            }
        }
        out
    }

    /// The model set in the tool's settings file.
    fn configured(&mut self, homes: &Homes, id: &ProviderId) -> Option<ModelInfo> {
        let (path, parse) = settings_file(homes, id)?;
        let Some(stamp) = files::stamp(&path) else {
            self.settings.remove(id);
            return None;
        };
        if let Some(seen) = self.settings.get(id) {
            if seen.path == path && seen.stamp == stamp {
                return seen.info.clone();
            }
        }
        let info = if stamp.len > files::SETTINGS_CAP {
            None
        } else {
            files::read_capped(&path, files::SETTINGS_CAP).and_then(|bytes| parse(&bytes))
        };
        let seen = Seen { path, stamp, info };
        let info = seen.info.clone();
        self.settings.insert(id.clone(), seen);
        info
    }

    /// The model of the newest session, from the end of its file.
    fn last_used(&mut self, homes: &Homes, id: &ProviderId) -> Option<ModelInfo> {
        let (newest, parse): (Option<PathBuf>, TailParser) = match id {
            ProviderId::Claude => (
                session::newest_claude_session(&homes.claude().join("projects")),
                session::claude_last,
            ),
            ProviderId::Codex => (
                session::newest_codex_session(&homes.codex.join("sessions")),
                session::codex_last,
            ),
            _ => return None,
        };
        // A compressed session cannot be read: no last-used model.
        let found = newest
            .filter(|p| !session::is_compressed(p))
            .and_then(|p| files::stamp(&p).map(|s| (p, s)));
        let Some((path, stamp)) = found else {
            self.sessions.remove(id);
            return None;
        };
        if let Some(seen) = self.sessions.get(id) {
            if seen.path == path && seen.stamp == stamp {
                return seen.info.clone();
            }
        }
        // A tail without a model record keeps the model last found: one long
        // answer can fill 64 KB, and a session just started has no answer
        // yet, so the previous session's model is still the one last used.
        let before = self.sessions.get(id).and_then(|seen| seen.info.clone());
        let info = files::read_tail(&path, files::TAIL_LEN)
            .and_then(|tail| parse(&tail))
            .or(before);
        self.sessions.insert(
            id.clone(),
            Seen {
                path,
                stamp,
                info: info.clone(),
            },
        );
        info
    }
}

/// Where the tools keep their files.
#[derive(Debug, Clone)]
struct Homes {
    /// %USERPROFILE%
    user: PathBuf,
    /// `CODEX_HOME`, else %USERPROFILE%\.codex
    codex: PathBuf,
    /// `GROK_HOME`, else %USERPROFILE%\.grok
    grok: PathBuf,
    /// `COPILOT_HOME`, else %USERPROFILE%\.copilot
    copilot: PathBuf,
}

impl Homes {
    /// From the environment; None without a home folder, so that no path is
    /// ever taken relative to the working directory.
    fn from_env() -> Option<Self> {
        let user = test_home()
            .or_else(dirs::home_dir)
            .filter(|p| p.is_absolute())?;
        let tool = |var: &str, dir: &str| env_dir(var).unwrap_or_else(|| user.join(dir));
        Some(Homes {
            codex: tool("CODEX_HOME", ".codex"),
            grok: tool("GROK_HOME", ".grok"),
            copilot: tool("COPILOT_HOME", ".copilot"),
            user,
        })
    }

    /// Fixed folders under one root, for tests.
    #[cfg(test)]
    fn under(root: &std::path::Path) -> Self {
        Homes {
            user: root.to_path_buf(),
            codex: root.join(".codex"),
            grok: root.join(".grok"),
            copilot: root.join(".copilot"),
        }
    }

    fn claude(&self) -> PathBuf {
        self.user.join(".claude")
    }

    fn antigravity_cli(&self) -> PathBuf {
        self.user.join(".gemini").join("antigravity-cli")
    }
}

/// A tool's settings file and the parser for it; None for a tool that has
/// none (Cursor). The files are the ones `consent::READ_INFO` names.
fn settings_file(homes: &Homes, id: &ProviderId) -> Option<(PathBuf, SettingsParser)> {
    let file: (PathBuf, SettingsParser) = match id {
        ProviderId::Claude => (homes.claude().join("settings.json"), settings::claude),
        ProviderId::Codex => (homes.codex.join("config.toml"), settings::codex),
        ProviderId::Grok => (homes.grok.join("config.toml"), settings::grok),
        ProviderId::Copilot => (homes.copilot.join("settings.json"), settings::copilot),
        ProviderId::Antigravity => (
            homes.antigravity_cli().join("settings.json"),
            settings::antigravity,
        ),
        ProviderId::Cursor => return None,
    };
    Some(file)
}

/// Debug builds only: `AILIMITS_MODEL_HOME` stands in for %USERPROFILE% when
/// the model labels are read, so a live test can show prepared settings and
/// sessions without touching the tools' own files. Sign-in reads are not
/// affected. Release builds never look at it.
fn test_home() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        env_dir("AILIMITS_MODEL_HOME")
    } else {
        None
    }
}

/// A folder named by an environment variable, when set to an absolute path.
fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

#[cfg(test)]
mod tests {
    use super::files::test_dir::TempDir;
    use super::*;
    use std::time::{Duration, Instant, SystemTime};

    const ALL: [ProviderId; 6] = [
        ProviderId::Claude,
        ProviderId::Codex,
        ProviderId::Copilot,
        ProviderId::Antigravity,
        ProviderId::Grok,
        ProviderId::Cursor,
    ];

    fn reads(tool_settings: bool, last_session: bool) -> Reads {
        Reads {
            tool_settings,
            last_session,
            ..Reads::default()
        }
    }

    fn reader(dir: &TempDir) -> ModelReader {
        ModelReader {
            homes: Some(Homes::under(dir.path())),
            ..ModelReader::default()
        }
    }

    fn info(id: &str, effort: Option<&str>) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            effort: effort.map(String::from),
        }
    }

    fn touch(path: &std::path::Path, secs_ago: u64) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(secs_ago))
            .unwrap();
    }

    /// Every tool set up as on a PC that has them all.
    fn every_tool(dir: &TempDir) {
        dir.write(
            ".claude/settings.json",
            r#"{"model": "claude-opus-5-5", "modelSettings": {"claude-opus-5-5": {"effortLevel": "xhigh"}}}"#,
        );
        dir.write(
            ".codex/config.toml",
            "model = \"gpt-6.1-sol\"\nmodel_reasoning_effort = \"high\"\n",
        );
        dir.write(
            ".grok/config.toml",
            "[models]\ndefault = \"grok-4.5\"\ndefault_reasoning_effort = \"low\"\n",
        );
        dir.write(".copilot/settings.json", r#"{"model": "auto"}"#);
        dir.write(
            ".gemini/antigravity-cli/settings.json",
            r#"{"model": "Gemini 3.1 Pro (High)"}"#,
        );
        let claude = dir.write(
            ".claude/projects/D--work/s1.jsonl",
            "{\"type\":\"assistant\",\"effort\":\"high\",\"message\":{\"model\":\"claude-sonnet-5\"}}\n",
        );
        let codex = dir.write(
            ".codex/sessions/2026/11/11/rollout-2026-11-11T09-00-00-x.jsonl",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-6.1\",\"effort\":\"xhigh\"}}\n",
        );
        touch(&claude, 5);
        touch(&codex, 5);
    }

    #[test]
    fn nothing_is_read_without_the_reads_allowed() {
        let dir = TempDir::new("gate");
        every_tool(&dir);
        let mut r = reader(&dir);
        assert!(r
            .read(&reads(false, false), ModelSource::Configured, &ALL)
            .is_empty());
        // Sessions allowed but not chosen as the source: still nothing.
        assert!(r
            .read(&reads(false, true), ModelSource::Configured, &ALL)
            .is_empty());
        // The source chosen but sessions not allowed: nothing.
        assert!(r
            .read(&reads(false, false), ModelSource::LastUsed, &ALL)
            .is_empty());
        assert!(r.settings.is_empty() && r.sessions.is_empty());
    }

    #[test]
    fn the_configured_models_of_every_tool() {
        let dir = TempDir::new("configured");
        every_tool(&dir);
        let mut r = reader(&dir);
        let m = r.read(&reads(true, false), ModelSource::Configured, &ALL);
        assert_eq!(
            m[&ProviderId::Claude],
            info("claude-opus-5-5", Some("xhigh"))
        );
        assert_eq!(m[&ProviderId::Codex], info("gpt-6.1-sol", Some("high")));
        assert_eq!(m[&ProviderId::Grok], info("grok-4.5", Some("low")));
        assert_eq!(m[&ProviderId::Copilot], info("auto", None));
        assert_eq!(
            m[&ProviderId::Antigravity],
            info("Gemini 3.1 Pro (High)", None)
        );
        assert!(!m.contains_key(&ProviderId::Cursor));
        // Only the providers asked for.
        let one = r.read(
            &reads(true, false),
            ModelSource::Configured,
            &[ProviderId::Codex],
        );
        assert_eq!(one.len(), 1);
        // Session files were not looked at.
        assert!(r.sessions.is_empty());
    }

    #[test]
    fn last_used_reads_sessions_and_falls_back_to_the_configured_model() {
        let dir = TempDir::new("last");
        every_tool(&dir);
        let mut r = reader(&dir);
        let m = r.read(&reads(true, true), ModelSource::LastUsed, &ALL);
        assert_eq!(
            m[&ProviderId::Claude],
            info("claude-sonnet-5", Some("high"))
        );
        assert_eq!(m[&ProviderId::Codex], info("gpt-6.1", Some("xhigh")));
        // Tools without sessions keep their configured model.
        assert_eq!(m[&ProviderId::Grok], info("grok-4.5", Some("low")));
        // Sessions allowed but settings not: tools without sessions show nothing.
        let mut r = reader(&dir);
        let m = r.read(&reads(false, true), ModelSource::LastUsed, &ALL);
        assert_eq!(m.len(), 2);
        assert!(r.settings.is_empty());
        // last_session off: the configured models.
        let m = r.read(&reads(true, false), ModelSource::LastUsed, &ALL);
        assert_eq!(
            m[&ProviderId::Claude],
            info("claude-opus-5-5", Some("xhigh"))
        );
        assert!(r.sessions.is_empty());
    }

    #[test]
    fn a_compressed_newest_session_means_the_configured_model() {
        let dir = TempDir::new("zst");
        every_tool(&dir);
        let zst = dir.write(
            ".codex/sessions/2026/11/11/rollout-2026-11-11T10-00-00-y.jsonl.zst",
            b"\x28\xB5\x2F\xFD\x00",
        );
        touch(&zst, 0);
        let mut r = reader(&dir);
        let m = r.read(
            &reads(true, true),
            ModelSource::LastUsed,
            &[ProviderId::Codex],
        );
        assert_eq!(m[&ProviderId::Codex], info("gpt-6.1-sol", Some("high")));
    }

    #[test]
    fn files_are_read_again_only_when_they_change() {
        let dir = TempDir::new("stamp");
        let path = dir.write(".codex/config.toml", "model = \"gpt-6.1\"\n");
        let set_time = |t: SystemTime| {
            let f = std::fs::File::options().write(true).open(&path).unwrap();
            f.set_modified(t).unwrap();
        };
        let then = SystemTime::now() - Duration::from_secs(100);
        set_time(then);
        let mut r = reader(&dir);
        let ids = [ProviderId::Codex];
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert_eq!(m[&ProviderId::Codex].id, "gpt-6.1");
        // Same length, same time: the cached answer, not the new content.
        std::fs::write(&path, "model = \"gpt-7.1\"\n").unwrap();
        set_time(then);
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert_eq!(m[&ProviderId::Codex].id, "gpt-6.1");
        // A new time: read again.
        touch(&path, 0);
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert_eq!(m[&ProviderId::Codex].id, "gpt-7.1");
        // Gone: no model, and nothing cached.
        std::fs::remove_file(&path).unwrap();
        assert!(r
            .read(&reads(true, false), ModelSource::Configured, &ids)
            .is_empty());
        assert!(r.settings.is_empty());
    }

    #[test]
    fn a_session_tail_without_a_model_keeps_what_the_file_said() {
        let dir = TempDir::new("keep");
        let answer = "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-5-5\"}}\n";
        let path = dir.write(".claude/projects/p/s.jsonl", answer);
        let mut r = reader(&dir);
        let ids = [ProviderId::Claude];
        let m = r.read(&reads(false, true), ModelSource::LastUsed, &ids);
        assert_eq!(m[&ProviderId::Claude].id, "claude-opus-5-5");
        // One long record pushes the answer out of the 64 KB tail.
        let long = format!(
            "{{\"type\":\"user\",\"message\":{{\"content\":\"{}\"}}}}\n",
            "x".repeat(70_000)
        );
        std::fs::write(&path, format!("{answer}{long}")).unwrap();
        let m = r.read(&reads(false, true), ModelSource::LastUsed, &ids);
        assert_eq!(m[&ProviderId::Claude].id, "claude-opus-5-5");
        // A fresh reader has nothing to keep: no model.
        let mut fresh = reader(&dir);
        assert!(fresh
            .read(&reads(false, true), ModelSource::LastUsed, &ids)
            .is_empty());
    }

    #[test]
    fn a_new_session_without_an_answer_keeps_the_model_last_used() {
        let dir = TempDir::new("next");
        let answer = "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-5-5\"}}\n";
        let first = dir.write(".claude/projects/p/a.jsonl", answer);
        touch(&first, 120);
        let mut r = reader(&dir);
        let ids = [ProviderId::Claude];
        let m = r.read(&reads(false, true), ModelSource::LastUsed, &ids);
        assert_eq!(m[&ProviderId::Claude].id, "claude-opus-5-5");
        // A session just started: the user's first message, no answer yet.
        dir.write(
            ".claude/projects/p/b.jsonl",
            "{\"type\":\"user\",\"message\":{\"content\":\"hi\"}}\n",
        );
        let m = r.read(&reads(false, true), ModelSource::LastUsed, &ids);
        assert_eq!(m[&ProviderId::Claude].id, "claude-opus-5-5");
        // Its first answer takes over.
        dir.write(
            ".claude/projects/p/b.jsonl",
            "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-sonnet-5-5\"}}\n",
        );
        let m = r.read(&reads(false, true), ModelSource::LastUsed, &ids);
        assert_eq!(m[&ProviderId::Claude].id, "claude-sonnet-5-5");
    }

    #[test]
    fn big_broken_and_missing_files_give_no_model() {
        let dir = TempDir::new("broken");
        // Over the cap, even with a model inside.
        let mut big = String::from("{\"model\": \"claude-opus-5-5\", \"pad\": \"");
        big.push_str(&"x".repeat(300 * 1024));
        big.push_str("\"}");
        dir.write(".claude/settings.json", big);
        dir.write(".codex/config.toml", "model = \"gpt-6.1\"\n[broken");
        dir.write(".grok/config.toml", "[models]\ndefault = 4\n");
        dir.write(".copilot/settings.json", b"\xFF\xFE{\x00}\x00");
        // A folder where a file should be.
        std::fs::create_dir_all(dir.path().join(".gemini/antigravity-cli/settings.json")).unwrap();
        dir.write(
            ".claude/projects/p/s.jsonl",
            "{\"type\":\"assistant\",\"message\":{\"model\":",
        );
        let mut r = reader(&dir);
        assert!(r
            .read(&reads(true, true), ModelSource::LastUsed, &ALL)
            .is_empty());
        // No files at all.
        let empty = TempDir::new("none");
        let mut r = reader(&empty);
        assert!(r
            .read(&reads(true, true), ModelSource::LastUsed, &ALL)
            .is_empty());
    }

    #[test]
    fn a_bom_and_crlf_settings_file_reads() {
        let dir = TempDir::new("bom");
        dir.write(
            ".copilot/settings.json",
            b"\xEF\xBB\xBF{\r\n  \"model\": \"gpt-5.5\",\r\n  \"effortLevel\": \"high\"\r\n}\r\n",
        );
        dir.write(
            ".grok/config.toml",
            b"\xEF\xBB\xBF[models]\r\ndefault = \"grok-4.5\"\r\n",
        );
        let mut r = reader(&dir);
        let m = r.read(&reads(true, false), ModelSource::Configured, &ALL);
        assert_eq!(m[&ProviderId::Copilot], info("gpt-5.5", Some("high")));
        assert_eq!(m[&ProviderId::Grok], info("grok-4.5", None));
    }

    /// What the user is told is read (consent.rs) is what the readers read:
    /// the same settings file per tool, sessions for Claude Code and Codex
    /// only, nothing for the tools without a reader.
    #[test]
    fn the_consent_text_names_the_files_read() {
        let root = std::path::Path::new(r"C:\Users\someone");
        let homes = Homes::under(root);
        for id in &ALL {
            let told = crate::consent::read_info(id.config_id()).unwrap();
            match settings_file(&homes, id) {
                Some((path, _)) => {
                    let rel = path.strip_prefix(root).unwrap().display().to_string();
                    let named = format!(r"%USERPROFILE%\{rel}: ");
                    let text = told.settings.unwrap_or_default();
                    assert!(text.starts_with(&named), "{id:?}: {text:?} vs {named:?}");
                }
                None => assert!(told.settings.is_none(), "{id:?}"),
            }
            let reads_sessions = matches!(id, ProviderId::Claude | ProviderId::Codex);
            assert_eq!(told.session.is_some(), reads_sessions, "{id:?}");
        }
    }

    /// The cost of finding the newest session on this PC. Prints counts,
    /// times and the model/effort found, nothing else from the files.
    /// `cargo test --lib session_walk_cost -- --ignored --nocapture`
    #[test]
    #[ignore = "reads this PC's session folders"]
    fn session_walk_cost() {
        fn walk(dir: &std::path::Path, files: &mut usize, dirs: &mut usize) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            *dirs += 1;
            for e in entries.flatten() {
                match e.file_type() {
                    Ok(t) if t.is_dir() => walk(&e.path(), files, dirs),
                    Ok(t) if t.is_file() => {
                        let name = e.file_name();
                        let name = name.to_string_lossy();
                        if name.ends_with(".jsonl") || name.ends_with(".jsonl.zst") {
                            *files += 1;
                            let _ = e.metadata().and_then(|m| m.modified());
                        }
                    }
                    _ => {}
                }
            }
        }
        fn best_of(n: usize, mut f: impl FnMut()) -> Duration {
            (0..n)
                .map(|_| {
                    let t = Instant::now();
                    f();
                    t.elapsed()
                })
                .min()
                .unwrap()
        }
        let homes = Homes::from_env().unwrap();
        for (name, root) in [
            ("claude projects", homes.claude().join("projects")),
            ("codex sessions", homes.codex.join("sessions")),
        ] {
            let (mut files, mut dirs) = (0, 0);
            let t = Instant::now();
            walk(&root, &mut files, &mut dirs);
            let first = t.elapsed();
            let warm = best_of(10, || walk(&root, &mut 0, &mut 0));
            println!("{name}: full walk {files} session files in {dirs} folders, first {first:?}, warm {warm:?}");
        }
        let projects = homes.claude().join("projects");
        let sessions = homes.codex.join("sessions");
        let t = Instant::now();
        session::newest_claude_session(&projects);
        let first = t.elapsed();
        let warm = best_of(20, || {
            session::newest_claude_session(&projects);
        });
        println!("claude bounded (top {} project folders, top level only): first {first:?}, warm {warm:?}", session::MAX_PROJECT_DIRS);
        let t = Instant::now();
        session::newest_codex_session(&sessions);
        let first = t.elapsed();
        let warm = best_of(20, || {
            session::newest_codex_session(&sessions);
        });
        println!(
            "codex bounded (newest {} day folders): first {first:?}, warm {warm:?}",
            session::CODEX_DAY_DIRS
        );

        let all = reads(true, true);
        let mut reader = ModelReader::new();
        for source in [ModelSource::Configured, ModelSource::LastUsed] {
            let t = Instant::now();
            let models = reader.read(&all, source, &ALL);
            let first = t.elapsed();
            let warm = best_of(20, || {
                reader.read(&all, source, &ALL);
            });
            println!("read cycle {source:?}: first {first:?}, warm (cached) {warm:?}");
            for id in &ALL {
                let m = models.get(id);
                println!(
                    "  {}: {:?} / {:?}",
                    id.display_name(),
                    m.map(|m| m.id.as_str()),
                    m.and_then(|m| m.effort.as_deref())
                );
            }
        }
    }
}
