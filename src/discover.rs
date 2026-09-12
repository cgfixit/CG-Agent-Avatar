//! Find a user-owned CG-Agent-Harness listener on 127.0.0.1.
//! Used when headless :8790 is down because the desktop .app bound port 0.
//! Does not scan the ephemeral range. Does not talk to the focus socket.

use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use crate::origin::LoopbackOrigin;

pub const OLLAMA_PORT: u16 = 11434;
const MAX_CANDIDATES: usize = 16;

pub fn looks_like_harness(v: &Value) -> bool {
    if !v.is_object() {
        return false;
    }
    if v.get("models").is_some() {
        return false;
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
        if len > 1_048_576 {
            return false;
        }
    }
    let Ok(v) = resp.json::<Value>() else {
        return false;
    };
    looks_like_harness(&v)
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

/// Prefer `preferred` (8790 / harness.json). Else first lsof candidate that probes as harness.
pub fn resolve_port(preferred: u16) -> u16 {
    if probe_harness(preferred) {
        return preferred;
    }
    for port in listen_ports_from_lsof() {
        if port != preferred && probe_harness(port) {
            return port;
        }
    }
    preferred
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
