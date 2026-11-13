// models/settings.rs — the model and effort each tool is set to, from the
// bytes of its own settings file. Each parser takes only its two keys; any
// other shape (broken JSON or TOML, a wrong type, a missing key) is None.
//
// Shapes verified on the owner's PC 2026-11-11 (key names only):
// - Claude Code settings.json: top-level `model` when one is chosen (absent
//   there: the default model is not written), `effortLevel` under
//   `modelSettings.<model id>` (top-level `effortLevel` in older versions).
// - Codex CLI config.toml: top-level `model`, `model_reasoning_effort`.
// - Grok CLI config.toml: `[models] default_reasoning_effort` (`default`,
//   the model, from the CLI's own docs: user-guide/05-configuration.md).
// - Copilot CLI settings.json: top-level `model` ("auto"); `effortLevel`
//   from its docs, not set there.
// - Antigravity CLI settings.json (.gemini\antigravity-cli): top-level
//   `model`, stored as the display label ("Claude Sonnet 4.6 (Thinking)").
// A file with an effort but no model (Claude Code and Grok CLI as the owner
// has them: the default model is not written) gives the effort alone, with
// an empty id.

use super::files::{clean, clean_toml};
use super::ModelInfo;
use serde_json::Value;

/// Claude Code `settings.json`: `model`, and the effort set for that model
/// (`modelSettings.<model>.effortLevel`, else the top-level `effortLevel`).
pub(super) fn claude(bytes: &[u8]) -> Option<ModelInfo> {
    let root: Value = serde_json::from_slice(bytes).ok()?;
    let id = clean(root.get("model")).and_then(|m| claude_model_id(&m));
    let per_model = id.as_deref().and_then(|id| {
        root.get("modelSettings")
            .and_then(|m| m.get(id))
            .and_then(|m| m.get("effortLevel"))
    });
    let effort = clean(per_model).or_else(|| clean(root.get("effortLevel")));
    found(id, effort)
}

/// The model with its effort, the effort alone (an empty id) when no model
/// is written, or None when the file has neither.
fn found(id: Option<String>, effort: Option<String>) -> Option<ModelInfo> {
    if id.is_none() && effort.is_none() {
        return None;
    }
    Some(ModelInfo {
        id: id.unwrap_or_default(),
        effort,
    })
}

/// A Claude Code model id without its context option: "opus[1m]" -> "opus".
/// The option is not part of the name, and one rename then covers both.
pub(super) fn claude_model_id(id: &str) -> Option<String> {
    let id = match id
        .strip_suffix(']')
        .and_then(|s| s.rfind('[').map(|i| &s[..i]))
    {
        Some(base) => base.trim(),
        None => id,
    };
    (!id.is_empty()).then(|| id.to_string())
}

/// Codex CLI `config.toml`: top-level `model`, `model_reasoning_effort`.
pub(super) fn codex(bytes: &[u8]) -> Option<ModelInfo> {
    let root = toml_table(bytes)?;
    found(
        clean_toml(root.get("model")),
        clean_toml(root.get("model_reasoning_effort")),
    )
}

/// Grok CLI `config.toml`: `[models] default`, `[models]
/// default_reasoning_effort`.
pub(super) fn grok(bytes: &[u8]) -> Option<ModelInfo> {
    let root = toml_table(bytes)?;
    let models = root.get("models")?.as_table()?;
    found(
        clean_toml(models.get("default")),
        clean_toml(models.get("default_reasoning_effort")),
    )
}

/// Copilot CLI `settings.json`: `model`, `effortLevel`.
pub(super) fn copilot(bytes: &[u8]) -> Option<ModelInfo> {
    let root: Value = serde_json::from_slice(bytes).ok()?;
    found(clean(root.get("model")), clean(root.get("effortLevel")))
}

/// Antigravity CLI `settings.json`: `model`, a display label. No effort is
/// stored apart from the label ("(Thinking)").
pub(super) fn antigravity(bytes: &[u8]) -> Option<ModelInfo> {
    let root: Value = serde_json::from_slice(bytes).ok()?;
    Some(ModelInfo {
        id: clean(root.get("model"))?,
        effort: None,
    })
}

