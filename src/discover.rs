//! Find a user-owned CG-Agent-Harness listener on 127.0.0.1.
//! Used when headless :8790 is down because the desktop .app bound port 0.
//! Does not scan the ephemeral range. Does not talk to the focus socket.

use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use crate::client::Client;
use crate::http::{self, MAX_BODY};
use crate::origin::LoopbackOrigin;

pub const OLLAMA_PORT: u16 = 11434;
const MAX_CANDIDATES: usize = 16;

/// Where a harness was actually found reachable. Fresh homes default to
/// `tls.enabled: true` with no HTTP fallback, so the scheme is not a detail
/// — it decides which `Client` constructor to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reachable {
    Http(u16),
    Https(u16),
}

pub fn looks_like_harness(v: &Value) -> bool {
    if !v.is_object() {
        return false;
    }
    if v.get("models").is_some() {
        return false;
    }
    // Fresh, auth-enabled homes answer /api/status before login with a thin
    // shape that carries no "model" field at all (see cg-agent-harness
    // src/server/routes/core.rs: status()). That is still unambiguously the
    // harness, not "nothing here" — the account just hasn't logged in yet.
    if v.get("auth_enabled").and_then(Value::as_bool) == Some(true) && v.get("model").is_none() {
        return true;
    }
    let model = v.get("model").and_then(Value::as_str).unwrap_or("");
    if model.is_empty() {
        return false;
    }
    v.get("api_key_optional").and_then(Value::as_bool).is_some()
}

pub fn is_harness_command(cmd: &str) -> bool {
    let c = cmd.trim();
    c.eq_ignore_ascii_case("cgagentharness") || {
        let lower = c.to_ascii_lowercase();
        lower.starts_with("cgagentharness")
    }
}

pub fn parse_loopback_listen_port(name: &str) -> Option<u16> {
    let name = name.split_whitespace().next()?;
    let rest = name.strip_prefix("127.0.0.1:")?;
    let port: u16 = rest.parse().ok()?;
    if (1024..=65535).contains(&port) && port != OLLAMA_PORT {
        Some(port)
    } else {
        None
    }
}

/// Parse `lsof -F pcn` (pid, command, name) records.
pub fn parse_lsof_fields(text: &str) -> Vec<u16> {
    let mut cmd = String::new();
    let mut ports = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let (tag, rest) = line.split_at(1);
        match tag {
            "p" => cmd.clear(),
            "c" => cmd = rest.to_string(),
            "n" if is_harness_command(&cmd) => {
                if let Some(port) = parse_loopback_listen_port(rest) {
                    if ports.len() < MAX_CANDIDATES && !ports.contains(&port) {
                        ports.push(port);
                    }
                }
            }
            _ => {}
        }
    }
    ports
}

pub fn probe_harness(port: u16) -> bool {
    let Ok(origin) = LoopbackOrigin::parse(&format!("http://127.0.0.1:{port}")) else {
        return false;
    };
    let Ok(url) = origin.url_for("/api/status") else {
        return false;
    };
    let Ok(http) = reqwest::blocking::Client::builder()
        .user_agent("cg-agent/0.1")
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .connect_timeout(Duration::from_secs(1))
        .build()
    else {
        return false;
    };
    let Ok(resp) = http.get(url).send() else {
        return false;
    };
    let code = resp.status().as_u16();
    if (300..400).contains(&code) || code != 200 {
        return false;
    }
    if let Some(len) = resp.content_length() {
        if len > MAX_BODY {
            return false;
        }
    }
    let Ok(bytes) = http::read_bounded(resp, MAX_BODY) else {
        return false;
    };
    let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    looks_like_harness(&v)
}

/// Probe a candidate port over HTTPS with a pinned leaf certificate, trying
/// IPv4 loopback then IPv6 loopback (the harness's cert SANs cover both).
/// Reuses `Client` so this gets the same size cap, redirect refusal, and
/// scheme guard as every other request this app makes — no bespoke TLS code
/// here.
pub fn probe_harness_https(port: u16, pinned_cert: &[u8]) -> bool {
    probe_https_host(port, pinned_cert, "127.0.0.1") || probe_https_host(port, pinned_cert, "[::1]")
}

fn probe_https_host(port: u16, pinned_cert: &[u8], host: &str) -> bool {
    let Ok(origin) = LoopbackOrigin::parse_https(&format!("https://{host}:{port}")) else {
        return false;
    };
    let Ok(client) = Client::new_https(origin, pinned_cert) else {
        return false;
    };
    client.status().is_ok()
}

/// Prefer `preferred` (8790 / harness.json). Tries HTTPS with the pinned
/// certificate first when one is available — fresh homes have no HTTP
/// fallback — then falls back to plain HTTP for legacy
/// (`tls.enabled: false`) homes. Returns `None` if nothing on the
/// loopback range answers as a harness at all, instead of silently handing
/// back `preferred` as if it had been confirmed.
pub fn resolve_reachable(preferred: u16, pinned_cert: Option<&[u8]>) -> Option<Reachable> {
    resolve_reachable_from_candidates(preferred, pinned_cert, listen_ports_from_lsof)
}

