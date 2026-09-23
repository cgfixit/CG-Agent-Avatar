//! Source-level contracts. If someone adds an agent route to the client, CI fails.

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
    // Lookups happen only on an explicit request.
    use cg_agent::web_intent::{parse, Intent};
    assert_eq!(parse("what happened in the news today?"), Intent::Chat);
    assert_eq!(parse("tell me about ollama.com"), Intent::Chat);
}
