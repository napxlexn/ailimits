// config/paths.rs — where the other tools keep their files. Every path the
// app may read of another tool (sign-in, settings, sessions) and every folder
// whose existence says a tool is installed is resolved here, once, so that
// the providers, the model labels, the consent text and the ailimits-auth
// CLI can never disagree about which file is meant.
//
// Only absolute paths. Without an absolute home folder nothing resolves. A
// tool-home variable (CODEX_HOME, GROK_HOME, COPILOT_HOME) that is set but
// not absolute leaves that tool's folder unknown: nothing of it is read —
// never a path relative to the app's working directory, and never the
// default folder instead, which may hold another account's stale sign-in.
//
// Nothing here touches the disk: these are names, not reads.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The folders the tools keep their files in, resolved from the environment
/// (or fixed under one home, for tests and probes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPaths {
    /// %USERPROFILE%
    user: PathBuf,
    /// %APPDATA% (the roaming profile folder).
    roaming: PathBuf,
    /// `CODEX_HOME` when set (absolute), else %USERPROFILE%\.codex; None
    /// when the variable is set but not absolute.
    codex: Option<PathBuf>,
    /// `GROK_HOME`, else %USERPROFILE%\.grok; the same rule.
    grok: Option<PathBuf>,
    /// `COPILOT_HOME`, else %USERPROFILE%\.copilot; the same rule.
    copilot: Option<PathBuf>,
}

/// A folder whose existence suggests a tool is installed, and how much it
/// says. Only `is_dir` is ever asked of it: nothing inside is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSign {
    pub path: PathBuf,
    /// A weak sign: the folder belongs to a related tool rather than the one
    /// read (GitHub CLI for Copilot, Gemini CLI for Antigravity), so it says
    /// little about whether the provider will work. The welcome should mark
    /// such a tool as found without ticking it.
    pub weak: bool,
}

impl ToolPaths {
    /// From this process's environment: the user's profile and roaming
    /// folders as Windows reports them, and the tool-home variables. None
    /// without an absolute profile folder.
    pub fn from_env() -> Option<Self> {
        Self::resolve(dirs::home_dir(), dirs::config_dir(), |var| {
            std::env::var_os(var)
        })
    }

