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
