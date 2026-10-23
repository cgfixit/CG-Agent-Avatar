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
        "http://user:pass@127.0.0.1:8790",
        "http://127.0.0.1:8790/api/status",
        "file:///etc/passwd",
        "gopher://127.0.0.1:8790",
    ];
    for b in baits {
        assert!(LoopbackOrigin::parse(b).is_err(), "should reject {b}");
        let https = b.replacen("http://", "https://", 1);
        assert!(
            LoopbackOrigin::parse_https(&https).is_err(),
            "should reject {https}"
        );
    }
}

#[test]
fn allowlisted_loopback_still_works() {
    assert!(LoopbackOrigin::parse("http://127.0.0.1:8790").is_ok());
    assert!(LoopbackOrigin::parse("http://127.0.0.1").is_ok());
    assert!(LoopbackOrigin::parse("http://[::1]:8790").is_ok());
    assert!(LoopbackOrigin::parse_https("https://127.0.0.1:8790").is_ok());
    assert!(LoopbackOrigin::parse_https("https://[::1]:8790").is_ok());
    // HTTPS requires the explicit pinned-client path, never the legacy parser.
    assert!(LoopbackOrigin::parse("https://127.0.0.1:8790").is_err());
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
        "<meta name=\"csrf-token\" content=\"tok\ninject\">",
        "<meta name=\"csrf-token\" content=\"tok\rinject\">",
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
    let mut server = mockito::Server::new();
    let token = "synthetic-csrf-token-for-redaction";
    let console = server
        .mock("GET", "/")
        .with_body(format!(r#"<meta name="csrf-token" content="{token}">"#))
        .expect(1)
        .create();
    let mut c = Client::new(LoopbackOrigin::parse(&server.url()).unwrap()).unwrap();
    c.refresh_csrf().unwrap();
    console.assert();
    let s = format!("{c:?}");
    assert!(!s.contains(token));
    assert!(s.contains("[redacted]"));
    assert!(s.contains("127.0.0.1"));
}

#[test]
fn oversized_status_json_is_rejected_before_parsing() {
    let mut server = mockito::Server::new();
    let body = format!(
        "{{\"model\":\"{}\",\"provider\":\"x\",\"api_key_optional\":true}}",
        "m".repeat(2_000_000)
    );
    for chunked in [false, true] {
        let mock = server
            .mock("GET", "/api/status")
            .with_header("content-type", "application/json")
            .expect(1);
        let response = if chunked {
            let body = body.clone();
            mock.with_chunked_body(move |writer| writer.write_all(body.as_bytes()))
        } else {
            mock.with_header("content-length", &body.len().to_string())
                .with_body(&body)
        }
        .create();
        let c = Client::new(LoopbackOrigin::parse(&server.url()).unwrap()).unwrap();
        let Err(err) = c.status() else {
            panic!("oversized status was accepted");
        };
        assert!(
            matches!(err, ClientError::ResponseTooLarge),
            "chunked={chunked}: {err:?}"
        );
        response.assert();
    }
}
