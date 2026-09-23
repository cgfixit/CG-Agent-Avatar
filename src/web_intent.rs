//! Explicit web-lookup requests in plain words. Pure parsing, no I/O.
//!
//! Direct Ollama only reaches the web when the user literally asks for it:
//! "search the web for …", "look up …", "google …", or "read <link>". Any
//! other message stays a local chat, so nothing leaves the machine by
//! accident. The lookup itself runs through the local Ollama daemon (see
//! `ollama.rs`); this module only decides whether one was requested.

use std::net::{Ipv4Addr, Ipv6Addr};

use url::{Host, Url};

/// Longest search query forwarded, in characters.
pub const MAX_QUERY_CHARS: usize = 300;
const MAX_LINK_BYTES: usize = 2048;

/// Leading filler that doesn't change the request ("hey, can you please …").
const POLITE: &[&str] = &[
    "please",
    "pls",
    "hey",
    "ok",
    "okay",
    "can you",
    "could you",
    "would you",
    "will you",
];

/// Search verbs, longest first. Bare "search"/"research" are deliberately
/// absent: "search results look wrong…" or "research shows…" is chat.
const SEARCH: &[&str] = &[
    "search the web for",
    "search the internet for",
    "search online for",
    "search the web",
    "search the internet",
    "search online",
    "web search for",
    "web search",
    "search for",
    "look up",
    "google",
];

/// Verbs that read a link, when the message also contains an http(s) URL.
const FETCH: &[&str] = &["read", "open", "fetch", "summarize", "summarise", "visit"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Plain local chat. No web access.
    Chat,
    /// Search the web for this query.
    Search(String),
    /// Read this public page.
    Fetch(Url),
    /// Asked to read a link that is not a public http(s) page
    /// (loopback, private network, `.local`, embedded credentials, …).
    RefusedLink,
}

pub fn parse(message: &str) -> Intent {
    let rest = strip_polite(message.trim());
    if let Some(after) = FETCH.iter().find_map(|verb| strip_verb(rest, verb)) {
        if let Some(link) = first_link(after) {
            return public_link(link).map_or(Intent::RefusedLink, Intent::Fetch);
        }
    }
    if let Some(after) = SEARCH.iter().find_map(|verb| strip_verb(rest, verb)) {
        let query = clean_query(after);
        if !query.is_empty() {
            return Intent::Search(query);
        }
    }
    Intent::Chat
}

/// ASCII-case-insensitive prefix strip. `prefix` must be ASCII; `str::get`
/// returns `None` instead of panicking when the cut isn't a char boundary.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// Strips `word` only when a separator follows it, so "googled" or
/// "okapi" never count as "google" or "ok".
fn strip_word<'a>(s: &'a str, word: &str, separators: &[char]) -> Option<&'a str> {
    let rest = strip_prefix_ci(s, word)?;
    let next = rest.chars().next()?;
    (next.is_whitespace() || separators.contains(&next))
        .then(|| rest.trim_start_matches(|c: char| c.is_whitespace() || separators.contains(&c)))
}

fn strip_verb<'a>(s: &'a str, verb: &str) -> Option<&'a str> {
    strip_word(s, verb, &[':'])
}

fn strip_polite(mut s: &str) -> &str {
    while let Some(rest) = POLITE.iter().find_map(|w| strip_word(s, w, &[','])) {
        s = rest;
    }
    s
}

fn clean_query(raw: &str) -> String {
    const TAIL: &str = " please";
    let mut q = raw.trim().trim_end_matches(['?', '.', '!']).trim_end();
    if let Some(cut) = q.len().checked_sub(TAIL.len()) {
        if q.get(cut..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(TAIL))
        {
            q = q[..cut].trim_end_matches([',', ' ']);
        }
    }
    q.chars().take(MAX_QUERY_CHARS).collect()
}

/// First `http://` or `https://` token, without trailing sentence punctuation.
fn first_link(s: &str) -> Option<&str> {
    let start = s.char_indices().map(|(i, _)| i).find(|&i| {
        let tail = &s[i..];
        strip_prefix_ci(tail, "https://").is_some() || strip_prefix_ci(tail, "http://").is_some()
    })?;
    let token = s[start..]
        .split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\''))
        .next()?;
    Some(token.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}']))
}

/// A link Ollama's cloud may fetch on the user's behalf: http(s), no
/// credentials, and a public host. Private hosts are refused so internal
/// names and addresses are never sent off the machine.
fn public_link(raw: &str) -> Option<Url> {
    if raw.len() > MAX_LINK_BYTES {
        return None;
    }
    let mut url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let public = match url.host()? {
        Host::Domain(domain) => public_domain(domain),
        Host::Ipv4(ip) => public_v4(ip),
        Host::Ipv6(ip) => public_v6(ip),
    };
    if !public {
        return None;
    }
    url.set_fragment(None);
    Some(url)
}

