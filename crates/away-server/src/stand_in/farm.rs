//! What stands in for a farm's web access in the tests: it signs its readers in the way
//! Windows does over HTTP, and gives them the feed of what it publishes, each thing's
//! icon, and the file that says how the thing is reached.
//!
//! It is built only for tests: nothing of it is in a release.

use std::sync::{Arc, Mutex};

use away_part::address::read_form;
use away_part::base64;
use away_part::http::{answer, plain, Answer, Answers, Listener};
use away_part::{Lines, Measures, Stopper};
use away_rdp::ntlm::{self, Account};
use away_secret::Secret;
use hyper::header::{HeaderValue, AUTHORIZATION, COOKIE, LOCATION, WWW_AUTHENTICATE};
use hyper::{Method, StatusCode};

/// The token a reader that signed in comes back with.
const TOKEN: &str = "66140BAB5ACA35B3840824AB935C2B8A9B006207174498D1005EE31B332D6C57";
const FEED: &str = "/RDWeb/Feed/webfeed.aspx";
const SIGN_IN: &str = "/RDWeb/FeedLogin/WebFeedLogin.aspx";

/// One thing the stand-in publishes.
#[derive(Clone)]
struct Published {
    title: String,
    alias: String,
    desktop: bool,
    collection: String,
    icon: Option<Vec<u8>>,
    /// Counts up whenever the thing is changed.
    changed: u64,
}

struct Inner {
    account: Account,
    published: Vec<Published>,
    changes: u64,
    /// The first two messages of the sign-in that is under way.
    first: Vec<u8>,
    second: Vec<u8>,
    answers: bool,
    /// How often the feed was given, and how often an icon or a file.
    feeds: u64,
    pieces: u64,
}

/// A stand-in for a farm's web access, on this machine.
pub struct Farm {
    address: String,
    inner: Arc<Mutex<Inner>>,
    stopper: Stopper,
}

