use std::net::SocketAddr;

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use axum_server::tls_rustls::{from_tcp_rustls, RustlsConfig};

pub async fn start_hello_world_server(tls: bool) -> anyhow::Result<u16> {
    if tls {
        return start_tls_hello_world_server(
            include_bytes!("../assets/cert.pem"),
            include_bytes!("../assets/key.pem"),
        )
        .await;
    }

    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let incoming = tokio::net::TcpListener::bind(&addr).await?;
    let local_addr = incoming.local_addr()?;

    let app = hello_world_router();

    tokio::spawn(async move {
        axum::serve(incoming, app).await.unwrap();
    });

    Ok(local_addr.port())
}

/// Starts a TLS server that presents the given certificate and key, and responds
/// with `hello` to any request on `/`.
pub async fn start_tls_hello_world_server(cert: &'static [u8], key: &'static [u8]) -> anyhow::Result<u16> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let incoming = tokio::net::TcpListener::bind(&addr).await?;
    let local_addr = incoming.local_addr()?;

    let app = hello_world_router();

    let config = RustlsConfig::from_pem(cert.to_vec(), key.to_vec()).await.unwrap();

    tokio::spawn(async move {
        from_tcp_rustls(incoming.into_std().unwrap(), config)
            .expect("should build server")
            .serve(app.into_make_service())
            .await
            .expect("should serve");
    });

    Ok(local_addr.port())
}

/// Starts a TLS server presenting a certificate signed by the test CA for
/// `wronghost.example.com`. A client connecting as `localhost` receives a
/// validly-signed certificate that does not match the hostname, like
/// `wrong.host.badssl.com`.
pub async fn start_wrong_host_server() -> anyhow::Result<u16> {
    start_tls_hello_world_server(
        include_bytes!("../assets/wrong-host-cert.pem"),
        include_bytes!("../assets/wrong-host-key.pem"),
    )
    .await
}

/// Starts a TLS server presenting a validly-signed certificate for `localhost`
/// that expired in 2020, like `expired.badssl.com`.
pub async fn start_expired_cert_server() -> anyhow::Result<u16> {
    start_tls_hello_world_server(
        include_bytes!("../assets/expired-cert.pem"),
        include_bytes!("../assets/expired-key.pem"),
    )
    .await
}

fn hello_world_router() -> Router {
    async fn hello_world() -> Response {
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("hello"))
            .unwrap()
    }

    Router::new().route("/", get(hello_world))
}