    /// The rule itself, over given folders and variables. A roaming folder
    /// that is missing or not absolute is taken as %USERPROFILE%\AppData\
    /// Roaming, where Windows keeps it.
    pub fn resolve(
        home: Option<PathBuf>,
        roaming: Option<PathBuf>,
        env: impl Fn(&str) -> Option<OsString>,
    ) -> Option<Self> {
        let user = home.filter(|p| p.is_absolute())?;
        let roaming = roaming
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| default_roaming(&user));
        let tool = |var: &str, dir: &str| tool_home(&user, env(var), dir);
        Some(ToolPaths {
            codex: tool("CODEX_HOME", ".codex"),
            grok: tool("GROK_HOME", ".grok"),
            copilot: tool("COPILOT_HOME", ".copilot"),
            roaming,
            user,
        })
    }

    /// Every folder under one home, as on a PC where no tool-home variable
    /// is set: for tests, and for probes over copied homes.
    pub fn at_home(root: &Path) -> Self {
        ToolPaths {
            user: root.to_path_buf(),
            roaming: default_roaming(root),
            codex: Some(root.join(".codex")),
            grok: Some(root.join(".grok")),
            copilot: Some(root.join(".copilot")),
        }
    }

    /// %USERPROFILE%
    pub fn user(&self) -> &Path {
        &self.user
    }

    /// %APPDATA%
    pub fn roaming(&self) -> &Path {
        &self.roaming
    }

    // ─── Claude Code ──────────────────────────────────────────────

    /// %USERPROFILE%\.claude
    pub fn claude_dir(&self) -> PathBuf {
        self.user.join(".claude")
    }

    /// Claude Code's sign-in: `.claude\.credentials.json`.
    pub fn claude_credentials(&self) -> PathBuf {
        self.claude_dir().join(".credentials.json")
    }

    /// Claude Code's settings: `.claude\settings.json`.
    pub fn claude_settings(&self) -> PathBuf {
        self.claude_dir().join("settings.json")
    }

    /// Claude Code's sessions: `.claude\projects\<project>\<session>.jsonl`.
    pub fn claude_projects(&self) -> PathBuf {
        self.claude_dir().join("projects")
    }

    /// Claude Code's local statistics, `.claude\stats-cache.json`: only a
    /// sign that Claude Code ran here (it holds no limits). Never opened.
    pub fn claude_stats_cache(&self) -> PathBuf {
        self.claude_dir().join("stats-cache.json")
    }

    // ─── Codex CLI ────────────────────────────────────────────────

    /// `CODEX_HOME`, else %USERPROFILE%\.codex.
    pub fn codex_home(&self) -> Option<&Path> {
        self.codex.as_deref()
    }

    /// Codex CLI's sign-in: `auth.json` in its home.
    pub fn codex_auth(&self) -> Option<PathBuf> {
        self.codex_home().map(|h| h.join("auth.json"))
    }

    /// Codex CLI's settings: `config.toml` in its home.
    pub fn codex_config(&self) -> Option<PathBuf> {
        self.codex_home().map(|h| h.join("config.toml"))
    }

    /// Codex CLI's sessions: `sessions\YYYY\MM\DD\rollout-*.jsonl`.
    pub fn codex_sessions(&self) -> Option<PathBuf> {
        self.codex_home().map(|h| h.join("sessions"))
    }

    // ─── Grok CLI ─────────────────────────────────────────────────

    /// `GROK_HOME`, else %USERPROFILE%\.grok.
    pub fn grok_home(&self) -> Option<&Path> {
        self.grok.as_deref()
    }

    /// Grok CLI's sign-in: `auth.json` in its home.
    pub fn grok_auth(&self) -> Option<PathBuf> {
        self.grok_home().map(|h| h.join("auth.json"))
    }

    /// Grok CLI's settings: `config.toml` in its home.
    pub fn grok_config(&self) -> Option<PathBuf> {
        self.grok_home().map(|h| h.join("config.toml"))
    }

    // ─── Copilot ──────────────────────────────────────────────────

    /// `COPILOT_HOME`, else %USERPROFILE%\.copilot (the Copilot CLI's home).
    pub fn copilot_home(&self) -> Option<&Path> {
        self.copilot.as_deref()
    }

    /// The Copilot CLI's settings: `settings.json` in its home.
    pub fn copilot_settings(&self) -> Option<PathBuf> {
        self.copilot_home().map(|h| h.join("settings.json"))
    }

    /// GitHub CLI's folder, %APPDATA%\GitHub CLI: only a sign that gh is
    /// installed; its sign-in is asked of gh itself, never read here.
    pub fn gh_config_dir(&self) -> PathBuf {
        self.roaming.join("GitHub CLI")
    }

    // ─── Antigravity ──────────────────────────────────────────────

    /// The Antigravity app's folder, %APPDATA%\Antigravity.
    pub fn antigravity_app_dir(&self) -> PathBuf {
        self.roaming.join("Antigravity")
    }

    /// %USERPROFILE%\.gemini (Gemini CLI, and the Antigravity CLI under it).
    pub fn gemini_dir(&self) -> PathBuf {
        self.user.join(".gemini")
    }

    /// The older Gemini CLI's sign-in: `.gemini\oauth_creds.json`.
    pub fn gemini_oauth_creds(&self) -> PathBuf {
        self.gemini_dir().join("oauth_creds.json")
    }

    /// The Antigravity CLI's folder: `.gemini\antigravity-cli`.
    pub fn antigravity_cli_dir(&self) -> PathBuf {
        self.gemini_dir().join("antigravity-cli")
    }

    /// The Antigravity CLI's settings: `.gemini\antigravity-cli\settings.json`.
    pub fn antigravity_cli_settings(&self) -> PathBuf {
        self.antigravity_cli_dir().join("settings.json")
    }

    // ─── Cursor ───────────────────────────────────────────────────

    /// The Cursor editor's folder, %APPDATA%\Cursor.
    pub fn cursor_dir(&self) -> PathBuf {
        self.roaming.join("Cursor")
    }

    /// The Cursor editor's state database:
    /// %APPDATA%\Cursor\User\globalStorage\state.vscdb.
    pub fn cursor_state_db(&self) -> PathBuf {
        self.cursor_dir()
            .join("User")
            .join("globalStorage")
            .join("state.vscdb")
    }

    // ─── Installed tools ──────────────────────────────────────────

    /// The folders whose existence says a provider's tool is installed, by
    /// config id ("gemini" is Antigravity's old id). Empty for an unknown id
    /// and for a tool whose home is unknown.
    pub fn tool_signs(&self, provider_id: &str) -> Vec<ToolSign> {
        let strong = |path: PathBuf| ToolSign { path, weak: false };
        let weak = |path: PathBuf| ToolSign { path, weak: true };
        match provider_id {
            "claude" => vec![strong(self.claude_dir())],
            "codex" => self
                .codex_home()
                .map(Path::to_path_buf)
                .map(strong)
                .into_iter()
                .collect(),
            "copilot" => {
                let mut signs: Vec<ToolSign> = self
                    .copilot_home()
                    .map(Path::to_path_buf)
                    .map(strong)
                    .into_iter()
                    .collect();
                signs.push(weak(self.gh_config_dir()));
                signs
            }
            "antigravity" | "gemini" => vec![
                strong(self.antigravity_app_dir()),
                strong(self.antigravity_cli_dir()),
                weak(self.gemini_dir()),
            ],
            "grok" => self
                .grok_home()
                .map(Path::to_path_buf)
                .map(strong)
                .into_iter()
                .collect(),
            "cursor" => vec![strong(self.cursor_dir())],
            _ => Vec::new(),
        }
    }
}

