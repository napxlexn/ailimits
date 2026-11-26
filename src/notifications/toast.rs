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
        if quiet(title, body, &[]) {
            return Ok(());
        }
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

    /// A toast with buttons, each `(label, action)`: `on_click` gets the
    /// action of the one clicked, on a thread of Windows' own, while the
    /// app runs. A click on the toast itself, not a button, does nothing.
    pub fn show_with_buttons(
        title: &str,
        body: &str,
        buttons: &[(&str, &str)],
        on_click: impl Fn(&str) + Send + 'static,
    ) -> Result<()> {
        if quiet(title, body, buttons) {
            return Ok(());
        }
        #[cfg(target_os = "windows")]
        {
            use tauri_winrt_notification::Toast;
            let mut toast = Toast::new(&crate::platform::toast_app_id())
                .title(title)
                .text1(body);
            for (label, action) in buttons {
                toast = toast.add_button(label, action);
            }
            toast
                .on_activated(move |action| {
                    if let Some(action) = action {
                        on_click(&action);
                    }
                    Ok(())
                })
                .show()
                .map_err(|e| anyhow::anyhow!("toast failed: {e}"))?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = on_click;
            tracing::info!("toast: {title} — {body}");
        }
        Ok(())
    }
}

/// Debug builds only: with `AILIMITS_E2E_QUIET_TOASTS` set, a toast goes to
/// the log instead of the screen (a test copy must not show one on the
/// owner's screen under the app's name). True when it was kept quiet.
fn quiet(title: &str, body: &str, buttons: &[(&str, &str)]) -> bool {
    #[cfg(debug_assertions)]
    if std::env::var_os("AILIMITS_E2E_QUIET_TOASTS").is_some() {
        let labels: Vec<&str> = buttons.iter().map(|(label, _)| *label).collect();
        tracing::info!("toast kept quiet (E2E): {title} | {body} | {labels:?}");
        return true;
    }
    let _ = (title, body, buttons);
    false
}

#[cfg(test)]
mod tests {
    /// Shows a real toast under the registered identity, for eyes:
    /// `cargo test preview_toast -- --ignored`. The header must read
    /// "AI Limits" with the app icon, not "Windows PowerShell".
    #[test]
    #[ignore = "shows a real toast"]
    fn preview_toast() {
        crate::platform::register_toast_identity();
        super::ToastNotifier::show("AI Limits", "Toast identity check").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
