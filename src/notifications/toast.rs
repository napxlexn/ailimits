// notifications/toast.rs — Windows toast notifications.

use anyhow::Result;

/// Toast notification display.
pub struct ToastNotifier;

impl ToastNotifier {
    /// Show a toast.
    pub fn show(title: &str, body: &str) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            use tauri_winrt_notification::Toast;
            Toast::new(Toast::POWERSHELL_APP_ID)
                .title(title)
                .text1(body)
                .show()
                .map_err(|e| anyhow::anyhow!("toast failed: {e}"))?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Other OSes: log only.
            tracing::info!("toast: {title} — {body}");
        }
        Ok(())
    }
}
