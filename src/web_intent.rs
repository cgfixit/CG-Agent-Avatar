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

/// Verbs that read a link, when the link is the object of the verb.
const FETCH: &[&str] = &["read", "open", "fetch", "summarize", "summarise", "visit"];

/// How many words may sit between a read verb and its link
/// ("summarize the contents of <link>").
const MAX_LINK_WORDS: usize = 4;

/// First word after a search verb that marks a statement, not a command:
/// "Google is a company", "look up to your parents", "search for and
/// destroy". A command's object starts with a noun, a determiner, or a
/// question word, never one of these.
const NOT_A_COMMAND: &[&str] = &[
    "is", "are", "was", "were", "am", "be", "been", "being", "has", "have", "had", "do", "does",
    "did", "isn't", "aren't", "wasn't", "weren't", "doesn't", "don't", "didn't", "can", "could",
    "will", "would", "should", "shall", "may", "might", "must", "to", "of", "at", "on", "in",
    "from", "with", "by", "and", "or", "but",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Plain local chat. No web access.
    Chat,
    /// Looks like a search request but is too thin to act on (a bare verb with
    /// a one-word object). Answer locally and offer the explicit phrasing.
    Offer(String),
    /// Search the web for this query.
    Search(String),
    /// Read this public page.
    Fetch(Url),
    /// Asked to read a link that is not a public http(s) page
    /// (loopback, private network, `.local`, embedded credentials, …).
    RefusedLink,
}

pub fn parse(message: &str) -> Intent {
    let trimmed = message.trim();
    let rest = strip_polite(trimmed);
    // "please google rust" is unmistakably an imperative; a bare "Google Maps
    // is useful" is not.
    let polite = rest.len() != trimmed.len();
    if let Some(after) = FETCH.iter().find_map(|verb| strip_verb(rest, verb)) {
        if let Some(link) = link_object(after) {
            return public_link(link).map_or(Intent::RefusedLink, Intent::Fetch);
        }
    }
    match search_gate(rest, polite) {
        Gate::Run(query) => Intent::Search(query),
        Gate::Ask(query) => Intent::Offer(query),
        Gate::Skip => Intent::Chat,
    }
}

/// How sure a message is a search command.
enum Gate {
    /// Unmistakable: run the search.
    Run(String),
    /// Command-shaped but ambiguous (a bare verb with a one-word object).
    Ask(String),
    /// Not a search request.
    Skip,
}

fn search_gate(rest: &str, polite: bool) -> Gate {
    let Some((verb, after)) = SEARCH
        .iter()
        .find_map(|verb| strip_verb(rest, verb).map(|after| (*verb, after)))
    else {
        return Gate::Skip;
    };
    let query = clean_query(after);
    let mut words = query.split_whitespace();
    let Some(first) = words.next() else {
        return Gate::Skip;
    };
    let names_the_web = ["web", "internet", "online"]
        .iter()
        .any(|w| verb.contains(w));
    let colon = rest
        .get(verb.len()..)
        .is_some_and(|tail| tail.trim_start().starts_with(':'));
    // Unambiguous commands: a named venue plus "for" ("search the web for is
    // Rust memory safe?") or a colon ("web search: can X do Y?"). Whatever
    // follows is the query, even if it starts like a statement.
    let explicit = (names_the_web && verb.ends_with("for")) || colon;
    // A polite prefix ("please google …", "can you search for …") makes the
    // verb an imperative too. "look up" stays filtered: "look up to" and "look
    // up at" are ordinary phrasal verbs.
    let imperative = explicit || (polite && verb != "look up");
    if !imperative && NOT_A_COMMAND.contains(&bare_word(first).as_str()) {
        return Gate::Skip;
    }
    if explicit || polite {
        Gate::Run(query)
    } else if verb == "google" || names_the_web {
        // "google" is also a company name ("Google Chrome keeps crashing"), and
        // "web search" is also a noun ("Web search results expose bias"): neither
        // may leave the machine on its own. Offer the explicit phrasing instead.
        Gate::Ask(query)
    } else if words.next().is_some() {
        Gate::Run(query)
    } else {
        Gate::Ask(query)
    }
}

/// Lowercased, punctuation-trimmed, with curly apostrophes straightened.
fn bare_word(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '\u{2019}')
        .replace('\u{2019}', "'")
        .to_lowercase()
}