fn written(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

impl Inner {
    fn feed(&mut self) -> Answer {
        self.feeds += 1;
        let mut feed = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ResourceCollection PubDate=\"2026-11-14T19:24:19Z\" SchemaVersion=\"1.1\" xmlns=\"http://schemas.microsoft.com/ts/2007/05/tswf\">\n  <Publisher LastUpdated=\"2026-11-14T18:06:02Z\" Name=\"Work Resources\" ID=\"broker.corp.example\" Description=\"\">\n    <Resources>\n",
        );
        for thing in &self.published {
            let icon = match thing.icon {
                Some(_) => {
                    format!("<Icon32 Dimensions=\"32x32\" FileType=\"Png\" FileURL=\"/RDWeb/Feed/rdp/{}_32x32.png\" />", thing.alias)
                }
                None => String::new(),
            };
            feed.push_str(&format!(
                "      <Resource ID=\"{alias}\" Alias=\"{alias}\" Title=\"{title}\" LastUpdated=\"change {changed}\" Type=\"{kind}\">\n        <Icons>{icon}</Icons>\n        <HostingTerminalServers><HostingTerminalServer><ResourceFile FileExtension=\".rdp\" URL=\"/RDWeb/Feed/rdp/cpub-{alias}.rdp\" /></HostingTerminalServer></HostingTerminalServers>\n      </Resource>\n",
                alias = thing.alias,
                title = written(&thing.title),
                changed = thing.changed,
                kind = if thing.desktop { "Desktop" } else { "RemoteApp" },
            ));
        }
        feed.push_str("    </Resources>\n  </Publisher>\n</ResourceCollection>\n");
        answer(StatusCode::OK, "text/xml; charset=utf-8", feed)
    }

    /// An icon, or the file that says how something is reached.
    fn piece(&mut self, named: &str) -> Answer {
        self.pieces += 1;
        for thing in &self.published {
            if named == format!("{}_32x32.png", thing.alias) {
                return match &thing.icon {
                    Some(icon) => answer(StatusCode::OK, "image/png", icon.clone()),
                    None => plain(StatusCode::NOT_FOUND, "No such picture.\n"),
                };
            }
            if named == format!("cpub-{}.rdp", thing.alias) {
                let mut file = format!(
                    "full address:s:BROKER.CORP.EXAMPLE\r\nloadbalanceinfo:s:tsv://MS Terminal Services Plugin.1.{}\r\n",
                    thing.collection
                );
                if !thing.desktop {
                    file.push_str(&format!("remoteapplicationmode:i:1\r\nremoteapplicationprogram:s:||{}\r\n", thing.alias));
                }
                // such a file is written in two bytes a character
                let wide: Vec<u8> = [0xff, 0xfe].into_iter().chain(file.encode_utf16().flat_map(u16::to_le_bytes)).collect();
                return answer(StatusCode::OK, "application/x-rdp", wide);
            }
        }
        plain(StatusCode::NOT_FOUND, "Nothing of that name is published.\n")
    }

    /// The sign-in, the way Windows signs in over HTTP: a challenge for what the reader
    /// can do, and the token for a proof that it knows the account's password.
    fn sign_in(&mut self, named: Option<&str>) -> Answer {
        let asks = |with: Option<String>| {
            let mut asks = plain(StatusCode::UNAUTHORIZED, "Sign in to read the feed.\n");
            match with {
                Some(challenge) => {
                    asks.headers_mut().append(WWW_AUTHENTICATE, HeaderValue::from_str(&format!("NTLM {challenge}")).expect("letters"));
                }
                None => {
                    asks.headers_mut().append(WWW_AUTHENTICATE, HeaderValue::from_static("Negotiate"));
                    asks.headers_mut().append(WWW_AUTHENTICATE, HeaderValue::from_static("NTLM"));
                }
            }
            asks
        };
        let Some(message) = named
            .and_then(|named| named.strip_prefix("NTLM "))
            .and_then(|letters| base64::read_for_addresses(&letters.trim().replace('+', "-").replace('/', "_").replace('=', "")))
        else {
            return asks(None);
        };
        match message.get(8) {
            Some(1) => {
                self.first = message;
                self.second = ntlm::challenge_for_tests("BROKER", "CORP", "corp.example", 26_100);
                asks(Some(base64::ordinary(&self.second)))
            }
            Some(3) => match ntlm::check_for_tests(&self.first, &self.second, &message, &self.account) {
                Ok(Some(_)) => answer(StatusCode::OK, "application/x-msts-webfeed-login; charset=utf-8", TOKEN),
                _ => asks(None),
            },
            _ => asks(None),
        }
    }
}

impl Farm {
    /// A farm's web access at an address of its own choosing, which takes this account.
    pub async fn start(account: &str, password: &str) -> Farm {
        Farm::start_at("127.0.0.1:0", account, password).await
    }

