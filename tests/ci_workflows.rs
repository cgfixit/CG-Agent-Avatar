//! Workflow contracts. YAML is config; these strings are the merge gate.

fn audit_yml() -> &'static str {
    include_str!("../.github/workflows/audit.yml")
}

#[test]
fn audit_does_not_use_rustsec_audit_check() {
    let y = audit_yml();
    assert!(
        !y.contains("rustsec/audit-check"),
        "audit-check cargo-installs unpinned cargo-audit against rust-toolchain.toml 1.88"
    );
}

#[test]
fn audit_installs_cargo_audit_locked_and_versioned() {
    let y = audit_yml();
    assert!(y.contains("cargo install cargo-audit --locked --version 0.22.2"));
    assert!(y.contains("RUSTUP_TOOLCHAIN"));
    assert!(y.contains("cargo audit"));
}

#[test]
fn audit_never_uses_pull_request_target() {
    // Comments may mention the forbidden trigger; the YAML key must not exist.
    assert!(!audit_yml().contains("pull_request_target:"));
}

fn bundle_yml() -> &'static str {
    include_str!("../.github/workflows/bundle.yml")
}

#[test]
fn bundle_has_noon_eastern_and_manual_dispatch() {
    let y = bundle_yml();
    assert!(y.contains("workflow_dispatch"));
    assert!(y.contains("timezone: \"America/New_York\""));
    assert!(y.contains("0 12 * * *"));
    assert!(y.contains("package-app.sh"));
    assert!(y.contains("ditto"));
    assert!(!y.contains("pull_request_target:"));
    assert!(!y.contains("cancel-in-progress: true"));
}

#[test]
fn bundle_release_is_not_on_push_only() {
    let y = bundle_yml();
    assert!(y.contains("github.event_name == 'schedule'"));
    assert!(y.contains("workflow_dispatch"));
    assert!(y.contains("contents: write"));
}

#[test]
fn bundle_pin_dtolnay_requires_toolchain_input() {
    let y = bundle_yml();
    assert!(
        y.contains("toolchain: 1.88"),
        "dtolnay/rust-toolchain@6c977a6 requires toolchain; omit fails with 'toolchain is a required input'"
    );
}

#[test]
fn bundle_release_job_has_git_and_repo() {
    let y = bundle_yml();
    assert!(
        y.matches("actions/checkout@").count() >= 2,
        "release job needs checkout; gh release create fails without a git repo"
    );
    assert!(y.contains("GH_REPO: ${{ github.repository }}"));
}

#[test]
fn bundle_release_runs_on_macos() {
    let y = bundle_yml();
    assert!(
        !y.contains("ubuntu-latest"),
        "release job must run on macos-latest, not ubuntu"
    );
    assert_eq!(y.matches("runs-on: macos-latest").count(), 2);
}

#[test]
fn builds_and_checks_reject_lockfile_drift() {
    for source in [
        include_str!("../.github/workflows/ci.yml"),
        include_str!("../scripts/ci.sh"),
        include_str!("../scripts/package-app.sh"),
    ] {
        for line in source.lines().filter(|line| {
            ["cargo build", "cargo test", "cargo clippy", "cargo deny"]
                .iter()
                .any(|command| line.contains(command))
        }) {
            assert!(
                line.contains("--locked"),
                "unlocked Cargo invocation: {line}"
            );
        }
    }
    assert!(
        include_str!("../.github/workflows/ci.yml").contains("arguments: --all-features --locked")
    );
}

#[test]
fn pull_requests_test_the_release_toolchain_on_macos() {
    let toolchain = include_str!("../rust-toolchain.toml")
        .lines()
        .find_map(|line| line.strip_prefix("channel = \""))
        .unwrap()
        .trim_end_matches('"');
    let ci = include_str!("../.github/workflows/ci.yml");
    let msrv = ci
        .split("  msrv:")
        .nth(1)
        .unwrap()
        .split("  deny:")
        .next()
        .unwrap();
    assert!(msrv.contains("runs-on: macos-latest"));
    assert!(msrv.contains(&format!("toolchain: {toolchain}")));
    assert!(msrv.contains("cargo test --locked --all-targets"));
}

