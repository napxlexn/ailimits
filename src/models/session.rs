// models/session.rs — the model last used, from the end of the newest
// session file of Claude Code and Codex CLI. Only the model and the effort
// of one record are taken; no other field of any record is kept.
//
// Shapes verified on the owner's PC 2026-11-11 (key names only):
// - Claude Code: projects\<project>\<session>.jsonl, one record per line;
//   an answer is `{"type": "assistant", "isSidechain": false, "effort":
//   "xhigh", "message": {"model": "claude-opus-5-5", ...}, ...}`. Files
//   deeper down (<session>\subagents\...) are subagents, whose model is the
//   one the main session picked for them, not the user's: never read.
// - Codex CLI: sessions\YYYY\MM\DD\rollout-*.jsonl; a turn starts with
//   `{"type": "turn_context", "payload": {"model", "effort", ...}}`, newer
//   versions also write `{"type": "event_msg", "payload": {"type":
//   "thread_settings_applied", "thread_settings": {"model",
//   "reasoning_effort", ...}}}`. Newer versions compress finished sessions
//   to .jsonl.zst (every file on the owner's PC): those cannot be read
//   without a decoder, so a compressed newest session means no last-used
//   model.

use super::files::clean;
use super::ModelInfo;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// At most this many Claude Code project folders are looked into per read:
/// the ones a session was most recently started in.
pub(super) const MAX_PROJECT_DIRS: usize = 32;
/// Codex day folders looked into per read, newest first: today's and the
/// one before, for a session that runs past midnight.
pub(super) const CODEX_DAY_DIRS: usize = 2;

/// The model of the last Claude Code answer in a session tail.
pub(super) fn claude_last(tail: &str) -> Option<ModelInfo> {
    last_record(tail, |r| {
        if r.get("type")?.as_str()? != "assistant"
            || r.get("isSidechain") == Some(&Value::Bool(true))
        {
            return None;
        }
        let id = clean(r.get("message")?.get("model"))?;
        // "<synthetic>" marks a message Claude Code wrote itself.
        if id.starts_with('<') {
            return None;
        }
        Some(ModelInfo {
            id: super::settings::claude_model_id(&id)?,
            effort: clean(r.get("effort")),
        })
    })
}

/// The model of the last Codex turn (or settings change) in a session tail.
pub(super) fn codex_last(tail: &str) -> Option<ModelInfo> {
    last_record(tail, |r| {
        let payload = r.get("payload")?;
        match r.get("type")?.as_str()? {
            "turn_context" => Some(ModelInfo {
                id: clean(payload.get("model"))?,
                effort: clean(payload.get("effort")),
            }),
            "event_msg" if payload.get("type")?.as_str()? == "thread_settings_applied" => {
                let settings = payload.get("thread_settings")?;
                Some(ModelInfo {
                    id: clean(settings.get("model"))?,
                    effort: clean(settings.get("reasoning_effort")),
                })
            }
            _ => None,
        }
    })
}

/// The last line, from the end, that parses and that `pick` takes. Lines
/// without a "model" key are skipped unparsed.
fn last_record(tail: &str, pick: impl Fn(&Value) -> Option<ModelInfo>) -> Option<ModelInfo> {
    tail.lines()
        .rev()
        .filter(|line| line.contains("\"model\""))
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find_map(|record| pick(&record))
}

/// The newest main-session file of Claude Code: `<projects>\<project>\*.jsonl`
/// in the project folders a session was most recently started in.
pub(super) fn newest_claude_session(projects: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<(SystemTime, PathBuf)> = fs::read_dir(projects)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.0));
    dirs.truncate(MAX_PROJECT_DIRS);
    newest_file(dirs.iter().map(|(_, d)| d.as_path()), |name| {
        name.ends_with(".jsonl")
    })
}

/// The newest Codex session file, compressed or not, in the newest day
/// folders of `<sessions>\YYYY\MM\DD`.
pub(super) fn newest_codex_session(sessions: &Path) -> Option<PathBuf> {
    let mut days = Vec::new();
    'years: for year in numbered_dirs(sessions) {
        for month in numbered_dirs(&year) {
            for day in numbered_dirs(&month) {
                days.push(day);
                if days.len() == CODEX_DAY_DIRS {
                    break 'years;
                }
            }
        }
    }
    newest_file(days.iter().map(PathBuf::as_path), |name| {
        name.starts_with("rollout-") && (name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"))
    })
}

