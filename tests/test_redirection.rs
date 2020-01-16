use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use attohttpc::ErrorKind;
use lazy_static::lazy_static;
use rouille::{router, Response};

lazy_static! {
    static ref STARTED: bool = {
        thread::spawn(move || {
            rouille::start_server("localhost:55123", move |request| router!(request,
                (GET) ["/301"] => Response::redirect_301("/301"),
                (GET) ["/304"] => Response::text("").with_status_code(304),
                _ => Response::empty_404()
            ))
        });

        let start = Instant::now();
        let timeout = Duration::from_secs(10);

        // Wait until server is ready. 10s timeout in case of error creating server.
        while TcpStream::connect(("localhost", 55123)).is_err() {
            if start.elapsed() > timeout {
                panic!("time out in server creation");
            }
            thread::sleep(Duration::from_millis(100));
        }

        true
    };
}

#[test]
fn test_redirection_default() {
    let _ = *STARTED;

    match attohttpc::get("http://localhost:55123/301").send() {
        Err(err) => match err.kind() {
            ErrorKind::TooManyRedirections => (),
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn test_redirection_0() {
    let _ = *STARTED;

    match attohttpc::get("http://localhost:55123/301").max_redirections(0).send() {
        Err(err) => match err.kind() {
            ErrorKind::TooManyRedirections => (),
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn test_redirection_disallowed() {
    let _ = *STARTED;

    let resp = attohttpc::get("http://localhost:55123/301")
        .follow_redirects(false)
        .send()
        .unwrap();

    assert!(resp.status().is_redirection());
}

#[test]
fn test_redirection_not_redirect() {
    let _ = *STARTED;

    match attohttpc::get("http://localhost:55123/304").send() {
        Ok(_) => (),
        _ => panic!(),
    }
}
