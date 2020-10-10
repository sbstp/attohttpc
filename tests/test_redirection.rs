use std::net::SocketAddr;

use attohttpc::ErrorKind;
use warp::Filter;

async fn make_server() -> Result<u16, anyhow::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let incoming = tokio::net::TcpListener::bind(&addr).await?;
    let local_addr = incoming.local_addr()?;

    let a = warp::path("301").map(|| warp::redirect::redirect(http::Uri::from_static("/301")));
    let b = warp::path("304").map(|| {
        http::Response::builder()
            .header("Location", "/304")
            .status(http::StatusCode::NOT_MODIFIED)
            .body("")
    });

    let server = warp::serve(a.or(b)).serve_incoming(incoming);
    tokio::spawn(server);

    Ok(local_addr.port())
}

#[tokio::test(threaded_scheduler)]
async fn test_redirection_default() -> Result<(), anyhow::Error> {
    let port = make_server().await?;

    match attohttpc::get(format!("http://localhost:{}/301", port)).send() {
        Err(err) => match err.kind() {
            ErrorKind::TooManyRedirections => (),
            _ => panic!(),
        },
        _ => panic!(),
    }

    Ok(())
}

#[tokio::test(threaded_scheduler)]
async fn test_redirection_0() -> Result<(), anyhow::Error> {
    let port = make_server().await?;

    match attohttpc::get(format!("http://localhost:{}/301", port))
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

#[tokio::test(threaded_scheduler)]
async fn test_redirection_disallowed() -> Result<(), anyhow::Error> {
    let port = make_server().await?;

    let resp = attohttpc::get(format!("http://localhost:{}/301", port))
        .follow_redirects(false)
        .send()
        .unwrap();

    assert!(resp.status().is_redirection());

    Ok(())
}

#[tokio::test(threaded_scheduler)]
async fn test_redirection_not_redirect() -> Result<(), anyhow::Error> {
    let port = make_server().await?;

    match attohttpc::get(format!("http://localhost:{}/304", port)).send() {
        Ok(_) => (),
        _ => panic!(),
    }

    Ok(())
}
