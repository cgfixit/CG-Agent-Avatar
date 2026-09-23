//! Direct loopback Ollama OpenAI-compatible relay. Not the harness.

use std::time::Duration;

use crate::http::{self, ReadError, MAX_BODY};
use crate::origin::{LoopbackOrigin, OriginError};
use crate::validate::{self, ValidateError};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

pub const PORT: u16 = 11434;
pub const MODEL: &str = "qwen3.8:27b-mlx";
pub const POST_CHAT: &str = "/v1/chat/completions";
pub const GET_TAGS: &str = "/api/tags";
const ALLOWED: &[&str] = &[POST_CHAT, GET_TAGS];
const USER_AGENT: &str = "cg-agent/0.1";
const GET_TIMEOUT: Duration = Duration::from_secs(8);
const CHAT_TIMEOUT: Duration = Duration::from_secs(720);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
pub const SYSTEM_PROMPT: &str = include_str!("../resources/direct-ollama-system.md");

#[derive(Debug, Error)]
pub enum OllamaError {
    #[error(transparent)]
    Origin(#[from] OriginError),
    #[error(transparent)]
    Validate(#[from] ValidateError),
    #[error("ollama asleep")]
    Unreachable,
    #[error("ollama http {status}")]
    Http { status: u16 },
    #[error("unexpected json")]
    Json,
    #[error("response too large")]
    ResponseTooLarge,
    #[error("redirect refused")]
    Redirect,
    #[error("model {MODEL} not listed")]
    ModelMissing,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatBody<'a> {
    model: &'static str,
    messages: [ChatMessage<'a>; 2],
    stream: bool,
}

pub struct Ollama {
    origin: LoopbackOrigin,
    http: reqwest::blocking::Client,
}

impl Ollama {
    pub fn new() -> Result<Self, OllamaError> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(CHAT_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|_| OllamaError::Unreachable)?;
        Ok(Self {
            origin: LoopbackOrigin::from_port(PORT),
            http,
        })
    }

    pub fn from_origin(origin: LoopbackOrigin) -> Result<Self, OllamaError> {
        let mut s = Self::new()?;
        s.origin = origin;
        Ok(s)
    }

    fn url(&self, path: &'static str) -> Result<String, OllamaError> {
        Ok(self.origin.url_on_allowlist(path, ALLOWED)?)
    }

    pub fn tags_ok(&self) -> Result<bool, OllamaError> {
        let resp = self
            .http
            .get(self.url(GET_TAGS)?)
            .timeout(GET_TIMEOUT)
            .send()
            .map_err(|_| OllamaError::Unreachable)?;
        let status = resp.status().as_u16();
        if (300..400).contains(&status) {
            return Err(OllamaError::Redirect);
        }
        if status != 200 {
            return Err(OllamaError::Http { status });
        }
        if let Some(len) = resp.content_length() {
            if len > MAX_BODY {
                return Err(OllamaError::ResponseTooLarge);
            }
        }
        let bytes = response_bytes(resp)?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| OllamaError::Json)?;
        let models = v
            .get("models")
            .and_then(|m| m.as_array())
            .ok_or(OllamaError::Json)?;
        Ok(models.iter().any(|m| {
            m.get("name")
                .and_then(|n| n.as_str())
                // Exact tag only: chat always requests MODEL, so a sibling
                // quantization would pass here and then 404 on the first send.
                .is_some_and(|n| n == MODEL)
        }))
    }

    pub fn chat(&self, message: &str) -> Result<String, OllamaError> {
        let message = validate::message(message)?;
        let body = chat_body(message);
        let resp = self
            .http
            .post(self.url(POST_CHAT)?)
            .timeout(CHAT_TIMEOUT)
            .json(&body)
            .send()
            .map_err(|_| OllamaError::Unreachable)?;
        let status = resp.status().as_u16();
        if (300..400).contains(&status) {
            return Err(OllamaError::Redirect);
        }
        // Ollama answers an unknown model with 404; so does a build without the
        // OpenAI-compatible API. Only report ModelMissing when the tag list
        // confirms the model is absent.
        if status == 404 && matches!(self.tags_ok(), Ok(false)) {
            return Err(OllamaError::ModelMissing);
        }
        if status != 200 {
            return Err(OllamaError::Http { status });
        }
        if let Some(len) = resp.content_length() {
            if len > MAX_BODY {
                return Err(OllamaError::ResponseTooLarge);
            }
        }
        let bytes = response_bytes(resp)?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| OllamaError::Json)?;
        v.get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(str::to_string)
            .ok_or(OllamaError::Json)
    }
}

fn chat_body(message: &str) -> ChatBody<'_> {
    ChatBody {
        model: MODEL,
        messages: [
            ChatMessage {
                role: "system",
                content: SYSTEM_PROMPT,
            },
            ChatMessage {
                role: "user",
                content: message,
            },
        ],
        stream: false,
    }
}

fn response_bytes(resp: reqwest::blocking::Response) -> Result<Vec<u8>, OllamaError> {
    http::read_bounded(resp, MAX_BODY).map_err(|e| match e {
        ReadError::Io => OllamaError::Json,
        ReadError::TooLarge => OllamaError::ResponseTooLarge,
    })
}

