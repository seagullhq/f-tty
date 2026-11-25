//! Desktop notifications through the platform's own tool.

/// Fire and forget; a missing tool just means no popup.
pub(crate) fn send(title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("notify-send")
        .args(["--app-name=ftty", "--", title, body])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            "on run argv\ndisplay notification (item 2 of argv) with title (item 1 of argv)\nend run",
            title,
            body,
        ])
        .spawn();
    // ponytail: no Windows popup yet (needs a toast API); the tab dot still shows.
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = (title, body);
}
