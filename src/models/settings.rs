// models/settings.rs — the model and effort each tool is set to, from the
// bytes of its own settings file. Each parser takes only its two keys; any
// other shape (broken JSON or TOML, a wrong type, a missing key) is None.
//
// Shapes verified on the owner's PC 2026-11-11 (key names only):
// - Claude Code settings.json: top-level `model` when one is chosen (absent
//   there: the default model is not written; `/model opus` writes the
//   alias), `effortLevel` under `modelSettings.<full model id>` (top-level
//   `effortLevel` in older versions). Verified live 2026-11-13 with Claude
//   Code 2.1.323: `/effort` saves under the full id even with an alias set.
// - Codex CLI config.toml: top-level `model`, `model_reasoning_effort`.
// - Grok CLI config.toml: `[models] default_reasoning_effort` (`default`,
//   the model, from the CLI's own docs: user-guide/05-configuration.md).
// - Copilot CLI settings.json: top-level `model` ("auto"); `effortLevel`
//   from its docs, not set there.
// - Antigravity CLI settings.json (.gemini\antigravity-cli): top-level
//   `model`, stored as the display label ("Claude Sonnet 4.6 (Thinking)").
// - Cursor (the editor), verified 2026-11-16: the state database's entry
//   `...persistentStorage.applicationUser` holds `aiSettings.modelConfig`,
//   one object per place a model is chosen; the Agent's is `composer`:
//   `{"modelName": "grok-4.7", "maxMode": false, "selectedModels":
//   [{"modelId": "grok-4.7", "parameters": [{"id": "reasoning_effort",
//   "value": "high"}, {"id": "context", "value": "256k"}, ...]}]}`;
//   "default" is the model Cursor calls Auto. Only that object leaves the
//   database (files.rs `cursor_agent_model`).
// A file with an effort but no model (Claude Code and Grok CLI as the owner
// has them: the default model is not written) gives the effort alone, with
// an empty id.

use super::files::{clean, clean_toml};
use super::ModelInfo;
use serde_json::Value;

/// Claude Code `settings.json`: `model`, and the effort set for that model.
///
/// `/effort` keeps the effort per model under the model's full id
/// (`modelSettings.claude-opus-5-5.effortLevel`), while `/model opus` writes
/// the alias ("opus"). So the effort is the one under the model's own key,
/// else under the newest full id of the alias's family ("opus" ->
/// `claude-opus-5-5`), else the older top-level `effortLevel`. With no model
/// written (Claude Code's default model), the effort shows alone when every
/// model's saved effort agrees; when they differ, which one applies is not
/// written anywhere, so none is shown.
pub(super) fn claude(bytes: &[u8]) -> Option<ModelInfo> {
    let root: Value = serde_json::from_slice(bytes).ok()?;
    let id = clean(root.get("model")).and_then(|m| claude_model_id(&m));
    let per_model = root.get("modelSettings").and_then(Value::as_object);
    let effort_of = |key: &str| {
        per_model
            .and_then(|m| m.get(key))
            .and_then(|m| clean(m.get("effortLevel")))
    };
    let effort = match id.as_deref() {
        Some(id) => {
            effort_of(id).or_else(|| family_key(id, per_model?).and_then(|k| effort_of(&k)))
        }
        None => None,
    }
    .or_else(|| clean(root.get("effortLevel")))
    .or_else(|| {
        if id.is_some() {
            return None;
        }
        let mut efforts = per_model?.values().map(|m| clean(m.get("effortLevel")));
        let first = efforts.next()??;
        efforts
            .all(|e| e.as_deref() == Some(first.as_str()))
            .then_some(first)
    });
    found(id, effort)
}

