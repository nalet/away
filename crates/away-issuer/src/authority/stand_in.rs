//! What stands in for an organisation's authority in the tests: pages on this machine,
//! behind TLS, that take a ticket for their service as a Windows authority's pages do,
//! read a request as an authority reads one, and issue certificates with an authority
//! that exists for the tests alone.
//!
//! Like a real authority, they decide themselves what they issue: one account may ask,
//! with a proof that is bound to the connection; a request in another's name needs an
//! agent's certificate that they issued themselves; and there are accounts they issue
//! nothing for, whoever asks.
//!
//! It is built only for tests: nothing of it is in a release.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use away_part::address::read_form;
use away_part::base64;
use away_part::http::{answer, carried, Answer, Answers, Listener};
use away_part::{Lines, Measures, Stopper};
use away_winauth::certificate::{Certificate, FOR_ASKING_FOR_OTHERS};
use away_winauth::kerberos::crypto::Key;
use away_winauth::kerberos::gss::stand_in::{answer as answer_the_ticket, key_in_the_ticket, Ends};
use away_winauth::kerberos::gss::Bound;
use away_winauth::kerberos::pkinit::{Held, Signs};
use away_winauth::request::read;
use away_winauth::request::stand_in::{issue, read_in_the_name_of, For};
use hyper::header::{HeaderValue, AUTHORIZATION, WWW_AUTHENTICATE};
use hyper::{Method, StatusCode};

/// How far in the past a certificate begins, in seconds, as a Windows authority begins
/// its certificates: clocks need not agree to the second.
const BEGINS_BEFORE: u64 = 600;
/// How long an agent's certificate is good for, in seconds.
const AN_AGENTS_IS_GOOD_FOR: u64 = 30 * 24 * 3600;

/// What the pages are made with.
pub struct With {
    /// The certificate the pages show for `localhost`, and its key, as their bytes.
    pub shows: (Vec<u8>, Vec<u8>),
    /// The key of the pages' service, as the domain controller knows it.
    pub of_the_service: Key,
    /// The authority that issues, and its key, as their bytes.
    pub issues_with: (Vec<u8>, Vec<u8>),
    /// The one account that may ask, by its name.
    pub asked_as: String,
    /// The domain as accounts are written with it before a backslash: `CORP`.
    pub domain: String,
    /// The realm, as accounts are written with it in a certificate: `CORP.EXAMPLE`.
    pub realm: String,
    /// The kind of certificate that signs a person in.
    pub for_signing_in: String,
    /// The kind of certificate that allows asking for others.
    pub for_an_agent: String,
    /// Accounts the authority issues nothing for, by their names.
    pub never_for: Vec<String>,
    /// How long a certificate for signing in is good for from the moment it is issued,
    /// in seconds.
    pub good_for: u64,
}

/// A certificate the pages issued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issued {
    /// Its number.
    pub number: u32,
    /// Its kind.
    pub kind: String,
    /// The account it is for, as a certificate writes it.
    pub account: String,
    /// Whether an agent asked for it in the account's name.
    pub by_an_agent: bool,
}

/// What the pages saw.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seen {
    /// The account each request was signed in as.
    pub signed_in: Vec<String>,
    /// How many requests showed no ticket the pages take: none, one for another
    /// service, one of another account, or one whose proof is bound to another
    /// connection.
    pub not_signed_in: usize,
    /// What was issued.
    pub issued: Vec<Issued>,
    /// What was refused, each with the words the pages gave.
    pub refused: Vec<String>,
}

/// The pages, answering.
pub struct Pages {
    /// Their address: `https://localhost:…/certsrv/`.
    pub address: String,
    seen: Arc<Mutex<Seen>>,
    _stopper: Stopper,
}

