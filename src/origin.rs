//! Loopback-only origins. `localhost` is rejected: it can resolve off-loopback.

use thiserror::Error;
use url::Url;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OriginError {
    #[error("url is not a valid http origin")]
    InvalidUrl,
    #[error("only http://127.0.0.1 or http://[::1] is allowed")]
    NotLoopback,
    #[error("https is not used for the local harness")]
    NotHttp,
    #[error("path is not on the companion allowlist")]
    PathNotAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackOrigin {
    url: Url,
}

fn check_loopback_shape(url: &Url) -> Result<(), OriginError> {
    match url.host() {
        Some(url::Host::Ipv4(ip)) if ip.is_loopback() => {}
        Some(url::Host::Ipv6(ip)) if ip.is_loopback() => {}
        _ => return Err(OriginError::NotLoopback),
    }
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(OriginError::InvalidUrl);
    }
    let path = url.path();
    if path != "/" && !path.is_empty() {
        return Err(OriginError::InvalidUrl);
    }
    Ok(())
}

impl LoopbackOrigin {
    pub fn from_port(port: u16) -> Self {
        let url = Url::parse(&format!("http://127.0.0.1:{port}")).expect("static loopback url");
        Self { url }
    }

    pub fn parse(raw: &str) -> Result<Self, OriginError> {
        let url = Url::parse(raw).map_err(|_| OriginError::InvalidUrl)?;
        if url.scheme() != "http" {
            return Err(OriginError::NotHttp);
        }
        check_loopback_shape(&url)?;
        Ok(Self { url })
    }

    /// Opt-in HTTPS variant for a harness home that requires TLS (fresh
    /// homes default to `tls.enabled: true`). Callers must pin a specific
    /// leaf certificate for the connection — this type only restricts the
    /// scheme/host/shape, it does not by itself imply any certificate trust.
    pub fn from_port_https(port: u16) -> Self {
        let url = Url::parse(&format!("https://127.0.0.1:{port}")).expect("static loopback url");
        Self { url }
    }

    pub fn parse_https(raw: &str) -> Result<Self, OriginError> {
        let url = Url::parse(raw).map_err(|_| OriginError::InvalidUrl)?;
        if url.scheme() != "https" {
            return Err(OriginError::NotHttp);
        }
        check_loopback_shape(&url)?;
        Ok(Self { url })
    }

    pub fn is_https(&self) -> bool {
        self.url.scheme() == "https"
    }

    pub fn as_str(&self) -> &str {
        let s = self.url.as_str();
        s.strip_suffix('/').unwrap_or(s)
    }

    pub fn url_for(&self, path: &'static str) -> Result<String, OriginError> {
        if !(crate::paths::is_allowed_get(path) || crate::paths::is_allowed_post(path)) {
            return Err(OriginError::PathNotAllowed);
        }
        if path.contains("..") || path.contains("//") || path.contains('\0') || path.contains('\\')
        {
            return Err(OriginError::PathNotAllowed);
        }
        Ok(format!("{}{path}", self.as_str()))
    }

    /// Join an exact path that is on a caller allowlist (Ollama relay, not harness).
    pub fn url_on_allowlist(
        &self,
        path: &'static str,
        allowed: &[&str],
    ) -> Result<String, OriginError> {
        if !allowed.contains(&path) {
            return Err(OriginError::PathNotAllowed);
        }
        if path.contains("..") || path.contains("//") || path.contains('\0') || path.contains('\\')
        {
            return Err(OriginError::PathNotAllowed);
        }
        if !path.starts_with('/') {
            return Err(OriginError::PathNotAllowed);
        }
        Ok(format!("{}{path}", self.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_port_is_ipv4_loopback() {
        let o = LoopbackOrigin::from_port(8790);
        assert_eq!(o.as_str(), "http://127.0.0.1:8790");
        assert_eq!(
            o.url_for("/api/status").unwrap(),
            "http://127.0.0.1:8790/api/status"
        );
    }

    #[test]
    fn accepts_explicit_ipv4_and_ipv6_loopback() {
        assert!(LoopbackOrigin::parse("http://127.0.0.1:8790").is_ok());
        assert!(LoopbackOrigin::parse("http://[::1]:8790").is_ok());
    }

    #[test]
    fn rejects_localhost_name() {
        assert_eq!(
            LoopbackOrigin::parse("http://localhost:8790").unwrap_err(),
            OriginError::NotLoopback
        );
    }

    #[test]
    fn rejects_non_loopback_and_https_and_credentials() {
        assert_eq!(
            LoopbackOrigin::parse("http://8.8.8.8:8790").unwrap_err(),
            OriginError::NotLoopback
        );
        assert_eq!(
            LoopbackOrigin::parse("https://127.0.0.1:8790").unwrap_err(),
            OriginError::NotHttp
        );
        assert_eq!(
            LoopbackOrigin::parse("http://user:pass@127.0.0.1:8790").unwrap_err(),
            OriginError::InvalidUrl
        );
        assert_eq!(
            LoopbackOrigin::parse("http://127.0.0.1:8790/?x=1").unwrap_err(),
            OriginError::InvalidUrl
        );
        assert_eq!(
            LoopbackOrigin::parse("http://127.0.0.1:8790/foo").unwrap_err(),
            OriginError::InvalidUrl
        );
        assert_eq!(
            LoopbackOrigin::parse("http://127.0.0.1:8790#frag").unwrap_err(),
            OriginError::InvalidUrl
        );
        assert_eq!(
            LoopbackOrigin::parse("http://169.254.169.254:8790").unwrap_err(),
            OriginError::NotLoopback
        );
        assert_eq!(
            LoopbackOrigin::parse("http://0.0.0.0:8790").unwrap_err(),
            OriginError::NotLoopback
        );
    }

    #[test]
    fn https_variant_accepts_loopback_only() {
        let o = LoopbackOrigin::from_port_https(8790);
        assert!(o.is_https());
        assert_eq!(o.as_str(), "https://127.0.0.1:8790");
        assert!(!LoopbackOrigin::from_port(8790).is_https());

        assert!(LoopbackOrigin::parse_https("https://127.0.0.1:8790").is_ok());
        assert!(LoopbackOrigin::parse_https("https://[::1]:8790").is_ok());
        assert_eq!(
            LoopbackOrigin::parse_https("http://127.0.0.1:8790").unwrap_err(),
            OriginError::NotHttp
        );
        assert_eq!(
            LoopbackOrigin::parse_https("https://8.8.8.8:8790").unwrap_err(),
            OriginError::NotLoopback
        );
        assert_eq!(
            LoopbackOrigin::parse_https("https://user:pass@127.0.0.1:8790").unwrap_err(),
            OriginError::InvalidUrl
        );
    }

    #[test]
    fn plain_parse_still_rejects_https_unchanged() {
        // parse() is used broadly by legacy callers; https support must be
        // strictly opt-in via parse_https/from_port_https.
        assert_eq!(
            LoopbackOrigin::parse("https://127.0.0.1:8790").unwrap_err(),
            OriginError::NotHttp
        );
    }

    #[test]
    fn url_for_rejects_forbidden_and_traversal() {
        let o = LoopbackOrigin::from_port(8790);
        assert_eq!(
            o.url_for("/api/agent/run").unwrap_err(),
            OriginError::PathNotAllowed
        );
        assert_eq!(
            o.url_for("/api/chat/../agent/run").unwrap_err(),
            OriginError::PathNotAllowed
        );
        assert!(o.url_for("/api/chat").is_ok());
    }
}
