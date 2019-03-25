use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use attohttpc::ErrorKind;
use lazy_static::lazy_static;
use rouille::Response;

lazy_static! {
    static ref STARTED: bool = {
        let started = Arc::new(AtomicBool::new(false));
        let started_clone = started.clone();

        thread::spawn(move || {
            started_clone.store(true, Ordering::SeqCst);
            rouille::start_server("0.0.0.0:55123", move |_| Response::redirect_301("/"));
        });

        while !started.load(Ordering::SeqCst) {}
        true
    };
}

#[test]
fn test_redirection_default() {
    let _ = *STARTED;

    match attohttpc::get("http://localhost:55123/").send() {
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

    match attohttpc::get("http://localhost:55123/").max_redirections(0).send() {
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

    let resp = attohttpc::get("http://localhost:55123/")
        .follow_redirects(false)
        .send()
        .unwrap();

    assert!(resp.status().is_redirection());
}
