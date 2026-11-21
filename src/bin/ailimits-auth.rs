// ailimits-auth — console utility for provider authorization.
//
// Commands:
//   ailimits-auth status                     show detected auth sources
//   ailimits-auth set <provider>             store an API key / PAT in Credential Manager
//   ailimits-auth remove <provider>          remove the key
//   ailimits-auth set-usage-token <p>        store a manual usage token (claude | codex)
//   ailimits-auth remove-usage-token <p>     remove the usage token
//   ailimits-auth login cursor               sign the widget in to Cursor (its own session)
//   ailimits-auth logout cursor              forget that session
//
// Secrets are stored ONLY in Windows Credential Manager (service "ailimits");
// config.toml holds nothing but a label.
//
// `status` changes nothing: it never creates, renames or writes config.toml,
// and it reads another tool's sign-in only where the widget itself may
// (`[reads] sign_in` in config.toml). Elsewhere it says whether the file is
// there, without opening it. No token is ever printed or kept.

use ailimits::config::{
    paths::ToolPaths,
    schema::{AuthMethod, Config},
    storage::{self, Peek},
};
use ailimits::providers::cursor::session;
use ailimits::providers::grok::{self, GrokLogin};
use ailimits::providers::{self, TokenCheck};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::time::{Duration, Instant};

/// Credential Manager service name.
const KEYRING_SERVICE: &str = "ailimits";

/// Providers that accept a stored key (Codex uses a usage token instead).
const PROVIDERS: &[(&str, &str)] = &[("claude", "claude_api_key"), ("copilot", "copilot_pat")];

/// Manual usage tokens: tried by the widget BEFORE the CLI-owned token files.
const USAGE_TOKEN_PROVIDERS: &[(&str, &str)] = &[
    ("claude", "claude_usage_token"),
    ("codex", "codex_usage_token"),
];

/// How long `gh` may take to say whether it is signed in to github.com.
const GH_TIMEOUT: Duration = Duration::from_secs(10);

/// Waits before asking the usage endpoint again when its answer said
/// nothing about the token (a server error, no answer).
const CHECK_RETRIES: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(4)];

/// The line every command that changed the config or a stored secret ends
/// with. A running widget looks at config.toml every 2 seconds while the
/// user is at the PC and merges an edit with its own changes (also when it
/// quits); it reads the stored keys, tokens and its Cursor session anew on
/// every update (`providers::pasted_key`, `keyring_secret`).
const PICKED_UP_HINT: &str = "→ A running widget picks this up by itself, no restart needed: a changed setting within a few seconds, a new or removed key or token at its next update.";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("status") => status(),
        Some("set") => set(args.get(1).map(String::as_str)),
        Some("remove") => remove(args.get(1).map(String::as_str)),
        Some("set-usage-token") => set_usage_token(args.get(1).map(String::as_str)),
        Some("remove-usage-token") => remove_usage_token(args.get(1).map(String::as_str)),
        Some("login") => login(args.get(1).map(String::as_str)),
        Some("logout") => logout(args.get(1).map(String::as_str)),
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
    println!("  ailimits-auth status                    show auth status (changes nothing)");
    println!(
        "  ailimits-auth set <provider>            store a key (claude: API key, copilot: PAT)"
    );
    println!("  ailimits-auth remove <provider>         remove the key");
    println!(
        "  ailimits-auth set-usage-token <p>       store a manual usage token (claude | codex)"
    );
    println!("  ailimits-auth remove-usage-token <p>    remove the usage token");
    println!("  ailimits-auth login cursor              sign in to Cursor (opens the browser)");
    println!("  ailimits-auth logout cursor             forget the Cursor session");
    println!();
    println!("By default everything works through subscriptions, no keys needed:");
    println!("  Claude — Claude Code token, Codex — Codex CLI token, Copilot — gh CLI token,");
    println!("  Antigravity — Antigravity CLI token, Grok — Grok CLI login,");
    println!("  Cursor — its own session: sign in once (login cursor, or in Rows and models).");
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

