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

fn main() -> Result<()> {
    #[cfg(target_os = "windows")]
    if another_instance_running() {
        // A second instance exits silently — otherwise two widgets
        // and doubled requests.
        return Ok(());
    }

    init_logging();
    info!(
        "AI Limits Widget v{} starting...",
        env!("CARGO_PKG_VERSION")
    );

    app::run()
}

#[cfg(test)]
mod tests {
    use super::RotatingLog;
    use std::io::Write;

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
