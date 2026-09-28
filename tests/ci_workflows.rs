//! Workflow contracts. YAML is config; these strings are the merge gate.
//!
//! Files under `.github/workflows` are discovered at test time, not from a
//! fixed list. Action `uses` keys and `permissions` are read with a YAML
//! parser so an unnamed step or a quoted permission value cannot slip past a
//! single spelling of `uses: ` / `: write`.

use std::fs;
use std::path::{Path, PathBuf};

use yaml_rust2::yaml::Hash;
use yaml_rust2::{Yaml, YamlLoader};

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
    let workflows = all_workflows();
    for required in ["ci.yml", "audit.yml", "bundle.yml", "gitleaks.yml"] {
        assert!(
            workflows.iter().any(|(name, _)| name == required),
            "workflow discovery missed {required}"
        );
    }
    for (name, yml) in &workflows {
        let steps: Vec<&str> = yml.split("- name:").collect();
        let checkouts: Vec<&&str> = steps
            .iter()
            .filter(|step| step.contains("uses: actions/checkout@"))
            .collect();
        // These four workflows check out the repo. A later workflow file is
        // still scanned: every checkout it does have must drop credentials,
        // including an unnamed `- uses:` step the split above can miss.
        if ["ci.yml", "audit.yml", "bundle.yml", "gitleaks.yml"].contains(&name.as_str()) {
            assert!(!checkouts.is_empty(), "{name}: no checkout step found");
        }
        for step in checkouts {
            assert!(
                step.contains("persist-credentials: false"),
                "{name}: checkout without persist-credentials: false:\n{step}"
            );
        }
        assert_checkouts_do_not_persist_credentials(name, yml);
    }
}

fn workflow_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github/workflows")
}

fn is_workflow_file(path: &Path) -> bool {
    path.is_file()
        && matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yml" | "yaml")
        )
}

/// Every workflow file in `dir`, sorted by file name. `.yml` and `.yaml` both
/// count; GitHub Actions runs either extension.
fn load_workflows_from(dir: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let entries = fs::read_dir(dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()));
    for entry in entries {
        let path = entry.unwrap_or_else(|err| panic!("{err}")).path();
        if !is_workflow_file(&path) {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_else(|| panic!("workflow path has no name: {}", path.display()))
            .to_string_lossy()
            .into_owned();
        let body = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        found.push((name, body));
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

fn all_workflows() -> Vec<(String, String)> {
    let found = load_workflows_from(&workflow_dir());
    for required in ["audit.yml", "bundle.yml", "ci.yml", "gitleaks.yml"] {
        assert!(
            found.iter().any(|(name, _)| name == required),
            "workflow discovery missed {required}"
        );
    }
    found
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
            let Some(action) = uses_line_remainder(line) else {
                continue;
            };
            let (_, at) = action
                .split_once('@')
                .unwrap_or_else(|| panic!("{name}: action without a ref: {line}"));
            let sha = at
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches(['"', '\'', '}', ',']);
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
        // The line scan above now accepts `- uses:` as well as `uses:`. The
        // parser also sees job-level and flow-style `uses` keys.
        check_action_pins(&name, &yml).unwrap_or_else(|err| panic!("{err}"));
    }
}

#[test]
fn workflow_tokens_are_read_only_except_the_release_job() {
    for (name, yml) in all_workflows() {
        let body = yaml_without_comments(&yml);
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
        check_permissions(&name, &yml).unwrap_or_else(|err| panic!("{err}"));
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
        assert_caches_save_only_from_main(&name, &yml);
    }
    assert!(
        !bundle_yml().contains("rust-cache"),
        "release builds stay uncached"
    );
}

/// 40 hex digits, used by fixture workflows. Not a real commit.
const PINNED_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Remainder of a `uses:` line, including a trailing `# vX` comment.
///
/// Matches a named step (`uses: ...`), an unnamed step (`- uses: ...`), and a
/// one-line flow step (`- {uses: ...}`). Full-line comments are ignored.
fn uses_line_remainder(line: &str) -> Option<&str> {
    if line.trim_start().starts_with('#') {
        return None;
    }
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("- ")
        .map(str::trim_start)
        .unwrap_or(trimmed);
    let rest = rest.trim_start_matches('{').trim_start();
    rest.strip_prefix("uses:")
        .map(str::trim_start)
        .filter(|value| !value.is_empty())
}

fn action_token(remainder: &str) -> String {
    let code = remainder.split('#').next().unwrap_or(remainder).trim();
    code.trim_matches(|c| matches!(c, '"' | '\'' | '{' | '}' | ','))
        .to_owned()
}

fn line_pins_action(line: &str, action: &str) -> bool {
    let Some(remainder) = uses_line_remainder(line) else {
        return false;
    };
    action_token(remainder) == action && remainder.contains("# v")
}

fn sha_pin_error(action: &str) -> Option<&'static str> {
    let Some((repo, sha)) = action.split_once('@') else {
        return Some("action without a ref");
    };
    if repo.is_empty()
        || repo.chars().any(char::is_whitespace)
        || sha.len() != 40
        || !sha.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Some("not pinned to a full commit SHA");
    }
    None
}

