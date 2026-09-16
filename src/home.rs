//! Read the harness home port. Never reads dotenv or API-key files.

use std::path::Path;

pub const DEFAULT_PORT: u16 = 8790;
pub const HOME_DIRNAME: &str = ".CGagentHarness";
pub const HOME_ENV: &str = "CGAGENTHARNESS_HOME";

pub fn default_home() -> std::path::PathBuf {
    if let Ok(override_home) = std::env::var(HOME_ENV) {
        let p = std::path::PathBuf::from(&override_home);
        if is_safe_home(&p) {
            return p;
        }
    }
    dirs_home().join(HOME_DIRNAME)
}

fn is_safe_home(p: &Path) -> bool {
    if !p.is_absolute() {
        return false;
    }
    if p.as_os_str().is_empty() {
        return false;
    }
    for c in p.components() {
        if matches!(c, std::path::Component::ParentDir) {
            return false;
        }
    }
    true
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/"))
}

const MAX_CERT_BYTES: u64 = 64 * 1024;

/// Read the harness's own pinned leaf certificate (`tls/server.pem`) for
/// HTTPS connections. Fresh homes generate and persist this file per
/// cg-agent-harness's `docs/SECURE_RESEARCH.md`; there is no other source of
/// trust — this app never talks to the harness's tls admin CLI and never
/// changes system/keychain trust. Same hardening as `port_from_home`:
/// reject symlinks, world-writable files, and oversized files; only this one
/// fixed path is ever read, never a dotenv-style config file.
pub fn read_pinned_cert(home: &Path) -> Option<Vec<u8>> {
    let path = home.join("tls").join("server.pem");
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_file() {
        return None;
    }
    if meta.len() == 0 || meta.len() > MAX_CERT_BYTES {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o002 != 0 {
            return None;
        }
    }
    let bytes = std::fs::read(&path).ok()?;
    if bytes.len() as u64 != meta.len() {
        return None;
    }
    Some(bytes)
}

pub fn port_from_home(home: &Path) -> u16 {
    let path = home.join("harness.json");
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return DEFAULT_PORT;
    };
    if !meta.file_type().is_file() {
        return DEFAULT_PORT;
    }
    if meta.len() > 64 * 1024 {
        return DEFAULT_PORT;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o002 != 0 {
            return DEFAULT_PORT;
        }
    }
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return DEFAULT_PORT;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return DEFAULT_PORT;
    };
    match v.get("port").and_then(|p| p.as_u64()) {
        Some(p) if (1024..=65535).contains(&p) => p as u16,
        _ => DEFAULT_PORT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(port_from_home(dir.path()), 8790);
    }

    #[test]
    fn reads_loopback_range_port() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("harness.json"), r#"{"port": 9001}"#).unwrap();
        assert_eq!(port_from_home(dir.path()), 9001);
    }

    #[test]
    fn junk_or_out_of_range_is_default() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("harness.json"), "nope").unwrap();
        assert_eq!(port_from_home(dir.path()), 8790);
        fs::write(dir.path().join("harness.json"), r#"{"port": 80}"#).unwrap();
        assert_eq!(port_from_home(dir.path()), 8790);
    }

    #[test]
    fn env_file_is_never_consulted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".env"),
            "CGAGENTHARNESS_API_KEY=EXAMPLEONLY_sk_live\nport=9999\n",
        )
        .unwrap();
        assert_eq!(port_from_home(dir.path()), 8790);
    }

    #[test]
    fn symlink_harness_json_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("elsewhere.json");
        fs::write(&target, r#"{"port": 9001}"#).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, dir.path().join("harness.json")).unwrap();
            assert_eq!(port_from_home(dir.path()), 8790);
        }
    }

    #[test]
    fn missing_cert_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_pinned_cert(dir.path()).is_none());
    }

    #[test]
    fn reads_valid_pinned_cert() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tls")).unwrap();
        fs::write(
            dir.path().join("tls").join("server.pem"),
            "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let bytes = read_pinned_cert(dir.path()).unwrap();
        assert!(std::str::from_utf8(&bytes)
            .unwrap()
            .contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn oversized_cert_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tls")).unwrap();
        fs::write(
            dir.path().join("tls").join("server.pem"),
            "x".repeat(MAX_CERT_BYTES as usize + 1),
        )
        .unwrap();
        assert!(read_pinned_cert(dir.path()).is_none());
    }

    #[test]
    fn symlink_cert_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tls")).unwrap();
        let target = dir.path().join("elsewhere.pem");
        fs::write(&target, "cert bytes").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, dir.path().join("tls").join("server.pem")).unwrap();
            assert!(read_pinned_cert(dir.path()).is_none());
        }
    }

    #[test]
    fn relative_home_override_is_ignored() {
        assert!(!is_safe_home(Path::new("relative/home")));
        assert!(!is_safe_home(Path::new("/tmp/../etc")));
        assert!(is_safe_home(Path::new("/Users/example/.CGagentHarness")));
    }
}
