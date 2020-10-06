use std::thread;

use flume::unbounded;
use rouille;
use url::Url;

#[test]
fn test_http_url() {
    let (sender, receiver) = unbounded();

    let server = rouille::Server::new("localhost:0", move |request| {
        sender.send(request.url()).unwrap();
        rouille::Response::text("hello")
    })
    .unwrap();

    let port = server.server_addr().port();
    thread::spawn(|| {
        server.run();
    });

    let proxy_url = Url::parse(&format!("http://localhost:{}", port)).unwrap();

    let settings = attohttpc::ProxySettingsBuilder::new()
        .http_proxy(proxy_url.clone())
        .https_proxy(proxy_url.clone())
        .build();

    let mut sess = attohttpc::Session::new();
    sess.proxy_settings(settings);

    // Request with http
    sess.get("http://reddit.com/").send().unwrap();
    let url = receiver.recv().unwrap();
    assert_eq!(url, "http://reddit.com/");
}

// TODO: use hyper https://github.com/hyperium/hyper/blob/35825c4614b22c95ad9e214eb1d2849f89c82598/examples/http_proxy.rs
#[test]
#[cfg(any(feature = "tls", feature = "tls-rustls"))]
fn test_https_url() {
    use tiny_http::{Response, Server, ServerConfig, SslConfig};

    let conf = ServerConfig {
        addr: "localhost:0",
        ssl: Some(SslConfig {
            certificate: include_bytes!("cert.pem").to_vec(),
            private_key: include_bytes!("key.pem").to_vec(),
        }),
    };

    let remote_server = Server::new(conf).unwrap();
    let remote_port = remote_server.server_addr().port();

    thread::spawn(move || {
        for req in remote_server.incoming_requests() {
            req.respond(Response::new(200.into(), vec![], "hello".as_bytes(), None, None))
                .unwrap();
        }
    });

    let resp = attohttpc::get(format!("https://localhost:{}", remote_port))
        .danger_accept_invalid_certs(true)
        .send()
        .unwrap();

    assert_eq!(resp.text().unwrap(), "hello");
}
