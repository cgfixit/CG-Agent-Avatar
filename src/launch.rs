//! Launch the bundled CG Agent Harness.app via macOS Launch Services when
//! the user selects Harness mode. Never spawns a bare process, never reads
//! or writes a filesystem path for it, and never sends arguments.
//!
//! This is the one deliberate exception to "does not start the harness":
//! see SECURITY.md's "Harness launch" row. It launches strictly by the
//! harness desktop app's own `CFBundleIdentifier`, resolved by Launch
//! Services regardless of install location, exactly like `open -a` or a
//! Dock click would. Relaunching an already-running instance activates it
//! rather than duplicating it (`NSWorkspaceOpenConfiguration`'s default
//! `createsNewApplicationInstance: false`), and the harness's own
//! home-scoped instance lock (its `desktop/src/instance.rs`) is a second,
//! independent line of defense against a duplicate.

/// The harness desktop app's own `CFBundleIdentifier`, verified against
/// `cg-agent-harness/desktop/Info.plist`. Do not hardcode a filesystem path
/// instead - Launch Services resolves this by identifier regardless of
/// where the app is installed.
pub const HARNESS_BUNDLE_ID: &str = "com.cgfixit.agent-harness";

/// Ask Launch Services to launch (or activate, if already running) the
/// harness desktop app. Returns `false` only when no app with
/// [`HARNESS_BUNDLE_ID`] is registered on this machine (not installed) -
/// callers should treat that as "nothing to launch," not an error to retry.
#[cfg(target_os = "macos")]
pub fn launch_harness_app() -> bool {
    use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
    use objc2_foundation::NSString;

    let workspace = NSWorkspace::sharedWorkspace();
    let bundle_id = NSString::from_str(HARNESS_BUNDLE_ID);
    let Some(url) = workspace.URLForApplicationWithBundleIdentifier(&bundle_id) else {
        return false;
    };
    let config = NSWorkspaceOpenConfiguration::configuration();
    workspace.openApplicationAtURL_configuration_completionHandler(&url, &config, None);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_id_matches_harness_desktop_app() {
        assert_eq!(HARNESS_BUNDLE_ID, "com.cgfixit.agent-harness");
    }
}