fn toml_table(bytes: &[u8]) -> Option<toml::Table> {
    toml::from_str(std::str::from_utf8(bytes).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, effort: Option<&str>) -> Option<ModelInfo> {
        Some(ModelInfo {
            id: id.into(),
            effort: effort.map(String::from),
        })
    }

    /// Inputs no parser may choke on: each must give None, never panic.
    const BROKEN: &[&[u8]] = &[
        b"",
        b"   ",
        b"{",
        b"[1, 2]",
        b"null",
        b"\"model\"",
        b"model = ",
        b"[models",
        b"\xFF\xFE{\x00}\x00",
        b"\x00\x01\x02",
        b"{\"model\": 5}",
        b"{\"model\": null}",
        b"{\"model\": [\"a\"]}",
        b"{\"model\": {\"id\": \"x\"}}",
        b"{\"model\": \"\"}",
        b"{\"model\": \"  \"}",
        b"{\"model\": \"a\\nb\"}",
        b"model = 5",
        b"model = \"\"",
        b"models = \"x\"",
        b"[models]\ndefault = 5",
        b"[models]\ndefault = [\"grok-4\"]",
    ];

    #[test]
    fn broken_or_odd_files_give_no_model() {
        for bytes in BROKEN {
            assert!(
                claude(bytes).is_none(),
                "claude {:?}",
                String::from_utf8_lossy(bytes)
            );
            assert!(
                codex(bytes).is_none(),
                "codex {:?}",
                String::from_utf8_lossy(bytes)
            );
            assert!(
                grok(bytes).is_none(),
                "grok {:?}",
                String::from_utf8_lossy(bytes)
            );
            assert!(
                copilot(bytes).is_none(),
                "copilot {:?}",
                String::from_utf8_lossy(bytes)
            );
            assert!(
                antigravity(bytes).is_none(),
                "ag {:?}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn claude_reads_the_model_and_its_effort() {
        // The shape seen on the owner's PC, plus a chosen model.
        let file = br#"{
            "permissions": {"allow": []},
            "model": "claude-opus-5-5",
            "modelSettings": {"claude-fable-5": {"effortLevel": "high"},
                              "claude-opus-5-5": {"effortLevel": "xhigh"}},
            "theme": "dark"
        }"#;
        assert_eq!(claude(file), info("claude-opus-5-5", Some("xhigh")));
        // The older top-level effort.
        let old = br#"{"model": "opus", "effortLevel": "medium"}"#;
        assert_eq!(claude(old), info("opus", Some("medium")));
        // The per-model effort wins over the top-level one.
        let both = br#"{"model": "opus", "effortLevel": "low",
                        "modelSettings": {"opus": {"effortLevel": "high"}}}"#;
        assert_eq!(claude(both), info("opus", Some("high")));
        // No model chosen and no effort that applies to the default: nothing.
        let none = br#"{"modelSettings": {"claude-opus-5-5": {"effortLevel": "xhigh"}}}"#;
        assert_eq!(claude(none), None);
        // The owner's PC: no model written, a top-level effort: the effort.
        assert_eq!(
            claude(br#"{"effortLevel": "xhigh", "theme": "dark"}"#),
            info("", Some("xhigh"))
        );
        // A wrong effort type is no effort; the model still shows.
        let odd = br#"{"model": "sonnet", "effortLevel": 3, "modelSettings": []}"#;
        assert_eq!(claude(odd), info("sonnet", None));
        // The context option is dropped.
        assert_eq!(claude(br#"{"model": "sonnet[1m]"}"#), info("sonnet", None));
        assert_eq!(claude(br#"{"model": "[1m]"}"#), None);
    }

    #[test]
    fn codex_reads_the_top_level_keys_only() {
        let file = b"personality = \"x\"\r\nmodel = \"gpt-6.1-sol\"\r\nmodel_reasoning_effort = \"high\"\r\n\r\n[profiles.fast]\r\nmodel = \"gpt-5.4-mini\"\r\n";
        assert_eq!(codex(file), info("gpt-6.1-sol", Some("high")));
        assert_eq!(codex(b"model = \"o3\""), info("o3", None));
        // A model only in a profile is not the configured one.
        assert_eq!(codex(b"[profiles.fast]\nmodel = \"gpt-5.4-mini\"\n"), None);
        // An effort with no model: the effort.
        assert_eq!(
            codex(b"model_reasoning_effort = \"low\"\n"),
            info("", Some("low"))
        );
        // Broken TOML anywhere in the file: nothing.
        assert_eq!(codex(b"model = \"gpt-6.1\"\n[broken\n"), None);
    }

    #[test]
    fn grok_reads_the_models_table() {
        let file = b"[cli]\ninstaller = \"x\"\n\n[models]\ndefault = \"grok-4.5\"\ndefault_reasoning_effort = \"high\"\n";
        assert_eq!(grok(file), info("grok-4.5", Some("high")));
        // The owner's PC: an effort but no model chosen: the effort.
        assert_eq!(
            grok(b"[models]\ndefault_reasoning_effort = \"high\"\n"),
            info("", Some("high"))
        );
        // A top-level `default` is not the model.
        assert_eq!(grok(b"default = \"grok-4.5\"\n"), None);
    }

    #[test]
    fn copilot_and_antigravity_read_their_model() {
        assert_eq!(
            copilot(br#"{"enabledPlugins": {"a": true}, "model": "auto"}"#),
            info("auto", None)
        );
        assert_eq!(
            copilot(br#"{"model": "gpt-5.5", "effortLevel": "high"}"#),
            info("gpt-5.5", Some("high"))
        );
        assert_eq!(
            copilot(br#"{"effortLevel": "medium"}"#),
            info("", Some("medium"))
        );
        assert_eq!(
            antigravity(
                br#"{"model": "Claude Sonnet 4.6 (Thinking)", "trustedWorkspaces": ["x"]}"#
            ),
            info("Claude Sonnet 4.6 (Thinking)", None)
        );
    }
}
