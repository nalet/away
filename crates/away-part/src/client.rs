//! Asks another service over HTTPS, as the server asks the sign-in service: one request
//! a connection, an answer of a bounded length within a bounded time, and no redirect
//! is followed.
//!
//! A [`Conversation`] is one connection that stays open for several requests, for a
//! service whose sign-in is tied to the connection it was made on.
//!
//! The service's certificate is checked against the public authorities, which are in
//! the build since the image holds none, and against those the operator adds.

use std::sync::Arc;
use std::time::Duration;

use away_secret::Secret;
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::Bytes;
use hyper::header::{HeaderValue, ACCEPT, AUTHORIZATION, CONNECTION, CONTENT_TYPE, HOST, USER_AGENT};
use hyper::{Method, Request};
use hyper_util::rt::TokioIo;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::address::{form, written_inside, Address};
use crate::{base64, version};

/// What came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Got {
    /// The answer's status.
    pub status: u16,
    /// What it carries.
    pub body: Bytes,
}

/// Why nothing came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failed {
    /// The service's name is not known, or it takes no connection.
    NotReached(String),
    /// The connection couldn't be secured: the certificate is of no authority that is
    /// trusted, is for another name, or has run out.
    NotSecured(String),
    /// No answer came within the time.
    NoAnswerInTime,
    /// The answer is longer than an answer may be.
    TooLong,
    /// What came back is no answer, or the connection broke.
    Broken(String),
}

impl std::fmt::Display for Failed {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failed::NotReached(why) => write!(out, "it couldn't be reached: {why}"),
            Failed::NotSecured(why) => write!(out, "the connection to it couldn't be secured: {why}"),
            Failed::NoAnswerInTime => out.write_str("it didn't answer in time"),
            Failed::TooLong => out.write_str("its answer is longer than an answer may be"),
            Failed::Broken(why) => write!(out, "what came back is no answer: {why}"),
        }
    }
}

/// The authorities a service's certificate is checked against: the public ones, and
/// those of these certificates that the operator added, written as PEM.
fn trusted(also_trusted: Option<&[u8]>) -> Result<RootCertStore, String> {
    let mut trusted = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    if let Some(also) = also_trusted {
        let mut added = 0;
        for certificate in CertificateDer::pem_slice_iter(also) {
            let certificate = certificate.map_err(|error| format!("a certificate of an added authority can't be read: {error}"))?;
            trusted.add(certificate).map_err(|error| format!("a certificate of an added authority can't be used: {error}"))?;
            added += 1;
        }
        if added == 0 {
            return Err("the added authorities hold no certificate".to_string());
        }
    }
    Ok(trusted)
}

/// What secures connections, for a service that speaks this protocol on them.
fn securing(also_trusted: Option<&[u8]>, speaks: &[u8]) -> Result<TlsConnector, String> {
    let mut config = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|error| error.to_string())?
        .with_root_certificates(trusted(also_trusted)?)
        .with_no_client_auth();
    config.alpn_protocols = vec![speaks.to_vec()];
    Ok(TlsConnector::from(Arc::new(config)))
}

/// A connection that is secured.
pub type Secured = tokio_rustls::client::TlsStream<TcpStream>;

/// Secures connections to a service that speaks something else than HTTP, such as a
/// database: the service's certificate is checked as [`Client`] checks one.
#[derive(Clone)]
pub struct Secures(TlsConnector);

impl Secures {
    /// For a service that speaks this protocol, as it is named when a connection is
    /// secured. It trusts the public authorities, and those of these certificates that
    /// the operator added, written as PEM.
    pub fn new(also_trusted: Option<&[u8]>, speaks: &[u8]) -> Result<Secures, String> {
        securing(also_trusted, speaks).map(Secures)
    }

    /// Secures a connection that is open, to the service of this name.
    pub async fn secure(&self, host: &str, connection: TcpStream) -> Result<Secured, String> {
        let name = ServerName::try_from(host.to_string()).map_err(|error| error.to_string())?;
        self.0.connect(name, connection).await.map_err(|error| error.to_string())
    }
}

/// Asks services.
#[derive(Clone)]
pub struct Client {
    secures: TlsConnector,
    within: Duration,
    at_most: usize,
}

