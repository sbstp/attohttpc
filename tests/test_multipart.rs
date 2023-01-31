use std::io::{Cursor, Read};
use std::net::SocketAddr;
use std::sync::mpsc::{sync_channel, Receiver};
use std::thread;

use mime::Mime;
use multipart::server::Multipart;
use tokio::runtime::Builder;
use warp::Filter;

fn start_server() -> (u16, Receiver<Option<String>>) {
    let (send, recv) = sync_channel(1);
    let rt = Builder::new_multi_thread().enable_io().enable_time().build().unwrap();
    // ported from warp::multipart, which has a length limit (and we're generic over Read)
    let filter = warp::path("multipart")
        .and(
            warp::header::<Mime>("content-type")
                .and_then(|ct: Mime| async move {
                    ct.get_param("boundary")
                        .map(|mime| mime.to_string())
                        .ok_or_else(warp::reject::reject)
                })
                .and(warp::body::bytes())
                .map(|boundary, bytes| Multipart::with_body(Cursor::new(bytes), boundary)),
        )
        .map(move |mut form: Multipart<_>| {
            let mut found_text = false;
            let mut found_file = false;
            let mut err = false;
            let mut buf = String::new();
            form.foreach_entry(|mut entry| {
                if err {
                    return;
                }
                entry.data.read_to_string(&mut buf).unwrap();
                if !found_text && &*entry.headers.name == "Hello" && buf == "world!" {
                    found_text = true;
                } else if !found_file
                    && &*entry.headers.name == "file"
                    && entry.headers.filename.as_deref() == Some("hello.txt")
                    && entry.headers.content_type.as_ref().map(|x| x.as_ref() == "text/plain") == Some(true)
                    && buf == "Hello, world!"
                {
                    found_file = true;
                } else {
                    send.send(Some(format!("Unexpected entry {:?} = {:?}", entry.headers, buf)))
                        .unwrap();
                    err = true;
                }
                buf.clear();
            })
            .unwrap();
            if err {
                return "ERR";
            }
            send.send(Some(
                match (found_text, found_file) {
                    (false, false) => "Missing both fields!",
                    (true, false) => "Missing file field!",
                    (false, true) => "Missing text field!",
                    (true, true) => {
                        send.send(None).unwrap();
                        return "OK";
                    }
                }
                .to_string(),
            ))
            .unwrap();
            "ERR"
        });
    let (addr, fut) =
        rt.block_on(async { warp::serve(filter).bind_ephemeral("0.0.0.0:0".parse::<SocketAddr>().unwrap()) });
    let port = addr.port();
    thread::spawn(move || {
        rt.block_on(fut);
    });
    (port, recv)
}

#[test]
fn test_multipart_default() -> attohttpc::Result<()> {
    let file = attohttpc::MultipartFile::new("file", b"Hello, world!")
        .with_type("text/plain")?
        .with_filename("hello.txt");
    let form = attohttpc::MultipartBuilder::new()
        .with_text("Hello", "world!")
        .with_file(file)
        .build()?;

    let (port, recv) = start_server();

    attohttpc::post(format!("http://localhost:{port}/multipart"))
        .body(form)
        .send()?
        .text()?;

    if let Some(err) = recv.recv().unwrap() {
        panic!("{}", err);
    }

    Ok(())
}
