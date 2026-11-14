//! The pages: the web app's files, which are part of the program, and the rules a
//! browser is given with them.
//!
//! The app is one page. Every address that is none of the server's own answers with
//! that page, which then shows the view the address names, so a link, a reload and the
//! back button lead to it. The page's script, styles and fonts have their content's
//! hash in their names: a browser may keep them for good, and asks for the page itself
//! every time.

use away_part::http::{answer, Answer};
use hyper::body::Bytes;
use hyper::header::{HeaderValue, CACHE_CONTROL, CONTENT_SECURITY_POLICY};
use hyper::StatusCode;

include!(concat!(env!("OUT_DIR"), "/pages.rs"));

/// What a browser may load for a page: files from Away's own address, and nothing from
/// anywhere else. No script or style written into a page runs, the page can't be shown
/// inside another, and it sends its forms nowhere.
pub const ONLY_FROM_AWAY: &str =
    "default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self'; \
                                  base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// Whether the program holds the pages: it does unless it was built without them.
pub fn are_built() -> bool {
    file("/index.html").is_some()
}

fn file(address: &str) -> Option<(&'static str, &'static [u8])> {
    FILES.iter().find(|(its, _, _)| *its == address).map(|(_, kind, holds)| (*kind, *holds))
}

/// The answer to a browser that asks for an address which is none of the server's own:
/// one of the pages' files, or the page itself. Gives nothing where no pages are built.
pub fn at(address: &str) -> Option<Answer> {
    if let Some((kind, holds)) = file(address).filter(|_| address != "/index.html") {
        let mut answer = answer(StatusCode::OK, kind, Bytes::from_static(holds));
        // its name changes when it does
        answer.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable"));
        return Some(answer);
    }
    // a file that isn't there is not the page: a browser that asks for a script gets no page back
    if address.starts_with("/assets/") {
        return None;
    }
    let (kind, holds) = file("/index.html")?;
    let mut answer = answer(StatusCode::OK, kind, Bytes::from_static(holds));
    answer.headers_mut().insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(ONLY_FROM_AWAY));
    Some(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_allows_aways_own_address_and_nothing_else() {
        for part in ONLY_FROM_AWAY.split("; ") {
            let (what, from) = part.split_once(' ').unwrap();
            assert!(matches!(from, "'none'" | "'self'"), "{what} may come from {from}");
        }
        assert!(ONLY_FROM_AWAY.starts_with("default-src 'none'; "));
        for never in ["unsafe-inline", "unsafe-eval", "data:", "blob:", "http", "*"] {
            assert!(!ONLY_FROM_AWAY.contains(never), "{never}");
        }
    }

    #[test]
    fn every_address_is_the_page_and_a_file_is_itself() {
        if !are_built() {
            assert!(at("/").is_none() && at("/sessions").is_none());
            return;
        }
        for address in ["/", "/sessions", "/settings", "/machines/fin-app-02", "/index.html"] {
            let page = at(address).unwrap();
            assert_eq!(page.headers()["content-type"], "text/html; charset=utf-8", "{address}");
            assert_eq!(page.headers()["content-security-policy"], ONLY_FROM_AWAY);
            assert!(page.headers().get("cache-control").is_none(), "the listener says that a page is never kept");
        }
        let (script, _, _) = FILES.iter().find(|(address, _, _)| address.ends_with(".js")).expect("the page's script");
        let file = at(script).unwrap();
        assert_eq!(file.headers()["content-type"], "text/javascript; charset=utf-8");
        assert_eq!(file.headers()["cache-control"], "public, max-age=31536000, immutable");
        assert!(at("/assets/no-such-file.js").is_none(), "a file that isn't there is no page");
        // the page names its files by Away's own address alone
        let (_, _, page) = FILES.iter().find(|(address, _, _)| *address == "/index.html").unwrap();
        let page = std::str::from_utf8(page).unwrap();
        assert!(!page.contains("http://") && !page.contains("https://") && !page.contains("<style") && !page.contains("onload="));
    }
}
