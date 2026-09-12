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
    fn relative_home_override_is_ignored() {
        assert!(!is_safe_home(Path::new("relative/home")));
        assert!(!is_safe_home(Path::new("/tmp/../etc")));
        assert!(is_safe_home(Path::new("/Users/example/.CGagentHarness")));
    }
}
