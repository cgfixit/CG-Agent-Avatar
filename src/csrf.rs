//! CSRF token from the harness console HTML. Never log the value.

pub const MAX_HTML_BYTES: usize = 512_000;
pub const MIN_TOKEN: usize = 8;
pub const MAX_TOKEN: usize = 128;

pub fn from_html(html: &str) -> Option<String> {
    if html.len() > MAX_HTML_BYTES {
        return None;
    }
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("csrf-token") {
        let abs = from + rel;
        let tag_start = html[..abs].rfind('<')?;
        let tag_end = abs + html[abs..].find('>')?;
        let tag = &html[tag_start..=tag_end];
        if attr(tag, "name")
            .map(|n| n.eq_ignore_ascii_case("csrf-token"))
            .unwrap_or(false)
        {
            let token = attr(tag, "content")?;
            if let Some(ok) = accept_token(&token) {
                return Some(ok);
            }
        }
        from = abs + 1;
    }
    None
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let idx = lower.find(&key)?;
    let after = tag[idx + key.len()..].trim_start();
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

pub fn accept_token(token: &str) -> Option<String> {
    let n = token.len();
    if !(MIN_TOKEN..=MAX_TOKEN).contains(&n) {
        return None;
    }
    if !token
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    Some(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_meta_content() {
        let html = r#"<html><head><meta name="csrf-token" content="abc123XYZ"></head></html>"#;
        assert_eq!(from_html(html).as_deref(), Some("abc123XYZ"));
    }

    #[test]
    fn extracts_when_attributes_reversed() {
        let html = r#"<meta content="tok_rev12" name="csrf-token">"#;
        assert_eq!(from_html(html).as_deref(), Some("tok_rev12"));
    }

    #[test]
    fn missing_or_empty_is_none() {
        assert_eq!(from_html("<html></html>"), None);
        assert_eq!(from_html(r#"<meta name="csrf-token" content="">"#), None);
        assert_eq!(
            from_html("<meta name=\"csrf-token\" content=\"bad token\">"),
            None
        );
    }

    #[test]
    fn rejects_header_injection_and_short_tokens() {
        assert_eq!(
            from_html("<meta name=\"csrf-token\" content=\"abc\\r\\nX-Evil: 1\">"),
            None
        );
        assert_eq!(
            from_html("<meta name=\"csrf-token\" content=\"tok\nmore\">"),
            None
        );
        assert_eq!(
            from_html(r#"<meta name="csrf-token" content="short">"#),
            None
        );
        assert!(from_html(r#"<meta name="csrf-token" content="tok12345">"#).is_some());
    }

    #[test]
    fn rejects_oversized_html() {
        let mut html = "x".repeat(MAX_HTML_BYTES + 1);
        html.push_str(r#"<meta name="csrf-token" content="tok12345">"#);
        assert_eq!(from_html(&html), None);
    }

    #[test]
    fn accept_token_is_urlsafe_only() {
        assert!(accept_token("tok_123-AB").is_some());
        assert!(accept_token("has.dot").is_none());
        assert!(accept_token("has/slash").is_none());
        assert!(accept_token("").is_none());
    }
}
