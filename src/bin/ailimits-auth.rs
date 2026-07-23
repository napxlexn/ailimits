// ailimits-auth — console utility for provider authorization.
//
// Commands:
//   ailimits-auth status                     show detected auth sources
//   ailimits-auth set <provider>             store an API key / PAT in Credential Manager
//   ailimits-auth remove <provider>          remove the key
//   ailimits-auth set-usage-token <p>        store a manual usage token (claude | codex)
//   ailimits-auth remove-usage-token <p>     remove the usage token
//
// Secrets are stored ONLY in Windows Credential Manager (service "ailimits");
// config.toml holds nothing but a label.

use ailimits::config::{
    schema::{AuthMethod, Config},
    storage,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};

/// Credential Manager service name.
const KEYRING_SERVICE: &str = "ailimits";

/// Providers that accept a stored key (Codex uses a usage token instead).
const PROVIDERS: &[(&str, &str)] = &[("claude", "claude_api_key"), ("copilot", "copilot_pat")];

/// Manual usage tokens: tried by the widget BEFORE the CLI-owned token files.
const USAGE_TOKEN_PROVIDERS: &[(&str, &str)] = &[
    ("claude", "claude_usage_token"),
    ("codex", "codex_usage_token"),
];

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_USAGE_BETA_HEADER: &str = "oauth-2025-04-20";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("status") => status(),
        Some("set") => set(args.get(1).map(String::as_str)),
        Some("remove") => remove(args.get(1).map(String::as_str)),
        Some("set-usage-token") => set_usage_token(args.get(1).map(String::as_str)),
        Some("remove-usage-token") => remove_usage_token(args.get(1).map(String::as_str)),
        _ => {
            print_usage();
            Ok(())
        }
    }
}

fn print_usage() {
    println!("ailimits-auth — provider authorization for the AI Limits widget");
    println!();
    println!("Usage:");
    println!("  ailimits-auth status                    show auth status");
    println!(
        "  ailimits-auth set <provider>            store a key (claude: API key, copilot: PAT)"
    );
    println!("  ailimits-auth remove <provider>         remove the key");
    println!(
        "  ailimits-auth set-usage-token <p>       store a manual usage token (claude | codex)"
    );
    println!("  ailimits-auth remove-usage-token <p>    remove the usage token");
    println!();
    println!("By default everything works through subscriptions, no keys needed:");
    println!("  Claude — Claude Code token, Codex — Codex CLI token, Copilot — gh CLI token.");
}

/// Key label for a provider.
fn label_for(provider: &str) -> Result<&'static str> {
    PROVIDERS
        .iter()
        .find(|(id, _)| *id == provider)
        .map(|(_, label)| *label)
        .with_context(|| format!("provider '{provider}' has no key (expected: claude or copilot)"))
}

/// Usage-token label for a provider.
fn usage_label_for(provider: &str) -> Result<&'static str> {
    USAGE_TOKEN_PROVIDERS
        .iter()
        .find(|(id, _)| *id == provider)
        .map(|(_, label)| *label)
        .with_context(|| format!("usage token is supported for claude and codex, got '{provider}'"))
}