/// The link a read verb applies to: the first link within a few plain words
/// of the verb. A comma, period or other sentence break first, or a colon that
/// does not introduce the link, means the link belongs to another clause:
/// "open question: why does https://x fail" is chat.
fn link_object(after: &str) -> Option<&str> {
    let tokens: Vec<&str> = after.split_whitespace().take(MAX_LINK_WORDS + 1).collect();
    // A real link, not just a word that starts with "http" ("HTTP docs").
    // The scheme must not be glued to a word: "(https://…)" and
    // "[notes](https://…)" count, "xhttps://…" does not.
    let is_link = |token: &str| {
        first_link(token).is_some()
            && token.to_ascii_lowercase().find("http").is_some_and(|at| {
                token[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric())
            })
    };
    for (i, token) in tokens.iter().enumerate() {
        if is_link(token) {
            return first_link(token);
        }
        // A colon introduces the link when it comes next, or after a Markdown
        // label that may span several words ("page: [release notes](https://…)").
        let introduces_link = tokens.get(i + 1).is_some_and(|next| {
            is_link(next) || (next.starts_with('[') && tokens[i + 1..].iter().any(|t| is_link(t)))
        });
        // Only a trailing mark ends the clause; "v1.2" is one word.
        if token.ends_with([',', ';', '.', '!', '?']) || (token.contains(':') && !introduces_link) {
            return None;
        }
    }
    None
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
        && ![
            ".localhost",
            ".localdomain",
            ".local",
            ".internal",
            ".lan",
            ".home.arpa",
        ]
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
    let seg = ip.segments();
    let first = seg[0];
    // Forms that embed or tunnel an IPv4 address (v4-compatible, NAT64, 6to4,
    // Teredo) or discard traffic (100::/64) are refused whole, so a private
    // address cannot be smuggled in as `http://[::7f00:1]/`.
    let tunnels_v4 = seg[..6] == [0; 6]
        || (first == 0x0064 && seg[1] == 0xff9b && seg[2..6] == [0; 4])
        || first == 0x2002
        || (first == 0x2001 && seg[1] == 0)
        || (first == 0x0100 && seg[1..4] == [0; 3]);
    !(tunnels_v4
        || ip.is_loopback()
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

    /// Messages that merely contain a search verb are chat. Each of these was
    /// sent to the cloud before the gate required a command-shaped message.
    #[test]
    fn statements_that_start_with_a_search_verb_stay_local() {
        for msg in [
            "Google is a company, what do you think?",
            "google was founded in 1998",
            "look up to your parents",
            "look up at the sky tonight",
            "search for and destroy missions are fun",
            "google doesn't mean what it used to",
            "Search the web is slow today",
            "web search can be creepy",
            // "look up to/at" stay phrasal verbs even after "please".
            "please look up to your parents",
            "can you look up at the sky",
        ] {
            assert_eq!(parse(msg), Intent::Chat, "{msg}");
        }
    }

    #[test]
    fn named_web_statements_are_offered_not_run() {
        for msg in ["Web search results expose bias", "search the web rust"] {
            assert!(matches!(parse(msg), Intent::Offer(_)), "{msg}");
        }
    }

    #[test]
    fn a_colon_may_follow_whitespace() {
        assert_eq!(parse("google : rust"), Intent::Search("rust".into()));
        assert_eq!(parse("look up : rust"), Intent::Search("rust".into()));
    }

    #[test]
    fn a_period_inside_a_word_does_not_end_the_clause() {
        assert!(matches!(
            parse("read the v1.2 docs at https://example.com"),
            Intent::Fetch(_)
        ));
        assert_eq!(
            parse("read the docs. then https://example.com"),
            Intent::Chat
        );
    }

    #[test]
    fn a_colon_may_introduce_a_multiword_markdown_label() {
        assert!(matches!(
            parse("summarize this page: [release notes](https://example.com/releases)"),
            Intent::Fetch(_)
        ));
        assert_eq!(
            parse("open question: why does https://example.com fail"),
            Intent::Chat
        );
    }

    #[test]
    fn markdown_links_are_read() {
        for msg in [
            "read [the release notes](https://example.com/releases)",
            "read (https://example.com/a)",
        ] {
            assert!(matches!(parse(msg), Intent::Fetch(_)), "{msg}");
        }
        assert_eq!(parse("read xhttps://example.com/a"), Intent::Chat);
    }

    #[test]
    fn thin_bare_requests_are_offered_not_run() {
        for (msg, q) in [
            ("search for meaning", "meaning"),
            ("google rust", "rust"),
            // A bare "google" is also a company name, so even a longer message
            // is only offered, never sent: these were leaked to the cloud.
            ("Google Maps is useful", "Maps is useful"),
            ("Google Chrome keeps crashing", "Chrome keeps crashing"),
            ("google rust 1.90 release date", "rust 1.90 release date"),
        ] {
            assert_eq!(parse(msg), Intent::Offer(q.to_string()), "{msg}");
        }
        // Statements are never offered a search.
        assert_eq!(parse("google is great"), Intent::Chat);
        assert_eq!(parse("look up to me"), Intent::Chat);
    }

    #[test]
    fn commands_with_substance_still_search() {
        for (msg, q) in [
            ("look up rust 1.90 release date", "rust 1.90 release date"),
            ("look up the capital of France", "the capital of France"),
            ("search for what is a monad", "what is a monad"),
            (
                "search for it's a wonderful life cast",
                "it's a wonderful life cast",
            ),
            // A polite prefix or a colon makes the verb unmistakably an imperative,
            // so one word is enough and a statement-like start is still a query.
            ("please google rust", "rust"),
            ("Please look up serendipity?", "serendipity"),
            ("please google is Rust memory safe?", "is Rust memory safe"),
            (
                "can you search for at home COVID tests",
                "at home COVID tests",
            ),
            (
                "please google rust 1.90 release date",
                "rust 1.90 release date",
            ),
            ("can you google what is a monad", "what is a monad"),
            ("Google: rust", "rust"),
            // The venue is named, so one word is enough.
            ("search the web for serendipity", "serendipity"),
            ("web search: rust", "rust"),
            ("search online for it", "it"),
        ] {
            assert_eq!(parse(msg), search(q), "{msg}");
        }
    }

    /// An explicit command keeps its query even when the query starts like a
    /// statement ("is", "can", "at"); only ambiguous bare verbs are filtered.
    #[test]
    fn explicit_searches_may_start_with_an_auxiliary() {
        for (msg, q) in [
            (
                "search the web for is Rust memory safe?",
                "is Rust memory safe",
            ),
            ("web search: can X do Y?", "can X do Y"),
            (
                "search online for at home COVID tests",
                "at home COVID tests",
            ),
            ("google: is it down", "is it down"),
        ] {
            assert_eq!(parse(msg), search(q), "{msg}");
        }
    }

    #[test]
    fn a_link_must_be_near_the_read_verb_in_the_same_clause() {
        for msg in [
            "open question: why does https://example.com/x?k=1 fail",
            "read my notes and then compare with https://example.com/a",
            "visit grandma, she sent https://example.com/a",
            "read the docs. then try https://example.com/a",
        ] {
            assert_eq!(parse(msg), Intent::Chat, "{msg}");
        }
        for (msg, url) in [
            (
                "read the article at https://example.com/a",
                "https://example.com/a",
            ),
            (
                "read HTTP docs at https://example.com/a",
                "https://example.com/a",
            ),
            (
                "read more at https://example.com/a",
                "https://example.com/a",
            ),
            (
                "summarize the contents of https://example.com/a",
                "https://example.com/a",
            ),
            (
                "summarize what happened at https://example.com/a",
                "https://example.com/a",
            ),
            (
                "summarize this page: https://example.com/a",
                "https://example.com/a",
            ),
            (
                "can you open the following link <https://example.com/a>",
                "https://example.com/a",
            ),
            (
                "fetch https://example.com/a and https://example.org/b",
                "https://example.com/a",
            ),
        ] {
            assert_eq!(parse(msg), fetch(url), "{msg}");
        }
    }

    #[test]
    fn long_queries_are_clipped() {
        let msg = format!("search the web for {}", "x".repeat(MAX_QUERY_CHARS + 50));
        match parse(&msg) {
            Intent::Search(q) => assert_eq!(q.chars().count(), MAX_QUERY_CHARS),
            other => panic!("expected search, got {other:?}"),
        }
    }

    #[test]
    fn ipv6_forms_that_embed_a_private_ipv4_are_refused() {
        for link in [
            "http://[::7f00:1]/",
            "http://[::127.0.0.1]/",
            "http://[64:ff9b::7f00:1]/",
            "http://[2002:7f00:1::]/",
            "http://[2001:0:4136:e378:8000:63bf:3fff:fdd2]/",
            "http://[100::1]/",
            "http://localhost.localdomain/x",
        ] {
            assert_eq!(
                parse(&format!("read {link}")),
                Intent::RefusedLink,
                "{link}"
            );
        }
        assert!(matches!(
            parse("read http://[2606:4700:4700::1111]/"),
            Intent::Fetch(_)
        ));
    }
}
