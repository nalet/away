//! A listener that answers HTTP: every connection is read until the part stops, and each
//! request is answered by one function.

use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderValue, CONTENT_TYPE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::TcpListener;
use tokio::task::JoinSet;

use crate::lines::Lines;
use crate::measures::Measures;
use crate::stop::Stop;

/// An answer to a request.
pub type Answer = Response<Full<Bytes>>;

/// An answer that is on its way: whatever answers may first ask somebody else.
pub type Answering = Pin<Box<dyn Future<Output = Answer> + Send>>;

/// What answers the requests of a listener.
pub type Answers = Arc<dyn Fn(Request<Incoming>) -> Answering + Send + Sync>;

/// Answers that are there at once, from the request's head alone.
pub fn at_once(answers: impl Fn(&Request<Incoming>) -> Answer + Send + Sync + 'static) -> Answers {
    Arc::new(move |request| {
        let answer = answers(&request);
        Box::pin(async move { answer })
    })
}

/// What a request carries, if it is no longer than this many bytes.
pub async fn carried(request: Request<Incoming>, at_most: usize) -> Option<Bytes> {
    Limited::new(request.into_body(), at_most).collect().await.ok().map(|whole| whole.to_bytes())
}

/// How long a connection may take to send the head of a request.
const HEAD_WITHIN: Duration = Duration::from_secs(10);
/// How long connections get to end once the part stops.
const END_WITHIN: Duration = Duration::from_secs(10);

/// An answer with a body of text.
pub fn answer(status: StatusCode, kind: &'static str, body: impl Into<Bytes>) -> Answer {
    let mut answer = Response::new(Full::new(body.into()));
    *answer.status_mut() = status;
    answer.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static(kind));
    answer
}

/// A short answer in plain words.
pub fn plain(status: StatusCode, words: &'static str) -> Answer {
    answer(status, "text/plain; charset=utf-8", words)
}

/// An answer as it goes out to a browser on the address Away answers on: with the rules
/// that keep a browser from loading anything from elsewhere, from guessing what a file
/// is, from showing the page inside another, and from keeping a copy. An answer that
/// brings a rule of its own keeps it: a page that may load Away's own files, and a file
/// that may be kept.
pub fn for_a_browser(mut answer: Answer) -> Answer {
    let headers = answer.headers_mut();
    for (name, value) in [
        ("content-security-policy", "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
    ] {
        headers.entry(name).or_insert(HeaderValue::from_static(value));
    }
    answer
}

/// A listener that is bound and not yet answering.
pub struct Listener {
    name: &'static str,
    socket: TcpListener,
    address: SocketAddr,
}

impl Listener {
    /// Binds an address, such as `0.0.0.0:8080`. Port 0 takes one that is free.
    pub async fn bind(name: &'static str, address: &str) -> std::io::Result<Listener> {
        let socket = TcpListener::bind(address).await?;
        let address = socket.local_addr()?;
        Ok(Listener { name, socket, address })
    }

    /// The address it is bound to.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Answers requests until the part stops, then lets open connections end.
    pub async fn answer_with(self, answers: Answers, mut stop: Stop, lines: Lines, measures: Measures) {
        let counted = Counted::new(&measures, self.name);
        let mut connections = JoinSet::new();
        loop {
            let accepted = tokio::select! {
                accepted = self.socket.accept() => accepted,
                _ = stop.wait() => break,
            };
            let stream = match accepted {
                Ok((stream, _)) => stream,
                Err(error) => {
                    // out of file handles, or a connection that ended before it was taken:
                    // say so and go on, a little later.
                    lines.warn("could not take a connection", &[("listener", self.name.into()), ("why", (&error.to_string()).into())]);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let answers = answers.clone();
            let counted = counted.clone();
            let mut stop = stop.clone();
            connections.spawn(async move {
                let service = service_fn(move |request: Request<Incoming>| {
                    let answering = answers(request);
                    let counted = counted.clone();
                    async move {
                        let answer = answering.await;
                        counted.count(answer.status());
                        Ok::<_, Infallible>(answer)
                    }
                });
                let connection = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .header_read_timeout(HEAD_WITHIN)
                    .serve_connection(TokioIo::new(stream), service);
                tokio::pin!(connection);
                tokio::select! {
                    _ = connection.as_mut() => {}
                    _ = stop.wait() => {
                        // answer what is being asked, then close
                        connection.as_mut().graceful_shutdown();
                        let _ = connection.await;
                    }
                }
            });
            // forget the connections that have ended
            while connections.try_join_next().is_some() {}
        }
        drop(self.socket);
        let ended = tokio::time::timeout(END_WITHIN, async { while connections.join_next().await.is_some() {} }).await;
        if ended.is_err() {
            lines.warn(
                "connections were still open when the part stopped",
                &[("listener", self.name.into()), ("open", (connections.len() as u64).into())],
            );
            connections.abort_all();
        }
    }
}

/// The requests a listener answered, by the kind of answer.
#[derive(Clone)]
struct Counted {
    by_kind: [crate::measures::Counter; 5],
}

impl Counted {
    fn new(measures: &Measures, listener: &'static str) -> Self {
        let of = |answer: &str| {
            measures.counter(
                "away_requests_total",
                "Requests answered, by listener and by the kind of answer.",
                &[("listener", listener), ("answer", answer)],
            )
        };
        Counted { by_kind: [of("1xx"), of("2xx"), of("3xx"), of("4xx"), of("5xx")] }
    }

    fn count(&self, status: StatusCode) {
        let kind = (status.as_u16() / 100).clamp(1, 5) as usize - 1;
        self.by_kind[kind].add_one();
    }
}