struct Inside {
    with: With,
    bound: Bound,
    issuing: Held,
    next: Mutex<u32>,
    issued: Mutex<HashMap<u32, Vec<u8>>>,
    seen: Arc<Mutex<Seen>>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Bytes as text between two lines of dashes.
pub fn between_dashes(what: &str, bytes: &[u8]) -> String {
    let letters = base64::ordinary(bytes);
    let lines: Vec<&str> = letters.as_bytes().chunks(64).map(|line| std::str::from_utf8(line).unwrap_or_default()).collect();
    format!("-----BEGIN {what}-----\r\n{}\r\n-----END {what}-----\r\n", lines.join("\r\n"))
}

fn page(title: &str, parts: &str) -> Answer {
    let page = format!(
        "\r\n<HTML>\r\n<Head>\r\n    <Meta HTTP-Equiv=\"Content-Type\" Content=\"text/html; charset=UTF-8\">\r\n    \
         <Title>Certificate Services</Title>\r\n</Head>\r\n<Body><Font ID=locPageFont Face=\"Arial\">\r\n\r\n\
         <P ID=locPageTitle> <B> {title} </B>\r\n{parts}\r\n</Font>\r\n</Body>\r\n</HTML>\r\n"
    );
    answer(StatusCode::OK, "text/html", page)
}

impl Pages {
    /// Opens the pages on a port of this machine that is free.
    pub async fn open(with: With) -> Pages {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let listener = Listener::bind("an authority's pages", "127.0.0.1:0")
            .await
            .expect("an address")
            .secured_with(between_dashes("CERTIFICATE", &with.shows.0).as_bytes(), between_dashes("PRIVATE KEY", &with.shows.1).as_bytes())
            .expect("the pages' certificate and its key");
        let address = format!("https://localhost:{}/certsrv/", listener.address().port());
        let held = Arc::new(Inside {
            bound: Bound::to_the_certificate(&with.shows.0).expect("the pages' certificate"),
            issuing: Held::new(&with.issues_with.0, &with.issues_with.1).expect("the authority that issues, with its key"),
            with,
            next: Mutex::new(449),
            issued: Mutex::new(HashMap::new()),
            seen: seen.clone(),
        });
        let answers: Answers = Arc::new(move |request| {
            let held = held.clone();
            Box::pin(async move {
                // whoever asks shows a ticket for the pages' service, with a proof that is
                // bound to the certificate the pages showed
                let token = request
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.strip_prefix("Negotiate "))
                    .and_then(|letters| base64::read_for_addresses(&letters.trim().replace('+', "-").replace('/', "_").replace('=', "")));
                let said = token.and_then(|token| {
                    let session = key_in_the_ticket(&token, &held.with.of_the_service).ok()?;
                    answer_the_ticket(&token, &session, None, Ends::Content, [4; 16]).ok().map(|(_, said, _)| said)
                });
                match said {
                    Some(said) if said.account == held.with.asked_as && said.bound == held.bound => {
                        held.seen.lock().expect("what was seen").signed_in.push(said.account);
                    }
                    _ => {
                        held.seen.lock().expect("what was seen").not_signed_in += 1;
                        let mut refused = answer(StatusCode::UNAUTHORIZED, "text/html", "<html><body>You are not signed in.</body></html>");
                        refused.headers_mut().append(WWW_AUTHENTICATE, HeaderValue::from_static("Negotiate"));
                        return refused;
                    }
                }
                let asked =
                    (request.method().clone(), request.uri().path().to_string(), request.uri().query().unwrap_or_default().to_string());
                match (&asked.0, asked.1.as_str()) {
                    (&Method::POST, "/certsrv/certfnsh.asp") => {
                        let sent = carried(request, 64 * 1024).await.unwrap_or_default();
                        held.asked(&String::from_utf8_lossy(&sent))
                    }
                    (&Method::GET, "/certsrv/certnew.cer") => {
                        let number = read_form(&asked.2)
                            .and_then(|asked| asked.into_iter().find(|(name, _)| name == "ReqID"))
                            .and_then(|(_, number)| number.parse::<u32>().ok());
                        match number.and_then(|number| held.issued.lock().expect("what was issued").get(&number).cloned()) {
                            Some(certificate) => {
                                answer(StatusCode::OK, "application/pkix-cert", between_dashes("CERTIFICATE", &certificate))
                            }
                            None => answer(StatusCode::NOT_FOUND, "text/html", "<html><body>No such certificate.</body></html>"),
                        }
                    }
                    _ => answer(StatusCode::NOT_FOUND, "text/html", "<html><body>Not found.</body></html>"),
                }
            })
        });
        let (stopper, stop) = away_part::stop::pair();
        tokio::spawn(listener.answer_with(answers, stop, Lines::kept_for("an authority's pages"), Measures::of("an authority's pages")));
        Pages { address, seen, _stopper: stopper }
    }

