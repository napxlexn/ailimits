// models/catalog.rs — the models each tool is known to offer, as the tool
// stores their ids, for the picker's list of names. Only which ids to offer
// for renaming: the names themselves still come from the generic rule
// (name.rs), so a model missing here is named all the same, and the model a
// tool uses right now is always offered whether it is listed or not.
//
// Sources, verified 2026-11-13 on the owner's PC (docs/en/PROVIDERS.md,
// "Model labels"):
// - Claude Code: the ids its answers carry (`message.model`), one live call
//   per model.
// - Codex CLI: `codex debug models`, the listed (not hidden) ones.
// - Grok CLI: `~/.grok/models_cache.json`.
// - Antigravity CLI: the labels it writes to settings.json.
// - Copilot CLI: the models `copilot help config` lists.
// - Cursor: Auto only; the editor's own list is not in a file read here.

use crate::providers::ProviderId;

/// The ids each tool is known to offer, in its own order.
pub fn known(provider: &ProviderId) -> &'static [&'static str] {
    match provider {
        ProviderId::Claude => &[
            "claude-fable-5-2",
            "claude-opus-5-5",
            "claude-sonnet-5-5",
            "claude-haiku-5-5",
        ],
        ProviderId::Codex => &[
            "gpt-6.1-sol",
            "gpt-6.1-terra",
            "gpt-6-astra",
            "gpt-6-sol",
            "gpt-6-luna",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
        ],
        ProviderId::Grok => &["grok-4.7", "grok-4.7-build-fast", "grok-4.6", "grok-4.5"],
        ProviderId::Antigravity => &[
            "Claude Opus 4.6 (Thinking)",
            "Claude Sonnet 4.6 (Thinking)",
            "Gemini 3.1 Pro (High)",
            "Gemini 3.1 Pro (Low)",
            "GPT-OSS 120B (Medium)",
        ],
        ProviderId::Copilot => &[
            "auto",
            "claude-fable-5",
            "claude-opus-5",
            "claude-sonnet-5",
            "claude-opus-4.8",
            "claude-opus-4.8-fast",
            "claude-opus-4.7",
            "claude-opus-4.6",
            "claude-opus-4.5",
            "claude-sonnet-4.6",
            "claude-sonnet-4.5",
            "claude-haiku-4.5",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "gpt-5.5",
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.3-codex",
            "gpt-5-mini",
            "gemini-3.6-flash",
            "gemini-3.5-flash",
            "gemini-3.1-pro-preview",
            "grok-4.5",
            "kimi-k2.7-code",
            "mai-code-1-flash-picker",
        ],
        ProviderId::Cursor => &["auto"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_lists_ids_once_and_names_them() {
        for id in [
            ProviderId::Claude,
            ProviderId::Codex,
            ProviderId::Copilot,
            ProviderId::Antigravity,
            ProviderId::Grok,
            ProviderId::Cursor,
        ] {
            let list = known(&id);
            assert!(!list.is_empty(), "{id:?}");
            for (i, m) in list.iter().enumerate() {
                assert!(!list[..i].contains(m), "{id:?} lists {m} twice");
                assert_eq!(m.trim(), *m);
                let name = crate::models::name_parts(m, &id, None);
                assert!(!name.model.is_empty(), "{id:?} {m}");
            }
        }
    }
}
