//! Objection handling: hostile origins, oversized payloads, CSRF junk, SSRF bait.

use cg_agent::client::{Client, ClientError};
use cg_agent::csrf;
use cg_agent::origin::{LoopbackOrigin, OriginError};
use cg_agent::validate;

#[test]
fn ssrf_baits_are_rejected() {
    let baits = [
        "http://169.254.169.254/",
        "http://10.0.0.1:8790",
        "http://192.168.1.1:8790",
        "http://localhost:8790",
        "http://[fd00::1]:8790",
        "https://127.0.0.1:8790",
        "http://user:pass@127.0.0.1:8790",
        "http://127.0.0.1:8790/api/status",
        "file:///etc/passwd",
        "gopher://127.0.0.1:8790",
    ];
    for b in baits {
        assert!(LoopbackOrigin::parse(b).is_err(), "should reject {b}");
    }
}

#[test]
fn allowlisted_loopback_still_works() {
    assert!(LoopbackOrigin::parse("http://127.0.0.1:8790").is_ok());
    assert!(LoopbackOrigin::parse("http://127.0.0.1").is_ok());
    assert!(LoopbackOrigin::parse("http://[::1]:8790").is_ok());
}

#[test]
fn cannot_join_agent_path() {
    let o = LoopbackOrigin::from_port(8790);
    assert_eq!(
        o.url_for("/api/agent/run").unwrap_err(),
        OriginError::PathNotAllowed
    );
    assert_eq!(
        o.url_for("/api/keys").unwrap_err(),
        OriginError::PathNotAllowed
    );
}

#[test]
fn csrf_rejects_injection_shapes() {
    let cases = [
        r#"<meta name="csrf-token" content="tok\ninject">"#,
        r#"<meta name="csrf-token" content="tok; inject">"#,
        r#"<meta name="csrf-token" content="">"#,
        r#"<meta name="csrf-token" content="%%%%">"#,
        "<html></html>",
    ];
    for html in cases {
        assert!(csrf::from_html(html).is_none(), "accepted {html}");
    }
}

#[test]
fn validate_refuses_path_session_ids() {
    for id in ["../x", "x/y", "x y", "x\ny", ""] {
        assert!(validate::session_id(id).is_err());
    }
}

#[test]
fn client_debug_does_not_print_token() {
    let c = Client::from_port(8790).unwrap();
    let s = format!("{c:?}");
    assert!(!s.to_ascii_lowercase().contains("token"));
    assert!(s.contains("127.0.0.1"));
}

#[test]
fn oversized_status_json_is_too_large_or_json() {
    let mut server = mockito::Server::new();
    let body = format!(
        "{{\"model\":\"{}\",\"provider\":\"x\"}}",
        "m".repeat(2_000_000)
    );
    let _m = server
        .mock("GET", "/api/status")
        .with_header("content-type", "application/json")
        .with_header("content-length", &body.len().to_string())
        .with_body(&body)
        .create();
    let origin = LoopbackOrigin::parse(&server.url()).unwrap();
    let c = Client::new(origin).unwrap();
    let err = c.status().unwrap_err();
    assert!(
        matches!(
            err,
            ClientError::ResponseTooLarge | ClientError::Json | ClientError::Unreachable
        ),
        "{err:?}"
    );
}