/// Whether an entry of the app's own (service "ailimits") is stored.
fn own_entry_stored(label: &str) -> bool {
    keyring::Entry::new(KEYRING_SERVICE, label)
        .and_then(|e| e.get_password())
        .is_ok()
}

/// "found" / "missing" for a file whose content is not looked at.
fn presence(path: &Path) -> &'static str {
    if path.is_file() {
        "found"
    } else {
        "missing"
    }
}

/// The note for a sign-in the widget is not allowed to read.
const NOT_ALLOWED: &str = "not read: the widget is not allowed to read it; allow it there first";

/// Show the state of every auth source. Changes nothing (see the top).
fn status() -> Result<()> {
    let config_path = storage::config_path();
    let peeked = storage::peek(&config_path);
    let config = match &peeked {
        Peek::Found(config) | Peek::LeftOut(config, _) => Some(config),
        _ => None,
    };
    let allowed = |id: &str| config.is_some_and(|c| c.sign_in_allowed(id));
    let paths = ToolPaths::from_env();

    println!("── Auth sources ─────────────────────────────────");
    match &paths {
        Some(paths) => tool_sources(paths, &allowed),
        None => println!("No absolute profile folder: no tool's files are looked for."),
    }

    // Manual usage tokens: the app's own entries.
    for (id, label) in USAGE_TOKEN_PROVIDERS {
        let mark = if own_entry_stored(label) {
            "stored"
        } else {
            "—"
        };
        println!("{id} usage token ({label}): {mark}");
    }

    // Cursor: only the widget's own session is used; the editor's sign-in is
    // never read. Its state and expiry, never the token.
    println!(
        "Cursor session ({}): {}",
        session::SESSION_LABEL,
        own_cursor_session()
    );

    // Stored keys.
    println!();
    println!("── Keys (Credential Manager) ─────────────────────");
    for (id, label) in PROVIDERS {
        let mark = if own_entry_stored(label) {
            "stored"
        } else {
            "—"
        };
        println!("{id:<8} ({label}): {mark}");
    }

    // The config as it stands: never created, renamed or written here.
    println!();
    println!("── Config ({}) ──", config_path.display());
    match peeked {
        Peek::Missing => println!("none yet — the widget creates it on its first start"),
        Peek::Unreadable(e) => println!("could not be read ({e})"),
        Peek::Unparsable(e) => {
            println!("could not be understood ({e}); the widget sets it aside on its next start")
        }
        Peek::Found(config) => print_config(&config),
        Peek::LeftOut(config, lines) => {
            println!(
                "{}; the commands that change it wait until it does",
                does_not_read(&lines)
            );
            print_config(&config);
        }
    }

    Ok(())
}

/// "config.toml line 12 does not read", "config.toml lines 3, 7 do not
/// read": where, never what.
fn does_not_read(lines: &[usize]) -> String {
    let list = lines
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if lines.len() == 1 {
        format!("config.toml line {list} does not read")
    } else {
        format!("config.toml lines {list} do not read")
    }
}

