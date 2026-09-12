//! The only HTTP paths this process is allowed to touch.
//! Agent/write/loop routes are not in this set and must stay uncallable.

pub const GET_ROOT: &str = "/";
pub const GET_STATUS: &str = "/api/status";
pub const GET_SESSIONS: &str = "/api/sessions";
pub const POST_SESSIONS: &str = "/api/sessions";
pub const POST_CHAT: &str = "/api/chat";

pub const ALLOWED_GET: &[&str] = &[GET_ROOT, GET_STATUS, GET_SESSIONS];
pub const ALLOWED_POST: &[&str] = &[POST_SESSIONS, POST_CHAT];

/// Routes the companion must never call. Documented so CI can lock the list.
pub const FORBIDDEN: &[&str] = &[
    "/api/agent/run",
    "/api/agent/jobs",
    "/api/agent/runs",
    "/api/chat/cancel",
    "/api/sessions/clear",
    "/api/soul",
    "/api/keys",
    "/api/web/fetch",
    "/api/web/search",
    "/api/web/inject",
    "/api/memory/add",
    "/api/memory/clear",
    "/api/auth/login",
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
        assert!(!is_allowed_get("/api/status/"));
        assert!(!is_allowed_get("/api/status?x=1"));
        assert!(!is_allowed_post("/api/chat/"));
        assert!(!is_allowed_get("/api/agent/run"));
        assert!(!is_allowed_post("/api/agent/run"));
    }

    #[test]
    fn forbidden_and_allowed_do_not_overlap() {
        for p in FORBIDDEN {
            assert!(!is_allowed_get(p), "{p}");
            assert!(!is_allowed_post(p), "{p}");
        }
    }
}
