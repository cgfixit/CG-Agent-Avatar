//! Discover must use argv-only lsof on 127.0.0.1, never a shell or wildcard bind.

#[test]
fn discover_lsof_is_argv_only_loopback() {
    let src = include_str!("../src/discover.rs");
    let prod = src.split("#[cfg(test)]").next().expect("prod");
    assert!(prod.contains("/usr/sbin/lsof"));
    assert!(prod.contains("/usr/bin/lsof"));
    assert!(prod.contains("\"-nP\""));
    assert!(prod.contains("\"-F\", \"pcn\""));
    assert!(prod.contains("-i4TCP@127.0.0.1"));
    assert!(prod.contains("-sTCP:LISTEN"));
    assert!(!prod.contains("sh -c"));
    assert!(!prod.contains("bash -c"));
    assert!(!prod.contains("0.0.0.0"));
    assert!(!prod.contains("localhost"));
}

#[test]
fn discover_skips_ollama_and_privileged_ports() {
    for (address, expected) in [
        ("127.0.0.1:11434", None),
        ("127.0.0.1:1023", None),
        ("127.0.0.1:1024", Some(1024)),
        ("127.0.0.1:65535", Some(65535)),
        ("127.0.0.1:65536", None),
        ("0.0.0.0:51234", None),
        ("[::1]:51234", None),
    ] {
        assert_eq!(
            cg_agent::discover::parse_loopback_listen_port(address),
            expected,
            "{address}"
        );
    }
}

#[test]
fn new_process_cannot_inherit_the_previous_harness_command() {
    let fields = "p1\ncCGagentHarness\nn127.0.0.1:51234\np2\nn127.0.0.1:51235\n";
    assert_eq!(cg_agent::discover::parse_lsof_fields(fields), [51234]);
}
