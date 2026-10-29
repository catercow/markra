#[cfg(not(target_os = "windows"))]
use std::process::Command;

/// A NUL-terminated UTF-16 copy of `text` for Win32 calls.
#[cfg(any(target_os = "windows", test))]
fn wide_null(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn validate_external_url(url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| "Invalid external URL".to_string())?;

    match parsed.scheme() {
        "http" | "https" | "mailto" => Ok(parsed.to_string()),
        _ => Err("Unsupported external URL scheme".to_string()),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn open_system_url(url: &str) -> Result<(), String> {
    Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to open external URL: {error}"))
}

// ShellExecuteW hands the URL to the default browser as-is. `cmd /C start` would
// treat `&` in a query string as a command separator and drop the rest.
#[cfg(target_os = "windows")]
pub(crate) fn open_system_url(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation = wide_null("open");
    let file = wide_null(url);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    let code = result as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(format!("Failed to open external URL (ShellExecute error {code})"))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn open_system_url(url: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to open external URL: {error}"))
}

#[tauri::command]
pub(crate) fn open_external_url(url: String) -> Result<(), String> {
    let external_url = validate_external_url(&url)?;
    open_system_url(&external_url)
}

#[cfg(test)]
mod tests {
    use super::{validate_external_url, wide_null};

    #[test]
    fn accepts_browser_and_mail_urls() {
        assert_eq!(
            validate_external_url("https://example.com/path").unwrap(),
            "https://example.com/path"
        );
        assert_eq!(
            validate_external_url("mailto:hello@example.com").unwrap(),
            "mailto:hello@example.com"
        );
    }

    #[test]
    fn rejects_non_external_url_schemes() {
        assert!(validate_external_url("javascript:alert(1)").is_err());
        assert!(validate_external_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn windows_url_keeps_every_query_parameter() {
        let url = "https://content-preview.catercow.com/__content_access/login?port=51234&challenge=abc";
        let wide = wide_null(url);
        assert_eq!(wide.last(), Some(&0));
        assert_eq!(String::from_utf16(&wide[..wide.len() - 1]).unwrap(), url);
    }
}