impl Client {
    /// A client that trusts the public authorities, and those of these certificates
    /// that the operator added, written as PEM.
    pub fn new(also_trusted: Option<&[u8]>) -> Result<Client, String> {
        Ok(Client { secures: securing(also_trusted, b"http/1.1")?, within: Duration::from_secs(10), at_most: 1024 * 1024 })
    }

    /// The same client, with another time an answer has to come within and another
    /// length it may have.
    pub fn with_limits(self, within: Duration, at_most: usize) -> Client {
        Client { within, at_most, ..self }
    }

    /// Asks for what is at an address.
    pub async fn get(&self, address: &Address) -> Result<Got, Failed> {
        self.ask(address, Method::GET, None, None).await
    }

    /// Sends a form to an address. Where the service knows Away as a client of its own,
    /// the client's name and secret go with it.
    pub async fn send_form(&self, address: &Address, pairs: &[(&str, &str)], as_client: Option<(&str, &Secret)>) -> Result<Got, Failed> {
        let named = as_client.map(|(name, secret)| {
            let both = Secret::new(format!("{}:{}", written_inside(name), written_inside(secret.expose())));
            Secret::new(format!("Basic {}", base64::ordinary(both.expose().as_bytes())))
        });
        self.ask(address, Method::POST, Some(form(pairs)), named).await
    }

    async fn ask(&self, address: &Address, method: Method, sent: Option<String>, named: Option<Secret>) -> Result<Got, Failed> {
        let broken = |error: hyper::http::Error| Failed::Broken(error.to_string());
        let mut request = Request::builder()
            .method(method)
            .uri(&address.path)
            .header(HOST, address.host_named())
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, format!("away/{}", version::VERSION))
            .header(CONNECTION, "close");
        if sent.is_some() {
            request = request.header(CONTENT_TYPE, "application/x-www-form-urlencoded");
        }
        if let Some(named) = named {
            let mut value = HeaderValue::from_str(named.expose()).map_err(|error| Failed::Broken(error.to_string()))?;
            value.set_sensitive(true);
            request = request.header(AUTHORIZATION, value);
        }
        let request = request.body(Full::new(Bytes::from(sent.unwrap_or_default()))).map_err(broken)?;
        match tokio::time::timeout(self.within, self.exchange(address, request)).await {
            Ok(got) => got,
            Err(_) => Err(Failed::NoAnswerInTime),
        }
    }

    async fn exchange(&self, address: &Address, request: Request<Full<Bytes>>) -> Result<Got, Failed> {
        let connection =
            TcpStream::connect((address.host.as_str(), address.port)).await.map_err(|error| Failed::NotReached(error.to_string()))?;
        if !address.secured {
            return answer_of(TokioIo::new(connection), request, self.at_most).await;
        }
        let name = ServerName::try_from(address.host.clone()).map_err(|error| Failed::NotSecured(error.to_string()))?;
        let secured = self.secures.connect(name, connection).await.map_err(|error| Failed::NotSecured(error.to_string()))?;
        answer_of(TokioIo::new(secured), request, self.at_most).await
    }
}

/// What came back in a conversation: the answer with its headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answered {
    /// The answer's status.
    pub status: u16,
    /// Its headers: each name in small letters, and its value where that is text.
    pub headers: Vec<(String, String)>,
    /// What it carries.
    pub body: Bytes,
}

impl Answered {
    /// The first header of this name, in small letters.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(its, _)| its == name).map(|(_, value)| value.as_str())
    }

    /// Every header of this name.
    pub fn headers_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        self.headers.iter().filter(move |(its, _)| its == name).map(|(_, value)| value.as_str())
    }
}

/// One connection to a service, kept open for several requests one after the other.
pub struct Conversation {
    sender: hyper::client::conn::http1::SendRequest<Full<Bytes>>,
    driving: tokio::task::JoinHandle<()>,
    host: String,
    within: Duration,
    at_most: usize,
}

impl Drop for Conversation {
    fn drop(&mut self) {
        self.driving.abort();
    }
}