/// Show the state of every auth source.
fn status() -> Result<()> {
    println!("── Auth sources ─────────────────────────────────");

    // Claude Code subscription: OAuth token.
    let claude_dir = dirs::home_dir().unwrap_or_default().join(".claude");
    let creds_path = claude_dir.join(".credentials.json");
    if creds_path.exists() {
        match read_oauth_expiry(&creds_path) {
            Some((expires_at, sub_type)) => {
                let valid = expires_at > Utc::now();
                let mark = if valid { "✓" } else { "✗ (expired)" };
                println!("Claude OAuth ({sub_type} subscription): {mark}");
                println!(
                    "  token valid until {} UTC",
                    expires_at.format("%Y-%m-%d %H:%M")
                );
                if !valid {
                    println!("  → run Claude Code, it refreshes the token automatically");
                }
            }
            None => println!("Claude OAuth: ✗ (.credentials.json has no valid token)"),
        }
    } else {
        println!("Claude OAuth: ✗ (no .credentials.json — run Claude Code once)");
    }

    // Claude Code local files.
    let statusline = claude_dir.join("statusline.jsonl").exists();
    let stats = claude_dir.join("stats-cache.json").exists();
    println!("statusline.jsonl: {}", if statusline { "✓" } else { "✗" });
    println!("stats-cache.json: {}", if stats { "✓" } else { "✗" });

    // Manual usage tokens.
    for (id, label) in USAGE_TOKEN_PROVIDERS {
        let present = keyring::Entry::new(KEYRING_SERVICE, label)
            .and_then(|e| e.get_password())
            .is_ok();
        println!(
            "{id} usage token ({label}): {}",
            if present { "✓ stored" } else { "—" }
        );
    }

    // Codex CLI auth.json.
    let codex_auth = dirs::home_dir()
        .unwrap_or_default()
        .join(".codex")
        .join("auth.json");
    println!(
        "Codex auth.json: {}",
        if codex_auth.exists() {
            "✓"
        } else {
            "✗ (run codex login)"
        }
    );

    // gh CLI for Copilot. Run from the system dir so a planted gh.exe in the
    // current directory cannot be invoked instead (CWD-first search order).
    let mut gh_cmd = std::process::Command::new("gh");
    gh_cmd.args(["auth", "token"]);
    if let Some(root) = std::env::var_os("SystemRoot") {
        gh_cmd.current_dir(std::path::Path::new(&root).join("System32"));
    }
    let gh_ok = gh_cmd.output().map(|o| o.status.success()).unwrap_or(false);
    println!(
        "gh CLI token (Copilot): {}",
        if gh_ok {
            "✓"
        } else {
            "✗ (gh auth login, or store a PAT)"
        }
    );

    // Antigravity (Google): the Credential Manager session first, then the
    // legacy Gemini CLI file — the same order the provider tries.
    println!(
        "Antigravity keyring (gemini:antigravity): {}",
        if ailimits::providers::antigravity::keyring_token_present() {
            "✓"
        } else {
            "✗ (run Antigravity once)"
        }
    );
    let gemini_creds = dirs::home_dir()
        .unwrap_or_default()
        .join(".gemini")
        .join("oauth_creds.json");
    println!(
        "legacy Gemini CLI oauth_creds.json: {}",
        if gemini_creds.exists() { "✓" } else { "✗" }
    );

    // Stored keys.
    println!();
    println!("── Keys (Credential Manager) ─────────────────────");
    for (id, label) in PROVIDERS {
        let present = keyring::Entry::new(KEYRING_SERVICE, label)
            .and_then(|e| e.get_password())
            .is_ok();
        println!(
            "{id:<8} ({label}): {}",
            if present { "✓ stored" } else { "—" }
        );
    }

    // Current config.
    println!();
    println!("── Config ({}) ──", storage::config_path().display());
    let config = load_config()?;
    for p in &config.providers {
        let method = match p.auth_method {
            AuthMethod::Subscription => "subscription",
            AuthMethod::ApiKey => "api_key",
        };
        let state = if p.enabled { "enabled" } else { "disabled" };
        println!("{:<8} {state}, method: {method}", p.id);
    }

    Ok(())
}

/// Store a key and update the config.
fn set(provider: Option<&str>) -> Result<()> {
    let provider = provider.context("specify a provider: ailimits-auth set <claude|copilot>")?;
    let label = label_for(provider)?;

    // Hidden input: the key is invisible and stays out of terminal history.
    let key = rpassword::prompt_password(format!("Paste the key for {provider}: "))
        .context("failed to read input")?;
    let key = key.trim();
    if key.is_empty() {
        bail!("empty key — nothing stored");
    }

    keyring::Entry::new(KEYRING_SERVICE, label)
        .context("keyring unavailable")?
        .set_password(key)
        .context("failed to store the key in Credential Manager")?;

    // Claude: a key implies the api_key method. Copilot: the PAT is just
    // a token source for the subscription method.
    let mut config = load_config()?;
    if let Some(p) = config.providers.iter_mut().find(|p| p.id == provider) {
        p.enabled = true;
        p.credential_label = label.to_string();
        if provider == "claude" {
            p.auth_method = AuthMethod::ApiKey;
        }
    }
    save_config(&config)?;

    println!("✓ Key for '{provider}' stored in Credential Manager (label: {label})");
    println!("→ Restart the widget to apply");
    Ok(())
}

