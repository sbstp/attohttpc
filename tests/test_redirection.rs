use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use attohttpc::ErrorKind;
use axum::body::Body;
use axum::response::Response;
use axum::routing::{any, get};
use axum::Router;
use http::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, TRANSFER_ENCODING};
use http::StatusCode;

async fn make_server() -> Result<u16, anyhow::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let incoming = tokio::net::TcpListener::bind(&addr).await?;
    let local_addr = incoming.local_addr()?;

    async fn x301() -> Response {
        Response::builder()
            .status(StatusCode::MOVED_PERMANENTLY)
            .header("Location", "/301")
            .body(Body::from(""))
            .unwrap()
    }

    async fn x304() -> Response {
        Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .body(Body::from(""))
            .unwrap()
    }

    let app = Router::new().route("/301", get(x301)).route("/304", get(x304));

    tokio::spawn(async move {
        axum::serve(incoming, app).await.unwrap();
    });

    Ok(local_addr.port())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_default() -> Result<(), anyhow::Error> {
    let port = make_server().await?;

    match attohttpc::get(format!("http://localhost:{port}/301")).send() {
        Err(err) => match err.kind() {
            ErrorKind::TooManyRedirections => (),
            _ => panic!(),
        },
        _ => panic!(),
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_0() -> Result<(), anyhow::Error> {
    let port = make_server().await?;

    match attohttpc::get(format!("http://localhost:{port}/301"))
        .max_redirections(0)
        .send()
    {
        Err(err) => match err.kind() {
            ErrorKind::TooManyRedirections => (),
            _ => panic!(),
        },
        _ => panic!(),
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_disallowed() -> Result<(), anyhow::Error> {
    let port = make_server().await?;

    let resp = attohttpc::get(format!("http://localhost:{port}/301"))
        .follow_redirects(false)
        .send()
        .unwrap();

    assert!(resp.status().is_redirection());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_not_redirect() -> Result<(), anyhow::Error> {
    let port = make_server().await?;

    match attohttpc::get(format!("http://localhost:{port}/304")).send() {
        Ok(_) => (),
        _ => panic!(),
    }

    Ok(())
}

/// Request details captured by the `/capture` endpoint of a test server.
#[derive(Default)]
struct CapturedRequest {
    method: String,
    body: Vec<u8>,
    content_length: Option<String>,
    content_type: Option<String>,
    transfer_encoding: Option<String>,
}

type SharedCapturedRequest = Arc<Mutex<CapturedRequest>>;

/// Headers captured by the `/capture` endpoint of a test server.
#[derive(Default)]
struct CapturedHeaders {
    authorization: Option<String>,
    cookie: Option<String>,
}

type SharedCapturedHeaders = Arc<Mutex<CapturedHeaders>>;

async fn spawn_server(app: Router) -> Result<u16, anyhow::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let incoming = tokio::net::TcpListener::bind(&addr).await?;
    let port = incoming.local_addr()?.port();

    tokio::spawn(async move {
        axum::serve(incoming, app).await.unwrap();
    });

    Ok(port)
}

/// Spawn a server that answers `POST /redirect` with the given redirect status and a
/// relative `Location: /capture` header, and captures the request it receives at
/// `/capture`.
async fn spawn_redirect_capture_server(status: StatusCode) -> Result<(u16, SharedCapturedRequest), anyhow::Error> {
    let captured: SharedCapturedRequest = Arc::new(Mutex::new(CapturedRequest::default()));
    let capture_state = captured.clone();

    let redirect = Router::new().route(
        "/redirect",
        any(move || async move {
            Response::builder()
                .status(status)
                .header("Location", "/capture")
                .body(Body::from(""))
                .unwrap()
        }),
    );

    let capture = Router::new().route(
        "/capture",
        any(move |req: axum::extract::Request| {
            let state = capture_state.clone();
            async move {
                let method = req.method().to_string();
                let content_length = req
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let content_type = req
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let transfer_encoding = req
                    .headers()
                    .get(TRANSFER_ENCODING)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let body = axum::body::to_bytes(req.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec();

                let mut captured = state.lock().unwrap();
                captured.method = method;
                captured.body = body;
                captured.content_length = content_length;
                captured.content_type = content_type;
                captured.transfer_encoding = transfer_encoding;
                "ok"
            }
        }),
    );

    let port = spawn_server(redirect.merge(capture)).await?;
    Ok((port, captured))
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_301_post_becomes_get() -> Result<(), anyhow::Error> {
    let (port, captured) = spawn_redirect_capture_server(StatusCode::MOVED_PERMANENTLY).await?;

    attohttpc::post(format!("http://localhost:{port}/redirect"))
        .text("hello")
        .send()?;

    let captured = captured.lock().unwrap();
    assert_eq!(captured.method, "GET");
    assert!(captured.body.is_empty());
    assert_eq!(captured.content_length, None);
    assert_eq!(captured.content_type, None);
    assert_eq!(captured.transfer_encoding, None);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_302_post_becomes_get() -> Result<(), anyhow::Error> {
    let (port, captured) = spawn_redirect_capture_server(StatusCode::FOUND).await?;

    attohttpc::post(format!("http://localhost:{port}/redirect"))
        .text("hello")
        .send()?;

    let captured = captured.lock().unwrap();
    assert_eq!(captured.method, "GET");
    assert!(captured.body.is_empty());
    assert_eq!(captured.content_length, None);
    assert_eq!(captured.content_type, None);
    assert_eq!(captured.transfer_encoding, None);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_303_post_becomes_get() -> Result<(), anyhow::Error> {
    let (port, captured) = spawn_redirect_capture_server(StatusCode::SEE_OTHER).await?;

    attohttpc::post(format!("http://localhost:{port}/redirect"))
        .text("hello")
        .send()?;

    let captured = captured.lock().unwrap();
    assert_eq!(captured.method, "GET");
    assert!(captured.body.is_empty());
    assert_eq!(captured.content_length, None);
    assert_eq!(captured.content_type, None);
    assert_eq!(captured.transfer_encoding, None);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_307_post_preserved() -> Result<(), anyhow::Error> {
    let (port, captured) = spawn_redirect_capture_server(StatusCode::TEMPORARY_REDIRECT).await?;

    attohttpc::post(format!("http://localhost:{port}/redirect"))
        .text("hello")
        .send()?;

    let captured = captured.lock().unwrap();
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.body, b"hello");
    assert_eq!(captured.content_length.as_deref(), Some("5"));
    assert_eq!(captured.content_type.as_deref(), Some("text/plain; charset=utf-8"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_308_post_preserved() -> Result<(), anyhow::Error> {
    let (port, captured) = spawn_redirect_capture_server(StatusCode::PERMANENT_REDIRECT).await?;

    attohttpc::post(format!("http://localhost:{port}/redirect"))
        .text("hello")
        .send()?;

    let captured = captured.lock().unwrap();
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.body, b"hello");
    assert_eq!(captured.content_length.as_deref(), Some("5"));
    assert_eq!(captured.content_type.as_deref(), Some("text/plain; charset=utf-8"));

    Ok(())
}

fn capture_route(state: SharedCapturedHeaders) -> Router {
    Router::new().route(
        "/capture",
        get(move |headers: http::HeaderMap| {
            let state = state.clone();
            async move {
                let mut captured = state.lock().unwrap();
                captured.authorization = headers
                    .get(AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                captured.cookie = headers.get(COOKIE).and_then(|v| v.to_str().ok()).map(str::to_owned);
                "ok"
            }
        }),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_cross_origin_strips_credentials() -> Result<(), anyhow::Error> {
    let captured: SharedCapturedHeaders = Arc::new(Mutex::new(CapturedHeaders::default()));

    // The redirect target is a different origin: same host, different port.
    let target_port = spawn_server(capture_route(captured.clone())).await?;

    let redirect_app = Router::new().route(
        "/redirect",
        get(move || async move {
            Response::builder()
                .status(StatusCode::FOUND)
                .header("Location", format!("http://localhost:{target_port}/capture"))
                .body(Body::from(""))
                .unwrap()
        }),
    );
    let redirect_port = spawn_server(redirect_app).await?;

    attohttpc::get(format!("http://localhost:{redirect_port}/redirect"))
        .bearer_auth("SECRET-TOKEN")
        .header(COOKIE, "sid=SECRETCOOKIE")
        .send()?;

    let captured = captured.lock().unwrap();
    assert_eq!(captured.authorization, None);
    assert_eq!(captured.cookie, None);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_redirection_same_origin_keeps_credentials() -> Result<(), anyhow::Error> {
    let captured: SharedCapturedHeaders = Arc::new(Mutex::new(CapturedHeaders::default()));

    let app = Router::new().route(
        "/redirect",
        get(|| async {
            Response::builder()
                .status(StatusCode::FOUND)
                .header("Location", "/capture")
                .body(Body::from(""))
                .unwrap()
        }),
    );
    let app = app.merge(capture_route(captured.clone()));
    let port = spawn_server(app).await?;

    attohttpc::get(format!("http://localhost:{port}/redirect"))
        .bearer_auth("SECRET-TOKEN")
        .header(COOKIE, "sid=SECRETCOOKIE")
        .send()?;

    let captured = captured.lock().unwrap();
    assert_eq!(captured.authorization.as_deref(), Some("Bearer SECRET-TOKEN"));
    assert_eq!(captured.cookie.as_deref(), Some("sid=SECRETCOOKIE"));

    Ok(())
}
