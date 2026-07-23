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

/// Logging init: stderr is invisible in a GUI app, so AILIMITS_LOG or RUST_LOG writes next to the config (ініціалізація логування: stderr невидимий у GUI-застосунку, тому AILIMITS_LOG або RUST_LOG пише поруч із конфігом).
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
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            builder
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(file))
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