#[test]
fn checkouts_never_persist_credentials() {
    // No job pushes with the checkout's token, so none may leave it in .git/config
    // while third-party actions and build scripts run.
    for (name, yml) in [
        ("ci.yml", include_str!("../.github/workflows/ci.yml")),
        ("audit.yml", audit_yml()),
        ("bundle.yml", bundle_yml()),
        (
            "gitleaks.yml",
            include_str!("../.github/workflows/gitleaks.yml"),
        ),
    ] {
        let steps: Vec<&str> = yml.split("- name:").collect();
        let checkouts: Vec<&&str> = steps
            .iter()
            .filter(|step| step.contains("uses: actions/checkout@"))
            .collect();
        assert!(!checkouts.is_empty(), "{name}: no checkout step found");
        for step in checkouts {
            assert!(
                step.contains("persist-credentials: false"),
                "{name}: checkout without persist-credentials: false:\n{step}"
            );
        }
    }
}

fn all_workflows() -> [(&'static str, &'static str); 4] {
    [
        ("ci.yml", include_str!("../.github/workflows/ci.yml")),
        ("audit.yml", audit_yml()),
        ("bundle.yml", bundle_yml()),
        (
            "gitleaks.yml",
            include_str!("../.github/workflows/gitleaks.yml"),
        ),
    ]
}

/// YAML with comment lines removed, so prose like "# contents: write only on
/// the release job" never satisfies or trips a contract.
fn yaml_without_comments(yml: &str) -> String {
    yml.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .map(|line| line.split(" # ").next().unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_action_is_pinned_to_a_full_commit_sha() {
    // A tag or branch ref can be moved by the action's maintainer (or an
    // attacker with their credentials); a 40-hex commit SHA cannot.
    for (name, yml) in all_workflows() {
        let mut pinned = 0;
        for line in yml.lines() {
            let Some(action) = line.trim_start().strip_prefix("uses: ") else {
                continue;
            };
            let (_, at) = action
                .split_once('@')
                .unwrap_or_else(|| panic!("{name}: action without a ref: {line}"));
            let sha = at.split_whitespace().next().unwrap_or_default();
            assert!(
                sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
                "{name}: not pinned to a full commit SHA: {line}"
            );
            assert!(
                at.contains("# v"),
                "{name}: SHA pin needs a trailing version comment for Dependabot and reviewers: {line}"
            );
            pinned += 1;
        }
        assert!(pinned > 0, "{name}: no actions found");
    }
}

#[test]
fn workflow_tokens_are_read_only_except_the_release_job() {
    for (name, yml) in all_workflows() {
        let body = yaml_without_comments(yml);
        assert!(
            body.contains("\npermissions:\n  contents: read\n"),
            "{name}: top-level permissions must be exactly contents: read"
        );
        assert!(
            !body.contains("pull_request_target"),
            "{name}: pull_request_target runs fork code with the base repo's token"
        );
        assert!(
            body.contains("\nconcurrency:\n"),
            "{name}: missing a concurrency group"
        );
        assert!(!body.contains("write-all"), "{name}: blanket write token");
        let writes = body.matches(": write").count();
        if name == "bundle.yml" {
            assert_eq!(
                writes, 1,
                "{name}: only the release job may write, and only contents"
            );
            assert!(body.contains("    permissions:\n      contents: write\n"));
        } else {
            assert_eq!(writes, 0, "{name}: no job may hold a write scope");
        }
    }
}

#[test]
fn stable_leg_really_runs_stable() {
    // rust-toolchain.toml outranks rustup's default toolchain, so installing
    // stable is not enough: without this override the matrix builds with 1.88.
    let ci = include_str!("../.github/workflows/ci.yml");
    let test_job = ci.split("  msrv:").next().unwrap();
    assert!(test_job.contains("toolchain: stable"));
    assert!(test_job.contains("RUSTUP_TOOLCHAIN: stable"));
}

#[test]
fn every_job_has_a_timeout() {
    for (name, yml) in all_workflows() {
        assert_eq!(
            yml.matches("runs-on:").count(),
            yml.matches("timeout-minutes:").count(),
            "{name}: every job needs timeout-minutes"
        );
    }
}

#[test]
fn caches_are_written_only_from_main() {
    for (name, yml) in all_workflows() {
        for step in yml
            .split("- name:")
            .filter(|step| step.contains("Swatinem/rust-cache@"))
        {
            assert!(
                step.contains("save-if: ${{ github.ref == 'refs/heads/main' }}"),
                "{name}: cache step may be written by PR runs:\n{step}"
            );
        }
    }
    assert!(
        !bundle_yml().contains("rust-cache"),
        "release builds stay uncached"
    );
}
