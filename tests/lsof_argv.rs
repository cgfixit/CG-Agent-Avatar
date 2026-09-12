//! Discover must use argv-only lsof on 127.0.0.1, never a shell or wildcard bind.

#[test]
fn discover_lsof_is_argv_only_loopback() {
    let src = include_str!("../src/discover.rs");
    let prod = src.split("#[cfg(test)]").next().expect("prod");
    assert!(prod.contains("/usr/sbin/lsof"));
    assert!(prod.contains("-i4TCP@127.0.0.1"));
    assert!(prod.contains("-sTCP:LISTEN"));
    assert!(!prod.contains("sh -c"));
    assert!(!prod.contains("bash -c"));
    assert!(!prod.contains("0.0.0.0"));
    assert!(!prod.contains("localhost"));
}

#[test]
fn discover_skips_ollama_and_privileged_ports() {
    assert_eq!(
        cg_agent::discover::parse_loopback_listen_port("127.0.0.1:11434"),
        None
    );
    assert_eq!(
        cg_agent::discover::parse_loopback_listen_port("127.0.0.1:80"),
        None
    );
    assert_eq!(
        cg_agent::discover::parse_loopback_listen_port("127.0.0.1:51234"),
        Some(51234)
    );
}
