use std::convert::Infallible;
use std::net::SocketAddr;

use hyper::server::conn::AddrIncoming;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};

use super::tls::{TlsAcceptor, TlsConfigBuilder};

pub async fn start_hello_world_server(tls: bool) -> Result<u16, hyper::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));

    async fn handler(_: Request<Body>) -> Result<Response<Body>, hyper::Error> {
        Ok(Response::new(Body::from("hello")))
    }

    let bound = AddrIncoming::bind(&addr)?;
    let addr = bound.local_addr();

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

    println!("Listening on http://{addr}");

    Ok(addr.port())
}
