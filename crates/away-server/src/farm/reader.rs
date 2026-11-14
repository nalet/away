//! Asks a farm's feed what the farm publishes: signs in at the farm's web access with
//! the account Away is given for that, reads the feed, and takes each thing's icon and
//! the file that says how it is reached.
//!
//! The farm signs its readers in the way Windows does, tied to the connection, so the
//! whole of it is one conversation. Nothing is followed that leads elsewhere than the
//! farm's own web access.

use std::sync::Arc;

use away_part::address::Address;
use away_part::base64;
use away_part::client::{Answered, Client, Conversation, Failed};
use away_rdp::ntlm::{self, Account, Fresh};
use away_secret::Secret;

use super::{feed, Item};
use crate::register::{is_a_picture, Farm};

/// The most things of one farm that are taken.
const MOST: usize = 500;

/// Reads farms' feeds, as one account.
pub struct Reader {
    client: Client,
    account: Account,
}

/// What a sign-in at a farm's web access gave.
enum SignedIn {
    /// A token, to come back to the feed with.
    Token(Secret),
    /// The feed itself.
    Feed(Answered),
}

impl Reader {
    /// A reader that signs in as this account: `CORP\svc-away`, or a name with its domain.
    pub fn new(client: Client, account: &str, password: Secret) -> Reader {
        Reader { client, account: Account::named(account, password) }
    }

    /// Reads what a farm publishes. What was read before is given along, so that only
    /// what the farm changed since is taken anew. `clock` is the time now as Windows
    /// counts it, and `fresh` what the sign-in needs that has to be new each time.
    pub async fn read(&self, farm: &Farm, kept: &[Item], clock: u64, fresh: Fresh) -> Result<Vec<Item>, String> {
        let at = Address::read(&farm.feed).map_err(|why| format!("The feed's address can't be used: {why}."))?;
        let reach = |failed: Failed| format!("The farm's feed: {failed}.");
        let mut talk = self.client.talk(&at).await.map_err(reach)?;
        let first = talk.get(&at.path, &[]).await.map_err(reach)?;
        let mut cookie: Option<Secret> = None;
        let answered = match first.status {
            200 => first,
            // the farm signs its readers in: at the feed itself, or where it sends them
            301 | 302 | 303 | 307 | 308 | 401 => {
                let sign_in_at = match first.status {
                    401 => at.path.clone(),
                    _ => on_the_farm(first.header("location"), &at)
                        .ok_or("The farm sends its readers elsewhere than its own web access to sign in, where Away doesn't follow.")?,
                };
                match self.sign_in(&mut talk, &at, &sign_in_at, clock, fresh).await? {
                    SignedIn::Feed(answered) => answered,
                    SignedIn::Token(token) => {
                        let with = Secret::new(format!(".ASPXAUTH={}", token.expose()));
                        let answered = fetch(&self.client, &at, &mut talk, &at.path, Some(&with)).await.map_err(reach)?;
                        cookie = Some(with);
                        answered
                    }
                }
            }
            status => return Err(format!("The farm's feed answered with the status {status}.")),
        };
        if answered.status != 200 {
            return Err(format!("The farm's feed answered with the status {}, also to a reader that is signed in.", answered.status));
        }
        let text = std::str::from_utf8(&answered.body).map_err(|_| "What the farm sent is no feed: it is no text.".to_string())?;
        let resources = feed::read(text).map_err(|why| format!("What the farm sent is no feed: {why}."))?;

        let mut items = Vec::with_capacity(resources.len().min(MOST));
        for resource in resources.into_iter().take(MOST) {
            // what the farm has not changed since it was read is kept as it is
            let before = kept.iter().find(|item| !resource.changed.is_empty() && item.resource == resource);
            if let Some(before) = before {
                items.push(before.clone());
                continue;
            }
            let mut reached = feed::Reached::default();
            if let Some(path) = &resource.file {
                if let Ok(Answered { status: 200, body, .. }) = fetch(&self.client, &at, &mut talk, path, cookie.as_ref()).await {
                    reached = feed::read_file(&body);
                }
            }
            let mut icon = None;
            if let Some(path) = &resource.icon {
                if let Ok(Answered { status: 200, body, .. }) = fetch(&self.client, &at, &mut talk, path, cookie.as_ref()).await {
                    icon = is_a_picture(&body).then(|| Arc::new(body.to_vec()));
                }
            }
            items.push(Item { resource, reached, icon });
        }
        Ok(items)
    }

