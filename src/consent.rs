// consent.rs — what each provider reads and where its token goes, for the
// questions asked before the first read and the one-time notice after the
// update. Nothing here reads a file: the table is text, and `found_tools`
// only asks whether a tool's folder exists.
//
// Keep in step with src/providers (the files and hosts are the ones the code
// uses) and with docs/en/PROVIDERS.md.

use std::path::PathBuf;

/// What one provider reads, in the words the picker shows.
#[derive(Debug, Clone, Copy)]
pub struct ReadInfo {
    /// Config id: "claude".
    pub id: &'static str,
    /// The tool whose files these are: "Claude Code".
    pub tool: &'static str,
    /// Sources of the sign-in, in the order the provider tries them.
    pub sign_in: &'static [&'static str],
    /// The only hosts the token is sent to.
    pub hosts: &'static [&'static str],
    /// The settings file and the keys read for the model, if the tool has one.
    pub settings: Option<&'static str>,
    /// The session files read for the model last used, if the tool has them.
    pub session: Option<&'static str>,
}

/// Every provider, in the default config's order.
pub const READ_INFO: &[ReadInfo] = &[
    ReadInfo {
        id: "claude",
        tool: "Claude Code",
        sign_in: &[r"%USERPROFILE%\.claude\.credentials.json"],
        hosts: &["api.anthropic.com"],
        settings: Some(r"%USERPROFILE%\.claude\settings.json: model, effortLevel"),
        session: Some(r"%USERPROFILE%\.claude\projects\...\*.jsonl (the end of the newest)"),
    },
    ReadInfo {
        id: "codex",
        tool: "Codex CLI",
        sign_in: &[r"%USERPROFILE%\.codex\auth.json"],
        hosts: &["chatgpt.com"],
        settings: Some(r"%USERPROFILE%\.codex\config.toml: model, model_reasoning_effort"),
        session: Some(r"%USERPROFILE%\.codex\sessions\...\*.jsonl (the end of the newest)"),
    },
    ReadInfo {
        id: "copilot",
        tool: "GitHub Copilot",
        sign_in: &[
            "a token you paste (kept in Windows Credential Manager)",
            "otherwise the token GitHub CLI gives (gh auth token)",
        ],
        hosts: &["api.github.com"],
        settings: Some(r"%USERPROFILE%\.copilot\settings.json: model, effortLevel"),
        session: None,
    },
    ReadInfo {
        id: "antigravity",
        tool: "Antigravity",
        sign_in: &[
            "its sign-in in Windows Credential Manager",
            r"otherwise %USERPROFILE%\.gemini\oauth_creds.json (older Gemini CLI)",
        ],
        hosts: &[
            "cloudcode-pa.googleapis.com",
            "daily-cloudcode-pa.googleapis.com",
        ],
        settings: Some(r"%USERPROFILE%\.gemini\antigravity-cli\settings.json: model"),
        session: None,
    },
    ReadInfo {
        id: "grok",
        tool: "Grok CLI",
        sign_in: &[r"%USERPROFILE%\.grok\auth.json"],
        hosts: &["cli-chat-proxy.grok.com"],
        settings: Some(r"%USERPROFILE%\.grok\config.toml: [models] default, default_reasoning_effort"),
        session: None,
    },
    ReadInfo {
        id: "cursor",
        tool: "Cursor",
        sign_in: &[
            "nothing when you sign in from the menu (the app keeps its own session)",
            r"otherwise the editor's %APPDATA%\Cursor\User\globalStorage\state.vscdb",
        ],
        hosts: &["api2.cursor.sh"],
        settings: None,
        session: None,
    },
];

/// The read info for a config id ("gemini" is the old name of Antigravity).
pub fn read_info(id: &str) -> Option<&'static ReadInfo> {
    let id = if id == "gemini" { "antigravity" } else { id };
    READ_INFO.iter().find(|r| r.id == id)
}

/// Providers whose tool is installed on this PC, judged only by whether its
/// folder exists: nothing inside is opened. Used to mark them in the welcome.
pub fn found_tools() -> Vec<&'static str> {
    READ_INFO
        .iter()
        .filter(|r| tool_dirs(r.id).iter().any(|d| d.is_dir()))
        .map(|r| r.id)
        .collect()
}

/// The folders whose existence says a tool is installed.
fn tool_dirs(id: &str) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let roaming = dirs::config_dir().unwrap_or_default();
    match id {
        "claude" => vec![home.join(".claude")],
        "codex" => vec![home.join(".codex")],
        "copilot" => vec![roaming.join("GitHub CLI"), home.join(".copilot")],
        "antigravity" => vec![roaming.join("Antigravity"), home.join(".gemini")],
        "grok" => vec![crate::providers::grok::auth_path()
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".grok"))],
        "cursor" => vec![roaming.join("Cursor")],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_provider_has_read_info() {
        for p in crate::config::schema::Config::default().providers {
            assert!(read_info(&p.id).is_some(), "no read info for {}", p.id);
        }
        assert_eq!(read_info("gemini").map(|r| r.id), Some("antigravity"));
    }

    /// The hosts named to the user are the hosts the code calls: every
    /// provider URL's host appears in that provider's list.
    #[test]
    fn hosts_match_the_provider_urls() {
        let sources = [
            ("claude", include_str!("providers/claude.rs")),
            ("codex", include_str!("providers/codex.rs")),
            ("copilot", include_str!("providers/copilot.rs")),
            ("antigravity", include_str!("providers/antigravity.rs")),
            ("grok", include_str!("providers/grok.rs")),
            ("cursor", include_str!("providers/cursor.rs")),
        ];
        for (id, src) in sources {
            let info = read_info(id).unwrap();
            for host in url_hosts(src) {
                assert!(
                    info.hosts.contains(&host.as_str()),
                    "{id} calls {host}, which the consent text does not name"
                );
            }
        }
    }

    fn url_hosts(src: &str) -> Vec<String> {
        src.match_indices("https://")
            .map(|(i, _)| {
                src[i + 8..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                    .collect::<String>()
            })
            .filter(|h| !h.is_empty() && !h.ends_with(".invalid"))
            .collect()
    }
}
