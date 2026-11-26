//! Source and serialized-request contracts for the current client boundaries.

#[test]
fn client_never_mentions_agent_routes() {
    let src = include_str!("../src/client.rs");
    assert!(
        !src.contains("/api/agent"),
        "client.rs must not mention /api/agent"
    );
    assert!(
        !src.contains("loop_turn"),
        "client.rs must not send loop_turn"
    );
}

#[test]
fn client_never_sets_forwarding_headers() {
    let src = include_str!("../src/client.rs");
    for h in [
        "X-Forwarded-For",
        "X-Forwarded-Host",
        "X-Forwarded-Proto",
        "X-Real-Ip",
        "Forwarded",
    ] {
        assert!(!src.contains(h), "must not set {h}");
    }
}

#[test]
fn home_never_reads_dotenv() {
    let src = include_str!("../src/home.rs");
    let prod = src.split("#[cfg(test)]").next().expect("prod");
    assert!(
        !prod.contains(".env"),
        "home.rs production must not mention .env"
    );
    assert!(!prod.contains("CGAGENTHARNESS_API_KEY"));
}

#[test]
fn forbidden_paths_are_documented() {
    for p in cg_agent::paths::FORBIDDEN {
        assert!(p.starts_with("/api/"));
        assert!(!cg_agent::paths::is_allowed_get(p));
        assert!(!cg_agent::paths::is_allowed_post(p));
    }
}

#[test]
fn harness_allowlist_is_limited_to_read_chat_and_self_auth() {
    assert_eq!(
        cg_agent::paths::ALLOWED_GET,
        ["/", "/api/status", "/api/sessions"]
    );
    assert_eq!(
        cg_agent::paths::ALLOWED_POST,
        [
            "/api/sessions",
            "/api/chat",
            "/api/auth/login",
            "/api/auth/password",
        ]
    );
}

#[test]
fn chat_serializer_cannot_grow_a_loop_field() {
    let v = cg_agent::client::chat_post_json("hi", "sid");
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["message", "session_id"]);
}

#[test]
fn discover_does_not_scan_or_touch_focus_socket() {
    let src = include_str!("../src/discover.rs");
    let prod = src.split("#[cfg(test)]").next().expect("prod");
    assert!(prod.contains("lsof"));
    assert!(!prod.contains("cgah-desktop"));
    assert!(!prod.contains("/_desktop/ready"));
    assert!(!prod.contains("0.0.0.0"));
}

#[test]
fn launch_uses_bundle_identifier_never_a_hardcoded_path() {
    let src = include_str!("../src/launch.rs");
    assert!(
        !src.contains("/Applications"),
        "launch.rs must resolve the harness app by bundle identifier, not a hardcoded path"
    );
    assert!(
        !src.contains("Command::new"),
        "launch.rs must not spawn a process"
    );
    assert_eq!(
        cg_agent::launch::HARNESS_BUNDLE_ID,
        "com.cgfixit.agent-harness"
    );
}

#[test]
fn ollama_relay_is_loopback_openai_compat_only() {
    let src = include_str!("../src/ollama.rs");
    let prod = src.split("#[cfg(test)]").next().expect("prod");
    assert!(prod.contains("/v1/chat/completions"));
    assert!(!prod.contains("/api/generate"));
    let v = cg_agent::ollama::chat_body_json("hi");
    assert_eq!(v["model"], "qwen3.8:27b-mlx");
    assert!(v.get("loop").is_none());
    // The reply is streamed on that same route, with a token budget that
    // keeps a full reply under the response cap.
    assert_eq!(v["stream"], true);
    assert_eq!(v["max_tokens"], 4096);
}

#[test]
fn streamed_replies_are_read_only_through_the_capped_reader() {
    for (name, src) in [
        ("client.rs", include_str!("../src/client.rs")),
        ("ollama.rs", include_str!("../src/ollama.rs")),
    ] {
        let prod = src.split("#[cfg(test)]").next().expect("prod");
        // One reader per backend, bound to the response cap and the turn's
        // deadline.
        assert_eq!(
            prod.matches("EventStream::new(reader, MAX_BODY, deadline)")
                .count(),
            1,
            "{name}"
        );
        assert_eq!(prod.matches("EventStream::new(").count(), 1, "{name}");
        // Nothing else in a streaming backend reads a body line by line or
        // whole: every other read goes through `http::read_bounded`.
        for unbounded in [
            "BufReader",
            ".lines()",
            "read_line(",
            "read_until(",
            "read_to_string(",
            "read_to_end(",
            ".text()",
            ".bytes()",
            ".chunk()",
            "copy_to(",
        ] {
            assert!(!prod.contains(unbounded), "{name} reads with {unbounded}");
        }
    }
    // The cap itself is the one every other response has.
    let http = include_str!("../src/http.rs");
    assert!(http.contains("pub(crate) const MAX_BODY: u64 = 1_048_576;"));
}

