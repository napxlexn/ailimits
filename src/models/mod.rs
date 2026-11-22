// models/mod.rs — the model and effort each tool is set to (or last used),
// and the names shown for them. See docs/en/PROVIDERS.md, "Model labels".
//
// Read only what `[reads]` allows; tolerant of any file shape (an unknown
// shape means no model, never an error); only the model id and the effort
// leave a reader, never other contents of the file.

pub mod catalog;
mod files;
mod name;
mod session;
mod settings;

use crate::config::paths::ToolPaths;
use crate::config::schema::{ModelSource, Reads};
use crate::providers::ProviderId;
use files::{DbUnread, Stamp, Unread};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    /// Fixed folders and environment for tests; None: from the environment
    /// on every read.
    fixed: Option<Env>,
    /// The settings file last read per provider.
    settings: HashMap<ProviderId, Seen>,
    /// The session file last read per provider.
    sessions: HashMap<ProviderId, Seen>,
}

/// Where the tools keep their files, and the environment variable that
/// sets a tool's model over its settings file.
#[derive(Debug, Clone)]
struct Env {
    homes: ToolPaths,
    /// `ANTHROPIC_MODEL` as this process sees it: Claude Code takes it over
    /// the `model` of its settings file.
    anthropic_model: Option<String>,
}

