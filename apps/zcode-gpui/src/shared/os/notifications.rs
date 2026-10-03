//! Task-completion OS notifications.
//! Suppressed while the main window is focused. On Windows, sends a toast notification
//! via PowerShell background dispatch.

#![allow(dead_code)]

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command;

pub fn notify_task_completed(title: &str, body: &str, is_window_focused: bool) {
    if is_window_focused {
        // Notification suppressed while user is actively focused on the window
        return;
    }

    let title_owned = title.replace('"', "\\\"").replace('\'', "''");
    let body_owned = body.replace('"', "\\\"").replace('\'', "''");

    #[cfg(windows)]
    std::thread::spawn(move || {
        let script = format!(
            r#"[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null;
$template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);
$textNodes = $template.GetElementsByTagName('text');
$textNodes.Item(0).AppendChild($template.CreateTextNode('{title_owned}')) > $null;
$textNodes.Item(1).AppendChild($template.CreateTextNode('{body_owned}')) > $null;
$toast = [Windows.UI.Notifications.ToastNotification]::new($template);
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('ZCode GPUI').Show($toast);"#
        );

        let _ = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn();
    });

    #[cfg(not(windows))]
    {
        let _ = (title_owned, body_owned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suppression_when_focused() {
        // Must not panic or spawn when focused
        notify_task_completed("Task Done", "Your command succeeded", true);
    }
}