/// The `modelSettings` key of the newest model in an alias's family: "opus"
/// -> "claude-opus-5-5" over "claude-opus-5". None for a full id (it has a
/// key of its own or none) and when no key is of the family.
fn family_key(alias: &str, per_model: &serde_json::Map<String, Value>) -> Option<String> {
    let alias = alias.to_ascii_lowercase();
    if alias.contains('-') || alias.is_empty() {
        return None;
    }
    let version = |key: &str| -> Vec<u32> {
        key.split('-')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect()
    };
    per_model
        .keys()
        .filter(|k| k.to_ascii_lowercase().split('-').any(|part| part == alias))
        .max_by(|a, b| version(a).cmp(&version(b)))
        .cloned()
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

/// Cursor's Agent model, from the `composer` object of the editor's model
/// settings: `modelName` ("default" is Auto, read as "auto" like Copilot's),
/// and the `reasoning_effort` parameter of that model, when it has one.
pub(super) fn cursor(composer: &[u8]) -> Option<ModelInfo> {
    let root: Value = serde_json::from_slice(composer).ok()?;
    let name = clean(root.get("modelName"))?;
    let id = if name.eq_ignore_ascii_case("default") {
        "auto".to_string()
    } else {
        name.clone()
    };
    let chosen = root
        .get("selectedModels")
        .and_then(Value::as_array)
        .and_then(|models| {
            models
                .iter()
                .find(|m| m.get("modelId").and_then(Value::as_str) == Some(name.as_str()))
        });
    let effort = chosen
        .and_then(|m| m.get("parameters"))
        .and_then(Value::as_array)
        .and_then(|params| {
            params
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some("reasoning_effort"))
        })
        .and_then(|p| clean(p.get("value")));
    Some(ModelInfo { id, effort })
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
            assert!(
                cursor(bytes).is_none(),
                "cursor {:?}",
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
        // No model chosen, every saved effort the same: that effort alone.
        let one = br#"{"modelSettings": {"claude-opus-5-5": {"effortLevel": "xhigh"}}}"#;
        assert_eq!(claude(one), info("", Some("xhigh")));
        // The owner's PC 2026-11-13.
        let agree = br#"{"modelSettings": {"claude-fable-5": {"effortLevel": "xhigh"},
                                           "claude-opus-5-5": {"effortLevel": "xhigh"}}}"#;
        assert_eq!(claude(agree), info("", Some("xhigh")));
        // They differ: which one the default model takes is not written.
        let differ = br#"{"modelSettings": {"claude-fable-5": {"effortLevel": "high"},
                                            "claude-opus-5-5": {"effortLevel": "xhigh"}}}"#;
        assert_eq!(claude(differ), None);
        let odd_one = br#"{"modelSettings": {"claude-fable-5": {"effortLevel": 3},
                                             "claude-opus-5-5": {"effortLevel": "xhigh"}}}"#;
        assert_eq!(claude(odd_one), None);
        assert_eq!(claude(br#"{"modelSettings": {}}"#), None);
        // The owner's PC before: no model written, a top-level effort: the effort.
        assert_eq!(
            claude(br#"{"effortLevel": "xhigh", "theme": "dark"}"#),
            info("", Some("xhigh"))
        );
        // A wrong effort type is no effort; the model still shows.
        let odd = br#"{"model": "sonnet", "effortLevel": 3, "modelSettings": []}"#;
        assert_eq!(claude(odd), info("sonnet", None));
        // An alias takes the effort of the newest model of its family.
        let alias = br#"{"model": "opus",
            "modelSettings": {"claude-opus-5": {"effortLevel": "low"},
                              "claude-opus-5-5": {"effortLevel": "max"},
                              "claude-fable-5": {"effortLevel": "high"}}}"#;
        assert_eq!(claude(alias), info("opus", Some("max")));
        let fable = br#"{"model": "fable",
            "modelSettings": {"claude-fable-5": {"effortLevel": "high"},
                              "claude-opus-5-5": {"effortLevel": "xhigh"}}}"#;
        assert_eq!(claude(fable), info("fable", Some("high")));
        // The alias's family has no effort saved: none, not another model's.
        let sonnet = br#"{"model": "sonnet[1m]",
            "modelSettings": {"claude-opus-5-5": {"effortLevel": "xhigh"}}}"#;
        assert_eq!(claude(sonnet), info("sonnet", None));
        // A full id without a key of its own takes no other model's effort.
        let full = br#"{"model": "claude-opus-5-6",
            "modelSettings": {"claude-opus-5-5": {"effortLevel": "xhigh"}}}"#;
        assert_eq!(claude(full), info("claude-opus-5-6", None));
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
    fn cursor_reads_the_agent_model_and_its_effort() {
        // The owner's editor, 2026-11-16.
        let agent = br#"{"modelName": "grok-4.7", "maxMode": false, "selectedModels":
            [{"modelId": "grok-4.7", "parameters": [{"id": "context", "value": "256k"},
             {"id": "reasoning_effort", "value": "high"}, {"id": "fast", "value": "false"}]}]}"#;
        assert_eq!(cursor(agent), info("grok-4.7", Some("high")));
        // Auto: no effort of its own.
        let auto = br#"{"modelName": "default", "maxMode": false,
            "selectedModels": [{"modelId": "default", "parameters": []}]}"#;
        assert_eq!(cursor(auto), info("auto", None));
        // The effort of another model in the list is not this one's.
        let other = br#"{"modelName": "gpt-5.5", "selectedModels":
            [{"modelId": "grok-4.7", "parameters": [{"id": "reasoning_effort", "value": "high"}]}]}"#;
        assert_eq!(cursor(other), info("gpt-5.5", None));
        // Odd shapes: no model, or the model without an effort.
        for odd in [
            &br#"{"maxMode": true}"#[..],
            br#"{"modelName": 4}"#,
            br#"{"modelName": ""}"#,
            br#"null"#,
            br#"[]"#,
        ] {
            assert_eq!(cursor(odd), None, "{:?}", String::from_utf8_lossy(odd));
        }
        let bad_params = br#"{"modelName": "x", "selectedModels": [{"modelId": "x", "parameters":
            [{"id": "reasoning_effort", "value": 3}, 5, null]}]}"#;
        assert_eq!(cursor(bad_params), info("x", None));
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
