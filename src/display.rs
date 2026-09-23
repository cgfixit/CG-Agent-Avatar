//! Treat model output as untrusted text. Never HTML, never a URL opener.

const MAX_BUBBLE_CHARS: usize = 400;
const MAX_EXPANDED_CHARS: usize = 8_000;

pub fn bubble_text(raw: &str) -> String {
    sanitized_text(raw, MAX_BUBBLE_CHARS)
}

pub fn expanded_text(raw: &str) -> String {
    sanitized_text(raw, MAX_EXPANDED_CHARS)
}

fn sanitized_text(raw: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut chars = 0usize;
    let mut in_ansi = false;
    for c in raw.chars() {
        if c == '\u{1b}' {
            in_ansi = true;
            continue;
        }
        if in_ansi {
            if c.is_ascii_alphabetic() {
                in_ansi = false;
            }
            continue;
        }
        if c == '\0' {
            continue;
        }
        if c.is_control() && c != '\n' && c != '\t' {
            continue;
        }
        out.push(c);
        chars += 1;
        if chars >= max_chars {
            out.push('…');
            break;
        }
    }
    out
}

/// Compact note appended when web lookups contributed to a reply: the
/// harness's own web_search/web_fetch tools (see cg-agent-harness
/// src/server/chat_web.rs), or a Direct Ollama lookup the user explicitly
/// asked for (`web_intent`). This app never calls the harness web routes
/// itself (`paths::FORBIDDEN`). Plain text — whatever this returns still
/// passes through `bubble_text` before display, same as the rest of the reply.
pub fn with_web_tools_note(reply: &str, tool_count: usize) -> String {
    if tool_count == 0 {
        return reply.to_string();
    }
    format!("{reply} [via web ×{tool_count}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_tools_note_is_appended_only_when_present() {
        assert_eq!(with_web_tools_note("hi", 0), "hi");
        assert_eq!(with_web_tools_note("hi", 2), "hi [via web ×2]");
    }

    #[test]
    fn strips_ansi_and_controls_keeps_text() {
        let raw = "hello\u{1b}[31mRED\u{1b}[0m\u{07} world";
        let out = bubble_text(raw);
        assert_eq!(out, "helloRED world");
        assert!(!out.contains('\u{1b}'));
        assert!(!out.contains('\u{07}'));
    }

    #[test]
    fn does_not_execute_html() {
        let raw = "<script>alert(1)</script>";
        let out = bubble_text(raw);
        assert_eq!(out, raw);
    }

    #[test]
    fn caps_length() {
        let raw = "x".repeat(1000);
        let out = bubble_text(&raw);
        assert!(out.chars().count() <= MAX_BUBBLE_CHARS + 1);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn drops_nul() {
        assert_eq!(bubble_text("a\0b"), "ab");
    }

    #[test]
    fn expanded_text_keeps_more_but_stays_bounded() {
        let out = expanded_text(&"x".repeat(10_000));
        assert!(out.chars().count() <= MAX_EXPANDED_CHARS + 1);
        assert!(out.ends_with('…'));
    }
}
