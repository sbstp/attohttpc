use std::thread;

use attohttpc::ErrorKind;
use lazy_static::lazy_static;
use rouille::{router, Response};

lazy_static! {
    static ref STARTED: u16 = {
        let server = rouille::Server::new("localhost:0", |request| {
            router!(request,
                (GET) ["/301"] => Response::redirect_301("/301"),
                (GET) ["/304"] => Response::text("").with_status_code(304),
                _ => Response::empty_404()
            )
        })
        .unwrap();
        let port = server.server_addr().port();
        thread::spawn(|| {
            server.run();
        });
        port
    };
}

#[test]
fn test_redirection_default() {
    let port = *STARTED;

    match attohttpc::get(format!("http://localhost:{}/301", port)).send() {
        Err(err) => match err.kind() {
            ErrorKind::TooManyRedirections => (),
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn test_redirection_0() {
    let port = *STARTED;

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
}

#[test]
fn test_redirection_disallowed() {
    let port = *STARTED;

    let resp = attohttpc::get(format!("http://localhost:{}/301", port))
        .follow_redirects(false)
        .send()
        .unwrap();

    assert!(resp.status().is_redirection());
}

#[test]
fn test_redirection_not_redirect() {
    let port = *STARTED;

    match attohttpc::get(format!("http://localhost:{}/304", port)).send() {
        Ok(_) => (),
        _ => panic!(),
    }
}
