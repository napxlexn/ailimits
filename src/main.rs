// Hide the console window on Windows.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

// Modules are declared in lib.rs — use them via the crate name.
use ailimits::app;
use anyhow::Result;
use tracing::info;

/// Single-instance guard: the named mutex lives until the process exits.
#[cfg(target_os = "windows")]
fn another_instance_running() -> bool {
    use windows::core::w;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;

    unsafe {
        // The mutex intentionally leaks — the OS releases it on process exit.
        let _ = CreateMutexW(None, true, w!("Local\\AiLimitsWidgetSingleInstance"));
        GetLastError() == ERROR_ALREADY_EXISTS
    }
}

/// The opt-in log file is capped: past this size the current file becomes
/// `ailimits.log.1` (replacing the previous one) and a fresh file starts, so a
/// debug session left running for weeks keeps at most two of these on disk.
const LOG_ROTATE_BYTES: u64 = 5 * 1024 * 1024;

/// A log file that rotates itself once it grows past `LOG_ROTATE_BYTES`. Only
/// the append path is ever used, so the running size is tracked here instead
/// of asked of the file system on every line.
struct RotatingLog {
    path: std::path::PathBuf,
    file: std::fs::File,
    size: u64,
    limit: u64,
}

impl RotatingLog {
    fn open(path: std::path::PathBuf) -> std::io::Result<Self> {
        Self::open_with_limit(path, LOG_ROTATE_BYTES)
    }

    fn open_with_limit(path: std::path::PathBuf, limit: u64) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            file,
            size,
            limit,
        })
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        let old = self.path.with_extension("log.1");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&self.path, &old)?;
        self.file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.size = 0;
        Ok(())
    }
}

impl std::io::Write for RotatingLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.size + buf.len() as u64 > self.limit {
            // A failed rename keeps writing to the old file: losing the log is
            // worse than an oversized one.
            let _ = self.rotate();
        }
        let n = self.file.write(buf)?;
        self.size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// Logging init: stderr is invisible in a GUI app, so AILIMITS_LOG or RUST_LOG
/// writes next to the config instead.
fn init_logging() {
    let file_filter = std::env::var("AILIMITS_LOG").ok();
    let rust_filter = std::env::var("RUST_LOG").ok();
    let filter = file_filter
        .clone()
        .or(rust_filter.clone())
        .unwrap_or_else(|| "ailimits=info".to_string());
    let builder = tracing_subscriber::fmt().with_env_filter(filter);

    if file_filter.is_some() || rust_filter.is_some() {
        let log_path = ailimits::config::storage::config_path().with_file_name("ailimits.log");
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(log) = RotatingLog::open(log_path) {
            builder
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(log))
                .init();
            return;
        }
    }
    builder.init();
}

/// Where a panic leaves its trace, next to the config and the log.
fn crash_log_path() -> std::path::PathBuf {
    ailimits::config::storage::config_path().with_file_name("crash.log")
}

/// Past this size `crash.log` becomes `crash.log.1` and starts over.
const CRASH_LOG_ROTATE_BYTES: u64 = 64 * 1024;

/// A source path as a panic location gives it, without the part that only
/// says where the build ran: a dependency's file comes as
/// `...\.cargo\registry\src\<index>\tao-0.37.0\src\...`, whose start names
/// the account of whoever built the exe.
fn short_source(file: &str) -> &str {
    for marker in ["registry\\src\\", "registry/src/"] {
        if let Some(at) = file.find(marker) {
            let rest = &file[at + marker.len()..];
            // Skip the registry index folder too.
            return rest
                .find(['\\', '/'])
                .map_or(rest, |slash| &rest[slash + 1..]);
        }
    }
    file
}

/// One line per panic: when, which version, which thread, where. Never the
/// panic's message: a panic while slicing text quotes the text it was
/// slicing, and that can be a file's content or a token.
fn crash_line(
    now: chrono::DateTime<chrono::Utc>,
    version: &str,
    thread: &str,
    location: Option<(&str, u32, u32)>,
) -> String {
    let at = match location {
        Some((file, line, col)) => format!("{}:{line}:{col}", short_source(file)),
        None => "an unknown place".to_string(),
    };
    format!(
        "{} AI Limits {version}: panic in thread '{thread}' at {at}\n",
        now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    )
}

/// Append a line to the crash log, rotating it when it grew too big. Best
/// effort: a crash log that cannot be written must not add a second panic.
fn append_crash_line(path: &std::path::Path, line: &str) {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > CRASH_LOG_ROTATE_BYTES) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// The production build aborts on a panic (`panic = "abort"`) and has no
/// console, so without this a panic anywhere - a provider's parser, the
/// picker, a background thread - made the widget vanish without a trace,
/// even with `AILIMITS_LOG` on. Now every panic leaves one line in
/// `crash.log` (`crash_line`). The previous hook still runs after it.
fn install_crash_log(path: std::path::PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let location = info.location().map(|l| (l.file(), l.line(), l.column()));
        let line = crash_line(
            chrono::Utc::now(),
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
            location,
        );
        append_crash_line(&path, &line);
        previous(info);
    }));
}

