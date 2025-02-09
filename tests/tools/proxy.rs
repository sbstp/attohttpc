// This code has been taken from the hyper project and slightly modified: https://github.com/hyperium/hyper/blob/master/examples/http_proxy.rs
// It's needed to create a proxy server for testing.

use http::{uri::Authority, StatusCode};
use hudsucker::{
    certificate_authority::CertificateAuthority,
    certificate_authority::RcgenAuthority,
    hyper::{Request, Response},
    rcgen::{CertificateParams, KeyPair},
    rustls::crypto::aws_lc_rs,
    rustls::ServerConfig,
    *,
};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;

struct NoCa;

impl CertificateAuthority for NoCa {
    async fn gen_server_config(&self, _authority: &Authority) -> Arc<ServerConfig> {
        unreachable!();
    }
}

#[derive(Clone)]
struct AllowHandler;

impl HttpHandler for AllowHandler {}

#[derive(Clone)]
struct DenyHandler;

impl HttpHandler for DenyHandler {
    async fn handle_request(&mut self, _ctx: &HttpContext, _: Request<Body>) -> RequestOrResponse {
        Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())
            .unwrap()
            .into()
    }
}

async fn start_proxy(tls: bool, handler: impl HttpHandler) -> anyhow::Result<u16> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = TcpListener::bind(addr).await?;

    if tls {
        let key_pair = include_str!("key.pem");
        let ca_cert = include_str!("cert.pem");
        let key_pair = KeyPair::from_pem(key_pair).expect("Failed to parse private key");
        let ca_cert = CertificateParams::from_ca_cert_pem(ca_cert)
            .expect("Failed to parse CA certificate")
            .self_signed(&key_pair)
            .expect("Failed to sign CA certificate");

        let ca = RcgenAuthority::new(key_pair, ca_cert, 1_000, aws_lc_rs::default_provider());

        let proxy = Proxy::builder()
            .with_listener(listener)
            .with_ca(ca)
            .with_rustls_client(aws_lc_rs::default_provider())
            .with_http_handler(handler)
            .build()
            .expect("Failed to create proxy");

        tokio::spawn(async move {
            proxy.start().await.unwrap();
        });
    } else {
        let proxy = Proxy::builder()
            .with_listener(listener)
            .with_ca(NoCa)
            .with_client(Client::builder(TokioExecutor::new()).build_http())
            .with_http_handler(handler)
            .build()
            .expect("Failed to create proxy");

        tokio::spawn(async move {
            proxy.start().await.unwrap();
        });
    }

    println!("Listening on http://{addr}");

    Ok(addr.port())
}

pub async fn start_proxy_server(tls: bool) -> anyhow::Result<u16> {
    start_proxy(tls, AllowHandler).await
}

pub async fn start_refusing_proxy_server(tls: bool) -> anyhow::Result<u16> {
    start_proxy(tls, DenyHandler).await
}
