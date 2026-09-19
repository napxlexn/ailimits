// notifications/toast.rs — Windows toast notifications.

use anyhow::Result;

/// Toast notification display.
pub struct ToastNotifier;

impl ToastNotifier {
    /// Show a toast under the app's own identity (see
    /// `platform::toast_app_id`): the header reads "AI Limits" with the
    /// app's icon, on every build. Registration of that identity is done
    /// once at startup by `platform::register_toast_identity`.
    pub fn show(title: &str, body: &str) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            use tauri_winrt_notification::Toast;
            Toast::new(&crate::platform::toast_app_id())
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

#[cfg(test)]
mod tests {
    /// Shows a real toast under the registered identity, for eyes:
    /// `cargo test preview_toast -- --ignored`. The header must read
    /// "AI Limits" with the app icon, not "Windows PowerShell".
    #[test]
    #[ignore]
    fn preview_toast() {
        crate::platform::register_toast_identity();
        super::ToastNotifier::show("AI Limits", "Toast identity check").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
