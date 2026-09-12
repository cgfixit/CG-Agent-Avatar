//! Blocking loopback client. Guarded POSTs send CSRF and never `loop`.
//! Redirects are refused. Only allowlisted paths. No forwarding headers.

use std::fmt;
use std::time::Duration;

use crate::csrf;
use crate::origin::{LoopbackOrigin, OriginError};
use crate::paths;
use crate::validate::{self, ValidateError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const SESSION_TITLE: &str = "CG-Agent";
const CSRF_HEADER: &str = "X-CyClaw-CSRF";
const USER_AGENT: &str = "cg-agent/0.1";
const GET_TIMEOUT: Duration = Duration::from_secs(8);
const CHAT_TIMEOUT: Duration = Duration::from_secs(720);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_BODY: u64 = 1_048_576;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Origin(#[from] OriginError),
    #[error(transparent)]
    Validate(#[from] ValidateError),
    #[error("harness unreachable")]
    Unreachable,
    #[error("csrf token missing from console html")]
    CsrfMissing,
    #[error("http {status}: {code}")]
    Http { status: u16, code: String },
    #[error("chat is already running")]
    ChatBusy,
    #[error("api key required — use the harness console")]
    KeyRequired,
    #[error("unexpected json")]
    Json,
    #[error("response too large")]
    ResponseTooLarge,
    #[error("redirect refused")]
    Redirect,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Status {
    pub model: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub api_key_optional: bool,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct ChatReply {
    pub session_id: String,
    pub reply: String,
    pub model: String,
}

#[derive(Serialize)]
struct ChatPost<'a> {
    message: &'a str,
    session_id: &'a str,
}

#[derive(Serialize)]
struct SessionPost<'a> {
    title: &'a str,
}

pub struct Client {
    origin: LoopbackOrigin,
    http: reqwest::blocking::Client,
    csrf: Option<String>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("origin", &self.origin.as_str())
            .field("csrf", &self.csrf.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl Client {
    pub fn new(origin: LoopbackOrigin) -> Result<Self, ClientError> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(CHAT_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|_| ClientError::Unreachable)?;
        Ok(Self {
            origin,
            http,
            csrf: None,
        })
    }

    pub fn from_port(port: u16) -> Result<Self, ClientError> {
        Self::new(LoopbackOrigin::from_port(port))
    }

    pub fn origin(&self) -> &LoopbackOrigin {
        &self.origin
    }

    fn get(&self, path: &'static str) -> Result<reqwest::blocking::Response, ClientError> {
        if !paths::is_allowed_get(path) {
            return Err(OriginError::PathNotAllowed.into());
        }
        let url = self.origin.url_for(path)?;
        let resp = self
            .http
            .get(url)
            .timeout(GET_TIMEOUT)
            .send()
            .map_err(|_| ClientError::Unreachable)?;
        check_redirect(&resp)?;
        Ok(resp)
    }

    fn post_json<T: Serialize>(
        &mut self,
        path: &'static str,
        body: &T,
        timeout: Duration,
    ) -> Result<reqwest::blocking::Response, ClientError> {
        if !paths::is_allowed_post(path) {
            return Err(OriginError::PathNotAllowed.into());
        }
        self.ensure_csrf()?;
        let token = self.csrf.clone().ok_or(ClientError::CsrfMissing)?;
        let url = self.origin.url_for(path)?;
        let resp = self
            .http
            .post(url)
            .timeout(timeout)
            .header(CSRF_HEADER, token)
            .json(body)
            .send()
            .map_err(|_| ClientError::Unreachable)?;
        check_redirect(&resp)?;
        Ok(resp)
    }

    pub fn status(&self) -> Result<Status, ClientError> {
        let resp = self.get(paths::GET_STATUS)?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(ClientError::Http {
                status,
                code: "STATUS".into(),
            });
        }
        parse_json(resp)
    }

    pub fn refresh_csrf(&mut self) -> Result<(), ClientError> {
        let resp = self.get(paths::GET_ROOT)?;
        if !resp.status().is_success() {
            return Err(ClientError::CsrfMissing);
        }
        cap_length(&resp)?;
        let html = resp.text().map_err(|_| ClientError::CsrfMissing)?;
        if html.len() as u64 > MAX_BODY {
            return Err(ClientError::ResponseTooLarge);
        }
        self.csrf = Some(csrf::from_html(&html).ok_or(ClientError::CsrfMissing)?);
        Ok(())
    }

    fn ensure_csrf(&mut self) -> Result<(), ClientError> {
        if self.csrf.is_none() {
            self.refresh_csrf()?;
        }
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>, ClientError> {
        let resp = self.get(paths::GET_SESSIONS)?;
        if !resp.status().is_success() {
            return Err(ClientError::Http {
                status: resp.status().as_u16(),
                code: "SESSIONS".into(),
            });
        }
        let v: Value = parse_json(resp)?;
        let arr = v
            .get("sessions")
            .and_then(|s| s.as_array())
            .ok_or(ClientError::Json)?;
        if arr.len() > 10_000 {
            return Err(ClientError::ResponseTooLarge);
        }
        arr.iter()
            .map(|s| {
                Ok(SessionSummary {
                    session_id: s
                        .get("session_id")
                        .and_then(|x| x.as_str())
                        .ok_or(ClientError::Json)?
                        .to_string(),
                    title: s
                        .get("title")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            })
            .collect()
    }

    pub fn ensure_session(&mut self) -> Result<String, ClientError> {
        if let Some(existing) = self
            .list_sessions()?
            .into_iter()
            .find(|s| s.title == SESSION_TITLE)
        {
            let id = validate::session_id(&existing.session_id)?;
            return Ok(id.to_string());
        }
        let body = SessionPost {
            title: SESSION_TITLE,
        };
        let resp = self.post_json(paths::POST_SESSIONS, &body, GET_TIMEOUT)?;
        let status = resp.status().as_u16();
        if status == 401 {
            return Err(ClientError::KeyRequired);
        }
        if status != 200 && status != 201 {
            return Err(map_http(resp));
        }
        let v: Value = parse_json(resp)?;
        let id = v
            .get("session_id")
            .and_then(|s| s.as_str())
            .ok_or(ClientError::Json)?;
        Ok(validate::session_id(id)?.to_string())
    }

    pub fn chat(&mut self, session_id: &str, message: &str) -> Result<ChatReply, ClientError> {
        let session_id = validate::session_id(session_id)?;
        let message = validate::message(message)?;
        let body = ChatPost {
            message,
            session_id,
        };
        debug_assert!(serde_json::to_value(&body)
            .ok()
            .and_then(|v| v.get("loop").cloned())
            .is_none());
        let resp = self.post_json(paths::POST_CHAT, &body, CHAT_TIMEOUT)?;
        let status = resp.status().as_u16();
        if status == 409 {
            return Err(ClientError::ChatBusy);
        }
        if status == 401 {
            return Err(ClientError::KeyRequired);
        }
        if status != 200 {
            return Err(map_http(resp));
        }
        let v: Value = parse_json(resp)?;
        let reply = v
            .get("reply")
            .and_then(|s| s.as_str())
            .ok_or(ClientError::Json)?;
        Ok(ChatReply {
            session_id: v
                .get("session_id")
                .and_then(|s| s.as_str())
                .unwrap_or(session_id)
                .to_string(),
            reply: reply.to_string(),
            model: v
                .get("model")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub title: String,
}

fn check_redirect(resp: &reqwest::blocking::Response) -> Result<(), ClientError> {
    let s = resp.status().as_u16();
    if (300..400).contains(&s) {
        return Err(ClientError::Redirect);
    }
    Ok(())
}

fn cap_length(resp: &reqwest::blocking::Response) -> Result<(), ClientError> {
    if let Some(len) = resp.content_length() {
        if len > MAX_BODY {
            return Err(ClientError::ResponseTooLarge);
        }
    }
    Ok(())
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    resp: reqwest::blocking::Response,
) -> Result<T, ClientError> {
    cap_length(&resp)?;
    let bytes = resp.bytes().map_err(|_| ClientError::Json)?;
    if bytes.len() as u64 > MAX_BODY {
        return Err(ClientError::ResponseTooLarge);
    }
    serde_json::from_slice(&bytes).map_err(|_| ClientError::Json)
}

fn map_http(resp: reqwest::blocking::Response) -> ClientError {
    let status = resp.status().as_u16();
    if (300..400).contains(&status) {
        return ClientError::Redirect;
    }
    let code = resp
        .json::<Value>()
        .ok()
        .and_then(|v| v.get("code").and_then(|c| c.as_str()).map(str::to_string))
        .unwrap_or_else(|| "HTTP".into());
    let code: String = code.chars().take(64).collect();
    ClientError::Http { status, code }
}

pub fn chat_post_json(message: &str, session_id: &str) -> Value {
    serde_json::to_value(&ChatPost {
        message,
        session_id,
    })
    .expect("chat body")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin_for(server: &mockito::ServerGuard) -> LoopbackOrigin {
        LoopbackOrigin::parse(&server.url()).expect("mockito binds loopback")
    }

    fn html_ok() -> &'static str {
        r#"<meta name="csrf-token" content="tok12345">"#
    }

    #[test]
    fn status_does_not_need_csrf() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/api/status")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"model":"qwen","provider":"ollama","api_key_optional":true,"version":"0"}"#,
            )
            .create();
        let c = Client::new(origin_for(&server)).unwrap();
        let s = c.status().unwrap();
        assert_eq!(s.model, "qwen");
        assert!(s.api_key_optional);
    }

    #[test]
    fn chat_sends_csrf_and_never_loop() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let chat = server
            .mock("POST", "/api/chat")
            .match_header("X-CyClaw-CSRF", "tok12345")
            .match_header("x-forwarded-for", mockito::Matcher::Missing)
            .match_header("origin", mockito::Matcher::Missing)
            .match_header("authorization", mockito::Matcher::Missing)
            .match_body(mockito::Matcher::Regex(r#""message":"hi""#.into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"session_id":"abc","reply":"hello","model":"qwen"}"#)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        let r = c.chat("abc", "hi").unwrap();
        assert_eq!(r.reply, "hello");
        chat.assert();
    }

    #[test]
    fn chat_json_has_exactly_two_keys_and_no_loop() {
        let v = chat_post_json("hi", "s1");
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 2);
        assert!(obj.contains_key("message"));
        assert!(obj.contains_key("session_id"));
        assert!(!obj.contains_key("loop"));
        assert!(!v.to_string().contains("loop"));
    }

    #[test]
    fn chat_busy_is_distinct() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let _chat = server
            .mock("POST", "/api/chat")
            .with_status(409)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":"CHAT_BUSY"}"#)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(c.chat("abc", "hi"), Err(ClientError::ChatBusy)));
    }

    #[test]
    fn reuses_session_by_title() {
        let mut server = mockito::Server::new();
        let _list = server
            .mock("GET", "/api/sessions")
            .with_header("content-type", "application/json")
            .with_body(r#"{"sessions":[{"session_id":"s1","title":"CG-Agent"}]}"#)
            .create();
        let create = server.mock("POST", "/api/sessions").expect(0).create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        assert_eq!(c.ensure_session().unwrap(), "s1");
        create.assert();
    }

    #[test]
    fn unreachable_status_is_asleep_signal() {
        let origin = LoopbackOrigin::from_port(1);
        let c = Client::new(origin).unwrap();
        assert!(matches!(c.status(), Err(ClientError::Unreachable)));
    }

    #[test]
    fn empty_and_huge_messages_are_refused_before_http() {
        let origin = LoopbackOrigin::from_port(1);
        let mut c = Client::new(origin).unwrap();
        assert!(matches!(
            c.chat("abc", "  "),
            Err(ClientError::Validate(ValidateError::EmptyMessage))
        ));
        let huge = "a".repeat(40_000);
        assert!(matches!(
            c.chat("abc", &huge),
            Err(ClientError::Validate(ValidateError::MessageTooLong))
        ));
        assert!(matches!(
            c.chat("../etc", "hi"),
            Err(ClientError::Validate(ValidateError::SessionIdInvalid))
        ));
    }

    #[test]
    fn debug_redacts_csrf() {
        let origin = LoopbackOrigin::from_port(8790);
        let mut c = Client::new(origin).unwrap();
        c.csrf = Some("supersecrettokenvalue".into());
        let d = format!("{c:?}");
        assert!(!d.contains("supersecrettokenvalue"));
        assert!(d.contains("[redacted]"));
    }

    #[test]
    fn redirect_is_refused() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/api/status")
            .with_status(302)
            .with_header("location", "http://127.0.0.1/evil")
            .create();
        let c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(c.status(), Err(ClientError::Redirect)));
    }

    #[test]
    fn key_required_is_distinct() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let _chat = server.mock("POST", "/api/chat").with_status(401).create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(c.chat("abc", "hi"), Err(ClientError::KeyRequired)));
    }
}