fn public_domain(domain: &str) -> bool {
    let d = domain.trim_end_matches('.').to_ascii_lowercase();
    d.contains('.')
        && ![".localhost", ".local", ".internal", ".lan", ".home.arpa"]
            .iter()
            .any(|suffix| d.ends_with(suffix))
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || a >= 240
        || (a == 100 && (64..128).contains(&b))
        || (a == 198 && (b == 18 || b == 19))
        || (a == 192 && b == 0 && c == 0))
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let first = ip.segments()[0];
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first & 0xffc0) == 0xfec0
        || (first == 0x2001 && ip.segments()[1] == 0x0db8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(q: &str) -> Intent {
        Intent::Search(q.to_string())
    }

    fn fetch(u: &str) -> Intent {
        Intent::Fetch(Url::parse(u).unwrap())
    }

    #[test]
    fn plain_search_phrasings() {
        for (msg, q) in [
            (
                "search the web for best pizza in Atlanta",
                "best pizza in Atlanta",
            ),
            (
                "Search the internet for Rust 1.90 release date?",
                "Rust 1.90 release date",
            ),
            (
                "search online for cheap flights to Denver",
                "cheap flights to Denver",
            ),
            ("web search: tokio vs smol", "tokio vs smol"),
            (
                "search for the latest macOS version",
                "the latest macOS version",
            ),
            (
                "look up the weather in Paris please",
                "the weather in Paris",
            ),
            ("Google: rust async book", "rust async book"),
            ("LOOK UP café opening hours", "café opening hours"),
            ("search the web for 東京 weather", "東京 weather"),
        ] {
            assert_eq!(parse(msg), search(q), "{msg}");
        }
    }

    #[test]
    fn politeness_is_ignored() {
        for msg in [
            "please look up ollama release notes",
            "hey, can you please look up ollama release notes?",
            "Could you look up ollama release notes",
            "ok google ollama release notes",
        ] {
            assert_eq!(parse(msg), search("ollama release notes"), "{msg}");
        }
    }

    #[test]
    fn everything_else_stays_local_chat() {
        for msg in [
            "search results look wrong in my app",
            "research shows sleep matters",
            "googled it already, no luck",
            "what's the weather like today?",
            "search for",
            "look up",
            "read me a poem",
            "open the pod bay doors",
            "okapi facts",
            "ñ search the web for x",
            "",
        ] {
            assert_eq!(parse(msg), Intent::Chat, "{msg}");
        }
    }

    #[test]
    fn read_verbs_with_a_link_fetch_it() {
        assert_eq!(
            parse("read https://example.com/a."),
            fetch("https://example.com/a")
        );
        assert_eq!(
            parse("Summarize this: https://Example.com/x?y=1#frag"),
            fetch("https://example.com/x?y=1")
        );
        assert_eq!(
            parse("can you open https://example.com/pricing and tell me the cheapest plan"),
            fetch("https://example.com/pricing")
        );
        assert_eq!(
            parse("please fetch (https://news.example.org/story)"),
            fetch("https://news.example.org/story")
        );
    }

    #[test]
    fn private_or_credentialed_links_are_refused() {
        for msg in [
            "read http://127.0.0.1:8790/",
            "read http://127.1/",
            "read http://192.168.1.10/admin",
            "read http://10.0.0.8/",
            "read http://100.64.1.1/",
            "read http://169.254.169.254/latest/meta-data",
            "read http://[::1]/",
            "read http://[fd00::1]/",
            "read http://[fe80::1]/",
            "read http://[fec0::1]/",
            "read http://[::ffff:192.168.0.1]/",
            "open http://localhost:3000",
            "read http://printer.local/",
            "read http://intranet/",
            "read https://user:pw@example.com/",
        ] {
            assert_eq!(parse(msg), Intent::RefusedLink, "{msg}");
        }
    }

    #[test]
    fn non_http_links_are_not_fetched() {
        assert_eq!(parse("read ftp://example.com/file"), Intent::Chat);
        assert_eq!(parse("read file:///etc/passwd"), Intent::Chat);
    }

    #[test]
    fn long_queries_are_clipped() {
        let msg = format!("search for {}", "x".repeat(MAX_QUERY_CHARS + 50));
        match parse(&msg) {
            Intent::Search(q) => assert_eq!(q.chars().count(), MAX_QUERY_CHARS),
            other => panic!("expected search, got {other:?}"),
        }
    }
}