#[test]
fn ollama_web_lookups_stay_on_the_loopback_daemon() {
    let src = include_str!("../src/ollama.rs");
    let prod = src.split("#[cfg(test)]").next().expect("prod");
    // Web access exists only as the local daemon's experimental routes, which
    // the daemon signs with its own `ollama signin` identity.
    assert!(prod.contains("\"/api/experimental/web_search\""));
    assert!(prod.contains("\"/api/experimental/web_fetch\""));
    // The app never talks to Ollama's cloud directly, carries no key, and
    // never offers the model tools.
    for banned in [
        "ollama.com",
        "OLLAMA_API_KEY",
        "Authorization",
        "Bearer",
        "\"tools\"",
        "tool_choice",
    ] {
        assert!(!prod.contains(banned), "ollama.rs mentions {banned}");
    }
    // Plain chat is still exactly the Soul prompt plus the user's message.
    let v = cg_agent::ollama::chat_body_json("hi");
    assert_eq!(v["messages"].as_array().map(Vec::len), Some(2));
    assert!(v.get("tools").is_none());
    // Presentation preferences never replace the capability/lookup rules.
    for style in cg_agent::settings::OutputStyle::ALL {
        assert!(style
            .system_prompt("Prefer short sentences.")
            .unwrap()
            .starts_with(cg_agent::ollama::SYSTEM_PROMPT));
    }
    // Lookups happen only on an explicit request.
    use cg_agent::web_intent::{parse, Intent};
    assert_eq!(parse("what happened in the news today?"), Intent::Chat);
    assert_eq!(parse("tell me about ollama.com"), Intent::Chat);
}

#[test]
fn pasteboard_is_write_only_host_only_and_sanitized() {
    let src = include_str!("../src/app.rs");
    let prod = src.split("#[cfg(test)]").next().expect("prod");
    // One write path: the explicit Copy Reply command.
    assert_eq!(prod.matches("NSPasteboard::generalPasteboard()").count(), 1);
    assert!(prod.contains("NSPasteboardContentsOptions::CurrentHostOnly"));
    assert!(!prod.contains("clearContents"), "use the host-only reset");
    // Avatar never reads what other apps copied.
    for read in [
        "stringForType",
        "dataForType",
        "propertyListForType",
        "readObjectsForClasses",
        "pasteboardItems",
        "availableTypeFromArray",
    ] {
        assert!(!prod.contains(read), "app.rs reads the pasteboard: {read}");
    }
    // What is copied goes through the display sanitizer first.
    assert!(prod.contains("display::copyable_reply("));
    assert_eq!(
        cg_agent::display::copyable_reply("a\u{1b}[2Jb\u{202e}", "a\u{1b}[2Jb\u{202e}", false)
            .as_deref(),
        Some("ab")
    );
}

#[test]
fn unanswered_message_stays_in_memory_and_returns_only_to_the_field() {
    let draft = include_str!("../src/draft.rs");
    let draft = draft.split("#[cfg(test)]").next().expect("prod");
    // The slot is a plain in-memory handoff: no disk, network, environment,
    // preferences, or logging.
    for io in [
        "std::fs",
        "std::io",
        "std::env",
        "std::net",
        "reqwest",
        "NSUserDefaults",
        "println!",
        "eprintln!",
        "dbg!",
        "log::",
        "tracing",
    ] {
        assert!(!draft.contains(io), "draft.rs reaches past memory: {io}");
    }
    let app = include_str!("../src/app.rs");
    let prod = app.split("#[cfg(test)]").next().expect("prod");
    // One way out: the main-thread tick puts it back in the chat field, never
    // resending it by itself.
    assert_eq!(prod.matches(".restore(|| input.stringValue()").count(), 1);
    assert!(prod.contains("input.setStringValue(&NSString::from_str(&message))"));
    // Saved preferences never carry it.
    let persist = prod
        .split("fn persist_settings")
        .nth(1)
        .and_then(|rest| rest.split("\n    fn ").next())
        .expect("persist_settings");
    assert!(!persist.contains("unanswered"));
    // A draft typed while waiting always wins.
    let slot = cg_agent::draft::Unanswered::default();
    slot.hold("old".into());
    assert_eq!(slot.restore(|| "new draft".into()), None);
}