fn resolve_reachable_from_candidates(
    preferred: u16,
    pinned_cert: Option<&[u8]>,
    candidates: impl FnOnce() -> Vec<u16>,
) -> Option<Reachable> {
    if let Some(cert) = pinned_cert {
        if probe_harness_https(preferred, cert) {
            return Some(Reachable::Https(preferred));
        }
    }
    if probe_harness(preferred) {
        return Some(Reachable::Http(preferred));
    }
    for port in candidates() {
        if port == preferred {
            continue;
        }
        if let Some(cert) = pinned_cert {
            if probe_harness_https(port, cert) {
                return Some(Reachable::Https(port));
            }
        }
        if probe_harness(port) {
            return Some(Reachable::Http(port));
        }
    }
    None
}

fn lsof_stdout() -> Option<String> {
    let bins = ["/usr/sbin/lsof", "/usr/bin/lsof"];
    for bin in bins {
        let Ok(out) = Command::new(bin)
            .args(["-nP", "-i4TCP@127.0.0.1", "-sTCP:LISTEN", "-F", "pcn"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        else {
            continue;
        };
        if !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    None
}

pub fn listen_ports_from_lsof() -> Vec<u16> {
    lsof_stdout()
        .map(|s| parse_lsof_fields(&s))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn harness_json_not_ollama_tags() {
        assert!(looks_like_harness(&json!({
            "model": "qwen3.8:27b-mlx",
            "provider": "ollama",
            "api_key_optional": true,
            "version": "0"
        })));
        assert!(!looks_like_harness(&json!({
            "models": [{"name": "qwen3.8:27b-mlx"}]
        })));
        assert!(!looks_like_harness(&json!({"model": "x"})));
        assert!(!looks_like_harness(&json!({"api_key_optional": true})));
    }

    #[test]
    fn recognizes_login_required_thin_shape_as_harness() {
        // Fresh, auth-enabled homes answer /api/status like this before
        // login — no "model" field at all.
        assert!(looks_like_harness(&json!({
            "version": "1.2.3",
            "auth_enabled": true,
            "status": "login required for operational details"
        })));
        // auth_enabled alone, without the thin shape's missing model, must
        // not short-circuit the ordinary check.
        assert!(!looks_like_harness(
            &json!({"auth_enabled": false, "model": ""})
        ));
    }

    #[test]
    fn resolve_reachable_is_none_when_nothing_answers() {
        // Keep the no-candidate case independent of unrelated desktop
        // Harness listeners on the developer's machine.
        assert_eq!(resolve_reachable_from_candidates(1, None, Vec::new), None);
        assert_eq!(
            resolve_reachable_from_candidates(1, Some(b"garbage"), Vec::new),
            None
        );
    }

    #[test]
    fn resolve_reachable_prefers_https_when_cert_available() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/api/status")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"model":"qwen","provider":"ollama","api_key_optional":true}"#)
            .create();
        let origin = LoopbackOrigin::parse(&server.url()).unwrap();
        let port = origin.as_str().rsplit(':').next().unwrap().parse().unwrap();
        // mockito only serves plain HTTP, so the https attempt fails and
        // this exercises the fallback path landing on Http(port), proving
        // resolve_reachable does not simply trust https and give up.
        assert_eq!(
            resolve_reachable_from_candidates(port, Some(b"garbage"), || {
                panic!("must not run lsof when the preferred port answers")
            }),
            Some(Reachable::Http(port))
        );
    }

    #[test]
    fn probe_harness_https_rejects_unparseable_cert() {
        assert!(!probe_harness_https(1, b"garbage"));
    }

    #[test]
    fn parse_lsof_keeps_sidecar_drops_others() {
        let text = "\
p221
ccgagentharness
n127.0.0.1:51234
p222
collama
n127.0.0.1:11434
p223
ccgagentharness
n127.0.0.1:443
p224
cChrome
n127.0.0.1:9333
p225
ccgagentharness
n10.0.0.5:51235
";
        assert_eq!(parse_lsof_fields(text), vec![51234]);
    }

    #[test]
    fn command_prefix_and_port_rules() {
        assert!(is_harness_command("cgagentharness"));
        assert!(is_harness_command("cgagentharness-desktop"));
        assert!(!is_harness_command("chrome"));
        assert_eq!(parse_loopback_listen_port("127.0.0.1:51234"), Some(51234));
        assert_eq!(parse_loopback_listen_port("127.0.0.1:11434"), None);
        assert_eq!(parse_loopback_listen_port("*:51234"), None);
        assert_eq!(parse_loopback_listen_port("0.0.0.0:51234"), None);
        assert_eq!(parse_loopback_listen_port("localhost:51234"), None);
    }

    #[test]
    fn probe_rejects_redirect_and_non_harness() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/api/status")
            .with_status(302)
            .with_header("location", "http://127.0.0.1/x")
            .create();
        let origin = LoopbackOrigin::parse(&server.url()).unwrap();
        let port = origin.as_str().rsplit(':').next().unwrap().parse().unwrap();
        assert!(!probe_harness(port));
    }

    #[test]
    fn probe_accepts_harness_status() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/api/status")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"model":"qwen","provider":"ollama","api_key_optional":true}"#)
            .create();
        let origin = LoopbackOrigin::parse(&server.url()).unwrap();
        let port = origin.as_str().rsplit(':').next().unwrap().parse().unwrap();
        assert!(probe_harness(port));
    }
}
