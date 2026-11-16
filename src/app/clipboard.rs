// app/clipboard.rs — a key or token pasted from the menu.
//
// The menu stores what the clipboard holds in Credential Manager, and the
// provider then sends it with every request. So it is taken only when it has
// the shape of the very thing asked for: a password copied earlier, or any
// other text, never becomes a bearer token sent to a provider every cycle.
// The checks look at the shape only; nothing of the text is logged or shown.

use anyhow::Result;

/// The longest secret accepted: the providers' own bound on a usage token. A
/// ChatGPT access token (a JWT) runs past a thousand characters; the old 500
/// turned every one of them away.
pub(super) const SECRET_MAX_LEN: usize = crate::providers::MAX_USAGE_TOKEN_LEN;
const SECRET_MIN_LEN: usize = 10;

/// What the menu asked to paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SecretKind {
    /// Claude: an Anthropic API key (`sk-ant-api...`).
    ClaudeApiKey,
    /// Copilot: a GitHub token (`ghp_`, `github_pat_`, `gho_`, `ghu_`, or
    /// a classic 40-character one).
    CopilotToken,
    /// Claude: an OAuth access token of Claude Code (`sk-ant-oat...`).
    ClaudeUsageToken,
    /// Codex: a ChatGPT access token, a JWT (three dot-separated parts).
    CodexUsageToken,
}

impl SecretKind {
    /// The menu's "Paste key" for this provider.
    pub(super) fn key_for(provider: &str) -> Option<Self> {
        match provider {
            "claude" => Some(Self::ClaudeApiKey),
            "copilot" => Some(Self::CopilotToken),
            _ => None,
        }
    }

    /// The menu's "Paste usage token" for this provider.
    pub(super) fn usage_token_for(provider: &str) -> Option<Self> {
        match provider {
            "claude" => Some(Self::ClaudeUsageToken),
            "codex" => Some(Self::CodexUsageToken),
            _ => None,
        }
    }

    /// What was expected, for the message when the clipboard holds
    /// something else.
    pub(super) fn expected(self) -> &'static str {
        match self {
            Self::ClaudeApiKey => "a Claude API key (sk-ant-...)",
            Self::CopilotToken => "a GitHub token (ghp_..., github_pat_...)",
            Self::ClaudeUsageToken => "a Claude token (sk-ant-...)",
            Self::CodexUsageToken => "a ChatGPT access token (eyJ...)",
        }
    }

    fn fits(self, s: &str) -> bool {
        let word = |t: &str| {
            !t.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        };
        match self {
            Self::ClaudeApiKey | Self::ClaudeUsageToken => {
                s.strip_prefix("sk-ant-").is_some_and(word)
            }
            Self::CopilotToken => {
                let prefixed = ["ghp_", "github_pat_", "gho_", "ghu_"]
                    .iter()
                    .any(|p| s.strip_prefix(p).is_some_and(word));
                let classic = s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit());
                prefixed || classic
            }
            Self::CodexUsageToken => {
                let parts: Vec<&str> = s.split('.').collect();
                parts.len() == 3
                    && s.starts_with("eyJ")
                    && parts.iter().all(|p| {
                        !p.is_empty()
                            && p.chars().all(|c| {
                                c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '='
                            })
                    })
            }
        }
    }
}

/// The secret in `text`, when it has the shape of `kind`; otherwise why not,
/// in words that never quote the text.
pub(super) fn check_secret(text: &str, kind: SecretKind) -> Result<String, String> {
    let s = text.trim();
    if s.is_empty() {
        return Err("the clipboard holds no text".to_string());
    }
    if s.len() < SECRET_MIN_LEN
        || s.len() > SECRET_MAX_LEN
        || !s.chars().all(|c| c.is_ascii_graphic())
        || !kind.fits(s)
    {
        return Err(format!("the clipboard does not hold {}", kind.expected()));
    }
    Ok(s.to_string())
}