/// The other tools' sign-ins: read only where `allowed`, otherwise only
/// whether the file is there.
fn tool_sources(paths: &ToolPaths, allowed: &dyn Fn(&str) -> bool) {
    // Claude Code subscription: OAuth token.
    let creds = paths.claude_credentials();
    if !creds.is_file() {
        println!("Claude OAuth: no (.credentials.json missing — run Claude Code once)");
    } else if !allowed("claude") {
        println!("Claude OAuth: .credentials.json found ({NOT_ALLOWED})");
    } else {
        match read_oauth_expiry(&creds) {
            Some((expires_at, sub_type)) => {
                let valid = expires_at > Utc::now();
                let mark = if valid { "OK" } else { "no (expired)" };
                println!("Claude OAuth ({sub_type} subscription): {mark}");
                println!(
                    "  token valid until {} UTC",
                    expires_at.format("%Y-%m-%d %H:%M")
                );
                if !valid {
                    println!("  → run Claude Code, it refreshes the token automatically");
                }
            }
            None => println!("Claude OAuth: no (.credentials.json has no valid token)"),
        }
    }

    // Claude Code's local stats file: a sign Claude Code has run here, not a
    // limit source (it holds no limits). Never opened.
    println!(
        "stats-cache.json: {}",
        presence(&paths.claude_stats_cache())
    );

    // Codex CLI auth.json (in CODEX_HOME when it is set). Never opened here.
    match paths.codex_auth() {
        Some(auth) if auth.is_file() => println!("Codex auth.json ({}): found", auth.display()),
        Some(auth) => println!(
            "Codex auth.json ({}): missing (run codex login)",
            auth.display()
        ),
        None => println!("Codex auth.json: unknown (CODEX_HOME is not an absolute path)"),
    }

    // gh CLI for Copilot: asked for github.com only, its answer never kept.
    let gh = if allowed("copilot") {
        match gh_signed_in() {
            GhState::SignedIn => "OK".to_string(),
            GhState::SignedOut => "no (gh auth login, or store a PAT)".to_string(),
            GhState::NotInstalled => {
                "no (gh not installed — install it, or store a PAT)".to_string()
            }
            GhState::TimedOut => format!("no answer within {} s", GH_TIMEOUT.as_secs()),
        }
    } else {
        "not asked: the widget is not allowed to ask gh; allow it there first".to_string()
    };
    println!("gh CLI sign-in for github.com (Copilot): {gh}");

    // Antigravity (Google): the Credential Manager session, the provider's
    // one source (the legacy Gemini CLI file is no longer read).
    let keyring = if !allowed("antigravity") {
        NOT_ALLOWED
    } else if ailimits::providers::antigravity::keyring_token_present() {
        "OK"
    } else {
        "no (run agy, the Antigravity CLI)"
    };
    println!("Antigravity keyring (gemini:antigravity): {keyring}");

    // Grok CLI login (in GROK_HOME when it is set): the same session rules
    // as the provider. Only the expiry is shown, never the token.
    match paths.grok_auth() {
        None => println!("Grok auth.json: unknown (GROK_HOME is not an absolute path)"),
        Some(auth) if !auth.is_file() => {
            println!("Grok auth.json: no (missing — run grok login)")
        }
        Some(_) if !allowed("grok") => println!("Grok auth.json: found ({NOT_ALLOWED})"),
        Some(auth) => match std::fs::read_to_string(&auth) {
            Ok(content) => match grok::parse_grok_auth(&content, Utc::now()) {
                Ok(GrokLogin::Token { expires_at, .. }) => {
                    println!("Grok auth.json: OK");
                    if let Some(t) = expires_at {
                        println!("  token valid until {} UTC", t.format("%Y-%m-%d %H:%M"));
                    }
                }
                Ok(GrokLogin::Expired) => {
                    println!("Grok auth.json: no (expired)");
                    println!("  → run grok, it refreshes the token automatically");
                }
                Ok(GrokLogin::Missing) => {
                    println!("Grok auth.json: no (no session — run grok login)")
                }
                Err(_) => println!("Grok auth.json: no (unreadable)"),
            },
            Err(_) => println!("Grok auth.json: no (unreadable)"),
        },
    }
}

/// The providers and what the widget may read, from the config as it is.
fn print_config(config: &Config) {
    for p in &config.providers {
        let method = match p.auth_method {
            AuthMethod::Subscription => "subscription",
            AuthMethod::ApiKey => "api_key",
            AuthMethod::UsageToken => "usage_token",
        };
        let state = if p.enabled { "enabled" } else { "disabled" };
        let reads = if config.sign_in_allowed(&p.id) {
            "may read the tool's sign-in"
        } else if config.app_sign_in_allowed(&p.id) {
            "uses the app's own sign-in only"
        } else {
            "reads nothing"
        };
        println!("{:<11} {state}, method: {method}, {reads}", p.id);
    }
    let yes_no = |b: bool| if b { "allowed" } else { "not allowed" };
    println!(
        "settings files for model labels: {}; newest session: {}",
        yes_no(config.reads.tool_settings),
        yes_no(config.reads.last_session)
    );
}

