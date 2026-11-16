// tests/uninstall_labels_test.rs — every secret the app keeps leaves with it.
//
// The app keeps its own secrets in Windows Credential Manager under the
// service "ailimits": keys, usage tokens and its own Cursor session. The
// uninstaller removes them by running ailimits-auth.exe from
// installer/ailimits.iss [UninstallRun]; a label that has no command there
// outlives the app on the user's machine. The labels are gathered twice —
// from the code's own helpers and by scanning the source for label-like
// literals — so a new one cannot slip past either way. Nothing here touches
// Credential Manager: it only reads source files.

use ailimits::config::schema::Config;
use ailimits::providers::cursor::session::SESSION_LABEL;
use ailimits::providers::{key_label_for, usage_token_label_for, ProviderId};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Literals that look like labels (a provider id and `_`) but name fields of
/// a provider's API answer, not anything kept in Credential Manager.
const NOT_LABELS: &[&str] = &["copilot_plan"];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative))
        .unwrap_or_else(|e| panic!("read {relative}: {e}"))
}

/// Every provider id the app has.
fn provider_ids() -> Vec<String> {
    let ids: Vec<String> = Config::default()
        .providers
        .iter()
        .map(|p| p.id.clone())
        .collect();
    for id in &ids {
        assert!(ProviderId::from_config_id(id).is_some(), "{id}");
    }
    ids
}

/// Each label the app writes, with the ailimits-auth command that removes it.
fn labels_and_commands() -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    for id in provider_ids() {
        if let Some(label) = key_label_for(&id) {
            labels.insert(label.to_string(), format!("remove {id}"));
        }
        if let Some(label) = usage_token_label_for(&id) {
            labels.insert(label.to_string(), format!("remove-usage-token {id}"));
        }
    }
    labels.insert(SESSION_LABEL.to_string(), "logout cursor".to_string());
    labels
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The string literals of one line of Rust (escapes are not unfolded; a
/// label never has any).
fn literals(line: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else { break };
        found.push(&after[..end]);
        rest = &after[end + 1..];
    }
    found
}

/// Whether a literal reads like a Credential Manager label: a provider id
/// (or the legacy "gemini"), `_`, then lower-case words.
fn looks_like_label(literal: &str, ids: &[String]) -> bool {
    let prefixes = ids.iter().map(String::as_str).chain(["gemini"]);
    prefixes.into_iter().any(|id| {
        match literal.strip_prefix(id).and_then(|r| r.strip_prefix('_')) {
            Some(rest) => {
                !rest.is_empty()
                    && rest
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            }
            None => false,
        }
    })
}

/// The ailimits-auth commands [UninstallRun] runs, by their RunOnceId.
struct UninstallRun {
    commands: Vec<String>,
}

fn uninstall_run() -> UninstallRun {
    let iss = read("installer/ailimits.iss");
    let mut in_section = false;
    let mut commands = Vec::new();
    let mut ids = BTreeSet::new();
    for line in iss.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[UninstallRun]");
            continue;
        }
        if !in_section || line.is_empty() || line.starts_with(';') {
            continue;
        }
        let field = |name: &str| {
            line.split(';')
                .map(str::trim)
                .find_map(|part| part.strip_prefix(name))
                .map(|v| v.trim().trim_matches('"').to_string())
        };
        let id = field("RunOnceId:").unwrap_or_else(|| panic!("no RunOnceId: {line}"));
        assert!(ids.insert(id.clone()), "RunOnceId {id} twice");
        let flags = field("Flags:").unwrap_or_default();
        assert!(flags.contains("runhidden"), "{id}: no window at uninstall");
        let file = field("Filename:").unwrap();
        if file == r"{app}\ailimits-auth.exe" {
            commands.push(field("Parameters:").unwrap_or_else(|| panic!("{id}: no command")));
        }
    }
    UninstallRun { commands }
}

#[test]
fn every_label_the_app_writes_is_removed_at_uninstall() {
    let run = uninstall_run();
    for (label, command) in labels_and_commands() {
        assert!(
            run.commands.contains(&command),
            "Credential Manager label `{label}` would outlive the uninstall: \
             installer/ailimits.iss [UninstallRun] needs `ailimits-auth {command}`"
        );
    }
}