impl Env {
    fn from_process() -> Option<Self> {
        Some(Env {
            homes: tool_paths()?,
            anthropic_model: std::env::var("ANTHROPIC_MODEL").ok(),
        })
    }
}

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
        let Some(env) = self.fixed.clone().or_else(Env::from_process) else {
            return out;
        };
        for id in ids {
            if out.contains_key(id) {
                continue;
            }
            let last = if use_sessions {
                self.last_used(&env.homes, id)
            } else {
                None
            };
            let info = last.or_else(|| {
                if reads.tool_settings {
                    self.configured(&env, id)
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

    /// The model set in the tool's settings file. A file that could not be
    /// read this time (a lock, an I/O error) changes nothing: the label
    /// shown stays, and as its stamp is not taken, the next cycle reads the
    /// file again. Only a file that was read decides what is shown.
    fn configured(&mut self, env: &Env, id: &ProviderId) -> Option<ModelInfo> {
        if *id == ProviderId::Cursor {
            return self.cursor_configured(&env.homes);
        }
        let Some(path) = settings_file(&env.homes, id) else {
            self.settings.remove(id);
            return None;
        };
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
            match files::read_capped(&path, files::SETTINGS_CAP) {
                Ok(bytes) => bytes.and_then(|bytes| parse_settings(id, &bytes, env)),
                Err(Unread) => return kept(&self.settings, id, &path),
            }
        };
        let seen = Seen { path, stamp, info };
        let info = seen.info.clone();
        self.settings.insert(id.clone(), seen);
        info
    }

    /// The model of the newest session, from the end of its file.
    fn last_used(&mut self, homes: &ToolPaths, id: &ProviderId) -> Option<ModelInfo> {
        let (newest, parse): (Option<PathBuf>, TailParser) = match id {
            ProviderId::Claude => (
                session::newest_claude_session(&homes.claude_projects()),
                session::claude_last,
            ),
            ProviderId::Codex => (
                homes
                    .codex_sessions()
                    .and_then(|dir| session::newest_codex_session(&dir)),
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
        // Read back from the end until a model record turns up, at most
        // 1 MB. None there keeps the model last found: a session just
        // started has no answer yet, so the previous session's model is
        // still the one last used. A read that failed (a lock, the file cut
        // short while read) keeps it too, without taking the new stamp, so
        // the next cycle reads the file again.
        let before = self.sessions.get(id).and_then(|seen| seen.info.clone());
        let info = match files::scan_back(&path, files::TAIL_STEP, files::TAIL_CAP, parse) {
            Ok(found) => found.or(before),
            Err(Unread) => return before,
        };
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

    /// Cursor's Agent model: one entry of the editor's state database, of
    /// which only the Agent's model settings are taken (files.rs
    /// `cursor_agent_model`). Read again only when the database or its
    /// write-ahead log changed; while it cannot be read (Cursor holds a
    /// lock, an I/O error), the model found before stays and the next cycle
    /// asks again. While Cursor is closed and the database is where it
    /// cannot be read without leaving files beside it (a network
    /// %APPDATA%), the model found before stays until the database changes.
    /// Only an answer from the database is kept.
    fn cursor_configured(&mut self, homes: &ToolPaths) -> Option<ModelInfo> {
        let id = ProviderId::Cursor;
        let path = homes.cursor_state_db();
        let Some(stamp) = files::db_stamp(&path) else {
            self.settings.remove(&id);
            return None;
        };
        if let Some(seen) = self.settings.get(&id) {
            if seen.path == path && seen.stamp == stamp {
                return seen.info.clone();
            }
        }
        let info = match files::cursor_agent_model(&path) {
            Ok(agent) => agent.and_then(|json| settings::cursor(json.as_bytes())),
            Err(DbUnread::ThisTime) => return kept(&self.settings, &id, &path),
            // Not opened while Cursor is closed: what was shown stays, kept
            // by this stamp, so the database is not looked at again until
            // it changes (Cursor running brings its -wal).
            Err(DbUnread::UntilChanged) => kept(&self.settings, &id, &path),
        };
        self.settings.insert(
            id,
            Seen {
                path,
                stamp,
                info: info.clone(),
            },
        );
        info
    }
}

/// What a file that could not be read this time leaves on screen: the
/// model last read from that same file, if any.
fn kept(seen: &HashMap<ProviderId, Seen>, id: &ProviderId, path: &Path) -> Option<ModelInfo> {
    seen.get(id)
        .filter(|s| s.path == path)
        .and_then(|s| s.info.clone())
}

/// A tool's settings file; None for Cursor, whose model is in the editor's
/// state database (`cursor_configured`), and for a tool whose home is
/// unknown (its variable set to a relative path). The files are the ones
/// `consent::READ_INFO` names, resolved by `config::paths`.
fn settings_file(homes: &ToolPaths, id: &ProviderId) -> Option<PathBuf> {
    match id {
        ProviderId::Claude => Some(homes.claude_settings()),
        ProviderId::Codex => homes.codex_config(),
        ProviderId::Grok => homes.grok_config(),
        ProviderId::Copilot => homes.copilot_settings(),
        ProviderId::Antigravity => Some(homes.antigravity_cli_settings()),
        ProviderId::Cursor => None,
    }
}

/// The model in a settings file's bytes, by the tool's own rules.
fn parse_settings(id: &ProviderId, bytes: &[u8], env: &Env) -> Option<ModelInfo> {
    match id {
        ProviderId::Claude => settings::claude(bytes, env.anthropic_model.as_deref()),
        ProviderId::Codex => settings::codex(bytes),
        ProviderId::Grok => settings::grok(bytes),
        ProviderId::Copilot => settings::copilot(bytes),
        ProviderId::Antigravity => settings::antigravity(bytes),
        ProviderId::Cursor => None,
    }
}

/// Where the tools keep their files, for the model labels.
///
/// Debug builds only: `AILIMITS_MODEL_HOME` stands in for %USERPROFILE%
/// (and %APPDATA% under its usual place), so a live test can show prepared
/// settings and sessions without touching the tools' own files. Sign-in
/// reads are not affected. Release builds never look at it.
fn tool_paths() -> Option<ToolPaths> {
    let test_home = if cfg!(debug_assertions) {
        env_dir("AILIMITS_MODEL_HOME")
    } else {
        None
    };
    match test_home {
        Some(home) => ToolPaths::resolve(Some(home), None, |var| std::env::var_os(var)),
        None => ToolPaths::from_env(),
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

    /// The length of what is read is bounded where it is read
    /// (files.rs `clean`: at most 128 bytes): an id far past any a tool
    /// writes gives no label at all, so nothing downstream ever measures or
    /// wraps more; one at the bound is kept whole.
    #[test]
    fn a_huge_model_id_is_not_taken() {
        let dir = TempDir::new("model-huge-id");
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        let read = |model: &str| {
            std::fs::write(
                claude.join("settings.json"),
                format!(r#"{{"model": "{model}"}}"#),
            )
            .unwrap();
            reader(&dir)
                .read(
                    &reads(true, false),
                    ModelSource::Configured,
                    &[ProviderId::Claude],
                )
                .remove(&ProviderId::Claude)
        };
        assert!(read(&"m".repeat(5000)).is_none());
        let at_bound = "m".repeat(128);
        assert_eq!(read(&at_bound).map(|i| i.id), Some(at_bound));
    }

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
        reader_at(dir.path(), None)
    }

    fn reader_at(home: &Path, anthropic_model: Option<&str>) -> ModelReader {
        ModelReader {
            fixed: Some(Env {
                homes: ToolPaths::at_home(home),
                anthropic_model: anthropic_model.map(String::from),
            }),
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
        // Records past the 1 MB read push the answer out of reach.
        let long = format!(
            "{{\"type\":\"user\",\"message\":{{\"content\":\"{}\"}}}}\n",
            "x".repeat(1_100_000)
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

    /// Claude Code 2.1.323 writes a 110-160 KB `prompt_snapshot` record
    /// right after a session's first answer: the answer is found behind it,
    /// by a reader that never saw the file before.
    #[test]
    fn the_answer_is_found_behind_a_long_record() {
        let dir = TempDir::new("snapshot");
        let answer = "{\"type\":\"assistant\",\"effort\":\"low\",\"message\":{\"model\":\"claude-haiku-5-5\"}}\n";
        let snapshot = format!(
            "{{\"type\":\"attachment\",\"attachment\":{{\"type\":\"prompt_snapshot\",\"text\":\"{}\"}}}}\n",
            "y".repeat(150_000)
        );
        let small = "{\"type\":\"system\",\"subtype\":\"turn_duration\"}\n".repeat(40);
        dir.write(
            ".claude/projects/p/s.jsonl",
            format!("{{\"type\":\"user\"}}\n{answer}{snapshot}{small}"),
        );
        let mut r = reader(&dir);
        let m = r.read(
            &reads(false, true),
            ModelSource::LastUsed,
            &[ProviderId::Claude],
        );
        assert_eq!(m[&ProviderId::Claude].id, "claude-haiku-5-5");
        assert_eq!(m[&ProviderId::Claude].effort.as_deref(), Some("low"));
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

    /// Holds a file open without sharing, as a scanner or a backup does.
    #[cfg(target_os = "windows")]
    fn lock(path: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::File::options()
            .read(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    }

    /// A settings file that cannot be read for a moment keeps its label,
    /// and is read again on the next cycle even though it does not change
    /// after that: one failed read never caches "no model" for the day.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_settings_file_that_cannot_be_read_keeps_its_label_and_is_read_again() {
        let dir = TempDir::new("unread");
        let path = dir.write(".codex/config.toml", "model = \"gpt-6.1\"\n");
        touch(&path, 100);
        let ids = [ProviderId::Codex];
        let configured = |r: &mut ModelReader| {
            r.read(&reads(true, false), ModelSource::Configured, &ids)
                .get(&ProviderId::Codex)
                .map(|m| m.id.clone())
        };
        // Locked from the very first read: nothing yet, nothing cached.
        let mut r = reader(&dir);
        let held = lock(&path);
        assert_eq!(configured(&mut r), None);
        assert!(r.settings.is_empty());
        drop(held);
        assert_eq!(configured(&mut r).as_deref(), Some("gpt-6.1"));
        // Changed, then locked while the cycle reads it: the label stays.
        std::fs::write(&path, "model = \"gpt-7.1\"\n").unwrap();
        touch(&path, 50);
        let held = lock(&path);
        assert_eq!(configured(&mut r).as_deref(), Some("gpt-6.1"));
        assert_eq!(configured(&mut r).as_deref(), Some("gpt-6.1"));
        drop(held);
        // Released, the file untouched since: read again all the same.
        assert_eq!(configured(&mut r).as_deref(), Some("gpt-7.1"));
    }

    /// The same for the newest session: a session file that cannot be read
    /// keeps the model last used, and is read once it can be.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_session_that_cannot_be_read_keeps_the_model_last_used() {
        let dir = TempDir::new("unread-session");
        let path = dir.write(
            ".claude/projects/p/s.jsonl",
            "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-opus-5-5\"}}\n",
        );
        touch(&path, 100);
        let ids = [ProviderId::Claude];
        let last = |r: &mut ModelReader| {
            r.read(&reads(false, true), ModelSource::LastUsed, &ids)
                .get(&ProviderId::Claude)
                .map(|m| m.id.clone())
        };
        let mut r = reader(&dir);
        assert_eq!(last(&mut r).as_deref(), Some("claude-opus-5-5"));
        std::fs::write(
            &path,
            "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-sonnet-5-5\"}}\n",
        )
        .unwrap();
        touch(&path, 50);
        let held = lock(&path);
        assert_eq!(last(&mut r).as_deref(), Some("claude-opus-5-5"));
        drop(held);
        assert_eq!(last(&mut r).as_deref(), Some("claude-sonnet-5-5"));
    }

    /// The tool-home variables move the settings and sessions read, and a
    /// relative one reads nothing of that tool: not the working directory,
    /// not the default folder (it may be another account's).
    #[test]
    fn the_tool_home_variables_are_followed_and_only_when_absolute() {
        let dir = TempDir::new("homes");
        every_tool(&dir);
        let moved = dir.path().join("elsewhere");
        dir.write(
            "elsewhere/codex/config.toml",
            "model = \"gpt-6-luna\"\nmodel_reasoning_effort = \"low\"\n",
        );
        dir.write(
            "elsewhere/grok/config.toml",
            "[models]\ndefault = \"grok-4.7\"\n",
        );
        dir.write("elsewhere/copilot/settings.json", r#"{"model": "gpt-5.5"}"#);
        let with = |vars: Vec<(&'static str, std::ffi::OsString)>| {
            let homes = ToolPaths::resolve(Some(dir.path().to_path_buf()), None, move |var| {
                vars.iter().find(|(k, _)| *k == var).map(|(_, v)| v.clone())
            })
            .unwrap();
            ModelReader {
                fixed: Some(Env {
                    homes,
                    anthropic_model: None,
                }),
                ..ModelReader::default()
            }
        };
        let mut r = with(vec![
            ("CODEX_HOME", moved.join("codex").into_os_string()),
            ("GROK_HOME", moved.join("grok").into_os_string()),
            ("COPILOT_HOME", moved.join("copilot").into_os_string()),
        ]);
        let m = r.read(&reads(true, false), ModelSource::Configured, &ALL);
        assert_eq!(m[&ProviderId::Codex], info("gpt-6-luna", Some("low")));
        assert_eq!(m[&ProviderId::Grok], info("grok-4.7", None));
        assert_eq!(m[&ProviderId::Copilot], info("gpt-5.5", None));
        // The others are where they were.
        assert_eq!(m[&ProviderId::Claude].id, "claude-opus-5-5");
        // Codex's sessions moved too: none there, so the configured model.
        let m = r.read(
            &reads(true, true),
            ModelSource::LastUsed,
            &[ProviderId::Codex],
        );
        assert_eq!(m[&ProviderId::Codex], info("gpt-6-luna", Some("low")));
        // Relative: nothing of those tools at all.
        let mut r = with(vec![
            ("CODEX_HOME", "codex".into()),
            ("GROK_HOME", r".\grok".into()),
            ("COPILOT_HOME", r"\copilot".into()),
        ]);
        let m = r.read(&reads(true, true), ModelSource::LastUsed, &ALL);
        for id in [ProviderId::Codex, ProviderId::Grok, ProviderId::Copilot] {
            assert!(!m.contains_key(&id), "{id:?}");
        }
        assert_eq!(m[&ProviderId::Claude].id, "claude-sonnet-5");
    }

    /// `ANTHROPIC_MODEL` is the model Claude Code runs with over its
    /// settings file's `model`: the label follows it, and takes that
    /// model's effort.
    #[test]
    fn anthropic_model_in_the_environment_is_the_configured_claude_model() {
        let dir = TempDir::new("anthropic-model");
        dir.write(
            ".claude/settings.json",
            r#"{"model": "claude-opus-5-5", "modelSettings": {"claude-opus-5-5": {"effortLevel": "xhigh"}, "claude-sonnet-5-5": {"effortLevel": "low"}}}"#,
        );
        let ids = [ProviderId::Claude];
        let mut r = reader_at(dir.path(), Some("claude-sonnet-5-5"));
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert_eq!(
            m[&ProviderId::Claude],
            info("claude-sonnet-5-5", Some("low"))
        );
        // Set but empty: the file's model.
        let mut r = reader_at(dir.path(), Some("  "));
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert_eq!(
            m[&ProviderId::Claude],
            info("claude-opus-5-5", Some("xhigh"))
        );
    }

    /// What the user is told is read (consent.rs) is what the readers read:
    /// the same settings file per tool, sessions for Claude Code and Codex
    /// only, nothing for the tools without a reader.
    #[test]
    fn the_consent_text_names_the_files_read() {
        let root = std::path::Path::new(r"C:\Users\someone");
        let homes = ToolPaths::at_home(root);
        for id in &ALL {
            let told = crate::consent::read_info(id.config_id()).unwrap();
            let named = match settings_file(&homes, id) {
                Some(path) => {
                    let rel = path.strip_prefix(root).unwrap().display().to_string();
                    format!(r"%USERPROFILE%\{rel}: ")
                }
                None => {
                    assert_eq!(*id, ProviderId::Cursor);
                    let roaming = root.join("AppData").join("Roaming");
                    let db = homes.cursor_state_db();
                    let rel = db.strip_prefix(&roaming).unwrap().display().to_string();
                    format!(r"%APPDATA%\{rel}: ")
                }
            };
            let text = told.settings.unwrap_or_default();
            assert!(text.starts_with(&named), "{id:?}: {text:?} vs {named:?}");
            let reads_sessions = matches!(id, ProviderId::Claude | ProviderId::Codex);
            assert_eq!(told.session.is_some(), reads_sessions, "{id:?}");
        }
    }

    /// Cursor's Agent model, from a database shaped like the editor's: only
    /// the Agent's settings are taken; read again only after a change; a
    /// database without the entry, or not a database, gives no model.
    #[cfg(target_os = "windows")]
    #[test]
    fn cursor_reads_the_agent_model_from_the_editor_database() {
        use crate::platform::sqlite::test_support::TestDb;
        let dir = TempDir::new("cursor");
        let db_path = ToolPaths::at_home(dir.path()).cursor_state_db();
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let db = TestDb::create(&db_path);
        db.exec("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);");
        let key = "src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser";
        let entry = |composer: &str| {
            format!(
                "INSERT INTO ItemTable VALUES ('{key}', '{{\"theme\": \"x\", \"aiSettings\": {{\"modelConfig\": {{\"cmd-k\": {{\"modelName\": \"default\"}}, \"composer\": {composer}}}}}}}');"
            )
        };
        db.exec(&entry(
            r#"{"modelName": "grok-4.7", "maxMode": false, "selectedModels": [{"modelId": "grok-4.7", "parameters": [{"id": "reasoning_effort", "value": "high"}]}]}"#,
        ));
        // The sign-in sits in the same table; it is never what is read.
        db.exec("INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', 'not-a-token');");
        let mut r = reader(&dir);
        let ids = [ProviderId::Cursor];
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert_eq!(m[&ProviderId::Cursor], info("grok-4.7", Some("high")));
        // Last used: Cursor keeps no sessions, so the chosen model shows.
        let m = r.read(&reads(true, true), ModelSource::LastUsed, &ids);
        assert_eq!(m[&ProviderId::Cursor], info("grok-4.7", Some("high")));
        // Changed in the editor: read again.
        db.exec(&entry(r#"{"modelName": "default", "selectedModels": []}"#));
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert_eq!(m[&ProviderId::Cursor], info("auto", None));
        // Settings not allowed: nothing, and nothing kept.
        assert!(r
            .read(&reads(false, false), ModelSource::Configured, &ids)
            .is_empty());
        assert!(r.settings.is_empty());
        // No such entry: no model.
        db.exec(&format!("DELETE FROM ItemTable WHERE key = '{key}';"));
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert!(m.is_empty());
        // An entry that is not JSON: no model, no error.
        db.exec(&format!(
            "INSERT INTO ItemTable VALUES ('{key}', 'not json');"
        ));
        let m = r.read(&reads(true, false), ModelSource::Configured, &ids);
        assert!(m.is_empty());
        drop(db);
        // Not a database at all.
        std::fs::write(&db_path, b"not a database").unwrap();
        let mut fresh = reader(&dir);
        assert!(fresh
            .read(&reads(true, false), ModelSource::Configured, &ids)
            .is_empty());
    }

    /// Cursor's database that cannot be opened for a moment keeps the
    /// Agent's label (the error is not cached as "no model"), and is read
    /// again once it can be.
    #[cfg(target_os = "windows")]
    #[test]
    fn cursor_keeps_its_label_while_the_database_cannot_be_read() {
        use crate::platform::sqlite::test_support::TestDb;
        let dir = TempDir::new("cursor-unread");
        let db_path = ToolPaths::at_home(dir.path()).cursor_state_db();
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let key = "src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser";
        let write = |model: &str| {
            let db = TestDb::create(&db_path);
            db.exec("CREATE TABLE IF NOT EXISTS ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);");
            db.exec(&format!(
                "INSERT INTO ItemTable VALUES ('{key}', '{{\"aiSettings\": {{\"modelConfig\": {{\"composer\": {{\"modelName\": \"{model}\"}}}}}}}}');"
            ));
        };
        write("grok-4.7");
        touch(&db_path, 100);
        let ids = [ProviderId::Cursor];
        let mut r = reader(&dir);
        let label = |r: &mut ModelReader| {
            r.read(&reads(true, false), ModelSource::Configured, &ids)
                .get(&ProviderId::Cursor)
                .map(|m| m.id.clone())
        };
        assert_eq!(label(&mut r).as_deref(), Some("grok-4.7"));
        write("gpt-5.5-and-more");
        touch(&db_path, 50);
        let held = lock(&db_path);
        assert_eq!(label(&mut r).as_deref(), Some("grok-4.7"));
        drop(held);
        assert_eq!(label(&mut r).as_deref(), Some("gpt-5.5-and-more"));
    }

    /// Cursor's database where it has no URI form (a network %APPDATA%;
    /// here a `\\?\` path): read while Cursor runs (its -wal is there);
    /// once Cursor is closed it is not opened at all - a plain open would
    /// leave -wal and -shm beside it - the label read before stays, and the
    /// database is not looked at again until it changes.
    #[cfg(target_os = "windows")]
    #[test]
    fn cursor_closed_at_a_path_without_a_uri_form_is_not_opened() {
        use crate::platform::sqlite::test_support::TestDb;
        let dir = TempDir::new("cursor-no-uri");
        let mut verbatim = std::ffi::OsString::from(r"\\?\");
        verbatim.push(dir.path().as_os_str());
        let home = PathBuf::from(verbatim);
        let db_path = ToolPaths::at_home(dir.path()).cursor_state_db();
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let key = "src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser";
        let sidecars = || {
            ["-wal", "-shm"]
                .into_iter()
                .filter(|s| {
                    let mut p = db_path.as_os_str().to_owned();
                    p.push(s);
                    Path::new(&p).exists()
                })
                .collect::<Vec<_>>()
        };
        let ids = [ProviderId::Cursor];
        let mut r = reader_at(&home, None);
        let label = |r: &mut ModelReader| {
            r.read(&reads(true, false), ModelSource::Configured, &ids)
                .get(&ProviderId::Cursor)
                .map(|m| m.id.clone())
        };
        {
            // Cursor running: WAL, its -wal there.
            let db = TestDb::create(&db_path);
            db.exec("PRAGMA journal_mode=WAL;");
            db.exec("PRAGMA wal_autocheckpoint=0;");
            db.exec("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);");
            db.exec(&format!(
                "INSERT INTO ItemTable VALUES ('{key}', '{{\"aiSettings\": {{\"modelConfig\": {{\"composer\": {{\"modelName\": \"grok-4.7\"}}}}}}}}');"
            ));
            assert!(sidecars().contains(&"-wal"));
            assert_eq!(label(&mut r).as_deref(), Some("grok-4.7"));
        }
        // Cursor closed: its -wal and -shm are gone with it.
        assert!(sidecars().is_empty(), "{:?}", sidecars());
        let path = ToolPaths::at_home(&home).cursor_state_db();
        assert_eq!(
            files::cursor_agent_model(&path),
            Err(DbUnread::UntilChanged)
        );
        assert_eq!(label(&mut r).as_deref(), Some("grok-4.7"));
        assert!(sidecars().is_empty(), "left {:?}", sidecars());
        // Kept by the stamp: not asked again while nothing changed.
        let seen = r.settings.get(&ProviderId::Cursor).expect("kept");
        assert_eq!(seen.stamp, files::db_stamp(&path).expect("stamp"));
    }

    /// The labels for prepared homes: every folder under `AILIMITS_PROBE_HOMES`
    /// is read as a %USERPROFILE% (settings and sessions copied there from
    /// live calls), and the model, effort and full label of each tool are
    /// printed, both sources. Prints nothing else from the files.
    /// `AILIMITS_PROBE_HOMES=<dir> cargo test --lib probe_labels -- --ignored --nocapture`
    #[test]
    #[ignore = "reads the folders AILIMITS_PROBE_HOMES names"]
    fn probe_labels() {
        let Some(root) = env_dir("AILIMITS_PROBE_HOMES") else {
            println!("AILIMITS_PROBE_HOMES not set");
            return;
        };
        let mut homes: Vec<PathBuf> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        homes.sort();
        use crate::config::schema::ModelPart;
        let parts = [ModelPart::Model, ModelPart::Version, ModelPart::Effort];
        let names = std::collections::BTreeMap::new();
        for home in homes {
            let name = home.file_name().unwrap().to_string_lossy().to_string();
            for source in [ModelSource::Configured, ModelSource::LastUsed] {
                let mut r = reader_at(&home, None);
                let models = r.read(&reads(true, true), source, &ALL);
                for id in &ALL {
                    let Some(m) = models.get(id) else { continue };
                    let label = crate::rows::model_label(m, id, &parts, &names);
                    println!(
                        "PROBE\t{name}\t{source:?}\t{}\t{}\t{}\t{}",
                        id.display_name(),
                        m.id,
                        m.effort.as_deref().unwrap_or("-"),
                        label
                            .map(|l| l.steps().join(" | "))
                            .unwrap_or_else(|| "-".into())
                    );
                }
            }
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
        let homes = ToolPaths::from_env().unwrap();
        for (name, root) in [
            ("claude projects", homes.claude_projects()),
            ("codex sessions", homes.codex_sessions().unwrap_or_default()),
        ] {
            let (mut files, mut dirs) = (0, 0);
            let t = Instant::now();
            walk(&root, &mut files, &mut dirs);
            let first = t.elapsed();
            let warm = best_of(10, || walk(&root, &mut 0, &mut 0));
            println!("{name}: full walk {files} session files in {dirs} folders, first {first:?}, warm {warm:?}");
        }
        let projects = homes.claude_projects();
        let sessions = homes.codex_sessions().unwrap_or_default();
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
        // The read back from the end: as it runs, and its worst case (every
        // line parsed, no model found, the whole 1 MB read).
        if let Some(newest) = session::newest_claude_session(&projects) {
            let len = std::fs::metadata(&newest).map(|m| m.len()).unwrap_or(0);
            let real = best_of(5, || {
                let _ = files::scan_back(
                    &newest,
                    files::TAIL_STEP,
                    files::TAIL_CAP,
                    session::claude_last,
                );
            });
            let worst = best_of(5, || {
                let _ = files::scan_back(&newest, files::TAIL_STEP, files::TAIL_CAP, |t| {
                    session::claude_last(t);
                    None::<()>
                });
            });
            println!("claude newest session, {len} bytes: the read {real:?}, worst case {worst:?}");
        }
        // Cursor's Agent model: one lookup in the editor's database.
        let db = homes.cursor_state_db();
        if db.is_file() {
            let lookup = best_of(10, || {
                let _ = files::cursor_agent_model(&db);
            });
            println!("cursor agent model lookup: {lookup:?}");
        }

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
