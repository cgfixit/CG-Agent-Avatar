//! The only HTTP paths this process is allowed to touch.
//! Agent/write/loop routes are not in this set and must stay uncallable.

pub const GET_ROOT: &str = "/";
pub const GET_STATUS: &str = "/api/status";
pub const GET_SESSIONS: &str = "/api/sessions";
pub const POST_SESSIONS: &str = "/api/sessions";
pub const POST_CHAT: &str = "/api/chat";
/// Fresh cg-agent-harness homes require account login (tls.enabled +
/// auth.enabled both default true). This route is the harness's own public,
/// no-session, no-CSRF login endpoint (see cg-agent-harness
/// src/server/routes/mod.rs: `/api/auth/login` sits in its `auth_open`
/// router, outside the CSRF layer; `account_gate` lists it as public).
/// The companion may also change the authenticated account's own password
/// after a required bootstrap reset. It cannot manage other users.
pub const POST_AUTH_LOGIN: &str = "/api/auth/login";
pub const POST_AUTH_PASSWORD: &str = "/api/auth/password";

pub const ALLOWED_GET: &[&str] = &[GET_ROOT, GET_STATUS, GET_SESSIONS];
pub const ALLOWED_POST: &[&str] = &[
    POST_SESSIONS,
    POST_CHAT,
    POST_AUTH_LOGIN,
    POST_AUTH_PASSWORD,
];

/// Routes the companion must never call. Documented so CI can lock the list.
/// Every entry is a real, exact path from cg-agent-harness's
/// `REGISTERED_PATHS` (src/server/routes/mod.rs); parameterised routes such
/// as `/api/sessions/{session_id}/adopt` are excluded by the allowlist's
/// exact-match rule without needing a row here.
pub const FORBIDDEN: &[&str] = &[
    // Agent execution, scheduling, and write surfaces.
    "/api/agent/run",
    "/api/agent/jobs",
    "/api/agent/runs",
    "/api/agent/schedules",
    "/api/mcp/call",
    "/api/config/reload",
    "/api/ollama/pull",
    "/api/model",
    // Chat and session mutation beyond one turn in one owned session.
    "/api/chat/cancel",
    "/api/chat/attachments",
    "/api/sessions/clear",
    // Personality, memory, and key material.
    "/api/soul",
    "/api/soul/proposals",
    "/api/keys",
    "/api/memory/add",
    "/api/memory/clear",
    "/api/structured-memory/purge",
    "/api/structured-memory/gates",
    // The harness's own web access (this app never goes off-loopback via it).
    "/api/web/fetch",
    "/api/web/search",
    "/api/web/inject",
    "/api/web/research",
    "/api/web/allow",
    "/api/web/deny",
    // Account administration and bootstrap.
    "/api/auth/logout",
    "/api/auth/users",
    "/api/auth/bootstrap-password",
];

pub fn is_allowed_get(path: &str) -> bool {
    ALLOWED_GET.contains(&path)
}

pub fn is_allowed_post(path: &str) -> bool {
    ALLOWED_POST.contains(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_is_tiny_and_exact() {
        assert!(is_allowed_get("/"));
        assert!(is_allowed_get("/api/status"));
        assert!(is_allowed_get("/api/sessions"));
        assert!(is_allowed_post("/api/chat"));
        assert!(is_allowed_post("/api/sessions"));
        assert!(is_allowed_post("/api/auth/login"));
        assert!(is_allowed_post("/api/auth/password"));
        assert!(!is_allowed_get("/api/status/"));
        assert!(!is_allowed_get("/api/status?x=1"));
        assert!(!is_allowed_post("/api/chat/"));
        assert!(!is_allowed_get("/api/agent/run"));
        assert!(!is_allowed_post("/api/agent/run"));
        assert!(!is_allowed_post("/api/auth/logout"));
        assert!(!is_allowed_post("/api/auth/users"));
    }

    #[test]
    fn forbidden_and_allowed_do_not_overlap() {
        for p in FORBIDDEN {
            assert!(!is_allowed_get(p), "{p}");
            assert!(!is_allowed_post(p), "{p}");
        }
    }

    #[test]
    fn forbidden_entries_are_exact_unique_api_paths() {
        let mut seen = std::collections::BTreeSet::new();
        for p in FORBIDDEN {
            assert!(p.starts_with("/api/"), "{p}");
            assert!(!p.ends_with('/'), "{p}");
            assert!(!p.contains('{'), "parameterised route needs no row: {p}");
            assert!(seen.insert(*p), "duplicate FORBIDDEN entry {p}");
        }
        // Sibling routes of an allowlisted path stay out: the allowlist is an
        // exact match, so these never reach the harness even by accident.
        for p in [
            "/api/chat/attachments",
            "/api/sessions/clear",
            "/api/auth/logout",
        ] {
            assert!(FORBIDDEN.contains(&p), "{p}");
            assert!(!is_allowed_post(p), "{p}");
        }
    }
}