    /// What the pages saw so far.
    pub fn seen(&self) -> Seen {
        self.seen.lock().expect("what was seen").clone()
    }
}

impl Inside {
    fn number(&self) -> u32 {
        let mut next = self.next.lock().expect("the next number");
        *next += 1;
        *next - 1
    }

    /// A refusal, in a page built as a Windows authority's is.
    fn refuse(&self, says: &str) -> Answer {
        let said = format!("Your Request Id is {}. The disposition message is \"{says}\".", self.number());
        self.seen.lock().expect("what was seen").refused.push(said.clone());
        page(
            "Certificate Request Denied",
            &format!(
                "<P ID=locDenied> Your certificate request was denied. </P>\r\n\r\n<P ID=locInfoReqIDandReason>\r\n\r\n{said}\r\n</P>\r\n\r\n\
                 <P ID=locContactAdmin>Contact your administrator for further information.\r\n</P>"
            ),
        )
    }

    /// Takes a request, and issues a certificate for it or refuses.
    fn asked(&self, sent: &str) -> Answer {
        let form = read_form(sent).unwrap_or_default();
        let field = |name: &str| form.iter().find(|(its, _)| its == name).map(|(_, value)| value.clone()).unwrap_or_default();
        let kind = field("CertAttrib").strip_prefix("CertificateTemplate:").unwrap_or_default().to_string();
        let sent =
            base64::read_for_addresses(&field("CertRequest").replace('+', "-").replace('/', "_").replace('=', "")).unwrap_or_default();
        let with = &self.with;
        if kind != with.for_signing_in && kind != with.for_an_agent {
            return self.refuse(&format!(
                "Denied by Policy Module  0x80094800, The request was for a certificate template that is not supported by the \
                 Active Directory Certificate Services policy: {kind}.\r\n"
            ));
        }
        let issuer = Certificate::read(self.issuing.certificate()).expect("the authority that issues");
        // an agent's certificate is the account's own; one for signing in is asked for by
        // an agent, in the name of the account it is for
        let (asked, account, good_for, until) = match read_in_the_name_of(&sent) {
            Ok(signed) if kind == with.for_signing_in => {
                let agents = Certificate::read(&signed.agent).ok();
                let is_an_agent = agents.as_ref().is_some_and(|agents| {
                    agents.is_signed_by(&issuer)
                        && agents.is_for(FOR_ASKING_FOR_OTHERS)
                        && agents.is_good_at(now())
                        && agents.account.as_deref() == Some(&format!("{}@{}", with.asked_as, with.realm))
                });
                let name = signed.account.strip_prefix(&format!("{}\\", with.domain)).map(str::to_string);
                match name {
                    Some(name) if is_an_agent && !with.never_for.contains(&name) && !name.is_empty() => {
                        (signed.asked, format!("{name}@{}", with.realm), For::SigningIn, now() + with.good_for)
                    }
                    _ => return self.refuse("Denied by Policy Module"),
                }
            }
            Err(_) if kind == with.for_an_agent => {
                (sent, format!("{}@{}", with.asked_as, with.realm), For::AskingForOthers, now() + AN_AGENTS_IS_GOOD_FOR)
            }
            _ => return self.refuse("Denied by Policy Module"),
        };
        if read(&asked).is_err() {
            return self.refuse("Denied by Policy Module");
        }
        let number = self.number();
        let Ok(certificate) = issue(&asked, &account, good_for, &self.issuing, u64::from(number), (now() - BEGINS_BEFORE, until)) else {
            return self.refuse("Denied by Policy Module");
        };
        self.issued.lock().expect("what was issued").insert(number, certificate);
        self.seen.lock().expect("what was seen").issued.push(Issued { number, kind, account, by_an_agent: good_for == For::SigningIn });
        page(
            "Certificate Issued",
            &format!(
                "<P ID=locInfoCertIssued> The certificate you requested was issued to you.</P>\r\n\
                 <A Href=\"certnew.cer?ReqID={number}&amp;Enc=b64\">Download certificate</A>"
            ),
        )
    }
}