/// What `gh` says about a sign-in to github.com.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GhState {
    SignedIn,
    SignedOut,
    NotInstalled,
    TimedOut,
}

/// Ask gh whether it has a token for github.com — never another host, so
/// a GitHub Enterprise sign-in (or `GH_HOST`) is not mistaken for it. Only
/// the exit code is used: gh's output goes nowhere, so the token never
/// enters this process. std resolves "gh" without ever looking in the
/// working directory (Rust 1.58+), so a planted gh.exe cannot be picked up;
/// the system directory is set as the working directory only so gh itself
/// does not run relative to wherever this tool was launched from.
fn gh_signed_in() -> GhState {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("gh");
    cmd.args(["auth", "token", "--hostname", "github.com"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(root) = std::env::var_os("SystemRoot") {
        cmd.current_dir(Path::new(&root).join("System32"));
    }
    let Ok(mut child) = cmd.spawn() else {
        return GhState::NotInstalled;
    };
    let deadline = Instant::now() + GH_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return GhState::SignedIn,
            Ok(Some(_)) => return GhState::SignedOut,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return GhState::TimedOut;
            }
        }
    }
}

/// Store a key and update the config.
fn set(provider: Option<&str>) -> Result<()> {
    let provider = provider.context("specify a provider: ailimits-auth set <claude|copilot>")?;
    let label = label_for(provider)?;
    // The config first: one that cannot be used is refused before anything
    // is stored.
    let mut config = config_to_change()?;

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

    // The key stored is the source chosen (Claude's API key, Copilot's
    // token): only it is used from now on.
    if let Some(p) = config.providers.iter_mut().find(|p| p.id == provider) {
        p.enabled = true;
        p.credential_label = label.to_string();
        p.auth_method = AuthMethod::ApiKey;
    }
    // Setting a key up is the user asking for the provider, with the app's
    // own key: that allows the key, not the tool's files.
    config.allow_app_sign_in(provider);
    save_config(&config)?;

    println!("Key for '{provider}' stored in Credential Manager (label: {label})");
    println!("{PICKED_UP_HINT}");
    Ok(())
}

/// Store a manual usage token, and make it the source chosen.
fn set_usage_token(provider: Option<&str>) -> Result<()> {
    let provider =
        provider.context("specify a provider: ailimits-auth set-usage-token <claude|codex>")?;
    let label = usage_label_for(provider)?;
    let mut config = config_to_change()?;

    let token = rpassword::prompt_password(format!("Paste the {provider} usage token: "))
        .context("failed to read input")?;
    let token = token.trim();
    if token.is_empty() {
        bail!("empty token — nothing stored");
    }
    // The shape first, so a password pasted by mistake is never sent.
    if let Some(problem) = providers::usage_token_problem(provider, token) {
        bail!("{problem} — nothing stored");
    }
    let unchecked = check_usage_token(provider, token)?;

    keyring::Entry::new(KEYRING_SERVICE, label)
        .context("keyring unavailable")?
        .set_password(token)
        .context("failed to store the usage token in Credential Manager")?;

    // The token is the app's own source and the one chosen now: storing it
    // sets the provider up with it, as a key does, and allows nothing of
    // the tool's files.
    if let Some(p) = config.providers.iter_mut().find(|p| p.id == provider) {
        p.enabled = true;
        p.auth_method = AuthMethod::UsageToken;
        p.credential_label.clear();
    }
    config.allow_app_sign_in(provider);
    save_config(&config)?;

    println!("{provider} usage token stored (label: {label})");
    if let Some(why) = unchecked {
        println!("  It could not be checked ({why}), so it was stored as it is. If the row shows an error later, run this command again.");
    }
    println!("{PICKED_UP_HINT}");
    Ok(())
}