    /// The same, at an address that is chosen: `127.0.0.1:18083`.
    ///
    /// Tests that run in a browser tell it what to publish by sending it a form:
    /// `POST /stand-in?publishes=Reports&collection=Finance`, with `desktop=true` for a
    /// desktop; `POST /stand-in?withdraws=Reports`; `POST /stand-in?answers=false`.
    pub async fn start_at(at: &str, account: &str, password: &str) -> Farm {
        let listener = Listener::bind("farm", at).await.expect("an address");
        let address = format!("http://127.0.0.1:{}", listener.address().port());
        let inner = Arc::new(Mutex::new(Inner {
            account: Account::named(account, Secret::new(password)),
            published: Vec::new(),
            changes: 0,
            first: Vec::new(),
            second: Vec::new(),
            answers: true,
            feeds: 0,
            pieces: 0,
        }));
        let (stopper, stop) = away_part::stop::pair();
        let answers: Answers = {
            let inner = inner.clone();
            Arc::new(move |request| {
                let inner = inner.clone();
                Box::pin(async move {
                    let path = request.uri().path().to_string();
                    let asked = read_form(request.uri().query().unwrap_or_default()).unwrap_or_default();
                    let header = |name| request.headers().get(name).and_then(|value: &HeaderValue| value.to_str().ok()).map(str::to_string);
                    let (named, cookie) = (header(AUTHORIZATION), header(COOKIE));
                    let signed_in = cookie.is_some_and(|cookie| cookie.split(';').any(|pair| pair.trim() == format!(".ASPXAUTH={TOKEN}")));
                    let mut inner = inner.lock().expect("the stand-in's state");
                    if path == "/stand-in" && request.method() == Method::POST {
                        let said = |name: &str| asked.iter().find(|(its, _)| its == name).map(|(_, value)| value.clone());
                        if let Some(title) = said("publishes") {
                            let collection = said("collection").unwrap_or_else(|| "Finance".to_string());
                            inner.publish(&title, said("desktop").as_deref() == Some("true"), &collection, None);
                        }
                        if let Some(title) = said("withdraws") {
                            inner.published.retain(|thing| thing.title != title);
                        }
                        if let Some(answers) = said("answers") {
                            inner.answers = answers == "true";
                        }
                        return plain(StatusCode::OK, "done\n");
                    }
                    if !inner.answers {
                        return plain(StatusCode::SERVICE_UNAVAILABLE, "The farm's web access is not there.\n");
                    }
                    let to_the_sign_in = || {
                        let mut sent = plain(StatusCode::FOUND, "Sign in first.\n");
                        sent.headers_mut().insert(
                            LOCATION,
                            HeaderValue::from_static("/RDWeb/FeedLogin/WebFeedLogin.aspx?ReturnUrl=%2FRDWeb%2FFeed%2Fwebfeed.aspx"),
                        );
                        sent
                    };
                    match path.as_str() {
                        SIGN_IN => inner.sign_in(named.as_deref()),
                        FEED if signed_in => inner.feed(),
                        _ if path.starts_with("/RDWeb/Feed/rdp/") && signed_in => inner.piece(path.trim_start_matches("/RDWeb/Feed/rdp/")),
                        _ if path.starts_with("/RDWeb/Feed/") => to_the_sign_in(),
                        _ => plain(StatusCode::NOT_FOUND, "Nothing is at this address.\n"),
                    }
                })
            })
        };
        tokio::spawn(listener.answer_with(answers, stop, Lines::kept_for("farm"), Measures::of("farm")));
        Farm { address, inner, stopper }
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("the stand-in's state")
    }

    /// Where its feed is.
    pub fn feed(&self) -> String {
        format!("{}{FEED}", self.address)
    }

    /// Publishes a program or a desktop, or changes one it publishes already.
    pub fn publishes(&self, title: &str, desktop: bool, collection: &str, icon: Option<&[u8]>) {
        self.inner().publish(title, desktop, collection, icon.map(<[u8]>::to_vec));
    }

    /// Publishes something no longer.
    pub fn withdraws(&self, title: &str) {
        self.inner().published.retain(|thing| thing.title != title);
    }

    /// Whether it answers at all from now on.
    pub fn answers(&self, answers: bool) {
        self.inner().answers = answers;
    }

    /// How often it gave its feed, and how often an icon or a file.
    pub fn gave(&self) -> (u64, u64) {
        let inner = self.inner();
        (inner.feeds, inner.pieces)
    }

    /// It answers no longer, and takes no connection.
    pub fn stop(&self) {
        self.stopper.stop();
    }
}

impl Inner {
    fn publish(&mut self, title: &str, desktop: bool, collection: &str, icon: Option<Vec<u8>>) {
        self.changes += 1;
        let alias: String = title.chars().filter(|character| character.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
        let thing = Published { title: title.to_string(), alias, desktop, collection: collection.to_string(), icon, changed: self.changes };
        match self.published.iter_mut().find(|other| other.title == title) {
            Some(other) => *other = thing,
            None => self.published.push(thing),
        }
    }
}