/// Whether a session file is compressed (and so cannot be read).
pub(super) fn is_compressed(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zst"))
}

/// Subfolders named only by digits, highest first ("2026", "11", "09").
fn numbered_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            (!name.is_empty() && name.bytes().all(|b| b.is_ascii_digit())).then(|| (name, e.path()))
        })
        .collect();
    out.sort_by(|a, b| (b.0.len(), &b.0).cmp(&(a.0.len(), &a.0)));
    out.into_iter().map(|(_, p)| p).collect()
}

/// The most recently modified regular file directly in `dirs` whose name
/// `wanted` accepts. One listing per folder; on Windows the listing carries
/// the times, so no file is opened.
fn newest_file<'a>(
    dirs: impl Iterator<Item = &'a Path>,
    wanted: impl Fn(&str) -> bool,
) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for e in entries.flatten() {
            if !e.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            if !e
                .file_name()
                .to_str()
                .is_some_and(|n| wanted(&n.to_ascii_lowercase()))
            {
                continue;
            }
            let Some(modified) = e.metadata().ok().and_then(|m| m.modified().ok()) else {
                continue;
            };
            if best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, e.path()));
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::super::files::test_dir::TempDir;
    use super::*;
    use std::time::Duration;

    fn info(id: &str, effort: Option<&str>) -> Option<ModelInfo> {
        Some(ModelInfo {
            id: id.into(),
            effort: effort.map(String::from),
        })
    }

    /// Sets a file's modification time, so "newest" does not hang on the
    /// clock's resolution.
    fn touch(path: &Path, secs_ago: u64) {
        let t = SystemTime::now() - Duration::from_secs(secs_ago);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    const CLAUDE_ANSWER: &str = r#"{"type":"assistant","isSidechain":false,"effort":"xhigh","message":{"model":"claude-opus-5-5","role":"assistant","content":[{"type":"text","text":"hi"}]},"uuid":"1"}"#;

    #[test]
    fn claude_takes_the_last_answer() {
        let older = CLAUDE_ANSWER
            .replace("claude-opus-5-5", "claude-sonnet-5")
            .replace("xhigh", "high");
        let user =
            r#"{"type":"user","message":{"role":"user","content":"the model is \"model\": x"}}"#;
        let tail = format!("{older}\n{CLAUDE_ANSWER}\r\n{user}\n");
        assert_eq!(claude_last(&tail), info("claude-opus-5-5", Some("xhigh")));
        // An answer without an effort (older versions).
        let plain = r#"{"type":"assistant","message":{"model":"claude-haiku-5-5"}}"#;
        assert_eq!(claude_last(plain), info("claude-haiku-5-5", None));
    }

    #[test]
    fn claude_skips_what_is_not_the_users_answer() {
        let synthetic = r#"{"type":"assistant","message":{"model":"<synthetic>"}}"#;
        let side = CLAUDE_ANSWER
            .replace("\"isSidechain\":false", "\"isSidechain\":true")
            .replace("opus-5-5", "haiku-5-5");
        let request = r#"{"type":"api-request","params":{"model":"claude-fable-5"}}"#;
        let tail = format!("{CLAUDE_ANSWER}\n{synthetic}\n{side}\n{request}\n");
        assert_eq!(claude_last(&tail), info("claude-opus-5-5", Some("xhigh")));
        // Nothing usable at all.
        assert_eq!(claude_last(&format!("{synthetic}\n{request}\n")), None);
    }

    #[test]
    fn broken_tails_give_no_model() {
        let cut = &CLAUDE_ANSWER[..CLAUDE_ANSWER.len() / 2];
        for tail in [
            "",
            "\n\n",
            cut,
            "{\"model\"",
            "not json \"model\"",
            "[\"model\"]",
            r#"{"type":"assistant","message":"model"}"#,
            r#"{"type":"assistant","message":{"model":5}}"#,
            r#"{"type":"assistant","message":{"model":""}}"#,
            r#"{"type":5,"message":{"model":"x"}}"#,
            r#"{"type":"turn_context","payload":{"model":null}}"#,
            r#"{"type":"turn_context","payload":"model"}"#,
            r#"{"type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"model":[1]}}}"#,
            "\u{FFFD}\u{FFFD}\"model\"\u{FFFD}",
        ] {
            assert_eq!(claude_last(tail), None, "{tail}");
            assert_eq!(codex_last(tail), None, "{tail}");
        }
        // A broken last line does not hide a whole one before it.
        assert_eq!(
            claude_last(&format!("{CLAUDE_ANSWER}\n{cut}")),
            info("claude-opus-5-5", Some("xhigh"))
        );
    }

    #[test]
    fn codex_takes_the_last_turn_or_settings_change() {
        let turn = r#"{"timestamp":"t","type":"turn_context","ordinal":3,"payload":{"cwd":"x","model":"gpt-6.1-sol","effort":"high","summary":"auto"}}"#;
        let changed = r#"{"type":"event_msg","payload":{"type":"thread_settings_applied","thread_id":"x","thread_settings":{"model":"gpt-6.1","reasoning_effort":"xhigh"}}}"#;
        let item = r#"{"type":"response_item","payload":{"type":"message","content":[{"text":"\"model\": \"fake\""}]}}"#;
        assert_eq!(
            codex_last(&format!("{turn}\n{item}\n")),
            info("gpt-6.1-sol", Some("high"))
        );
        assert_eq!(
            codex_last(&format!("{turn}\n{changed}\n{item}\n")),
            info("gpt-6.1", Some("xhigh"))
        );
        assert_eq!(
            codex_last(&format!("{changed}\n{turn}\n")),
            info("gpt-6.1-sol", Some("high"))
        );
        // Another event type with a model is not a settings change.
        let other = r#"{"type":"event_msg","payload":{"type":"token_count","model":"x"}}"#;
        assert_eq!(codex_last(other), None);
    }

    #[test]
    fn the_newest_claude_session_is_a_main_session() {
        let dir = TempDir::new("claude-walk");
        let projects = dir.path().join("projects");
        let a = dir.write("projects/D--a/aaaa.jsonl", "{}");
        let b = dir.write("projects/D--b/bbbb.jsonl", "{}");
        let sub = dir.write("projects/D--b/bbbb/subagents/agent-1.jsonl", "{}");
        let other = dir.write("projects/D--b/notes.txt", "x");
        touch(&a, 300);
        touch(&b, 200);
        touch(&sub, 10);
        touch(&other, 5);
        assert_eq!(newest_claude_session(&projects), Some(b.clone()));
        // An older session appended to becomes the newest.
        touch(&a, 1);
        assert_eq!(newest_claude_session(&projects), Some(a));
        // Nothing there.
        assert_eq!(newest_claude_session(&dir.path().join("missing")), None);
        let empty = TempDir::new("claude-empty");
        assert_eq!(newest_claude_session(empty.path()), None);
    }

    #[test]
    fn the_newest_codex_session_is_in_the_newest_days() {
        let dir = TempDir::new("codex-walk");
        let sessions = dir.path().join("sessions");
        let old = dir.write(
            "sessions/2026/09/30/rollout-2026-09-30T10-00-00-x.jsonl",
            "{}",
        );
        let yesterday = dir.write(
            "sessions/2026/10/31/rollout-2026-10-31T23-00-00-y.jsonl",
            "{}",
        );
        let today = dir.write(
            "sessions/2026/11/01/rollout-2026-11-01T08-00-00-z.jsonl.zst",
            "x",
        );
        let stray = dir.write("sessions/2026/11/01/notes.jsonl", "{}");
        dir.write("sessions/2026/11/not-a-day/rollout-a.jsonl", "{}");
        touch(&old, 1);
        touch(&yesterday, 50);
        touch(&today, 100);
        touch(&stray, 0);
        // Yesterday's session still running is newer than today's finished one.
        assert_eq!(newest_codex_session(&sessions), Some(yesterday.clone()));
        touch(&today, 0);
        let newest = newest_codex_session(&sessions).unwrap();
        assert_eq!(newest, today);
        assert!(is_compressed(&newest));
        assert!(!is_compressed(&yesterday));
        // "10" sorts above "9" (numbers, not text): read as text, the two
        // newest days would be 9/30 and 9/29.
        let dir2 = TempDir::new("codex-sort");
        let a = dir2.write("sessions/2026/9/29/rollout-a.jsonl", "{}");
        let b = dir2.write("sessions/2026/9/30/rollout-b.jsonl", "{}");
        let c = dir2.write("sessions/2026/10/01/rollout-c.jsonl", "{}");
        touch(&a, 50);
        touch(&b, 100);
        touch(&c, 0);
        assert_eq!(newest_codex_session(&dir2.path().join("sessions")), Some(c));
        assert_eq!(newest_codex_session(&dir.path().join("missing")), None);
    }
}