/// Check a usage token with the widget's own check
/// (`providers::check_usage_token`: one read-only request to the host the
/// provider sends it to anyway). An answer that says neither yes nor no (a
/// server error, no answer) is asked again after 2 s and 4 s: at most three
/// requests in all.
///
/// Ok(None): taken — usage (200), or the rate limit (429), which the
/// endpoint answers only for a token it knows. Ok(Some(why)): not checked,
/// and why; the token is stored all the same, as the widget's menu does.
/// Err: refused (401/403), nothing to store.
fn check_usage_token(provider: &str, token: &str) -> Result<Option<String>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let mut waits = CHECK_RETRIES.iter();
    loop {
        match runtime.block_on(providers::check_usage_token(provider, token)) {
            TokenCheck::Accepted => return Ok(None),
            TokenCheck::Refused => {
                bail!("the usage endpoint refused the token (HTTP 401/403) — nothing stored")
            }
            TokenCheck::Unchecked(why) => match waits.next() {
                Some(wait) => std::thread::sleep(*wait),
                None => return Ok(Some(why)),
            },
        }
    }
}

fn remove_usage_token(provider: Option<&str>) -> Result<()> {
    let provider =
        provider.context("specify a provider: ailimits-auth remove-usage-token <claude|codex>")?;
    let label = usage_label_for(provider)?;

    match keyring::Entry::new(KEYRING_SERVICE, label).and_then(|e| e.delete_credential()) {
        Ok(()) => println!("{provider} usage token removed"),
        Err(keyring::Error::NoEntry) => println!("— no {provider} usage token stored"),
        Err(e) => bail!("failed to remove the usage token: {e}"),
    }

    println!("{PICKED_UP_HINT}");
    Ok(())
}

/// Remove a key and switch the provider back to the subscription method.
fn remove(provider: Option<&str>) -> Result<()> {
    let provider = provider.context("specify a provider: ailimits-auth remove <claude|copilot>")?;
    let label = label_for(provider)?;

    // The secret goes first and whatever the config says: the uninstaller
    // runs this, and a config.toml that cannot be read or changed must not
    // leave a key behind in Credential Manager.
    match keyring::Entry::new(KEYRING_SERVICE, label).and_then(|e| e.delete_credential()) {
        Ok(()) => println!("Key '{label}' removed from Credential Manager"),
        Err(keyring::Error::NoEntry) => println!("— no key '{label}' stored"),
        Err(e) => bail!("failed to remove the key: {e}"),
    }

    match config_to_change() {
        Ok(mut config) => {
            if let Some(p) = config.providers.iter_mut().find(|p| p.id == provider) {
                // Without a key everything runs on the subscription method.
                p.credential_label = String::new();
                p.auth_method = AuthMethod::Subscription;
                println!("'{provider}' switched back to the subscription method");
            }
            save_config(&config)?;
        }
        Err(e) => println!("config.toml left as it is ({e}); the key is removed all the same"),
    }

    println!("{PICKED_UP_HINT}");
    Ok(())
}

/// Only Cursor has a sign-in of the widget's own.
fn require_cursor(provider: Option<&str>, command: &str) -> Result<()> {
    match provider {
        Some("cursor") => Ok(()),
        Some(other) => bail!("{command} is for cursor only, got '{other}'"),
        None => bail!("specify a provider: ailimits-auth {command} cursor"),
    }
}

