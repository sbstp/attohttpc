use lazy_static::lazy_static;
use mime::Mime;
use multipart::server::Multipart;
use std::io::{Cursor, Read};
use std::net::SocketAddr;
use std::thread;
use tokio::runtime::Builder;
use warp::Filter;

lazy_static! {
    static ref STARTED: u16 = {
        let mut rt = Builder::new()
            .enable_io()
            .enable_time()
            .threaded_scheduler()
            .build()
            .unwrap();
        // ported from warp::multipart, which has a length limit (and we're generic over Read)
        let filter = warp::path("multipart").and(warp::header::<Mime>("content-type").and_then(|ct: Mime| async move {
            ct
                .get_param("boundary")
                .map(ToString::to_string)
                .ok_or_else(|| warp::reject::reject())
        }).and(warp::body::bytes()).map(|boundary, bytes| {
            Multipart::with_body(Cursor::new(bytes), boundary)
        })).map(|mut form: Multipart<_>| {
            let mut found_text = false;
            let mut found_file = false;
            let mut buf = String::new();
            form.foreach_entry(|mut entry| {
                entry.data.read_to_string(&mut buf).unwrap();
                if !found_text && &*entry.headers.name == "Hello" && buf == "world!" {
                    found_text = true;
                } else if !found_file && &*entry.headers.name == "file" && entry.headers.filename.as_deref() == Some("hello.txt") && entry.headers.content_type.as_ref().map(|x| x.to_string() == "text/plain") == Some(true) && buf == "Hello, world!" {
                    found_file = true;
                } else {
                    panic!("Unexpected entry {:?} = {:?}", entry.headers, buf);
                }
                buf.clear();
            }).unwrap();
            assert!(found_text);
            assert!(found_file);
            "OK"
        });
        let (addr, fut) =
            rt.block_on(async { warp::serve(filter).bind_ephemeral("0.0.0.0:0".parse::<SocketAddr>().unwrap()) });
        let port = addr.port();
        thread::spawn(move || {
            rt.block_on(fut);
        });
        port
    };
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

    let port = *STARTED;

    attohttpc::post(format!("http://localhost:{}/multipart", port))
        .body(form)
        .send()?
        .text()?;

    Ok(())
}