impl Client {
    /// Opens a connection to the service at an address, to ask it several things on.
    pub async fn talk(&self, address: &Address) -> Result<Conversation, Failed> {
        let opened = async {
            let connection =
                TcpStream::connect((address.host.as_str(), address.port)).await.map_err(|error| Failed::NotReached(error.to_string()))?;
            let broken = |error: hyper::Error| Failed::Broken(error.to_string());
            if !address.secured {
                let (sender, driven) = hyper::client::conn::http1::handshake(TokioIo::new(connection)).await.map_err(broken)?;
                return Ok((
                    sender,
                    tokio::spawn(async move {
                        let _ = driven.await;
                    }),
                ));
            }
            let name = ServerName::try_from(address.host.clone()).map_err(|error| Failed::NotSecured(error.to_string()))?;
            let secured = self.secures.connect(name, connection).await.map_err(|error| Failed::NotSecured(error.to_string()))?;
            let (sender, driven) = hyper::client::conn::http1::handshake(TokioIo::new(secured)).await.map_err(broken)?;
            Ok((
                sender,
                tokio::spawn(async move {
                    let _ = driven.await;
                }),
            ))
        };
        match tokio::time::timeout(self.within, opened).await {
            Ok(Ok((sender, driving))) => {
                Ok(Conversation { sender, driving, host: address.host_named(), within: self.within, at_most: self.at_most })
            }
            Ok(Err(failed)) => Err(failed),
            Err(_) => Err(Failed::NoAnswerInTime),
        }
    }
}

impl Conversation {
    /// Asks for what is at a path of the service, with these headers. A header whose
    /// value is secret is never written to a log.
    pub async fn get(&mut self, path: &str, headers: &[(&str, &Secret)]) -> Result<Answered, Failed> {
        let broken = |error: hyper::http::Error| Failed::Broken(error.to_string());
        let mut request = Request::builder()
            .method(Method::GET)
            .uri(path)
            .header(HOST, &self.host)
            .header(USER_AGENT, format!("away/{}", version::VERSION));
        for (name, value) in headers {
            let mut value = HeaderValue::from_str(value.expose()).map_err(|error| Failed::Broken(error.to_string()))?;
            value.set_sensitive(true);
            request = request.header(*name, value);
        }
        let request = request.body(Full::new(Bytes::new())).map_err(broken)?;
        let at_most = self.at_most;
        let asked = async {
            let broken = |error: hyper::Error| Failed::Broken(error.to_string());
            self.sender.ready().await.map_err(broken)?;
            let answer = self.sender.send_request(request).await.map_err(broken)?;
            let status = answer.status().as_u16();
            let headers: Vec<(String, String)> = answer
                .headers()
                .iter()
                .filter_map(|(name, value)| Some((name.as_str().to_ascii_lowercase(), value.to_str().ok()?.to_string())))
                .collect();
            match Limited::new(answer.into_body(), at_most).collect().await {
                Ok(whole) => Ok(Answered { status, headers, body: whole.to_bytes() }),
                Err(error) if error.downcast_ref::<LengthLimitError>().is_some() => Err(Failed::TooLong),
                Err(error) => Err(Failed::Broken(error.to_string())),
            }
        };
        match tokio::time::timeout(self.within, asked).await {
            Ok(answered) => answered,
            Err(_) => Err(Failed::NoAnswerInTime),
        }
    }
}

