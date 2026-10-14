//! Treat model output as untrusted text. Never HTML, never a URL opener.

const MAX_BUBBLE_CHARS: usize = 400;
const MAX_EXPANDED_CHARS: usize = 8_000;

pub fn bubble_text(raw: &str) -> String {
    sanitized_text(raw, MAX_BUBBLE_CHARS)
}

pub fn expanded_text(raw: &str) -> String {
    sanitized_text(raw, MAX_EXPANDED_CHARS)
}

#[derive(Clone, Copy)]
enum Esc {
    None,
    Start,
    Csi,
    Osc,
}

fn sanitized_text(raw: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut chars = 0usize;
    let mut esc = Esc::None;
    for c in raw.chars() {
        match esc {
            Esc::None if c == '\u{1b}' => {
                esc = Esc::Start;
                continue;
            }
            Esc::None => {}
            Esc::Start => {
                esc = match c {
                    '[' => Esc::Csi,
                    ']' => Esc::Osc,
                    // Two-character escape: drop the introducer and this byte.
                    _ => Esc::None,
                };
                continue;
            }
            // CSI ends at its final byte (0x40..=0x7E), not at "any letter",
            // so text after a stray ESC is not swallowed.
            Esc::Csi => {
                if ('\u{40}'..='\u{7e}').contains(&c) {
                    esc = Esc::None;
                }
                continue;
            }
            // OSC ends at BEL or ST (ESC \).
            Esc::Osc => {
                if c == '\u{7}' {
                    esc = Esc::None;
                } else if c == '\u{1b}' {
                    esc = Esc::Start;
                }
                continue;
            }
        }
        if c == '\0' {
            continue;
        }
        if c.is_control() && c != '\n' && c != '\t' {
            continue;
        }
        if is_bidi_control(c) {
            continue;
        }
        // Ellipsis only when more text was dropped, so a reply of exactly
        // `max_chars` is shown whole.
        if chars == max_chars {
            out.push('…');
            break;
        }
        out.push(c);
        chars += 1;
    }
    out
}

/// Fits already-sanitized preview text into the fixed-height reply bubble.
///
/// `chars_per_line` is the theme's wrap estimate. Runs of blank lines collapse
/// to one, and anything past `max_lines` wrapped rows ends in `…`, so a list or
/// a many-paragraph reply is never clipped mid-line with no sign more exists
/// behind **See More**.
pub fn fit_lines(text: &str, chars_per_line: usize, max_lines: usize) -> String {
    let per_row = chars_per_line.max(1);
    let mut out = String::new();
    let mut used = 0usize;
    let mut blank_run = 0usize;
    let mut first = true;
    for line in text.split('\n') {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        let rows = line.chars().count().max(1).div_ceil(per_row);
        if used + rows > max_lines {
            let room = max_lines.saturating_sub(used);
            if room > 0 {
                let kept: String = line.chars().take(room * per_row).collect();
                if !first {
                    out.push('\n');
                }
                out.push_str(kept.trim_end());
            }
            out.push('…');
            return out;
        }
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(line);
        used += rows;
    }
    out
}

/// Bidirectional embeddings, overrides, isolates and marks can make text
/// display in a different order than it reads (CVE-2021-42574, "Trojan
/// Source"). They are format characters, so `is_control` keeps them.
fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
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
    append_footer(reply, &format!(" [via web ×{tool_count}]"))
}

/// `reply` followed by `footer`, trimming the reply (with `…`) so the pair
/// still fits the expanded view. The footer discloses what left the machine,
/// so a very long reply must lose its own tail, never the footer. Counts raw
/// characters, which is at least as many as survive sanitizing.
fn append_footer(reply: &str, footer: &str) -> String {
    // Sanitize the untrusted reply on its own first. Concatenating raw would
    // let an unterminated escape at the end of the reply (`ESC ]`) swallow the
    // trusted footer when the pair is sanitized for display. Sanitizing is
    // idempotent, so the display pass leaves this text unchanged.
    let reply = sanitized_text(reply, usize::MAX);
    let room = MAX_EXPANDED_CHARS.saturating_sub(footer.chars().count());
    if reply.chars().count() <= room {
        return format!("{reply}{footer}");
    }
    let kept: String = reply.chars().take(room.saturating_sub(1)).collect();
    format!("{kept}…{footer}")
}

/// Longest part of a lookup note that echoes what was sent, in characters.
const LOOKUP_NOTE_CHARS: usize = 60;

/// Note appended to a Direct Ollama reply that used the web, naming what left
/// the machine (`what` is the search words, or the link without its scheme),
/// whitespace-collapsed and clipped. Harness replies keep the count-only
/// [`with_web_tools_note`], because Harness decides its own queries.
pub fn with_lookup_note(reply: &str, what: &str) -> String {
    let what = what.split_whitespace().collect::<Vec<_>>().join(" ");
    let shown: String = what.chars().take(LOOKUP_NOTE_CHARS).collect();
    let ellipsis = if what.chars().count() > LOOKUP_NOTE_CHARS {
        "…"
    } else {
        ""
    };
    append_footer(reply, &format!(" [via web: {shown}{ellipsis}]"))
}