/// Store a manual usage token without touching config/auth_method.
fn set_usage_token(provider: Option<&str>) -> Result<()> {
    let provider =
        provider.context("specify a provider: ailimits-auth set-usage-token <claude|codex>")?;
    let label = usage_label_for(provider)?;

    let token = rpassword::prompt_password(format!("Paste the {provider} usage token: "))
        .context("failed to read input")?;
    let token = token.trim();
    if token.is_empty() {
        bail!("empty token — nothing stored");
    }
    validate_usage_token(provider, token)?;

    keyring::Entry::new(KEYRING_SERVICE, label)
        .context("keyring unavailable")?
        .set_password(token)
        .context("failed to store the usage token in Credential Manager")?;

    println!("✓ {provider} usage token stored (label: {label})");
    println!("→ Restart the widget to apply");
    Ok(())
}

/// One read-only request to the provider's usage endpoint to verify the token.
fn validate_usage_token(provider: &str, token: &str) -> Result<()> {
    let url = match provider {
        "claude" => CLAUDE_USAGE_URL,
        _ => CODEX_USAGE_URL,
    };
    let status = tokio::runtime::Runtime::new()?.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            // Never follow a 3xx with the bearer token attached.
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let mut req = client.get(url).bearer_auth(token);
        if provider == "claude" {
            req = req.header("anthropic-beta", CLAUDE_USAGE_BETA_HEADER);
        } else {
            req = req.header("User-Agent", "ailimits-widget");
        }
        Ok::<_, anyhow::Error>(req.send().await?.status())
    })?;

    if status.as_u16() == 200 {
        Ok(())
    } else {
        bail!("the usage endpoint rejected the token: HTTP {status}");
    }
}

fn remove_usage_token(provider: Option<&str>) -> Result<()> {
    let provider =
        provider.context("specify a provider: ailimits-auth remove-usage-token <claude|codex>")?;
    let label = usage_label_for(provider)?;

    match keyring::Entry::new(KEYRING_SERVICE, label).and_then(|e| e.delete_credential()) {
        Ok(()) => println!("✓ {provider} usage token removed"),
        Err(keyring::Error::NoEntry) => println!("— no {provider} usage token stored"),
        Err(e) => bail!("failed to remove the usage token: {e}"),
    }

    println!("→ Restart the widget to apply");
    Ok(())
}

/// Remove a key and switch the provider back to the subscription method.
fn remove(provider: Option<&str>) -> Result<()> {
    let provider = provider.context("specify a provider: ailimits-auth remove <claude|copilot>")?;
    let label = label_for(provider)?;

    match keyring::Entry::new(KEYRING_SERVICE, label).and_then(|e| e.delete_credential()) {
        Ok(()) => println!("✓ Key '{label}' removed from Credential Manager"),
        Err(keyring::Error::NoEntry) => println!("— no key '{label}' stored"),
        Err(e) => bail!("failed to remove the key: {e}"),
    }

    let mut config = load_config()?;
    if let Some(p) = config.providers.iter_mut().find(|p| p.id == provider) {
        // Without a key everything runs on the subscription method.
        p.credential_label = String::new();
        p.auth_method = AuthMethod::Subscription;
        println!("✓ '{provider}' switched back to the subscription method");
    }
    save_config(&config)?;

    println!("→ Restart the widget to apply");
    Ok(())
}

/// Read expiresAt and the subscription type from .credentials.json.
fn read_oauth_expiry(path: &std::path::Path) -> Option<(DateTime<Utc>, String)> {
    let content = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    let oauth = value.get("claudeAiOauth")?;
    let ms = oauth.get("expiresAt")?.as_i64()?;
    let sub = oauth
        .get("subscriptionType")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    DateTime::<Utc>::from_timestamp_millis(ms).map(|dt| (dt, sub))
}

/// Config helpers — storage is async, the CLI is sync.
fn load_config() -> Result<Config> {
    tokio::runtime::Runtime::new()?.block_on(storage::load_or_default())
}

fn save_config(config: &Config) -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(storage::save(config))
}
