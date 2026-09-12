//! Darwin bundle contract: ATS local-only, accessory extra, no arbitrary loads.

const PLIST: &str = include_str!("../resources/Info.plist");

#[test]
fn bundle_identity() {
    assert!(PLIST.contains("<string>com.cgfixit.cg-agent</string>"));
    assert!(PLIST.contains("<string>cg-agent</string>"));
    assert!(PLIST.contains("<string>CG-Agent-MacOS-Avatar</string>"));
}

#[test]
fn accessory_no_dock() {
    assert!(PLIST.contains("<key>LSUIElement</key>"));
    let after = PLIST.split("<key>LSUIElement</key>").nth(1).expect("key");
    assert!(after.trim_start().starts_with("<true/>"));
}

#[test]
fn ats_local_networking_only() {
    assert!(PLIST.contains("<key>NSAllowsLocalNetworking</key>"));
    assert!(
        !PLIST.contains("NSAllowsArbitraryLoads"),
        "ATS must not allow arbitrary loads"
    );
}