/// Sign the widget in to Cursor: the link opens in the browser, and
/// approving it there hands the widget a session of its own. The editor's
/// session is not touched.
fn login(provider: Option<&str>) -> Result<()> {
    require_cursor(provider, "login")?;
    let request = session::LoginRequest::new()?;
    let url = request.url();
    println!("Approve the sign-in in your browser (it opens now), or open this link:");
    println!("  {url}");
    ailimits::platform::open_url(&url);
    println!(
        "Waiting for the sign-in (up to {} minutes)...",
        session::SIGN_IN_TIMEOUT.as_secs() / 60
    );

    let outcome = tokio::runtime::Runtime::new()?.block_on(session::complete_sign_in(request));
    match outcome {
        session::SignInOutcome::SignedIn(Some(exp)) => println!(
            "Signed in to Cursor, session valid until {} UTC",
            exp.format("%Y-%m-%d %H:%M")
        ),
        session::SignInOutcome::SignedIn(None) => println!("Signed in to Cursor"),
        session::SignInOutcome::TimedOut => {
            bail!("Cursor sign-in timed out — nothing was approved; run it again")
        }
        session::SignInOutcome::Failed(e) => bail!("Cursor sign-in failed: {e}"),
    }
    // Signing in is the user asking for the Cursor row. A widget that had it
    // switched off or not allowed only learns that from the config.
    let mut config = config_to_change()
        .context("the session is stored, but the widget's config could not be updated")?;
    let ready = config
        .providers
        .iter()
        .any(|p| p.id == "cursor" && config.provider_active(p));
    if ready {
        println!("→ The widget uses it from its next update");
    } else {
        if let Some(p) = config.providers.iter_mut().find(|p| p.id == "cursor") {
            p.enabled = true;
        }
        config.allow_app_sign_in("cursor");
        save_config(&config)?;
        println!("{PICKED_UP_HINT}");
    }
    Ok(())
}

/// Forget the widget's own Cursor session. The Cursor row then hides until
/// the next sign-in.
fn logout(provider: Option<&str>) -> Result<()> {
    require_cursor(provider, "logout")?;
    let had = session::is_stored();
    session::delete().context("failed to remove the Cursor session")?;
    if had {
        println!("Signed out of Cursor (label: {})", session::SESSION_LABEL);
    } else {
        println!("— no Cursor session stored");
    }
    Ok(())
}

/// The widget's own Cursor session, as `status` shows it: its state and
/// expiry, never the token.
fn own_cursor_session() -> String {
    let now = Utc::now();
    match session::load() {
        Ok(None) => {
            "— (sign in with ailimits-auth login cursor, or in Rows and models)".to_string()
        }
        Ok(Some(s)) if s.ended => "ended by Cursor — sign in again".to_string(),
        Ok(Some(s)) => match s.expires_at {
            Some(exp) if s.is_expired(now) => format!(
                "expired {} UTC — sign in again",
                exp.format("%Y-%m-%d %H:%M")
            ),
            Some(exp) => format!("OK, valid until {} UTC", exp.format("%Y-%m-%d %H:%M")),
            None => "OK".to_string(),
        },
        Err(e) => format!("no ({e})"),
    }
}