/// The app could not start at all: say so, instead of a start that does
/// nothing. The reason goes to the crash log as well. Our own error text
/// (a window, a font, the runtime), never a file's content.
fn report_failed_start(e: &anyhow::Error) {
    append_crash_line(
        &crash_log_path(),
        &format!(
            "{} AI Limits {}: could not start: {e:#}\n",
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            env!("CARGO_PKG_VERSION")
        ),
    );
    #[cfg(target_os = "windows")]
    {
        use windows::core::HSTRING;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let text = HSTRING::from(format!(
            "AI Limits could not start.\n\n{e}\n\nDetails are in {}.",
            crash_log_path().display()
        ));
        unsafe {
            MessageBoxW(
                None,
                &text,
                &HSTRING::from("AI Limits"),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

fn main() -> Result<()> {
    #[cfg(target_os = "windows")]
    if another_instance_running() {
        // A second instance would mean two widgets and doubled requests.
        // It does not just vanish either: the running one is asked to show
        // its widget, so starting the app again visibly does something.
        let _ = ailimits::platform::wake_running_instance();
        return Ok(());
    }

    install_crash_log(crash_log_path());
    init_logging();
    info!(
        "AI Limits Widget v{} starting...",
        env!("CARGO_PKG_VERSION")
    );

    let result = app::run();
    if let Err(e) = &result {
        report_failed_start(e);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{crash_line, install_crash_log, short_source, RotatingLog};
    use std::io::Write;

    #[test]
    fn a_crash_line_names_the_place_and_never_the_message() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-16T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let line = crash_line(now, "0.7.1", "main", Some(("src\\ui\\picker.rs", 1292, 17)));
        assert_eq!(
            line,
            "2026-11-16T10:00:00Z AI Limits 0.7.1: panic in thread 'main' at src\\ui\\picker.rs:1292:17\n"
        );
    }

    #[test]
    fn a_dependencys_path_drops_the_builders_folders() {
        assert_eq!(
            short_source(
                r"C:\Users\someone\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\tao-0.37.0\src\window.rs"
            ),
            r"tao-0.37.0\src\window.rs"
        );
        assert_eq!(short_source("src\\app.rs"), "src\\app.rs");
        assert_eq!(
            short_source("/rustc/abc/library/core/src/str/mod.rs"),
            "/rustc/abc/library/core/src/str/mod.rs"
        );
    }

    /// A real panic with a token-like, Cyrillic message: the log gets the
    /// version and the place, and not one character of the message.
    #[test]
    fn the_hook_writes_the_place_of_a_panic_and_not_its_text() {
        let dir = std::env::temp_dir().join(format!("ailimits-crash-{}", std::process::id()));
        let path = dir.join("crash.log");
        let _ = std::fs::remove_dir_all(&dir);
        let before = std::panic::take_hook();
        install_crash_log(path.clone());
        let caught = std::thread::Builder::new()
            .name("crash-test".into())
            .spawn(|| {
                let secret = "секретний-токен sk-ant-oat01-SECRET";
                panic!("cut inside {secret}");
            })
            .unwrap()
            .join();
        // Restore the harness's hook before asserting anything.
        let _ = std::panic::take_hook();
        std::panic::set_hook(before);
        assert!(caught.is_err());
        let log = std::fs::read_to_string(&path).unwrap();
        assert!(log.contains(env!("CARGO_PKG_VERSION")), "{log}");
        assert!(log.contains("'crash-test'"), "{log}");
        assert!(log.contains("main.rs:"), "{log}");
        assert!(!log.contains("секрет"), "{log}");
        assert!(!log.contains("SECRET"), "{log}");
        assert!(!log.contains("cut inside"), "{log}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn log_rotates_once_past_the_limit_and_keeps_one_predecessor() {
        let dir = std::env::temp_dir().join(format!("ailimits-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ailimits.log");
        let old = dir.join("ailimits.log.1");

        let mut log = RotatingLog::open_with_limit(path.clone(), 20).unwrap();
        log.write_all(b"first line, 15b\n").unwrap(); // 16 bytes: fits
        log.write_all(b"second line\n").unwrap(); // would pass 20: rotate first
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "first line, 15b\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second line\n");

        log.write_all(b"third line, 9\n").unwrap(); // 12 + 14 > 20: rotate again
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "second line\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "third line, 9\n");

        // Reopening picks the size up from the file, not from zero.
        drop(log);
        let log = RotatingLog::open_with_limit(path.clone(), 20).unwrap();
        assert_eq!(log.size, 14);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
