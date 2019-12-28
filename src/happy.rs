//! aaa
use std::io;
use std::iter::FusedIterator;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn intertwine<T, A, B>(mut ita: A, mut itb: B) -> Vec<T>
where
    A: FusedIterator<Item = T>,
    B: FusedIterator<Item = T>,
{
    let mut res = vec![];

    loop {
        match (ita.next(), itb.next()) {
            (Some(a), Some(b)) => {
                res.push(a);
                res.push(b);
            }
            (Some(a), None) => {
                res.push(a);
            }
            (None, Some(b)) => {
                res.push(b);
            }
            (None, None) => break,
        }
    }

    res
}

/// aaa
pub fn connect<A>(addrs: A) -> io::Result<TcpStream>
where
    A: ToSocketAddrs,
{
    let addrs: Vec<_> = addrs.to_socket_addrs()?.collect();
    let ipv4 = addrs.iter().filter(|a| a.is_ipv4()).cloned();
    let ipv6 = addrs.iter().filter(|a| a.is_ipv6()).cloned();
    let order = intertwine(ipv4, ipv6);

    let done = Arc::new(AtomicBool::new(false));
    let done_clone = done.clone();
    let (tx, rx) = channel();

    let start = Instant::now();

    thread::spawn(move || {
        for addr in order {
            if done.load(Ordering::SeqCst) {
                break;
            }

            let tx = tx.clone();
            thread::spawn(move || {
                match TcpStream::connect_timeout(&addr, Duration::from_secs(10)) {
                    Ok(sock) => {
                        let _ = tx.send(sock);
                    }
                    Err(_) => {}
                };
            });

            thread::sleep(Duration::from_millis(200));
        }
    });

    let res = match rx.recv() {
        Ok(sock) => Ok(sock),
        Err(_) => Err(io::ErrorKind::ConnectionRefused.into()),
    };

    done_clone.store(true, Ordering::SeqCst);

    println!("took {}ms", (Instant::now() - start).as_millis());

    res
}