/// Read expiresAt and the subscription type from .credentials.json.
fn read_oauth_expiry(path: &Path) -> Option<(DateTime<Utc>, String)> {
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

/// The config a command changes: the file as it stands, or a first start's
/// config when there is none yet. A file that cannot be read or understood
/// is refused, never replaced and never renamed: nothing is changed. So is
/// one with lines that do not read: saved, it would lose them, and unlike
/// the widget at its start this tool keeps no copy.
fn config_to_change() -> Result<Config> {
    config_at(&storage::config_path())
}

fn config_at(path: &Path) -> Result<Config> {
    match storage::peek(path) {
        Peek::Found(config) => Ok(*config),
        Peek::LeftOut(_, lines) => {
            let fix = if lines.len() == 1 { "it" } else { "them" };
            bail!(
                "{}; fix {fix} first — nothing was changed",
                does_not_read(&lines)
            )
        }
        Peek::Missing => Ok(Config::fresh_install()),
        Peek::Unreadable(e) => bail!(
            "config.toml could not be read ({e}) — nothing was changed; try again in a moment"
        ),
        Peek::Unparsable(e) => bail!(
            "config.toml could not be understood ({e}) — nothing was changed; fix it, or start the widget once to set it aside"
        ),
    }
}

/// Storage is async, the CLI is sync.
fn save_config(config: &Config) -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(storage::save(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A usage token is judged by the widget's own rules: its shape before
    /// anything is sent, then the providers' one check, which takes a 429
    /// as accepted. This tool sends to no host of its own.
    #[test]
    fn a_usage_token_is_checked_as_the_widget_checks_it() {
        assert!(providers::usage_token_problem("claude", "hunter2hunter2").is_some());
        assert!(providers::usage_token_problem("codex", "sk-ant-oat01-abc").is_some());
        assert!(providers::usage_token_problem("claude", "sk-ant-oat01-abc").is_none());
        let src = include_str!("ailimits-auth.rs");
        assert!(!src.contains(&["https", "://"].concat()));
        assert!(src.contains("providers::check_usage_token(provider, token)"));
    }

    /// The commands that change the config refuse one they cannot use, and
    /// never create, rename or replace anything to get past it.
    #[test]
    fn a_config_that_cannot_be_used_is_refused_and_left_alone() {
        let dir = std::env::temp_dir().join(format!("ailimits-auth-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        // None yet: a first start's config, and still no file.
        let fresh = config_at(&path).unwrap();
        assert_eq!(fresh, Config::fresh_install());
        assert!(!path.exists());
        std::fs::write(
            &path,
            "a = =\nb = =\nc = =\nd = =\ne = =\nf = =\ng = =\nh = =\ni = =\n",
        )
        .unwrap();
        let err = config_at(&path).unwrap_err().to_string();
        assert!(err.contains("nothing was changed"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "a = =\nb = =\nc = =\nd = =\ne = =\nf = =\ng = =\nh = =\ni = =\n"
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A config with lines that do not read is refused, the lines named:
    /// saved, it would lose them, with no copy kept. Fixed, it is taken.
    #[test]
    fn a_config_with_lines_left_out_is_refused_until_fixed() {
        let dir = std::env::temp_dir().join(format!("ailimits-auth-left-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let text =
            "[reads]\nsign_in = [\"claude\"]\n\n[ui]\nbrightness = \"50\"\npalette = \"ocean\"\n";
        std::fs::write(&path, text).unwrap();
        let err = config_at(&path).unwrap_err().to_string();
        assert_eq!(
            err,
            "config.toml line 5 does not read; fix it first — nothing was changed"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let two = text.replace("palette = \"ocean\"", "palette = ocean");
        std::fs::write(&path, &two).unwrap();
        let err = config_at(&path).unwrap_err().to_string();
        assert!(
            err.starts_with("config.toml lines 5, 6 do not read; fix them first"),
            "{err}"
        );
        let fixed = text.replace("\"50\"", "50");
        std::fs::write(&path, &fixed).unwrap();
        let config = config_at(&path).unwrap();
        assert_eq!(config.ui.brightness, 50);
        assert!(config.sign_in_allowed("claude"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The editor's sign-in is never read: none of its markers is left in
    /// this tool.
    #[test]
    fn the_editors_sign_in_is_never_read() {
        let src = include_str!("ailimits-auth.rs");
        let marker = ["cursorAuth", "/accessToken"].concat();
        assert!(!src.contains(&marker));
        assert!(!src.contains(&["editor_token", "_state"].concat()));
        assert!(!src.contains(&["state", ".vscdb"].concat()));
    }

    /// gh is asked about github.com only, and its output is never captured.
    #[test]
    fn gh_is_asked_for_github_com_only() {
        let src = include_str!("ailimits-auth.rs");
        let call = ["\"auth\", \"token\", \"--hostname\", \"github.com\""].concat();
        assert!(src.contains(&call));
        assert!(!src.contains(&[".output", "()"].concat()));
    }
}