    /// Signs in on the conversation, the way Windows signs in over HTTP: what the client
    /// can do, the farm's challenge, and the proof that the client knows the password.
    async fn sign_in(&self, talk: &mut Conversation, at: &Address, path: &str, clock: u64, fresh: Fresh) -> Result<SignedIn, String> {
        let reach = |failed: Failed| format!("The farm's sign-in: {failed}.");
        let says = |message: &[u8]| Secret::new(format!("NTLM {}", base64::ordinary(message)));
        let first = ntlm::negotiate();
        let asked = talk.get(path, &[("authorization", &says(&first))]).await.map_err(reach)?;
        if asked.status == 200 {
            return Ok(given(asked));
        }
        let challenge = asked
            .headers_named("www-authenticate")
            .find_map(|offered| offered.strip_prefix("NTLM "))
            .and_then(|letters| base64::read_for_addresses(&letters.trim().replace('+', "-").replace('/', "_").replace('=', "")))
            .ok_or("The farm asks its readers to sign in in a way Away doesn't speak there: it speaks NTLM.")?;
        let read =
            ntlm::read_challenge(&challenge).map_err(|_| "The farm's sign-in sent a challenge that is not built as one.".to_string())?;
        let (third, _) = ntlm::authenticate(&first, &challenge, &read, &self.account, &format!("HTTP/{}", at.host), &fresh, clock);
        let answered = talk.get(path, &[("authorization", &says(&third))]).await.map_err(reach)?;
        match answered.status {
            200 => Ok(given(answered)),
            401 | 403 => Err("The farm doesn't take the account Away reads its feed with: its name or its password is wrong, or it may not read the feed.".to_string()),
            status => Err(format!("The farm's sign-in answered with the status {status}.")),
        }
    }
}

/// What a sign-in's answer holds: a token to come back with, or the feed itself.
fn given(answered: Answered) -> SignedIn {
    let token = answered.header("content-type").is_some_and(|kind| kind.starts_with("application/x-msts-webfeed-login"));
    match std::str::from_utf8(&answered.body).map(str::trim) {
        Ok(text) if token && !text.is_empty() && text.len() <= 8192 && text.chars().all(|c| c.is_ascii_alphanumeric()) => {
            SignedIn::Token(Secret::new(text))
        }
        _ => SignedIn::Feed(answered),
    }
}

/// The path of an address the farm names, where it is on the farm's own web access.
fn on_the_farm(named: Option<&str>, feed: &Address) -> Option<String> {
    let named = named?;
    if named.starts_with('/') && !named.starts_with("//") {
        return Some(named.to_string());
    }
    let address = Address::read(named).ok()?;
    (address.secured == feed.secured && address.host.eq_ignore_ascii_case(&feed.host) && address.port == feed.port).then_some(address.path)
}

/// Asks for something of the farm's on the conversation. A conversation the farm ended
/// in the meantime is begun anew, once.
async fn fetch(client: &Client, at: &Address, talk: &mut Conversation, path: &str, cookie: Option<&Secret>) -> Result<Answered, Failed> {
    let headers: Vec<(&str, &Secret)> = cookie.iter().map(|cookie| ("cookie", *cookie)).collect();
    match talk.get(path, &headers).await {
        Err(Failed::Broken(_)) => {
            *talk = client.talk(at).await?;
            talk.get(path, &headers).await
        }
        answered => answered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_is_on_the_farms_own_web_access_is_followed() {
        let feed = Address::read("https://broker.corp.example/RDWeb/Feed/webfeed.aspx").unwrap();
        assert_eq!(
            on_the_farm(Some("/RDWeb/FeedLogin/WebFeedLogin.aspx?ReturnUrl=x"), &feed).as_deref(),
            Some("/RDWeb/FeedLogin/WebFeedLogin.aspx?ReturnUrl=x")
        );
        assert_eq!(
            on_the_farm(Some("https://BROKER.corp.example/RDWeb/FeedLogin/WebFeedLogin.aspx"), &feed).as_deref(),
            Some("/RDWeb/FeedLogin/WebFeedLogin.aspx")
        );
        for elsewhere in ["https://sign-in.example.com/login", "//elsewhere.example/x", "https://broker.corp.example:8443/x", "nonsense"] {
            assert_eq!(on_the_farm(Some(elsewhere), &feed), None, "{elsewhere}");
        }
        assert_eq!(on_the_farm(None, &feed), None);
    }

    #[test]
    fn a_sign_ins_answer_is_a_token_or_the_feed_itself() {
        let answered = |kind: &str, body: &str| Answered {
            status: 200,
            headers: vec![("content-type".to_string(), kind.to_string())],
            body: body.to_string().into(),
        };
        assert!(
            matches!(given(answered("application/x-msts-webfeed-login; charset=utf-8", "66140BAB5ACA\r\n")), SignedIn::Token(token) if token.expose() == "66140BAB5ACA")
        );
        // a token is letters and digits: anything else is not taken for one
        assert!(matches!(given(answered("application/x-msts-webfeed-login", "a b; c")), SignedIn::Feed(_)));
        assert!(matches!(given(answered("text/xml", "<ResourceCollection/>")), SignedIn::Feed(_)));
    }
}
