// hooks.rs — run user-configured shell commands on usage events.
//
// SECURITY: this is the ONE place the widget runs user-provided commands.
// It is opt-in (empty by default) and configurable ONLY in config.toml —
// there is deliberately no menu/clipboard path, so a malicious clipboard
// string can never become an auto-run command. Fire-and-forget: a hook
// never blocks the event loop and a failure is only logged.
//
// The command reaches cmd.exe exactly as written (`/S /C "<command>"`, see
// `cmd_arguments`), so quotes around a path with spaces work as they do in a
// console. Nothing the app knows is ever spliced into it: the event's details
// travel in environment variables only.

use crate::providers::ProviderId;
use crate::utc::{self, Timestamp};

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

/// How long a hook may run before it is stopped.
const HOOK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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

/// What cmd.exe is given for `command`, or None for a command that cannot
/// be one line of cmd.
///
/// `/S /C "<command>"`: with /S cmd strips exactly the outer pair of quotes
/// and runs the rest verbatim, so `"C:\Program Files\x\y.exe" --flag` stays
/// what the user wrote. Passed through Rust's own argument quoting instead
/// (the code before 0.7.1), the inner quotes arrived as `\"` and every such
/// hook failed with "is not recognized", silently. `/D` leaves out the
/// AutoRun commands the registry may add to every cmd.exe: a hook runs only
/// what its line says.
///
/// A line break would end the command early and run the rest as another
/// line; a NUL cannot be passed at all. Such a command is refused whole.
pub(crate) fn cmd_arguments(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() || command.chars().any(|c| matches!(c, '\r' | '\n' | '\0')) {
        return None;
    }
    Some(format!("/D /S /C \"{command}\""))
}

/// The process a hook runs as, with the event's details in its environment:
///   AILIMITS_EVENT     threshold | reset | startup
///   AILIMITS_PROVIDER  Claude | Codex | Copilot | …
///   AILIMITS_PERCENT   integer % (when known)
///   AILIMITS_RESET_AT  RFC3339 reset time (when known)
fn hook_command(
    command: &str,
    event: HookEvent,
    provider: &ProviderId,
    percent: Option<f32>,
    reset_at: Option<Timestamp>,
) -> Option<tokio::process::Command> {
    let args = cmd_arguments(command)?;
    let mut cmd = tokio::process::Command::new(system_cmd_exe());
    #[cfg(target_os = "windows")]
    cmd.raw_arg(args);
    #[cfg(not(target_os = "windows"))]
    cmd.arg(args);
    cmd.env("AILIMITS_EVENT", event.name());
    cmd.env("AILIMITS_PROVIDER", provider.display_name());
    if let Some(p) = percent {
        cmd.env("AILIMITS_PERCENT", (p.round() as u32).to_string());
    }
    if let Some(r) = reset_at {
        cmd.env("AILIMITS_RESET_AT", utc::to_rfc3339(r));
    }
    // CREATE_NO_WINDOW: no console flash from a GUI app.
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x0800_0000);
    cmd.kill_on_drop(true);
    Some(cmd)
}

/// How a hook ended.
#[derive(Debug, PartialEq, Eq)]
enum Ended {
    /// Exited with this code (0 is success).
    Code(i32),
    /// Killed after `HOOK_TIMEOUT`.
    TimedOut,
    /// Could not be started.
    NotStarted,
}

async fn run_to_end(mut cmd: tokio::process::Command) -> Ended {
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(_) => return Ended::NotStarted,
    };
    // Bound a runaway hook so it cannot linger as a zombie process.
    // start_kill terminates this cmd.exe but NOT a whole tree it may have
    // spawned — a full guarantee would need a Windows Job Object
    // (kill-on-close). Acceptable here: hooks are opt-in commands the user
    // wrote for themselves, not untrusted input.
    match tokio::time::timeout(HOOK_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => Ended::Code(status.code().unwrap_or(-1)),
        Ok(Err(_)) => Ended::Code(-1),
        Err(_) => {
            let _ = child.start_kill();
            Ended::TimedOut
        }
    }
}

