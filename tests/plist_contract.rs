//! Darwin bundle contract: ATS local-only, accessory extra, no arbitrary loads.

const PLIST: &str = include_str!("../resources/Info.plist");

fn assert_value(key: &str, value: &str) {
    let key_xml = format!("<key>{key}</key>");
    assert_eq!(PLIST.matches(&key_xml).count(), 1, "unique {key}");
    let (_, after) = PLIST.split_once(&key_xml).unwrap();
    assert!(
        after.trim_start().starts_with(value),
        "{key} must be {value}"
    );
}

#[test]
fn bundle_identity() {
    assert_value(
        "CFBundleIdentifier",
        "<string>com.cgfixit.cg-agent</string>",
    );
    assert_value("CFBundleExecutable", "<string>cg-agent</string>");
    assert_value("CFBundleName", "<string>CG-Agent-MacOS-Avatar</string>");
    assert_value(
        "CFBundleDisplayName",
        "<string>CG-Agent-MacOS-Avatar</string>",
    );
    assert_value("CFBundlePackageType", "<string>APPL</string>");
}

#[test]
fn accessory_no_dock() {
    assert_value("LSUIElement", "<true/>");
}

#[test]
fn ats_local_networking_only() {
    assert_value("NSAppTransportSecurity", "<dict>");
    assert_value("NSAllowsLocalNetworking", "<true/>");
    assert!(
        !PLIST.contains("NSAllowsArbitraryLoads"),
        "ATS must not allow arbitrary loads"
    );
}

#[test]
fn macos_support_and_retina_are_declared() {
    assert_value("LSMinimumSystemVersion", "<string>13.0</string>");
    assert_value("NSHighResolutionCapable", "<true/>");
}
