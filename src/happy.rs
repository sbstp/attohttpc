use std::io;
use std::iter::FusedIterator;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const RACE_DELAY: Duration = Duration::from_millis(200);

/// This function implements a basic form of the happy eyeballs RFC to quickly connect
/// to a domain which is available in both IPv4 and IPv6. Connection attempts are raced
/// against each other and the first to connect successfully wins the race.
///
/// If the timeout is not provided, a default timeout of 10 seconds is used.
pub fn connect<A>(addrs: A, timeout: impl Into<Option<Duration>>) -> io::Result<TcpStream>
where
    A: ToSocketAddrs,
{
    let timeout = timeout.into().unwrap_or(DEFAULT_CONNECTION_TIMEOUT);
    let addrs: Vec<_> = addrs.to_socket_addrs()?.collect();
    let ipv4 = addrs.iter().filter(|a| a.is_ipv4()).cloned();
    let ipv6 = addrs.iter().filter(|a| a.is_ipv6()).cloned();
    let order = intertwine(ipv6, ipv4);

    let (tx, rx) = channel();
    let mut last_err = None;

    let start = Instant::now();

    for addr in order {
        let tx = tx.clone();

        thread::spawn(move || {
            debug!("trying to connect to {}", addr);

            let _ = tx.send(TcpStream::connect_timeout(&addr, timeout));
        });

        match rx.recv_timeout(RACE_DELAY) {
            Ok(Ok(sock)) => {
                let end = Instant::now();
                let span = end - start;
                debug!("success, took {}ms", span.as_millis());

                return Ok(sock);
            }
            Ok(Err(err)) => {
                debug!("connection error: {} addr={}", err, addr);

                last_err = Some(err);
            }
            Err(RecvTimeoutError::Timeout) => {
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                unreachable!();
            }
        }
    }

    debug!(
        "could not connect to any address, took {}ms",
        (Instant::now() - start).as_millis()
    );

    Err(last_err.unwrap_or(io::ErrorKind::ConnectionRefused.into()))
}

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