/// Read a key or token of `kind` from the clipboard.
pub(super) fn read_clipboard_secret(kind: SecretKind) -> Result<String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| anyhow::anyhow!("clipboard unavailable: {e}"))?;
    let text = clipboard
        .get_text()
        .map_err(|_| anyhow::anyhow!("the clipboard is empty or holds no text"))?;
    check_secret(&text, kind).map_err(|why| anyhow::anyhow!(why))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.c2lnbmF0dXJlLXBhcnQ";

    #[test]
    fn each_kind_takes_its_own_shape() {
        let ok = |s: &str, k| check_secret(s, k).is_ok();
        assert!(ok("sk-ant-api03-AbC_dEf-123456", SecretKind::ClaudeApiKey));
        assert!(ok(
            "sk-ant-oat01-AbC_dEf-123456",
            SecretKind::ClaudeUsageToken
        ));
        assert!(ok("ghp_0123456789abcdefABCDEF", SecretKind::CopilotToken));
        assert!(ok(
            "github_pat_11ABCDEF_0123456789",
            SecretKind::CopilotToken
        ));
        assert!(ok(
            "0123456789abcdef0123456789abcdef01234567",
            SecretKind::CopilotToken
        ));
        assert!(ok(JWT, SecretKind::CodexUsageToken));
        // Surrounding whitespace from a copy is fine.
        assert_eq!(
            check_secret(&format!("  {JWT}\r\n"), SecretKind::CodexUsageToken).unwrap(),
            JWT
        );
    }

    #[test]
    fn a_password_or_another_tools_token_is_turned_away() {
        for text in [
            "Summer2026!pass",
            "correct horse battery staple",
            "hunter2hunter2",
        ] {
            for kind in [
                SecretKind::ClaudeApiKey,
                SecretKind::CopilotToken,
                SecretKind::ClaudeUsageToken,
                SecretKind::CodexUsageToken,
            ] {
                assert!(
                    check_secret(text, kind).is_err(),
                    "{kind:?} took a password"
                );
            }
        }
        // The wrong provider's token.
        assert!(check_secret("ghp_0123456789abcdefABCDEF", SecretKind::ClaudeApiKey).is_err());
        assert!(check_secret("sk-ant-oat01-AbCdEf123456", SecretKind::CodexUsageToken).is_err());
        assert!(check_secret(JWT, SecretKind::ClaudeUsageToken).is_err());
    }

    #[test]
    fn a_long_jwt_fits_and_a_huge_paste_does_not() {
        let long = format!(
            "eyJ{}.eyJ{}.{}",
            "a".repeat(900),
            "b".repeat(600),
            "c".repeat(342)
        );
        assert!(long.len() > 1000 && long.len() < SECRET_MAX_LEN);
        assert!(check_secret(&long, SecretKind::CodexUsageToken).is_ok());
        let huge = format!("sk-ant-{}", "a".repeat(SECRET_MAX_LEN));
        assert!(check_secret(&huge, SecretKind::ClaudeApiKey).is_err());
    }

    #[test]
    fn the_message_never_quotes_the_clipboard() {
        let why = check_secret("my-secret-password-1", SecretKind::ClaudeApiKey).unwrap_err();
        assert!(!why.contains("my-secret"), "{why}");
        assert!(check_secret("   ", SecretKind::ClaudeApiKey).is_err());
        // Inner whitespace or control characters are not a token.
        assert!(check_secret("sk-ant-api03 AbCdEf123456", SecretKind::ClaudeApiKey).is_err());
        assert!(check_secret("sk-ant-api03\u{7}AbCdEf123456", SecretKind::ClaudeApiKey).is_err());
    }

    #[test]
    fn only_the_menus_providers_have_a_kind() {
        assert_eq!(
            SecretKind::key_for("claude"),
            Some(SecretKind::ClaudeApiKey)
        );
        assert_eq!(
            SecretKind::key_for("copilot"),
            Some(SecretKind::CopilotToken)
        );
        assert_eq!(SecretKind::key_for("codex"), None);
        assert_eq!(
            SecretKind::usage_token_for("codex"),
            Some(SecretKind::CodexUsageToken)
        );
        assert_eq!(SecretKind::usage_token_for("copilot"), None);
    }
}
