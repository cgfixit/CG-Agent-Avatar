//! Blocking loopback client. Guarded POSTs send CSRF and never `loop`.
//! Redirects are refused. Only allowlisted paths. No forwarding headers.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::csrf;
use crate::http::{self, ReadError, MAX_BODY};
use crate::origin::{LoopbackOrigin, OriginError};
use crate::paths;
use crate::validate::{self, ValidateError};
use reqwest::cookie::Jar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// In-memory cookie jar that clients of one harness can share.
pub type SessionJar = Jar;

pub const SESSION_TITLE: &str = "CG-Agent";
const CSRF_HEADER: &str = "X-CyClaw-CSRF";
const USER_AGENT: &str = "cg-agent/0.1";
const GET_TIMEOUT: Duration = Duration::from_secs(8);
const CHAT_TIMEOUT: Duration = Duration::from_secs(720);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

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
    #[error("harness login required")]
    LoginRequired,
    #[error("bootstrap password must be changed — use Harness Password Reset…")]
    PasswordChangeRequired,
    #[error("harness certificate did not match the pinned trust — possible rotation or MITM")]
    CertMismatch,
    #[error("this client was built for a different scheme (http vs https)")]
    SchemeMismatch,
    #[error("unexpected json")]
    Json,
    #[error("response too large")]
    ResponseTooLarge,
    #[error("redirect refused")]
    Redirect,
    #[error("harness session is gone")]
    SessionGone,
    #[error("harness is rate limiting requests")]
    RateLimited,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Status {
    /// Absent on the thin, unauthenticated response a fresh (auth-enabled)
    /// home returns from `/api/status` before login — `#[serde(default)]`
    /// so that shape still deserializes instead of bubbling ClientError::Json.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub api_key_optional: bool,
    #[serde(default)]
    pub version: String,
    /// Only in the thin, pre-login shape of an auth-enabled home. The full
    /// shape omits it even when accounts are on.
    #[serde(default)]
    pub auth_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ChatReply {
    pub session_id: String,
    pub reply: String,
    pub model: String,
    /// Sources the harness's own web_search/web_fetch tools used for this
    /// reply, if any (see cg-agent-harness src/server/routes/core.rs). This
    /// app never calls /api/web/* itself; it only displays what the harness
    /// already decided to fetch under its own allowlist.
    pub web_tools: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct LoginInfo {
    pub username: String,
    pub role: String,
    pub must_change_password: bool,
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

#[derive(Serialize)]
struct LoginPost<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct PasswordChangePost<'a> {
    current_password: &'a str,
    password: &'a str,
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

/// Every client keeps its session cookie in memory only. Clients for the same
/// harness may share one jar so a login on one thread is seen by another.
fn builder(jar: Arc<Jar>) -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(CHAT_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .cookie_provider(jar)
}

impl Client {
    pub fn new(origin: LoopbackOrigin) -> Result<Self, ClientError> {
        Self::with_jar(origin, Arc::default())
    }

    /// Like [`Client::new`], sharing `jar` with other clients of this harness.
    pub fn with_jar(origin: LoopbackOrigin, jar: Arc<Jar>) -> Result<Self, ClientError> {
        if origin.is_https() {
            return Err(ClientError::SchemeMismatch);
        }
        let http = builder(jar).build().map_err(|_| ClientError::Unreachable)?;
        Ok(Self {
            origin,
            http,
            csrf: None,
        })
    }

    /// HTTPS with a single pinned leaf certificate (PEM), no other CA is
    /// trusted for this client. `cert_pem` should come from the harness's
    /// own home directory (see `home::read_pinned_cert`) — never from the
    /// network, and never with `danger_accept_invalid_certs`.
    pub fn new_https(origin: LoopbackOrigin, cert_pem: &[u8]) -> Result<Self, ClientError> {
        Self::https_with_jar(origin, cert_pem, Arc::default())
    }

    /// Like [`Client::new_https`], sharing `jar` with other clients of this harness.
    pub fn https_with_jar(
        origin: LoopbackOrigin,
        cert_pem: &[u8],
        jar: Arc<Jar>,
    ) -> Result<Self, ClientError> {
        if !origin.is_https() {
            return Err(ClientError::SchemeMismatch);
        }
        // reqwest::Certificate::from_pem defers all parsing to build(), and a
        // PEM with zero recognizable certificate blocks builds successfully
        // into an empty (trust-nothing) root store rather than erroring —
        // every future connection would then fail lazily at handshake time
        // with a generic error. Fail fast here instead so a bad pinned cert
        // is reported as CertMismatch immediately, not as "unreachable"
        // later.
        if !cert_pem
            .windows(b"-----BEGIN CERTIFICATE-----".len())
            .any(|w| w == b"-----BEGIN CERTIFICATE-----")
        {
            return Err(ClientError::CertMismatch);
        }
        let cert =
            reqwest::Certificate::from_pem(cert_pem).map_err(|_| ClientError::CertMismatch)?;
        let http = builder(jar)
            .tls_built_in_root_certs(false)
            .add_root_certificate(cert)
            .build()
            .map_err(|_| ClientError::CertMismatch)?;
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
            .map_err(|e| classify_send_error(&e))?;
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
        let url = self.origin.url_for(path)?;
        // The console token is per harness process. After a restart on the same
        // port the cached one is stale; the CSRF layer rejects before the
        // handler runs, so one refresh-and-resend is safe.
        for attempt in 0..2 {
            self.ensure_csrf()?;
            let token = self.csrf.clone().ok_or(ClientError::CsrfMissing)?;
            let resp = self
                .http
                .post(&url)
                .timeout(timeout)
                .header(CSRF_HEADER, token)
                .json(body)
                .send()
                .map_err(|e| classify_send_error(&e))?;
            check_redirect(&resp)?;
            if resp.status().as_u16() != 403 {
                return Ok(resp);
            }
            let code = error_code(resp);
            if attempt == 0 && code.as_deref() == Some("CSRF_TOKEN_INVALID") {
                self.csrf = None;
                continue;
            }
            return Err(forbidden(code));
        }
        unreachable!("the loop returns on its second pass")
    }

    /// Log in to a fresh (auth-enabled) harness home. Does not send a CSRF
    /// header — cg-agent-harness's `/api/auth/login` sits in its `auth_open`
    /// router, outside the CSRF layer. The response is
    /// `{username, role, must_change_password}` plus the session cookie;
    /// guarded routes keep using the console's process token from `GET /`.
    pub fn login(&mut self, username: &str, password: &str) -> Result<LoginInfo, ClientError> {
        let path = paths::POST_AUTH_LOGIN;
        if !paths::is_allowed_post(path) {
            return Err(OriginError::PathNotAllowed.into());
        }
        let url = self.origin.url_for(path)?;
        let body = LoginPost { username, password };
        let resp = self
            .http
            .post(url)
            .timeout(GET_TIMEOUT)
            .json(&body)
            .send()
            .map_err(|e| classify_send_error(&e))?;
        check_redirect(&resp)?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(map_http(resp));
        }
        let v: Value = parse_json(resp)?;
        Ok(LoginInfo {
            username: v
                .get("username")
                .and_then(|s| s.as_str())
                .unwrap_or(username)
                .to_string(),
            role: v
                .get("role")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string(),
            must_change_password: v
                .get("must_change_password")
                .and_then(|b| b.as_bool())
                .unwrap_or(false),
        })
    }

    /// Change only the current authenticated account's password. The harness
    /// revokes old sessions and returns a replacement cookie, so clear the
    /// console CSRF token and require a fresh one before the next mutation.
    pub fn change_password(
        &mut self,
        current_password: &str,
        password: &str,
    ) -> Result<(), ClientError> {
        let body = PasswordChangePost {
            current_password,
            password,
        };
        let resp = self.post_json(paths::POST_AUTH_PASSWORD, &body, GET_TIMEOUT)?;
        if resp.status().as_u16() != 200 {
            return Err(map_http(resp));
        }
        let v: Value = parse_json(resp)?;
        if v.get("ok").and_then(|ok| ok.as_bool()) != Some(true)
            || v.get("must_change_password")
                .and_then(|required| required.as_bool())
                != Some(false)
        {
            return Err(ClientError::Json);
        }
        self.csrf = None;
        Ok(())
    }

    pub fn status(&self) -> Result<Status, ClientError> {
        let resp = self.get(paths::GET_STATUS)?;
        let status = resp.status().as_u16();
        if status == 429 {
            return Err(ClientError::RateLimited);
        }
        if status != 200 {
            return Err(ClientError::Http {
                status,
                code: "STATUS".into(),
            });
        }
        let value: Value = parse_json(resp)?;
        if !crate::discover::looks_like_harness(&value) {
            return Err(ClientError::Json);
        }
        serde_json::from_value(value).map_err(|_| ClientError::Json)
    }

    pub fn refresh_csrf(&mut self) -> Result<(), ClientError> {
        let resp = self.get(paths::GET_ROOT)?;
        if !resp.status().is_success() {
            return Err(ClientError::CsrfMissing);
        }
        let bytes = response_bytes(resp)?;
        let html = std::str::from_utf8(&bytes).map_err(|_| ClientError::CsrfMissing)?;
        self.csrf = Some(csrf::from_html(html).ok_or(ClientError::CsrfMissing)?);
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
        let status = resp.status().as_u16();
        if status == 401 {
            return Err(classify_unauthorized(resp));
        }
        if status == 403 {
            return Err(classify_forbidden(resp));
        }
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
            return Err(classify_unauthorized(resp));
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
            return Err(classify_unauthorized(resp));
        }
        if status == 404 {
            // Unknown or foreign session, e.g. after the console cleared them.
            return match map_http(resp) {
                ClientError::Http { code, .. } if code == "HARNESS_SESSION_ERROR" => {
                    Err(ClientError::SessionGone)
                }
                other => Err(other),
            };
        }
        if status != 200 {
            return Err(map_http(resp));
        }
        let v: Value = parse_json(resp)?;
        let reply = v
            .get("reply")
            .and_then(|s| s.as_str())
            .ok_or(ClientError::Json)?;
        let web_tools = v
            .get("web_tools")
            .and_then(|w| w.as_array())
            .cloned()
            .unwrap_or_default();
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
            web_tools,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub title: String,
}

/// Distinguishes "harness is down / wrong port" from "harness is up but the
/// pinned certificate no longer matches" instead of collapsing both into one
/// opaque signal. Best-effort: it inspects the connect-error text for a TLS
/// certificate complaint, which does not change what is actually validated
/// (rustls already refused the handshake before this ever runs) — it only
/// changes which bubble message the user sees.
fn classify_send_error(e: &reqwest::Error) -> ClientError {
    if e.is_connect() {
        let mut cur: Option<&(dyn std::error::Error + 'static)> =
            Some(e as &(dyn std::error::Error + 'static));
        while let Some(err) = cur {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("certificate")
                || msg.contains("unknownissuer")
                || msg.contains("invalidcertificate")
            {
                return ClientError::CertMismatch;
            }
            cur = err.source();
        }
    }
    ClientError::Unreachable
}

/// A 401 with code "AUTH_REQUIRED" means a modern, account-gated harness
/// home is up and reachable but this client hasn't logged in yet — a very
/// different situation from the harness being unreachable, and one the
/// user can act on directly (Client::login). Anything else on a 401 falls
/// back to the older, now largely dead, optional-API-key model.
fn classify_unauthorized(resp: reqwest::blocking::Response) -> ClientError {
    match error_code(resp).as_deref() {
        Some("AUTH_REQUIRED") => ClientError::LoginRequired,
        _ => ClientError::KeyRequired,
    }
}

fn classify_forbidden(resp: reqwest::blocking::Response) -> ClientError {
    forbidden(error_code(resp))
}

/// The harness wraps errors as `{"detail":{"code":…,"message":…}}`
/// (cg-agent-harness src/server/errors.rs); a bare top-level `code` is
/// still accepted. Codes are clipped before they reach any message.
fn error_code(resp: reqwest::blocking::Response) -> Option<String> {
    let v = parse_json::<Value>(resp).ok()?;
    v.get("detail")
        .and_then(|d| d.get("code"))
        .or_else(|| v.get("code"))
        .and_then(Value::as_str)
        .map(|c| c.chars().take(64).collect())
}

fn forbidden(code: Option<String>) -> ClientError {
    match code.as_deref() {
        Some("AUTH_PASSWORD_CHANGE_REQUIRED") => ClientError::PasswordChangeRequired,
        _ => ClientError::Http {
            status: 403,
            code: code.unwrap_or_else(|| "FORBIDDEN".into()),
        },
    }
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
    let bytes = response_bytes(resp)?;
    serde_json::from_slice(&bytes).map_err(|_| ClientError::Json)
}

fn response_bytes(resp: reqwest::blocking::Response) -> Result<Vec<u8>, ClientError> {
    cap_length(&resp)?;
    http::read_bounded(resp, MAX_BODY).map_err(|e| match e {
        ReadError::Io => ClientError::Json,
        ReadError::TooLarge => ClientError::ResponseTooLarge,
    })
}

fn map_http(resp: reqwest::blocking::Response) -> ClientError {
    let status = resp.status().as_u16();
    if (300..400).contains(&status) {
        return ClientError::Redirect;
    }
    if status == 429 {
        return ClientError::RateLimited;
    }
    let code = error_code(resp).unwrap_or_else(|| "HTTP".into());
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

    #[test]
    fn list_sessions_login_required_is_distinct() {
        let mut server = mockito::Server::new();
        let _list = server
            .mock("GET", "/api/sessions")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":"AUTH_REQUIRED"}"#)
            .create();
        let c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(c.list_sessions(), Err(ClientError::LoginRequired)));
    }

    #[test]
    fn login_required_is_distinct_from_key_required() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let _chat = server
            .mock("POST", "/api/chat")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":"AUTH_REQUIRED"}"#)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(
            c.chat("abc", "hi"),
            Err(ClientError::LoginRequired)
        ));
    }

    #[test]
    fn password_change_required_is_distinct() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let _chat = server
            .mock("POST", "/api/chat")
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":"AUTH_PASSWORD_CHANGE_REQUIRED"}"#)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(
            c.chat("abc", "hi"),
            Err(ClientError::PasswordChangeRequired)
        ));
    }

    #[test]
    fn login_refreshes_console_csrf_before_guarded_routes() {
        let mut server = mockito::Server::new();
        // No GET / mock registered: login must not need to scrape HTML for CSRF.
        let login = server
            .mock("POST", "/api/auth/login")
            .match_header("X-CyClaw-CSRF", mockito::Matcher::Missing)
            .match_body(mockito::Matcher::Regex(
                r#""username":"admin","password":"hunter2""#.into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("set-cookie", "cgagentharness_session=abc; HttpOnly; Path=/")
            // The harness's real login body: no token field (its
            // tests/secure_portal.rs asserts the absence).
            .with_body(r#"{"username":"admin","role":"admin","must_change_password":false}"#)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        let info = c.login("admin", "hunter2").unwrap();
        assert_eq!(info.username, "admin");
        assert_eq!(info.role, "admin");
        assert!(!info.must_change_password);
        login.assert();

        // Guarded routes use the process token rendered in the console.
        let sessions = server
            .mock("GET", "/api/sessions")
            .with_header("content-type", "application/json")
            .with_body(r#"{"sessions":[]}"#)
            .create();
        let console = server.mock("GET", "/").with_body(html_ok()).create();
        let create = server
            .mock("POST", "/api/sessions")
            .match_header("X-CyClaw-CSRF", "tok12345")
            .with_header("content-type", "application/json")
            .with_body(r#"{"session_id":"s1"}"#)
            .create();
        assert_eq!(c.ensure_session().unwrap(), "s1");
        sessions.assert();
        console.assert();
        create.assert();
    }

    #[test]
    fn password_change_uses_console_csrf_and_clears_it_after_rotation() {
        let mut server = mockito::Server::new();
        let console = server.mock("GET", "/").with_body(html_ok()).create();
        let change = server
            .mock("POST", "/api/auth/password")
            .match_header("X-CyClaw-CSRF", "tok12345")
            .match_body(mockito::Matcher::Regex(
                r#""current_password":"admin","password":"new-password-123""#.into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header(
                "set-cookie",
                "cgagentharness_session=replacement; HttpOnly; Path=/",
            )
            .with_body(r#"{"ok":true,"must_change_password":false}"#)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        c.change_password("admin", "new-password-123").unwrap();
        assert!(c.csrf.is_none());
        console.assert();
        change.assert();
    }

    #[test]
    fn login_failure_carries_harness_code() {
        let mut server = mockito::Server::new();
        let _login = server
            .mock("POST", "/api/auth/login")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":"AUTH_LOGIN_FAILED","message":"bad credentials"}"#)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        let err = c.login("admin", "wrong").unwrap_err();
        assert!(matches!(
            err,
            ClientError::Http { status: 401, ref code } if code == "AUTH_LOGIN_FAILED"
        ));
    }

    #[test]
    fn https_client_rejects_http_origin_and_vice_versa() {
        assert!(matches!(
            Client::new(LoopbackOrigin::from_port_https(8790)),
            Err(ClientError::SchemeMismatch)
        ));
        assert!(matches!(
            Client::new_https(LoopbackOrigin::from_port(8790), b"not a cert"),
            Err(ClientError::SchemeMismatch)
        ));
    }

    #[test]
    fn https_client_rejects_pem_with_no_certificate_marker() {
        let origin = LoopbackOrigin::from_port_https(8790);
        assert!(matches!(
            Client::new_https(origin, b"not a real certificate"),
            Err(ClientError::CertMismatch)
        ));
    }

    #[test]
    fn https_client_rejects_malformed_certificate_body() {
        let origin = LoopbackOrigin::from_port_https(8790);
        let bad_pem =
            b"-----BEGIN CERTIFICATE-----\nnot valid base64 !!!\n-----END CERTIFICATE-----\n";
        assert!(matches!(
            Client::new_https(origin, bad_pem),
            Err(ClientError::CertMismatch)
        ));
    }

    #[test]
    fn status_tolerates_login_required_thin_shape() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/api/status")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"version":"1.2.3","auth_enabled":true,"status":"login required for operational details"}"#,
            )
            .create();
        let c = Client::new(origin_for(&server)).unwrap();
        let s = c.status().unwrap();
        assert!(s.model.is_empty());
        assert!(s.auth_enabled);
    }

    #[test]
    fn status_rejects_generic_json_even_over_a_valid_connection() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", "/api/status")
            .with_status(200)
            .with_body(r#"{"message":"ok","auth_enabled":true}"#)
            .create();
        let c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(c.status(), Err(ClientError::Json)));
    }

    #[test]
    fn chat_reply_carries_web_tools() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let _chat = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"session_id":"abc","reply":"hi","model":"qwen","web_tools":[{"tool":"web_search","query":"x"}]}"#,
            )
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        let r = c.chat("abc", "hi").unwrap();
        assert_eq!(r.web_tools.len(), 1);
    }

    /// cg-agent-harness src/server/errors.rs: every JSON error is nested.
    fn harness_error(code: &str) -> String {
        format!(r#"{{"detail":{{"code":"{code}","message":"m","details":{{}}}}}}"#)
    }

    #[test]
    fn nested_harness_error_codes_are_read() {
        for (status, code, want) in [
            (401, "AUTH_REQUIRED", "login"),
            (403, "AUTH_PASSWORD_CHANGE_REQUIRED", "password"),
            (502, "HARNESS_LLM_ERROR", "HARNESS_LLM_ERROR"),
        ] {
            let mut server = mockito::Server::new();
            let _html = server.mock("GET", "/").with_body(html_ok()).create();
            let _chat = server
                .mock("POST", "/api/chat")
                .with_status(status)
                .with_body(harness_error(code))
                .create();
            let mut c = Client::new(origin_for(&server)).unwrap();
            let err = c.chat("abc", "hi").unwrap_err();
            let ok = match want {
                "login" => matches!(err, ClientError::LoginRequired),
                "password" => matches!(err, ClientError::PasswordChangeRequired),
                code => matches!(err, ClientError::Http { code: ref got, .. } if got == code),
            };
            assert!(ok, "{status} {code}: {err:?}");
        }
    }

    #[test]
    fn stale_console_token_is_refreshed_once_after_a_harness_restart() {
        let mut server = mockito::Server::new();
        let console = server
            .mock("GET", "/")
            .with_body(html_ok())
            .expect(1)
            .create();
        let stale = server
            .mock("POST", "/api/chat")
            .match_header("X-CyClaw-CSRF", "stale-token-1")
            .with_status(403)
            .with_body(harness_error("CSRF_TOKEN_INVALID"))
            .expect(1)
            .create();
        let fresh = server
            .mock("POST", "/api/chat")
            .match_header("X-CyClaw-CSRF", "tok12345")
            .with_body(r#"{"session_id":"abc","reply":"back","model":"m"}"#)
            .expect(1)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        c.csrf = Some("stale-token-1".into());
        assert_eq!(c.chat("abc", "hi").unwrap().reply, "back");
        console.assert();
        stale.assert();
        fresh.assert();
    }

    #[test]
    fn csrf_retry_happens_once_then_reports_forbidden() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let chat = server
            .mock("POST", "/api/chat")
            .with_status(403)
            .with_body(harness_error("CSRF_TOKEN_INVALID"))
            .expect(2)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(
            c.chat("abc", "hi"),
            Err(ClientError::Http { status: 403, ref code }) if code == "CSRF_TOKEN_INVALID"
        ));
        chat.assert();
    }

    #[test]
    fn cleared_session_is_session_gone_and_rate_limit_is_distinct() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let _chat = server
            .mock("POST", "/api/chat")
            .with_status(404)
            .with_body(harness_error("HARNESS_SESSION_ERROR"))
            .create();
        let _status = server
            .mock("GET", "/api/status")
            .with_status(429)
            .with_header("retry-after", "3")
            .with_body(harness_error("RATE_LIMIT"))
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        assert!(matches!(c.chat("abc", "hi"), Err(ClientError::SessionGone)));
        assert!(matches!(c.status(), Err(ClientError::RateLimited)));
    }

    #[test]
    fn clients_sharing_a_jar_share_the_login() {
        let mut server = mockito::Server::new();
        let _login = server
            .mock("POST", "/api/auth/login")
            .with_header(
                "set-cookie",
                "cgagentharness_session=s3cr3t; HttpOnly; SameSite=Strict; Path=/",
            )
            .with_body(r#"{"username":"admin","role":"admin","must_change_password":false}"#)
            .create();
        let status = server
            .mock("GET", "/api/status")
            .match_header("cookie", "cgagentharness_session=s3cr3t")
            .with_body(r#"{"model":"m","provider":"ollama","api_key_optional":true}"#)
            .create();
        let jar = Arc::new(Jar::default());
        let mut chat = Client::with_jar(origin_for(&server), Arc::clone(&jar)).unwrap();
        let poller = Client::with_jar(origin_for(&server), jar).unwrap();
        chat.login("admin", "pw").unwrap();
        assert_eq!(poller.status().unwrap().model, "m");
        status.assert();
        // A client with its own jar stays logged out.
        let other = server
            .mock("GET", "/api/status")
            .match_header("cookie", mockito::Matcher::Missing)
            .with_body(r#"{"version":"1","auth_enabled":true,"status":"login required for operational details"}"#)
            .create();
        assert!(Client::new(origin_for(&server))
            .unwrap()
            .status()
            .unwrap()
            .model
            .is_empty());
        other.assert();
    }

    #[test]
    fn chat_reply_web_tools_defaults_empty() {
        let mut server = mockito::Server::new();
        let _html = server.mock("GET", "/").with_body(html_ok()).create();
        let _chat = server
            .mock("POST", "/api/chat")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"session_id":"abc","reply":"hi","model":"qwen"}"#)
            .create();
        let mut c = Client::new(origin_for(&server)).unwrap();
        let r = c.chat("abc", "hi").unwrap();
        assert!(r.web_tools.is_empty());
    }
}
