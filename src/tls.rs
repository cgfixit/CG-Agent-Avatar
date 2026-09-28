//! rustls crypto-provider selection.
//!
//! `reqwest` is built with `rustls-no-provider`, so the crate, not reqwest,
//! decides which cryptography backend the TLS stack uses. reqwest's own
//! `rustls` feature would pick `aws-lc-rs`, a C library that needs cmake and
//! ~30 extra crates; this crate keeps `ring`, the provider reqwest 0.12 used,
//! so the migration changes no shipped behavior. reqwest panics inside
//! `Client::build()` when no provider is installed, so every builder in this
//! crate calls [`ensure_crypto_provider`] first.

use std::sync::Once;

static INSTALL: Once = Once::new();

/// Install `ring` as the process-wide rustls provider, once.
///
/// Idempotent and cheap after the first call. If a provider was already
/// installed (only possible in tests that build their own clients), the
/// existing one is kept: `install_default` returns `Err` and that is ignored.
pub fn ensure_crypto_provider() {
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_a_provider_and_is_idempotent() {
        ensure_crypto_provider();
        ensure_crypto_provider();
        let provider = rustls::crypto::CryptoProvider::get_default().expect("provider installed");
        // ring's default suite list is what the pinned client negotiates with.
        assert_eq!(
            provider.cipher_suites.len(),
            rustls::crypto::ring::default_provider().cipher_suites.len()
        );
    }

    #[test]
    fn a_plain_http_client_builds_without_a_tls_target() {
        // reqwest builds the rustls config eagerly even for http:// clients, so a
        // missing provider would panic here rather than at first HTTPS use.
        ensure_crypto_provider();
        reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .expect("client builds once a provider is installed");
    }
}
