// This code has been taken from the hyper project and slightly modified: https://github.com/hyperium/hyper/blob/master/examples/http_proxy.rs
// It's needed to create a proxy server for testing.

use std::convert::Infallible;
use std::net::SocketAddr;

use futures_util::future::try_join;
use hyper::server::conn::AddrIncoming;
use hyper::service::{make_service_fn, service_fn};
use hyper::upgrade::Upgraded;
use hyper::{Body, Client, Method, Request, Response, Server, StatusCode};
use tokio::net::TcpStream;

use super::tls::{TlsAcceptor, TlsConfigBuilder};

type HttpClient = Client<hyper::client::HttpConnector>;

pub async fn start_proxy_server(tls: bool) -> Result<u16, hyper::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let client = HttpClient::new();

    let bound = AddrIncoming::bind(&addr)?;
    let addr = bound.local_addr();

    if tls {
        let make_service = make_service_fn(move |_| {
            let client = client.clone();
            async move { Ok::<_, Infallible>(service_fn(move |req| proxy(client.clone(), req))) }
        });

        let conf = TlsConfigBuilder::new()
            .cert(include_bytes!("cert.pem"))
            .key(include_bytes!("key.pem"))
            .build()
            .unwrap();
        let acceptor = TlsAcceptor::new(conf, bound);
        let server = Server::builder(acceptor);
        tokio::spawn(server.serve(make_service));
    } else {
        let make_service = make_service_fn(move |_| {
            let client = client.clone();
            async move { Ok::<_, Infallible>(service_fn(move |req| proxy(client.clone(), req))) }
        });

        let server = Server::builder(bound);
        tokio::spawn(server.serve(make_service));
    };

    println!("Listening on http://{}", addr);

    Ok(addr.port())
}

async fn proxy(client: HttpClient, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    // println!("req: {:?}", req);

    if Method::CONNECT == req.method() {
        // Received an HTTP request like:
        // ```
        // CONNECT www.domain.com:443 HTTP/1.1
        // Host: www.domain.com:443
        // Proxy-Connection: Keep-Alive
        // ```
        //
        // When HTTP method is CONNECT we should return an empty body
        // then we can eventually upgrade the connection and talk a new protocol.
        //
        // Note: only after client received an empty body with STATUS_OK can the
        // connection be upgraded, so we can't return a response inside
        // `on_upgrade` future.
        if let Some(addr) = req.uri().authority().map(|a| a.as_str()) {
            let addr = addr.to_string();
            tokio::task::spawn(async move {
                match hyper::upgrade::on(req).await {
                    Ok(upgraded) => {
                        if let Err(e) = tunnel(upgraded, &addr).await {
                            eprintln!("server io error: {}", e);
                        };
                    }
                    Err(e) => eprintln!("upgrade error: {}", e),
                }
            });

            Ok(Response::new(Body::empty()))
        } else {
            eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
            let mut resp = Response::new(Body::from("CONNECT must be to a socket address"));
            *resp.status_mut() = http::StatusCode::BAD_REQUEST;

            Ok(resp)
        }
    } else {
        client.request(req).await
    }
}

// Create a TCP connection to host:port, build a tunnel between the connection and
// the upgraded connection
async fn tunnel(upgraded: Upgraded, addr: &str) -> std::io::Result<()> {
    // Connect to remote server
    let mut server = TcpStream::connect(addr).await?;

    // Proxying data
    let amounts = {
        let (mut server_rd, mut server_wr) = server.split();
        let (mut client_rd, mut client_wr) = tokio::io::split(upgraded);

        let client_to_server = tokio::io::copy(&mut client_rd, &mut server_wr);
        let server_to_client = tokio::io::copy(&mut server_rd, &mut client_wr);

        try_join(client_to_server, server_to_client).await
    };

    // Print message when done
    match amounts {
        Ok((from_client, from_server)) => {
            println!("client wrote {} bytes and received {} bytes", from_client, from_server);
        }
        Err(e) => {
            println!("tunnel error: {}", e);
        }
    };
    Ok(())
}

pub async fn start_refusing_proxy_server(tls: bool) -> Result<u16, hyper::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));

    let bound = AddrIncoming::bind(&addr)?;
    let addr = bound.local_addr();

    async fn handler(_req: Request<Body>) -> http::Result<Response<Body>> {
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("bad request"))
    }

    if tls {
        let make_service = make_service_fn(move |_| async move { Ok::<_, Infallible>(service_fn(handler)) });

        let conf = TlsConfigBuilder::new()
            .cert(include_bytes!("cert.pem"))
            .key(include_bytes!("key.pem"))
            .build()
            .unwrap();
        let acceptor = TlsAcceptor::new(conf, bound);
        let server = Server::builder(acceptor);
        tokio::spawn(server.serve(make_service));
    } else {
        let make_service = make_service_fn(move |_| async move { Ok::<_, Infallible>(service_fn(handler)) });

        let server = Server::builder(bound);
        tokio::spawn(server.serve(make_service));
    };

    println!("Listening on http://{}", addr);

    Ok(addr.port())
}
