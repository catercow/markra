//! Per-launch secrets for the local preview server and control connection.

use std::fs;
use std::io::Write;
use std::path::Path;

/// 32 random bytes as lowercase hex.
pub fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Compares without stopping at the first difference, so timing doesn't reveal a prefix.
pub fn matches(expected: &str, given: &str) -> bool {
    let (expected, given) = (expected.as_bytes(), given.as_bytes());
    if expected.len() != given.len() {
        return false;
    }
    expected.iter().zip(given).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0
}

/// Writes `text` to `path` readable only by this user (0600 on Unix), replacing
/// any existing file without ever exposing the new contents with wider permissions.
pub fn write_private(path: &Path, text: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("no parent directory")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("secret"),
        std::process::id()
    ));
    let _ = fs::remove_file(&temp);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|error| error.to_string())?;
    file.write_all(text.as_bytes()).map_err(|error| error.to_string())?;
    drop(file);
    fs::rename(&temp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_hex() {
        let (a, b) = (random_token().unwrap(), random_token().unwrap());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(matches(&a, &a.clone()));
        assert!(!matches(&a, &b));
        assert!(!matches(&a, ""));
    }
}