fn hash_get<'a>(map: &'a Hash, key: &str) -> Option<&'a Yaml> {
    map.get(&Yaml::String(key.to_owned()))
}

fn parse_workflow(name: &str, yml: &str) -> Result<Yaml, String> {
    let mut docs = YamlLoader::load_from_str(yml).map_err(|err| format!("{name}: {err}"))?;
    if docs.len() != 1 {
        return Err(format!(
            "{name}: expected one YAML document, found {}",
            docs.len()
        ));
    }
    let doc = docs.pop().unwrap();
    if !doc.is_hash() {
        return Err(format!("{name}: workflow root must be a mapping"));
    }
    reject_hidden_permissions(name, &doc)?;
    Ok(doc)
}

fn jobs(doc: &Yaml) -> Result<Vec<(String, &Yaml)>, String> {
    let root = doc
        .as_hash()
        .ok_or_else(|| "workflow root must be a mapping".to_owned())?;
    let jobs = hash_get(root, "jobs").ok_or_else(|| "workflow is missing jobs".to_owned())?;
    let jobs = jobs
        .as_hash()
        .ok_or_else(|| "jobs must be a mapping".to_owned())?;
    let mut out = Vec::new();
    for (key, value) in jobs {
        let name = key
            .as_str()
            .ok_or_else(|| format!("job name must be a string, got {key:?}"))?
            .to_owned();
        if !value.is_hash() {
            return Err(format!("job {name} must be a mapping"));
        }
        out.push((name, value));
    }
    Ok(out)
}

fn yaml_string(value: &Yaml, what: &str) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{what} must be a string, got {value:?}"))
}

fn action_uses(doc: &Yaml) -> Result<Vec<String>, String> {
    let mut uses = Vec::new();
    for (job_name, job) in jobs(doc)? {
        let map = job
            .as_hash()
            .ok_or_else(|| format!("job {job_name} must be a mapping"))?;
        if let Some(value) = hash_get(map, "uses") {
            uses.push(yaml_string(value, &format!("job {job_name} uses"))?);
        }
        if let Some(steps) = hash_get(map, "steps") {
            let steps = steps
                .as_vec()
                .ok_or_else(|| format!("job {job_name} steps must be a sequence"))?;
            for (index, step) in steps.iter().enumerate() {
                let step = step.as_hash().ok_or_else(|| {
                    format!("job {job_name} step {index} must be a mapping, got {step:?}")
                })?;
                if let Some(value) = hash_get(step, "uses") {
                    uses.push(yaml_string(
                        value,
                        &format!("job {job_name} step {index} uses"),
                    )?);
                }
            }
        }
    }
    Ok(uses)
}

