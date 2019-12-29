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

    if let [addr] = &addrs[..] {
        debug!("DNS returned only one address, using fast path");
        return TcpStream::connect_timeout(addr, timeout);
    }

    let ipv4 = addrs.iter().filter(|a| a.is_ipv4()).copied();
    let ipv6 = addrs.iter().filter(|a| a.is_ipv6()).copied();
    let sorted = intertwine(ipv6, ipv4);

    let (tx, rx) = channel();
    let mut first_err = None;

    let start = Instant::now();

    // This loop will race each connection attempt against others, returning early if a
    // connection attempt is successful.
    for addr in sorted {
        let tx = tx.clone();

        thread::spawn(move || {
            debug!("trying to connect to {}", addr);

            let _ = tx.send(TcpStream::connect_timeout(&addr, timeout));
        });

        match rx.recv_timeout(RACE_DELAY) {
            Ok(Ok(sock)) => {
                debug!("success, took {}ms", start.elapsed().as_millis());

                return Ok(sock);
            }
            Ok(Err(err)) => {
                debug!("connection error: {} addr={}", err, addr);

                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                unreachable!();
            }
        }
    }

    // We must drop this handle to the sender in order to properly disconnect the channel
    // when all the threads are finished.
    drop(tx);

    // This loop waits for replies from the background threads. It will automatically timeout when the background
    // thread's connection attempt timeouts and the senders are dropped.
    // This loop is reached when some of the threads do not complete within the race delay.
    loop {
        match rx.recv() {
            Ok(Ok(sock)) => {
                debug!("success, took {}ms", start.elapsed().as_millis());

                return Ok(sock);
            }
            Ok(Err(err)) => {
                debug!("connection error: {}", err);

                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
            Err(_) => {
                // The channel is disconnected, we exit the loop
                break;
            }
        }
    }

    debug!(
        "could not connect to any address, took {}ms",
        start.elapsed().as_millis()
    );

    Err(first_err.unwrap_or(io::ErrorKind::ConnectionRefused.into()))
}

fn intertwine<T, A, B>(mut ita: A, mut itb: B) -> impl Iterator<Item = T>
where
    A: FusedIterator<Item = T>,
    B: FusedIterator<Item = T>,
{
    let mut stashed = None;

    std::iter::from_fn(move || {
        if let Some(b) = stashed.take() {
            return Some(b);
        }

        match (ita.next(), itb.next()) {
            (Some(a), Some(b)) => {
                stashed = Some(b);
                Some(a)
            }
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    })
}

#[test]
fn test_intertwine_even() {
    let x: Vec<u32> = intertwine(vec![1, 2, 3].into_iter(), vec![4, 5, 6].into_iter()).collect();
    assert_eq!(&x[..], &[1, 4, 2, 5, 3, 6][..]);
}

#[test]
fn test_intertwine_left() {
    let x: Vec<u32> = intertwine(vec![1, 2, 3, 100, 101].into_iter(), vec![4, 5, 6].into_iter()).collect();
    assert_eq!(&x[..], &[1, 4, 2, 5, 3, 6, 100, 101][..]);
}

#[test]
fn test_intertwine_right() {
    let x: Vec<u32> = intertwine(vec![1, 2, 3].into_iter(), vec![4, 5, 6, 100, 101].into_iter()).collect();
    assert_eq!(&x[..], &[1, 4, 2, 5, 3, 6, 100, 101][..]);
}
