// hooks.rs — run user-configured shell commands on usage events.
//
// SECURITY: this is the ONE place the widget runs user-provided commands.
// It is opt-in (empty by default) and configurable ONLY in config.toml —
// there is deliberately no menu/clipboard path, so a malicious clipboard
// string can never become an auto-run command. Fire-and-forget: a hook
// never blocks the event loop and a failure is only logged.

use crate::providers::ProviderId;
use chrono::{DateTime, Utc};

/// Absolute path to the system command interpreter, so a planted cmd.exe in
/// the working/app directory cannot hijack a hook. Falls back to the bare
/// name if %SystemRoot% is somehow unset.
pub(crate) fn system_cmd_exe() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let p = std::path::Path::new(&root).join("System32").join("cmd.exe");
            if p.exists() {
                return p;
            }
        }
    }
    std::path::PathBuf::from("cmd")
}

/// Which event fired.
#[derive(Clone, Copy)]
pub enum HookEvent {
    Threshold,
    Reset,
    Startup,
}

impl HookEvent {
    fn name(self) -> &'static str {
        match self {
            HookEvent::Threshold => "threshold",
            HookEvent::Reset => "reset",
            HookEvent::Startup => "startup",
        }
    }
}

/// Spawn `command` (if non-empty) detached on the tokio runtime, passing the
/// event context through environment variables:
///   AILIMITS_EVENT     threshold | reset | startup
///   AILIMITS_PROVIDER  Claude | Codex | Copilot | …
///   AILIMITS_PERCENT   integer % (when known)
///   AILIMITS_RESET_AT  RFC3339 reset time (when known)
pub fn run(
    runtime: &tokio::runtime::Runtime,
    command: &str,
    event: HookEvent,
    provider: ProviderId,
    percent: Option<f32>,
    reset_at: Option<DateTime<Utc>>,
) {
    let command = command.trim().to_string();
    if command.is_empty() {
        return;
    }
    let event_name = event.name();
    let provider_name = provider.display_name();
    let percent = percent.map(|p| (p.round() as u32).to_string());
    let reset = reset_at.map(|r| r.to_rfc3339());

    runtime.spawn(async move {
        // Absolute cmd.exe, not the bare name: a "cmd.exe" planted in the
        // process's working directory must never run instead of the system one.
        let mut cmd = tokio::process::Command::new(system_cmd_exe());
        cmd.args(["/C", &command]);
        cmd.env("AILIMITS_EVENT", event_name);
        cmd.env("AILIMITS_PROVIDER", provider_name);
        if let Some(p) = percent {
            cmd.env("AILIMITS_PERCENT", p);
        }
        if let Some(r) = reset {
            cmd.env("AILIMITS_RESET_AT", r);
        }
        // CREATE_NO_WINDOW: no console flash from a GUI app.
        #[cfg(target_os = "windows")]
        cmd.creation_flags(0x0800_0000);
        cmd.kill_on_drop(true);
        match cmd.spawn() {
            Ok(mut child) => {
                // Bound a runaway hook so it cannot linger as a zombie process.
                // start_kill terminates this cmd.exe but NOT a whole tree it may
                // have spawned — a full guarantee would need a Windows Job Object
                // (kill-on-close). Acceptable here: hooks are opt-in commands the
                // user wrote for themselves, not untrusted input.
                if tokio::time::timeout(std::time::Duration::from_secs(30), child.wait())
                    .await
                    .is_err()
                {
                    let _ = child.start_kill();
                    tracing::warn!("hook '{event_name}' timed out after 30s (killed)");
                }
            }
            Err(e) => tracing::warn!("hook '{event_name}' failed to spawn: {e}"),
        }
    });
}