/// Spawn `command` (if non-empty) detached on the tokio runtime. How it
/// ended is logged by event name and exit code - never the command itself,
/// which may carry a webhook secret of the user's.
pub fn run(
    runtime: &tokio::runtime::Runtime,
    command: &str,
    event: HookEvent,
    provider: ProviderId,
    percent: Option<f32>,
    reset_at: Option<Timestamp>,
) {
    if command.trim().is_empty() {
        return;
    }
    let event_name = event.name();
    let Some(cmd) = hook_command(command, event, &provider, percent, reset_at) else {
        tracing::warn!("hook '{event_name}' not run: a hook is one line, without line breaks");
        return;
    };
    runtime.spawn(async move {
        match run_to_end(cmd).await {
            Ended::Code(0) => tracing::debug!("hook '{event_name}' done"),
            Ended::Code(code) => tracing::warn!("hook '{event_name}' exited with code {code}"),
            Ended::TimedOut => {
                tracing::warn!("hook '{event_name}' timed out after 30s (killed)")
            }
            Ended::NotStarted => tracing::warn!("hook '{event_name}' failed to start"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_reaches_cmd_as_written() {
        assert_eq!(
            cmd_arguments(r#""C:\Program Files\x\y.exe" --flag "a b""#).unwrap(),
            r#"/D /S /C ""C:\Program Files\x\y.exe" --flag "a b"""#
        );
        assert_eq!(
            cmd_arguments("  echo hi  ").unwrap(),
            r#"/D /S /C "echo hi""#
        );
    }

    /// `AILIMITS_RESET_AT` as 0.7.0 set it: RFC 3339 in UTC with "+00:00"
    /// and 0, 3, 6 or 9 fractional digits.
    #[test]
    fn the_reset_reaches_the_hook_as_before() {
        let reset_at = |text: &str| {
            let reset = crate::utc::parse_rfc3339(text);
            assert!(reset.is_some(), "{text}");
            let cmd =
                hook_command("exit 0", HookEvent::Reset, &ProviderId::Claude, None, reset).unwrap();
            let value = cmd
                .as_std()
                .get_envs()
                .find(|(key, _)| *key == "AILIMITS_RESET_AT")
                .and_then(|(_, value)| value)
                .map(|v| v.to_string_lossy().into_owned());
            value.unwrap()
        };
        assert_eq!(
            reset_at("2026-11-16T12:00:00+02:00"),
            "2026-11-16T10:00:00+00:00"
        );
        assert_eq!(
            reset_at("2026-11-16T10:00:00.4Z"),
            "2026-11-16T10:00:00.400+00:00"
        );
        assert_eq!(
            reset_at("2026-11-16T10:00:00.000001Z"),
            "2026-11-16T10:00:00.000001+00:00"
        );
        assert_eq!(
            reset_at("2026-11-09T05:43:16.908847600Z"),
            "2026-11-09T05:43:16.908847600+00:00"
        );
        assert_eq!(
            reset_at("1969-12-31T23:59:59.5Z"),
            "1969-12-31T23:59:59.500+00:00"
        );
        let none = hook_command("exit 0", HookEvent::Reset, &ProviderId::Claude, None, None);
        assert!(!none
            .unwrap()
            .as_std()
            .get_envs()
            .any(|(key, _)| key == "AILIMITS_RESET_AT"));
    }

    #[test]
    fn a_line_break_refuses_the_command_whole() {
        assert_eq!(cmd_arguments("echo one\r\ndel x"), None);
        assert_eq!(cmd_arguments("echo one\ndel x"), None);
        assert_eq!(cmd_arguments("   "), None);
    }

    #[cfg(target_os = "windows")]
    fn ended(command: &str, percent: Option<f32>) -> Ended {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let cmd = hook_command(
            command,
            HookEvent::Threshold,
            &ProviderId::Claude,
            percent,
            None,
        )
        .unwrap();
        rt.block_on(run_to_end(cmd))
    }

    /// The hook from the bug report: an interpreter path in quotes. Before,
    /// cmd saw `'\"C:\...\cmd.exe\"' is not recognized` and exited 1.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_quoted_program_path_runs() {
        let cmd = system_cmd_exe();
        let line = format!(r#""{}" /C exit 7"#, cmd.display());
        assert_eq!(ended(&line, None), Ended::Code(7));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn quoted_arguments_keep_their_spaces() {
        assert_eq!(
            ended(r#"if "a b"=="a b" (exit 3) else (exit 4)"#, None),
            Ended::Code(3)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_event_arrives_in_the_environment() {
        assert_eq!(
            ended("exit %AILIMITS_PERCENT%", Some(42.4)),
            Ended::Code(42)
        );
    }
}
