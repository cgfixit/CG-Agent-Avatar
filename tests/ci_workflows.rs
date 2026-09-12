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