#[test]
fn the_uninstaller_runs_no_command_for_a_label_that_is_gone() {
    let wanted: BTreeSet<String> = labels_and_commands().into_values().collect();
    for command in uninstall_run().commands {
        assert!(
            wanted.contains(&command),
            "[UninstallRun] runs `ailimits-auth {command}`, which removes no label the app writes"
        );
    }
}

#[test]
fn the_source_has_no_label_the_helpers_do_not_know() {
    let ids = provider_ids();
    let known = labels_and_commands();
    let mut files = Vec::new();
    rust_files(&root().join("src"), &mut files);
    let mut unknown = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for (n, line) in text.lines().enumerate() {
            for literal in literals(line) {
                if looks_like_label(literal, &ids)
                    && !known.contains_key(literal)
                    && !NOT_LABELS.contains(&literal)
                {
                    unknown.push(format!("{}:{}: {literal}", file.display(), n + 1));
                }
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "label-like literals the uninstall check does not cover — a new Credential Manager \
         label needs a helper here and a command in [UninstallRun]; an API field goes in \
         NOT_LABELS:\n{}",
        unknown.join("\n")
    );
}

#[test]
fn ailimits_auth_removes_the_labels_the_app_writes() {
    // ailimits-auth keeps its own tables of labels: `remove <id>` and
    // `remove-usage-token <id>` delete what those name, so they must agree
    // with the app's helpers, and `logout cursor` must delete the session.
    let auth = read("src/bin/ailimits-auth.rs");
    let table = |name: &str| -> Vec<(String, String)> {
        let start = auth
            .find(&format!("const {name}: &[(&str, &str)]"))
            .unwrap_or_else(|| panic!("ailimits-auth has no {name} table"));
        let body = &auth[start..];
        let body = &body[..body.find("];").unwrap()];
        let parts = literals(body.split_once('=').unwrap().1);
        let (pairs, odd) = parts.as_chunks::<2>();
        assert!(odd.is_empty(), "{name} holds (id, label) pairs");
        pairs
            .iter()
            .map(|[id, label]| (id.to_string(), label.to_string()))
            .collect()
    };
    for (id, label) in table("PROVIDERS") {
        assert_eq!(key_label_for(&id), Some(label.as_str()), "key of {id}");
    }
    for (id, label) in table("USAGE_TOKEN_PROVIDERS") {
        assert_eq!(
            usage_token_label_for(&id),
            Some(label.as_str()),
            "usage token of {id}"
        );
    }
    let ids = provider_ids();
    let keys: Vec<_> = ids
        .iter()
        .filter(|id| key_label_for(id).is_some())
        .collect();
    let usage: Vec<_> = ids
        .iter()
        .filter(|id| usage_token_label_for(id).is_some())
        .collect();
    assert_eq!(table("PROVIDERS").len(), keys.len(), "every key provider");
    assert_eq!(
        table("USAGE_TOKEN_PROVIDERS").len(),
        usage.len(),
        "every usage-token provider"
    );

    let logout = &auth[auth.find("fn logout(").expect("a logout command")..];
    let logout = &logout[..logout.find("\n}").unwrap()];
    assert!(
        logout.contains("session::delete()"),
        "logout cursor deletes the stored session"
    );
    for command in ["\"remove\"", "\"remove-usage-token\"", "\"logout\""] {
        assert!(auth.contains(&format!("Some({command})")), "{command}");
    }
}

#[test]
fn the_label_scan_finds_what_it_should() {
    let ids = provider_ids();
    assert!(looks_like_label("claude_api_key", &ids));
    assert!(looks_like_label("cursor_session", &ids));
    assert!(looks_like_label("gemini_token", &ids));
    assert!(!looks_like_label("access_token", &ids));
    assert!(!looks_like_label("claude", &ids));
    assert!(!looks_like_label("claude_", &ids));
    assert!(!looks_like_label("cursor_session_test_{}", &ids));
    assert_eq!(
        literals(r#"keyring::Entry::new("ailimits", "codex_usage_token") // "x""#),
        ["ailimits", "codex_usage_token", "x"]
    );
}