/// Footer for a local answer to a thin search request: nothing was sent
/// online, and this is the exact phrasing that will search. `query` is clipped
/// like a lookup note.
pub fn with_search_offer(reply: &str, query: &str) -> String {
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    let shown: String = query.chars().take(LOOKUP_NOTE_CHARS).collect();
    let ellipsis = if query.chars().count() > LOOKUP_NOTE_CHARS {
        "…"
    } else {
        ""
    };
    append_footer(
        reply,
        &format!("\n\nAnswered on this Mac only. To look it up online, say: search the web for {shown}{ellipsis}"),
    )
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
    fn drops_bidi_overrides_but_keeps_emoji_joiners() {
        assert_eq!(
            bubble_text("pay \u{202E}lmth.exe\u{202C} now \u{2066}x\u{2069}"),
            "pay lmth.exe now x"
        );
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        assert_eq!(bubble_text(family), family);
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

    #[test]
    fn lookup_note_names_what_was_sent_and_clips_it() {
        assert_eq!(
            with_lookup_note("hi", "searched \"rust  release\n date\""),
            "hi [via web: searched \"rust release date\"]"
        );
        let long = with_lookup_note("hi", &"x".repeat(100));
        assert_eq!(long, format!("hi [via web: {}…]", "x".repeat(60)));
    }

    #[test]
    fn search_offer_gives_the_exact_phrase_and_clips_long_queries() {
        assert_eq!(
            with_search_offer("A rust is an oxide.", "rust"),
            "A rust is an oxide.\n\nAnswered on this Mac only. To look it up online, say: search the web for rust"
        );
        assert!(with_search_offer("r", &"y".repeat(90)).ends_with(&format!("{}…", "y".repeat(60))));
    }

    #[test]
    fn exact_length_text_gets_no_ellipsis() {
        let exact = "x".repeat(MAX_BUBBLE_CHARS);
        assert_eq!(bubble_text(&exact), exact);
        let over = "x".repeat(MAX_BUBBLE_CHARS + 1);
        assert_eq!(bubble_text(&over).chars().count(), MAX_BUBBLE_CHARS + 1);
        assert!(bubble_text(&over).ends_with('…'));
        // Dropped trailing controls are not "more text".
        assert_eq!(bubble_text(&format!("{exact}\0\u{7}")), exact);
    }

    #[test]
    fn fit_lines_keeps_text_that_fits_untouched() {
        let hint = "A little company for your next idea.\nAsk a question below.";
        assert_eq!(fit_lines(hint, 72, 6), hint);
        assert_eq!(fit_lines("", 72, 6), "");
        assert_eq!(fit_lines("\nlead", 72, 6), "\nlead");
    }

    #[test]
    fn fit_lines_collapses_blank_runs_and_marks_overflow() {
        assert_eq!(fit_lines("a\n\n\n\nb", 72, 6), "a\n\nb");
        let list = (1..=10)
            .map(|n| format!("{n}. step"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            fit_lines(&list, 72, 6),
            "1. step\n2. step\n3. step\n4. step\n5. step\n6. step…"
        );
        // Exactly full is not overflow.
        assert_eq!(fit_lines("a\nb", 72, 2), "a\nb");
    }

    #[test]
    fn fit_lines_cuts_a_long_wrapped_line_to_the_remaining_rows() {
        let text = format!("intro\n{}", "word ".repeat(100));
        let out = fit_lines(&text, 10, 3);
        // Two rows of ten characters remain after "intro".
        assert_eq!(out, "intro\nword word word word…");
    }

    #[test]
    fn a_stray_escape_does_not_swallow_the_reply() {
        assert_eq!(
            bubble_text("hi \u{1b}你好世界 this is the real answer"),
            "hi 好世界 this is the real answer"
        );
        assert_eq!(bubble_text("a\u{1b}[1;31mb\u{1b}[0mc"), "abc");
        assert_eq!(bubble_text("a\u{1b}]0;title\u{7}b"), "ab");
        assert_eq!(bubble_text("a\u{1b}]8;;http://x\u{1b}\\link"), "alink");
        assert_eq!(bubble_text("end\u{1b}"), "end");
    }

    #[test]
    fn footers_survive_a_reply_that_fills_the_expanded_view() {
        let long = "x".repeat(9_000);
        for (full, footer) in [
            (
                with_lookup_note(&long, "searched \"q\""),
                "[via web: searched \"q\"]",
            ),
            (with_web_tools_note(&long, 2), "[via web ×2]"),
            (with_search_offer(&long, "rust"), "search the web for rust"),
        ] {
            let shown = expanded_text(&full);
            assert!(
                shown.ends_with(footer),
                "{footer}: {}",
                &shown[shown.len() - 60..]
            );
            assert!(shown.chars().count() <= MAX_EXPANDED_CHARS);
            assert!(
                shown.contains("x…"),
                "the reply, not the footer, is trimmed"
            );
        }
        // A reply that fits is untouched.
        assert_eq!(
            with_lookup_note("short", "searched \"q\""),
            "short [via web: searched \"q\"]"
        );
        // Exactly full leaves no room for a trim marker beyond the footer.
        let exact = "y".repeat(MAX_EXPANDED_CHARS - " [via web ×1]".chars().count());
        assert_eq!(
            with_web_tools_note(&exact, 1),
            format!("{exact} [via web ×1]")
        );
    }

    #[test]
    fn an_unterminated_escape_in_the_reply_cannot_swallow_the_footer() {
        for hostile in [
            "answer\u{1b}]",
            "answer\u{1b}[",
            "answer\u{1b}",
            "answer\u{1b}]0;t",
        ] {
            for (full, footer) in [
                (
                    with_lookup_note(hostile, "searched \"q\""),
                    "answer [via web: searched \"q\"]",
                ),
                (with_web_tools_note(hostile, 1), "answer [via web ×1]"),
                (
                    with_search_offer(hostile, "rust"),
                    "answer\n\nAnswered on this Mac only. To look it up online, say: search the web for rust",
                ),
            ] {
                // Sanitizing drops the dangling escape and what it swallowed;
                // the footer is intact either way.
                let expected = footer;
                assert_eq!(expanded_text(&full), expected, "{hostile:?}");
                assert_eq!(bubble_text(&full), expected, "{hostile:?}");
            }
        }
    }
}