/// Sends one request on a connection and reads its answer.
async fn answer_of<Connection>(connection: Connection, request: Request<Full<Bytes>>, at_most: usize) -> Result<Got, Failed>
where
    Connection: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let broken = |error: hyper::Error| Failed::Broken(error.to_string());
    let (mut sender, driven) = hyper::client::conn::http1::handshake(connection).await.map_err(broken)?;
    let driving = tokio::spawn(driven);
    let got = async {
        let answer = sender.send_request(request).await.map_err(broken)?;
        let status = answer.status().as_u16();
        match Limited::new(answer.into_body(), at_most).collect().await {
            Ok(whole) => Ok(Got { status, body: whole.to_bytes() }),
            Err(error) if error.downcast_ref::<LengthLimitError>().is_some() => Err(Failed::TooLong),
            Err(error) => Err(Failed::Broken(error.to_string())),
        }
    }
    .await;
    driving.abort();
    got
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{answer, carried, plain, Answers, Listener};
    use crate::{Lines, Measures};
    use hyper::StatusCode;

    /// A service on this machine that answers with these answers until the test ends.
    async fn service(answers: Answers) -> (Address, crate::Stopper) {
        let listener = Listener::bind("test", "127.0.0.1:0").await.unwrap();
        let address = Address::read(&format!("http://127.0.0.1:{}/", listener.address().port())).unwrap();
        let (stopper, stop) = crate::stop::pair();
        tokio::spawn(listener.answer_with(answers, stop, Lines::kept_for("test"), Measures::of("test")));
        (address, stopper)
    }

    /// Answers with what it was asked: the method, the address, two headers and what the request carried.
    fn echo() -> Answers {
        Arc::new(|request| {
            Box::pin(async move {
                let header = |name: &str| request.headers().get(name).and_then(|value| value.to_str().ok()).unwrap_or("-").to_string();
                let head = format!(
                    "{} {} [{}] [{}] [{}]",
                    request.method(),
                    request.uri(),
                    header("host"),
                    header("authorization"),
                    header("content-type")
                );
                let sent = carried(request, 4096).await.unwrap_or_default();
                answer(StatusCode::OK, "text/plain", format!("{head}\n{}", String::from_utf8_lossy(&sent)))
            })
        })
    }

    #[tokio::test]
    async fn a_conversation_asks_several_things_on_one_connection_and_reads_the_headers() {
        // a service that says which connection a request came on, and counts them
        let connections = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let listener = Listener::bind("test", "127.0.0.1:0").await.unwrap();
        let address = Address::read(&format!("http://127.0.0.1:{}/", listener.address().port())).unwrap();
        let (_stopper, stop) = crate::stop::pair();
        let counted = connections.clone();
        let answers: Answers = Arc::new(move |request| {
            let counted = counted.clone();
            Box::pin(async move {
                let secret = request.headers().get("x-secret").and_then(|value| value.to_str().ok()).unwrap_or("-").to_string();
                let mut answered = answer(StatusCode::UNAUTHORIZED, "text/plain", format!("{} [{secret}]", request.uri()));
                answered.headers_mut().append("www-authenticate", HeaderValue::from_static("Negotiate"));
                answered.headers_mut().append("www-authenticate", HeaderValue::from_static("NTLM"));
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                answered
            })
        });
        tokio::spawn(listener.answer_with(answers, stop, Lines::kept_for("test"), Measures::of("test")));

        let client = Client::new(None).unwrap();
        let mut conversation = client.talk(&address).await.unwrap();
        let first = conversation.get("/one?a=b", &[]).await.unwrap();
        assert_eq!((first.status, &first.body[..]), (401, &b"/one?a=b [-]"[..]));
        assert_eq!(first.headers_named("www-authenticate").collect::<Vec<_>>(), ["Negotiate", "NTLM"]);
        assert_eq!(first.header("content-type"), Some("text/plain"));
        let second = conversation.get("/two", &[("x-secret", &Secret::new("held"))]).await.unwrap();
        assert_eq!(&second.body[..], b"/two [held]");
        assert_eq!(connections.load(std::sync::atomic::Ordering::SeqCst), 2);
        // a service that isn't there
        let nowhere = Address::read("http://127.0.0.1:1/").unwrap();
        assert!(matches!(client.talk(&nowhere).await, Err(Failed::NotReached(_))));
    }

    #[tokio::test]
    async fn it_asks_and_gets_the_answer() {
        let (address, _stopper) = service(echo()).await;
        let client = Client::new(None).unwrap();
        let got = client.get(&address.asking(&[("a", "b c")])).await.unwrap();
        assert_eq!(got.status, 200);
        assert_eq!(String::from_utf8_lossy(&got.body), format!("GET /?a=b%20c [{}] [-] [-]\n", address.host_named()));
    }

    #[tokio::test]
    async fn a_form_goes_with_the_clients_name_and_secret() {
        let (address, _stopper) = service(echo()).await;
        let client = Client::new(None).unwrap();
        let secret = Secret::new("s3cr:t/+");
        let got = client
            .send_form(&address, &[("grant_type", "authorization_code"), ("code", "a/b")], Some(("away client", &secret)))
            .await
            .unwrap();
        // the name and the secret are written as a form writes them, then joined and turned into letters
        let named = base64::ordinary(b"away%20client:s3cr%3At%2F%2B");
        assert_eq!(
            String::from_utf8_lossy(&got.body),
            format!(
                "POST / [{}] [Basic {named}] [application/x-www-form-urlencoded]\ngrant_type=authorization_code&code=a%2Fb",
                address.host_named()
            )
        );
    }

    #[tokio::test]
    async fn an_answer_that_is_too_long_or_too_late_is_no_answer() {
        let long: Answers = Arc::new(|_| Box::pin(async { answer(StatusCode::OK, "text/plain", vec![b'x'; 5000]) }));
        let (address, _stopper) = service(long).await;
        let client = Client::new(None).unwrap();
        assert_eq!(client.clone().with_limits(Duration::from_secs(5), 4999).get(&address).await, Err(Failed::TooLong));
        assert_eq!(client.clone().with_limits(Duration::from_secs(5), 5000).get(&address).await.map(|got| got.body.len()), Ok(5000));

        let late: Answers = Arc::new(|_| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                plain(StatusCode::OK, "late\n")
            })
        });
        let (address, _stopper) = service(late).await;
        assert_eq!(client.with_limits(Duration::from_millis(200), 4096).get(&address).await, Err(Failed::NoAnswerInTime));
    }

    #[tokio::test]
    async fn a_redirect_is_given_back_and_not_followed() {
        let elsewhere: Answers = Arc::new(|_| {
            Box::pin(async {
                let mut answer = plain(StatusCode::FOUND, "elsewhere\n");
                answer.headers_mut().insert("location", HeaderValue::from_static("http://127.0.0.1:1/"));
                answer
            })
        });
        let (address, _stopper) = service(elsewhere).await;
        assert_eq!(Client::new(None).unwrap().get(&address).await.map(|got| got.status), Ok(302));
    }

    #[tokio::test]
    async fn a_port_nothing_listens_on_is_not_reached() {
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let address = Address::read(&format!("http://127.0.0.1:{free}/")).unwrap();
        assert!(matches!(Client::new(None).unwrap().get(&address).await, Err(Failed::NotReached(_))));
    }

    /// A service on this machine that secures its connections with the fixtures'
    /// certificate for `localhost`, and answers every request with one line.
    async fn secured_service() -> Address {
        use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
        let certificate = CertificateDer::from(include_bytes!("../tests/fixtures/localhost.der").to_vec());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(include_bytes!("../tests/fixtures/localhost.key.der").to_vec()));
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)
            .unwrap();
        let secures = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((connection, _)) = listener.accept().await {
                let secures = secures.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let Ok(mut secured) = secures.accept(connection).await else { return };
                    let mut asked = [0u8; 2048];
                    let _ = secured.read(&mut asked).await;
                    let _ = secured.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 8\r\nconnection: close\r\n\r\nsecured\n").await;
                    let _ = secured.shutdown().await;
                });
            }
        });
        Address::read(&format!("https://localhost:{port}/")).unwrap()
    }

    #[tokio::test]
    async fn a_service_of_an_added_authority_is_trusted_and_of_an_unknown_one_is_not() {
        let address = secured_service().await;
        let knows = Client::new(Some(include_bytes!("../tests/fixtures/authority.pem"))).unwrap();
        assert_eq!(knows.get(&address).await.map(|got| got.body), Ok(Bytes::from_static(b"secured\n")));
        let knows_not = Client::new(None).unwrap();
        assert!(matches!(knows_not.get(&address).await, Err(Failed::NotSecured(_))));
        // the certificate is for localhost, and for no other name of this machine
        let other_name = Address { host: "127.0.0.1".to_string(), ..address };
        assert!(matches!(knows.get(&other_name).await, Err(Failed::NotSecured(_))));
    }

    #[test]
    fn authorities_that_are_none_are_refused() {
        assert!(Client::new(Some(b"nothing of the kind")).is_err());
        assert!(Client::new(Some(b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n")).is_err());
    }
}
