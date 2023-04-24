use std::net::SocketAddr;

use attohttpc::Session;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use url::Url;

#[tokio::test(flavor = "multi_thread")]
#[cfg(feature = "cookies")]
async fn test_redirection_default() -> Result<(), anyhow::Error> {
    async fn root() -> impl IntoResponse {
        (StatusCode::OK, [(header::SET_COOKIE, "foo=bar")], "Hello, World!")
    }

    let app = Router::new().route("/", get(root));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3939));
    tokio::spawn(axum::Server::bind(&addr).serve(app.into_make_service()));

    let sess = Session::new();
    sess.get("http://localhost:3939").send()?;
    let cookies = sess
        .cookie_jar()
        .cookies_for_url(&Url::parse("http://localhost:3939").unwrap());

    assert!(!cookies.is_empty());

    Ok(())
}