pub fn chat_body_json(message: &str) -> Value {
    serde_json::to_value(chat_body(message)).expect("ollama chat body")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin_for(server: &mockito::ServerGuard) -> LoopbackOrigin {
        LoopbackOrigin::parse(&server.url()).expect("mockito loopback")
    }

    #[test]
    fn body_has_fixed_model_no_loop_no_tools() {
        let v = chat_body_json("hi");
        assert_eq!(v["model"], MODEL);
        assert_eq!(v["stream"], false);
        assert!(v.get("loop").is_none());
        assert!(v.get("tools").is_none());
        assert_eq!(v["messages"][0]["role"], "system");
        assert_eq!(v["messages"][0]["content"], SYSTEM_PROMPT);
        assert_eq!(v["messages"][1]["role"], "user");
        assert_eq!(v["messages"][1]["content"], "hi");
    }

    #[test]
    fn refuses_non_allowlisted_path() {
        let o = LoopbackOrigin::from_port(PORT);
        assert!(o.url_on_allowlist("/api/generate", ALLOWED).is_err());
        assert!(o.url_on_allowlist(POST_CHAT, ALLOWED).is_ok());
    }

    #[test]
    fn chat_hits_relay_without_forwarding_headers() {
        let mut server = mockito::Server::new();
        let chat = server
            .mock("POST", POST_CHAT)
            .match_header("x-forwarded-for", mockito::Matcher::Missing)
            .match_header("origin", mockito::Matcher::Missing)
            .match_header("authorization", mockito::Matcher::Missing)
            .match_body(mockito::Matcher::Regex(format!("\"model\":\"{MODEL}\"")))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"yo"}}]}"#)
            .create();
        let o = Ollama::from_origin(origin_for(&server)).unwrap();
        assert_eq!(o.chat("hi").unwrap(), "yo");
        chat.assert();
    }

    #[test]
    fn unreachable_is_asleep() {
        let o = Ollama::from_origin(LoopbackOrigin::from_port(1)).unwrap();
        assert!(matches!(o.chat("hi"), Err(OllamaError::Unreachable)));
    }

    #[test]
    fn redirect_refused() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("POST", POST_CHAT)
            .with_status(302)
            .with_header("location", "http://127.0.0.1/x")
            .create();
        let o = Ollama::from_origin(origin_for(&server)).unwrap();
        assert!(matches!(o.chat("hi"), Err(OllamaError::Redirect)));
    }

    #[test]
    fn tags_sees_model() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", GET_TAGS)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"models":[{"name":"qwen3.8:27b-mlx"}]}"#)
            .create();
        let o = Ollama::from_origin(origin_for(&server)).unwrap();
        assert!(o.tags_ok().unwrap());
    }

    #[test]
    fn tags_rejects_sibling_tag_chat_would_not_request() {
        let mut server = mockito::Server::new();
        let _m = server
            .mock("GET", GET_TAGS)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"models":[{"name":"qwen3.8:27b-q4_K_M"},{"name":"qwen3.8:27b"}]}"#)
            .create();
        let o = Ollama::from_origin(origin_for(&server)).unwrap();
        assert!(!o.tags_ok().unwrap());
    }

    #[test]
    fn chat_404_with_model_absent_is_model_missing() {
        let mut server = mockito::Server::new();
        let _chat = server.mock("POST", POST_CHAT).with_status(404).create();
        let tags = server
            .mock("GET", GET_TAGS)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"models":[{"name":"qwen3.8:27b-q4_K_M"}]}"#)
            .create();
        let o = Ollama::from_origin(origin_for(&server)).unwrap();
        assert!(matches!(o.chat("hi"), Err(OllamaError::ModelMissing)));
        tags.assert();
    }

    #[test]
    fn chat_404_with_model_listed_stays_http_404() {
        // e.g. an Ollama build without /v1/chat/completions
        let mut server = mockito::Server::new();
        let _chat = server.mock("POST", POST_CHAT).with_status(404).create();
        let _tags = server
            .mock("GET", GET_TAGS)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"models":[{"name":"qwen3.8:27b-mlx"}]}"#)
            .create();
        let o = Ollama::from_origin(origin_for(&server)).unwrap();
        assert!(matches!(
            o.chat("hi"),
            Err(OllamaError::Http { status: 404 })
        ));
    }

    #[test]
    fn chat_404_with_tags_unavailable_stays_http_404() {
        let mut server = mockito::Server::new();
        let _chat = server.mock("POST", POST_CHAT).with_status(404).create();
        let _tags = server.mock("GET", GET_TAGS).with_status(500).create();
        let o = Ollama::from_origin(origin_for(&server)).unwrap();
        assert!(matches!(
            o.chat("hi"),
            Err(OllamaError::Http { status: 404 })
        ));
    }

    #[test]
    fn empty_message_never_hits_network() {
        let o = Ollama::from_origin(LoopbackOrigin::from_port(1)).unwrap();
        assert!(matches!(
            o.chat("  "),
            Err(OllamaError::Validate(ValidateError::EmptyMessage))
        ));
    }
}
