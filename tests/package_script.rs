#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn packages_the_reported_artifact_and_preserves_bundle_on_build_failure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for name in [
        "scripts",
        "resources",
        "assets",
        "tools",
        "external target/release",
    ] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    fs::write(
        root.join("scripts/package-app.sh"),
        include_str!("../scripts/package-app.sh"),
    )
    .unwrap();
    fs::write(root.join("resources/Info.plist"), "plist").unwrap();
    fs::write(root.join("assets/creature.png"), "image").unwrap();
    let executable = root.join("external target/release/cg-agent");
    fs::write(&executable, "current binary").unwrap();
    fs::write(
        root.join("artifact.json"),
        serde_json::to_string(&serde_json::json!({
            "reason": "compiler-artifact", "target": {"name": "cg-agent", "kind": ["bin"]},
            "executable": executable
        }))
        .unwrap(),
    )
    .unwrap();
    for (name, script) in [
        (
            "cargo",
            "#!/bin/sh\n[ -z \"$BUILD_FAIL\" ] || exit 1\ncat artifact.json\n",
        ),
        ("codesign", "#!/bin/sh\nexit 0\n"),
    ] {
        let path = root.join("tools").join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let run = |fail: bool| {
        Command::new("bash")
            .arg(root.join("scripts/package-app.sh"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    root.join("tools").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("BUILD_FAIL", if fail { "1" } else { "" })
            .output()
            .unwrap()
    };
    let success = run(false);
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    let bundled = root.join("dist/CG-Agent-MacOS-Avatar.app/Contents/MacOS/cg-agent");
    assert_eq!(fs::read_to_string(&bundled).unwrap(), "current binary");
    fs::write(&bundled, "previous good bundle").unwrap();
    assert!(!run(true).status.success());
    assert_eq!(
        fs::read_to_string(&bundled).unwrap(),
        "previous good bundle"
    );
    fs::write(
        root.join("artifact.json"),
        "{\"reason\":\"build-finished\",\"success\":true}\n",
    )
    .unwrap();
    assert!(!run(false).status.success());
    assert_eq!(
        fs::read_to_string(&bundled).unwrap(),
        "previous good bundle"
    );
}