/// A tool's home: the variable when it is set to an absolute path, the
/// default folder when it is not set (or set empty), None when it is set to
/// something relative.
fn tool_home(user: &Path, value: Option<OsString>, default_dir: &str) -> Option<PathBuf> {
    match value.filter(|v| !v.is_empty()) {
        None => Some(user.join(default_dir)),
        Some(v) => {
            let path = PathBuf::from(v);
            path.is_absolute().then_some(path)
        }
    }
}

fn default_roaming(user: &Path) -> PathBuf {
    user.join("AppData").join("Roaming")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const HOME: &str = r"C:\Users\someone";

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |var| map.get(var).cloned()
    }

    fn resolve(vars: &[(&str, &str)]) -> Option<ToolPaths> {
        ToolPaths::resolve(
            Some(PathBuf::from(HOME)),
            Some(PathBuf::from(r"C:\Users\someone\AppData\Roaming")),
            env(vars),
        )
    }

    #[test]
    fn the_default_folders_sit_in_the_profile() {
        let p = resolve(&[]).unwrap();
        assert_eq!(p, ToolPaths::at_home(Path::new(HOME)));
        let home = Path::new(HOME);
        assert_eq!(
            p.claude_credentials(),
            home.join(".claude").join(".credentials.json")
        );
        assert_eq!(p.claude_settings(), home.join(r".claude\settings.json"));
        assert_eq!(p.claude_projects(), home.join(r".claude\projects"));
        assert_eq!(p.codex_auth(), Some(home.join(r".codex\auth.json")));
        assert_eq!(p.codex_config(), Some(home.join(r".codex\config.toml")));
        assert_eq!(p.codex_sessions(), Some(home.join(r".codex\sessions")));
        assert_eq!(p.grok_auth(), Some(home.join(r".grok\auth.json")));
        assert_eq!(p.grok_config(), Some(home.join(r".grok\config.toml")));
        assert_eq!(
            p.copilot_settings(),
            Some(home.join(r".copilot\settings.json"))
        );
        assert_eq!(
            p.gemini_oauth_creds(),
            home.join(r".gemini\oauth_creds.json")
        );
        assert_eq!(
            p.antigravity_cli_settings(),
            home.join(r".gemini\antigravity-cli\settings.json")
        );
        assert_eq!(
            p.cursor_state_db(),
            home.join(r"AppData\Roaming\Cursor\User\globalStorage\state.vscdb")
        );
        assert_eq!(p.gh_config_dir(), home.join(r"AppData\Roaming\GitHub CLI"));
    }

    #[test]
    fn the_tool_home_variables_move_every_file_of_their_tool() {
        let p = resolve(&[
            ("CODEX_HOME", r"D:\codex"),
            ("GROK_HOME", r"D:\grok"),
            ("COPILOT_HOME", r"\\server\share\copilot"),
        ])
        .unwrap();
        // Sign-in, settings and sessions alike: the variable is never half
        // followed (the bug: Codex's sign-in read from ~\.codex anyway).
        assert_eq!(p.codex_auth(), Some(PathBuf::from(r"D:\codex\auth.json")));
        assert_eq!(
            p.codex_config(),
            Some(PathBuf::from(r"D:\codex\config.toml"))
        );
        assert_eq!(
            p.codex_sessions(),
            Some(PathBuf::from(r"D:\codex\sessions"))
        );
        assert_eq!(p.grok_auth(), Some(PathBuf::from(r"D:\grok\auth.json")));
        assert_eq!(p.grok_config(), Some(PathBuf::from(r"D:\grok\config.toml")));
        assert_eq!(
            p.copilot_settings(),
            Some(PathBuf::from(r"\\server\share\copilot\settings.json"))
        );
        // The other tools stay where they were.
        assert_eq!(
            p.claude_credentials(),
            PathBuf::from(r"C:\Users\someone\.claude\.credentials.json")
        );
        // An empty variable counts as not set.
        let p = resolve(&[("CODEX_HOME", "")]).unwrap();
        assert_eq!(
            p.codex_auth(),
            Some(PathBuf::from(r"C:\Users\someone\.codex\auth.json"))
        );
    }

    #[test]
    fn a_relative_tool_home_resolves_to_nothing_at_all() {
        for relative in [r"codex", r".\codex", r"\codex", r"C:codex"] {
            let p = resolve(&[
                ("CODEX_HOME", relative),
                ("GROK_HOME", relative),
                ("COPILOT_HOME", relative),
            ])
            .unwrap();
            // Not the working directory, and not ~\.codex instead.
            assert_eq!(p.codex_home(), None, "{relative}");
            assert_eq!(p.codex_auth(), None);
            assert_eq!(p.codex_config(), None);
            assert_eq!(p.codex_sessions(), None);
            assert_eq!(p.grok_auth(), None);
            assert_eq!(p.grok_config(), None);
            assert_eq!(p.copilot_settings(), None);
            assert!(p.tool_signs("codex").is_empty());
            assert!(p.tool_signs("grok").is_empty());
            // Copilot still has gh's folder as a sign.
            assert_eq!(p.tool_signs("copilot").len(), 1);
        }
    }

    #[test]
    fn no_absolute_profile_folder_resolves_nothing() {
        assert_eq!(ToolPaths::resolve(None, None, env(&[])), None);
        assert_eq!(
            ToolPaths::resolve(Some(PathBuf::from("Users\\x")), None, env(&[])),
            None
        );
        assert_eq!(
            ToolPaths::resolve(Some(PathBuf::new()), None, env(&[])),
            None
        );
        // A missing or relative roaming folder is the profile's own.
        for roaming in [None, Some(PathBuf::from("AppData"))] {
            let p = ToolPaths::resolve(Some(PathBuf::from(HOME)), roaming, env(&[])).unwrap();
            assert_eq!(p.roaming(), Path::new(r"C:\Users\someone\AppData\Roaming"));
        }
    }

    #[test]
    fn every_path_is_absolute() {
        let p = resolve(&[("CODEX_HOME", r"E:\c")]).unwrap();
        let mut all = vec![
            p.claude_credentials(),
            p.claude_settings(),
            p.claude_projects(),
            p.gh_config_dir(),
            p.antigravity_app_dir(),
            p.gemini_oauth_creds(),
            p.antigravity_cli_settings(),
            p.cursor_state_db(),
        ];
        all.extend(p.codex_auth());
        all.extend(p.codex_config());
        all.extend(p.codex_sessions());
        all.extend(p.grok_auth());
        all.extend(p.grok_config());
        all.extend(p.copilot_settings());
        for id in [
            "claude",
            "codex",
            "copilot",
            "antigravity",
            "grok",
            "cursor",
        ] {
            assert!(!p.tool_signs(id).is_empty(), "{id}");
            all.extend(p.tool_signs(id).into_iter().map(|s| s.path));
        }
        for path in all {
            assert!(path.is_absolute(), "{}", path.display());
        }
    }

    #[test]
    fn weak_signs_are_the_related_tools_only() {
        let p = ToolPaths::at_home(Path::new(HOME));
        let weak = |id: &str| -> Vec<PathBuf> {
            p.tool_signs(id)
                .into_iter()
                .filter(|s| s.weak)
                .map(|s| s.path)
                .collect()
        };
        assert_eq!(weak("copilot"), [p.gh_config_dir()]);
        assert_eq!(weak("antigravity"), [p.gemini_dir()]);
        assert_eq!(weak("gemini"), [p.gemini_dir()]);
        for id in ["claude", "codex", "grok", "cursor"] {
            assert!(weak(id).is_empty(), "{id}");
        }
        assert!(p.tool_signs("nope").is_empty());
    }

    /// What the user is asked to allow (consent.rs) names the sign-in files
    /// resolved here, in the words the questions use.
    #[test]
    fn the_consent_text_names_the_sign_in_files_resolved_here() {
        let root = Path::new(HOME);
        let p = ToolPaths::at_home(root);
        let as_told = |path: PathBuf| {
            let rel = path.strip_prefix(root).unwrap().display().to_string();
            format!(r"%USERPROFILE%\{rel}")
        };
        for (id, file) in [
            ("claude", p.claude_credentials()),
            ("codex", p.codex_auth().unwrap()),
            ("grok", p.grok_auth().unwrap()),
            ("antigravity", p.gemini_oauth_creds()),
        ] {
            let told = crate::consent::read_info(id).unwrap();
            let named = as_told(file);
            assert!(
                told.sign_in.iter().any(|s| s.contains(&named)),
                "{id}: {:?} does not name {named}",
                told.sign_in
            );
        }
    }
}
