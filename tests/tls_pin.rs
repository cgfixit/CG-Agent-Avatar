//! The Harness TLS pin, proven with real handshakes on loopback.
//!
//! `Client::new_https` trusts exactly one certificate (`tls_certs_only`). The
//! other TLS tests only feed it malformed PEM, so none of them show that the
//! right certificate is accepted or that any other certificate is refused.
//! These tests stand up a rustls server with a throwaway certificate and drive
//! the real client against it. They also pin `classify_send_error`, which tells
//! `CertMismatch` from `Unreachable` by reading error text: if a reqwest or
//! rustls bump rewords those errors, the mismatch cases fail here instead of
//! silently turning a rejected certificate into "harness asleep", which would
//! let discovery fall back to plain HTTP.
//!
//! Certificates are minted at run time, never committed, so no key-shaped
//! literal reaches secret scanning.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::sync::Arc;
use std::thread;

use cg_agent::client::{Client, ClientError};
use cg_agent::LoopbackOrigin;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

struct TestCert {
    pem: String,
    der: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
}

/// A self-signed leaf (`CA:FALSE`, like the harness's own) valid for `ips`.
fn mint(ips: &[IpAddr]) -> TestCert {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).expect("params");
    params.subject_alt_names = ips
        .iter()
        .map(|ip| rcgen::SanType::IpAddress(*ip))
        .collect();
    params.is_ca = rcgen::IsCa::ExplicitNoCa;
    let key = rcgen::KeyPair::generate().expect("key pair");
    let cert = params.self_signed(&key).expect("self-signed cert");
    TestCert {
        pem: cert.pem(),
        der: cert.der().clone(),
        key: PrivatePkcs8KeyDer::from(key.serialize_der()),
    }
}

fn loopback() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

const STATUS_BODY: &str =
    r#"{"model":"test-model","provider":"test","api_key_optional":true,"version":"0"}"#;

fn respond(stream: &mut (impl Read + Write)) {
    let mut seen = Vec::new();
    let mut buf = [0u8; 1024];
    while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => seen.extend_from_slice(&buf[..n]),
        }
    }
    let reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{STATUS_BODY}",
        STATUS_BODY.len()
    );
    let _ = stream.write_all(reply.as_bytes());
    let _ = stream.flush();
}

/// Serve `/api/status` over TLS with `cert`, returning the loopback port.
fn serve_tls(cert: &TestCert) -> u16 {
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("protocol versions")
    .with_no_client_auth()
    .with_single_cert(
        vec![cert.der.clone()],
        PrivateKeyDer::Pkcs8(cert.key.clone_key()),
    )
    .expect("server config");
    let config = Arc::new(config);
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for tcp in listener.incoming().flatten() {
            let config = Arc::clone(&config);
            thread::spawn(move || {
                let Ok(conn) = rustls::ServerConnection::new(config) else {
                    return;
                };
                let mut tls = rustls::StreamOwned::new(conn, tcp);
                respond(&mut tls);
                tls.conn.send_close_notify();
                let _ = tls.flush();
            });
        }
    });
    port
}

/// Serve the same endpoint over plain HTTP, like a squatter on a TLS home's port.
fn serve_plain() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for mut tcp in listener.incoming().flatten() {
            thread::spawn(move || respond(&mut tcp));
        }
    });
    port
}

fn pinned(pem: &str, port: u16) -> Client {
    Client::new_https(LoopbackOrigin::from_port_https(port), pem.as_bytes()).expect("pinned client")
}

#[test]
fn the_pinned_certificate_is_accepted() {
    let cert = mint(&[loopback()]);
    let port = serve_tls(&cert);
    let status = pinned(&cert.pem, port)
        .status()
        .expect("a server presenting the pinned certificate is trusted");
    assert_eq!(status.model, "test-model");
}

#[test]
fn a_different_valid_certificate_is_a_cert_mismatch() {
    let served = mint(&[loopback()]);
    let other = mint(&[loopback()]);
    let port = serve_tls(&served);
    assert!(
        matches!(
            pinned(&other.pem, port).status(),
            Err(ClientError::CertMismatch)
        ),
        "a certificate that is not the pinned one must surface as CertMismatch"
    );
}

#[test]
fn a_pinned_certificate_that_does_not_cover_loopback_is_rejected() {
    // Pinning is not just byte equality: the name still has to match, so a
    // certificate issued for another address cannot stand in for loopback.
    let cert = mint(&[IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))]);
    let port = serve_tls(&cert);
    assert!(matches!(
        pinned(&cert.pem, port).status(),
        Err(ClientError::CertMismatch)
    ));
}

#[test]
fn a_plain_http_listener_is_never_read_by_the_pinned_client() {
    let port = serve_plain();
    let cert = mint(&[loopback()]);
    assert!(
        pinned(&cert.pem, port).status().is_err(),
        "the pinned client must not accept a plaintext response"
    );
}