fn check_action_pins(name: &str, yml: &str) -> Result<(), String> {
    let doc = parse_workflow(name, yml)?;
    let uses = action_uses(&doc)?;
    if uses.is_empty() {
        return Err(format!("{name}: no actions found"));
    }
    for action in &uses {
        if let Some(err) = sha_pin_error(action) {
            return Err(format!("{name}: {err}: {action}"));
        }
        if !yml.lines().any(|line| line_pins_action(line, action)) {
            return Err(format!(
                "{name}: SHA pin needs a trailing version comment for Dependabot and reviewers: {action}"
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Access {
    Read,
    Write,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Permissions {
    ReadAll,
    WriteAll,
    Scopes(Vec<(String, Access)>),
}

fn parse_access(scope: &str, value: &Yaml) -> Result<Access, String> {
    let Some(text) = value.as_str() else {
        return Err(format!(
            "permission value for {scope} must be a string, got {value:?}"
        ));
    };
    match text {
        "read" => Ok(Access::Read),
        "write" => Ok(Access::Write),
        "none" => Ok(Access::None),
        other => Err(format!(
            "unsupported permission value {other:?} for {scope}"
        )),
    }
}

fn parse_permissions(value: &Yaml) -> Result<Permissions, String> {
    match value {
        Yaml::String(text) => match text.as_str() {
            "read-all" => Ok(Permissions::ReadAll),
            "write-all" => Ok(Permissions::WriteAll),
            other => Err(format!("unsupported permissions scalar {other:?}")),
        },
        Yaml::Hash(map) => {
            let mut scopes = Vec::new();
            for (key, item) in map {
                let scope = key
                    .as_str()
                    .ok_or_else(|| format!("permission scope must be a string, got {key:?}"))?;
                scopes.push((scope.to_owned(), parse_access(scope, item)?));
            }
            Ok(Permissions::Scopes(scopes))
        }
        other => Err(format!(
            "permissions must be a mapping or read-all/write-all, got {other:?}"
        )),
    }
}

fn is_exact_contents(perms: &Permissions, access: Access) -> bool {
    matches!(
        perms,
        Permissions::Scopes(scopes)
            if scopes.len() == 1 && scopes[0].0 == "contents" && scopes[0].1 == access
    )
}

fn require_contents_read(file: &str, perms: &Permissions) -> Result<(), String> {
    match perms {
        Permissions::ReadAll => Err(format!(
            "{file}: top-level permissions read-all expands the token beyond contents: read"
        )),
        Permissions::WriteAll => Err(format!(
            "{file}: top-level permissions write-all is a blanket write token"
        )),
        Permissions::Scopes(scopes)
            if scopes.len() == 1 && scopes[0].0 == "contents" && scopes[0].1 == Access::Read =>
        {
            Ok(())
        }
        other => Err(format!(
            "{file}: top-level permissions must be exactly contents: read, got {other:?}"
        )),
    }
}

fn require_job_read_only(file: &str, job: &str, perms: &Permissions) -> Result<(), String> {
    match perms {
        Permissions::ReadAll => Err(format!(
            "{file}: job {job} permissions read-all expands the token beyond contents: read"
        )),
        Permissions::WriteAll => Err(format!(
            "{file}: job {job} permissions write-all is a blanket write token"
        )),
        Permissions::Scopes(scopes)
            if scopes.is_empty()
                || (scopes.len() == 1
                    && scopes[0].0 == "contents"
                    && matches!(scopes[0].1, Access::Read | Access::None)) =>
        {
            Ok(())
        }
        other => Err(format!(
            "{file}: job {job} permissions expand beyond contents: read: {other:?}"
        )),
    }
}

fn check_permissions(name: &str, yml: &str) -> Result<(), String> {
    let doc = parse_workflow(name, yml)?;
    let root = doc
        .as_hash()
        .ok_or_else(|| format!("{name}: workflow root must be a mapping"))?;
    let top = hash_get(root, "permissions")
        .ok_or_else(|| format!("{name}: missing top-level permissions"))?;
    require_contents_read(name, &parse_permissions(top)?)?;

    let mut release_is_contents_write = false;
    for (job_name, job) in jobs(&doc)? {
        let map = job
            .as_hash()
            .ok_or_else(|| format!("{name}: job {job_name} must be a mapping"))?;
        let Some(node) = hash_get(map, "permissions") else {
            continue;
        };
        let perms = parse_permissions(node)?;
        if name == "bundle.yml" && job_name == "release" {
            if is_exact_contents(&perms, Access::Write) {
                release_is_contents_write = true;
            } else {
                return Err(format!(
                    "{name}: release job must set permissions to exactly contents: write, got {perms:?}"
                ));
            }
        } else if is_exact_contents(&perms, Access::Write) {
            return Err(format!(
                "{name}: only bundle.yml job release may set contents: write (found on job {job_name})"
            ));
        } else {
            require_job_read_only(name, &job_name, &perms)?;
        }
    }
    if name == "bundle.yml" && !release_is_contents_write {
        return Err(format!(
            "{name}: release job must set permissions to exactly contents: write"
        ));
    }
    Ok(())
}

fn allowed_permissions_path(path: &[String]) -> bool {
    path.is_empty() || (path.len() == 2 && path[0] == "jobs")
}

fn reject_hidden_permissions(name: &str, doc: &Yaml) -> Result<(), String> {
    fn walk(name: &str, node: &Yaml, path: &[String]) -> Result<(), String> {
        match node {
            Yaml::Hash(map) => {
                for (key, value) in map {
                    let key = key.as_str().ok_or_else(|| {
                        format!("{name}: non-string YAML key under {}", path.join("."))
                    })?;
                    if key == "<<" {
                        return Err(format!(
                            "{name}: YAML merge key is not expanded under {}",
                            path.join(".")
                        ));
                    }
                    if key == "permissions" && !allowed_permissions_path(path) {
                        return Err(format!(
                            "{name}: permissions key outside workflow or job level at {}.permissions",
                            path.join(".")
                        ));
                    }
                    let mut child = path.to_vec();
                    child.push(key.to_owned());
                    walk(name, value, &child)?;
                }
                Ok(())
            }
            Yaml::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    let mut child = path.to_vec();
                    child.push(index.to_string());
                    walk(name, item, &child)?;
                }
                Ok(())
            }
            Yaml::Alias(_) | Yaml::BadValue => Err(format!(
                "{name}: unresolved YAML alias or invalid value under {}",
                path.join(".")
            )),
            _ => Ok(()),
        }
    }
    walk(name, doc, &[])
}

fn assert_checkouts_do_not_persist_credentials(name: &str, yml: &str) {
    let doc = parse_workflow(name, yml).unwrap_or_else(|err| panic!("{err}"));
    for (job_name, job) in jobs(&doc).unwrap_or_else(|err| panic!("{name}: {err}")) {
        let Some(steps) = job
            .as_hash()
            .and_then(|map| hash_get(map, "steps"))
            .and_then(Yaml::as_vec)
        else {
            continue;
        };
        for step in steps {
            let Some(uses) = step
                .as_hash()
                .and_then(|map| hash_get(map, "uses"))
                .and_then(Yaml::as_str)
            else {
                continue;
            };
            if !uses.starts_with("actions/checkout@") {
                continue;
            }
            let flag = step
                .as_hash()
                .and_then(|map| hash_get(map, "with"))
                .and_then(Yaml::as_hash)
                .and_then(|with| hash_get(with, "persist-credentials"));
            assert!(
                matches!(flag, Some(Yaml::Boolean(false))),
                "{name}: job {job_name} checkout {uses} must set persist-credentials: false, got {flag:?}"
            );
        }
    }
}

fn assert_caches_save_only_from_main(name: &str, yml: &str) {
    let doc = parse_workflow(name, yml).unwrap_or_else(|err| panic!("{err}"));
    for (job_name, job) in jobs(&doc).unwrap_or_else(|err| panic!("{name}: {err}")) {
        let Some(steps) = job
            .as_hash()
            .and_then(|map| hash_get(map, "steps"))
            .and_then(Yaml::as_vec)
        else {
            continue;
        };
        for step in steps {
            let Some(uses) = step
                .as_hash()
                .and_then(|map| hash_get(map, "uses"))
                .and_then(Yaml::as_str)
            else {
                continue;
            };
            if !uses.starts_with("Swatinem/rust-cache@") {
                continue;
            }
            let save_if = step
                .as_hash()
                .and_then(|map| hash_get(map, "with"))
                .and_then(Yaml::as_hash)
                .and_then(|with| hash_get(with, "save-if"))
                .and_then(Yaml::as_str);
            assert_eq!(
                save_if,
                Some("${{ github.ref == 'refs/heads/main' }}"),
                "{name}: job {job_name} cache step may be written by PR runs"
            );
        }
    }
}

#[test]
fn workflow_discovery_matches_the_directory() {
    let mut listed = Vec::new();
    for entry in fs::read_dir(workflow_dir()).unwrap() {
        let path = entry.unwrap().path();
        if is_workflow_file(&path) {
            listed.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    listed.sort();
    let mut found: Vec<_> = all_workflows().into_iter().map(|(name, _)| name).collect();
    found.sort();
    assert_eq!(found, listed);
    assert!(found.len() >= 4, "discovery returned {found:?}");
}

#[test]
fn workflow_files_outside_the_old_list_are_discovered_and_pin_checked() {
    let dir = tempfile::tempdir().unwrap();
    let pinned = format!(
        "name: ok\non: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{PINNED_SHA} # v1\n"
    );
    let unpinned = "\
name: bad
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
";
    fs::write(dir.path().join("brand-new.yml"), unpinned).unwrap();
    fs::write(dir.path().join("also-new.yaml"), pinned).unwrap();
    fs::write(dir.path().join("notes.txt"), "not a workflow").unwrap();

    let loaded = load_workflows_from(dir.path());
    let names: Vec<_> = loaded.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        vec!["also-new.yaml", "brand-new.yml"],
        "a new workflow file must be loaded, and a non-workflow must not"
    );
    let bad = loaded
        .iter()
        .find(|(name, _)| name == "brand-new.yml")
        .unwrap();
    let err = check_action_pins(&bad.0, &bad.1).unwrap_err();
    assert!(err.contains("not pinned to a full commit SHA"), "{err}");
    let good = loaded
        .iter()
        .find(|(name, _)| name == "also-new.yaml")
        .unwrap();
    check_action_pins(&good.0, &good.1).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn unnamed_unpinned_uses_step_is_rejected() {
    let yml = format!(
        "\
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@{PINNED_SHA} # v1
      - uses: actions/checkout@v4
"
    );
    let legacy_prefix_hits = yml
        .lines()
        .filter(|line| line.trim_start().starts_with("uses: "))
        .count();
    assert_eq!(
        legacy_prefix_hits, 1,
        "a line scan of only `uses: ` misses the unnamed step"
    );
    let err = check_action_pins("ci.yml", &yml).unwrap_err();
    assert!(err.contains("not pinned to a full commit SHA"), "{err}");
    assert!(err.contains("actions/checkout@v4"), "{err}");
}

#[test]
fn unnamed_sha_pinned_uses_step_is_accepted() {
    let yml = format!(
        "\
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@{PINNED_SHA} # v4.0.0
"
    );
    check_action_pins("ci.yml", &yml).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn unnamed_sha_pin_without_version_comment_is_rejected() {
    let yml = format!(
        "\
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@{PINNED_SHA}
"
    );
    let err = check_action_pins("ci.yml", &yml).unwrap_err();
    assert!(
        err.contains("SHA pin needs a trailing version comment"),
        "{err}"
    );
}

#[test]
fn contents_write_on_a_non_release_job_is_rejected() {
    // Same indentation the old substring contract accepts. The write belongs
    // to `package`, not `release`.
    let yml = "\
name: bundle
on: workflow_dispatch
permissions:
  contents: read
jobs:
  package:
    runs-on: macos-latest
    permissions:
      contents: write
    steps:
      - run: echo package
  release:
    runs-on: macos-latest
    permissions:
      contents: read
    steps:
      - run: echo release
";
    let body = yaml_without_comments(yml);
    assert_eq!(body.matches(": write").count(), 1);
    assert!(body.contains("    permissions:\n      contents: write\n"));
    let err = check_permissions("bundle.yml", yml).unwrap_err();
    assert!(
        err.contains("only bundle.yml job release may set contents: write"),
        "{err}"
    );
    assert!(err.contains("package"), "{err}");
}

#[test]
fn contents_write_on_release_in_another_workflow_is_rejected() {
    let yml = "\
permissions:
  contents: read
jobs:
  release:
    permissions:
      contents: write
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let err = check_permissions("ci.yml", yml).unwrap_err();
    assert!(
        err.contains("only bundle.yml job release may set contents: write"),
        "{err}"
    );
}

#[test]
fn job_level_read_all_is_rejected() {
    let yml = "\
name: fixture
permissions:
  contents: read
jobs:
  test:
    permissions: read-all
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let body = yaml_without_comments(yml);
    assert!(body.contains("\npermissions:\n  contents: read\n"));
    assert!(!body.contains("write-all"), "{body}");
    assert_eq!(body.matches(": write").count(), 0, "{body}");
    let err = check_permissions("ci.yml", yml).unwrap_err();
    assert!(err.contains("read-all"), "{err}");
}

#[test]
fn top_level_read_all_scalar_is_rejected() {
    let yml = "\
permissions: read-all
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let err = check_permissions("ci.yml", yml).unwrap_err();
    assert!(err.contains("read-all"), "{err}");
}

#[test]
fn quoted_write_all_scalar_is_rejected() {
    let yml = "\
permissions:
  contents: read
jobs:
  test:
    permissions: \"write-all\"
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let err = check_permissions("ci.yml", yml).unwrap_err();
    assert!(err.contains("write-all"), "{err}");
}

#[test]
fn quoted_issues_write_is_rejected() {
    let yml = "\
permissions:
  contents: read
jobs:
  test:
    permissions:
      issues: \"write\"
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let body = yaml_without_comments(yml);
    assert_eq!(
        body.matches(": write").count(),
        0,
        "quoted write does not match the literal `: write` spelling:\n{body}"
    );
    assert!(!body.contains("write-all"));
    let err = check_permissions("ci.yml", yml).unwrap_err();
    assert!(err.contains("issues"), "{err}");
    assert!(err.contains("Write"), "{err}");
}

#[test]
fn single_quoted_contents_write_on_non_release_job_is_rejected() {
    let yml = "\
permissions:
  contents: read
jobs:
  package:
    permissions:
      contents: 'write'
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let body = yaml_without_comments(yml);
    assert_eq!(body.matches(": write").count(), 0, "{body}");
    let err = check_permissions("ci.yml", yml).unwrap_err();
    assert!(
        err.contains("only bundle.yml job release may set contents: write"),
        "{err}"
    );
    assert!(err.contains("package"), "{err}");
}

#[test]
fn flow_mapping_contents_write_on_non_release_job_is_rejected() {
    let yml = "\
permissions:
  contents: read
jobs:
  package:
    permissions: {contents: write}
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let err = check_permissions("bundle.yml", yml).unwrap_err();
    assert!(
        err.contains("only bundle.yml job release may set contents: write"),
        "{err}"
    );
    assert!(err.contains("package"), "{err}");
}

#[test]
fn extra_read_scope_is_rejected() {
    let yml = "\
permissions:
  contents: read
jobs:
  test:
    permissions:
      contents: read
      pull-requests: read
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    let err = check_permissions("ci.yml", yml).unwrap_err();
    assert!(err.contains("pull-requests"), "{err}");
    assert!(err.contains("expand beyond contents: read"), "{err}");
}

#[test]
fn quoted_contents_read_and_release_write_are_accepted() {
    let yml = "\
name: bundle
on: workflow_dispatch
permissions:
  contents: \"read\"
jobs:
  package:
    permissions:
      contents: \"read\"
    runs-on: macos-latest
    steps:
      - run: echo package
  release:
    permissions: {contents: \"write\"}
    runs-on: macos-latest
    steps:
      - run: echo release
";
    check_permissions("bundle.yml", yml).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn job_level_contents_none_and_empty_permissions_stay_allowed() {
    let yml = "\
permissions:
  contents: read
jobs:
  tightened:
    permissions:
      contents: none
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
  empty:
    permissions: {}
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
";
    check_permissions("ci.yml", yml).unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn permissions_nested_under_a_step_are_rejected() {
    let yml = "\
permissions:
  contents: read
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - name: hidden
        permissions:
          contents: write
        run: echo hi
";
    let err = check_permissions("ci.yml", yml).unwrap_err();
    assert!(
        err.contains("permissions key outside workflow or job level"),
        "{err}"
    );
}
