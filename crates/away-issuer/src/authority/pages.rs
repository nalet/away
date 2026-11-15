//! The pages a Windows authority publishes for asking it for certificates: what is
//! sent to them, and what they answer.
//!
//! The pages are written for people and say what happened in the language of the
//! machine they are on. What is read here are the marks that are the same in every
//! language: the link to the certificate, and the names the pages give their parts.

use away_part::base64;

/// Where a request is sent, beside the pages' own address.
pub const ASKED_AT: &str = "certfnsh.asp";

/// What the pages are sent with a request: the request itself and the kind of
/// certificate that is asked for, as the authority names its kinds.
pub fn form(request: &[u8], kind: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Mode", "newreq".to_string()),
        ("CertRequest", base64::ordinary(request)),
        ("CertAttrib", format!("CertificateTemplate:{kind}")),
        ("TargetStoreFlags", "0".to_string()),
        ("SaveCert", "yes".to_string()),
        ("ThumbPrint", String::new()),
    ]
}

/// What the pages answered to a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The certificate was issued, under this number of the authority's.
    Issued(u32),
    /// The authority refused, and says this about it.
    Refused(String),
    /// Something else: what the page says, as far as it is short.
    Other(String),
}

/// Where the certificate of a request is fetched, beside the pages' own address.
pub fn issued_at(number: u32) -> String {
    format!("certnew.cer?ReqID={number}&Enc=b64")
}

/// Reads what the pages answered.
pub fn read(page: &str) -> Answer {
    const LINK: &str = "certnew.cer?ReqID=";
    if let Some(at) = page.find(LINK) {
        let digits: String = page[at + LINK.len()..].chars().take_while(char::is_ascii_digit).collect();
        if let Ok(number) = digits.parse::<u32>() {
            return Answer::Issued(number);
        }
    }
    if part(page, "locDenied").is_some() {
        return Answer::Refused(part(page, "locInfoReqIDandReason").unwrap_or_else(|| shown(page, 400)));
    }
    Answer::Other(shown(page, 400))
}

/// The certificate in the file the pages give: the bytes between its two lines of
/// dashes.
pub fn certificate(file: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(file).ok()?;
    let mut lines = text.lines().map(str::trim).skip_while(|line| !line.starts_with("-----BEGIN CERTIFICATE"));
    lines.next()?;
    let letters: String = lines.take_while(|line| !line.starts_with("-----")).collect();
    let bytes = base64::read_for_addresses(&letters.replace('+', "-").replace('/', "_").replace('=', ""))?;
    (!bytes.is_empty()).then_some(bytes)
}

/// The text of the part of a page that carries this name, where the page has one.
fn part(page: &str, named: &str) -> Option<String> {
    let lower = page.to_ascii_lowercase();
    let at = lower.find(&format!("id={}", named.to_ascii_lowercase()))?;
    let from = at + lower[at..].find('>')? + 1;
    let until = lower[from..].find("</p>").map_or(page.len(), |end| from + end);
    let text = shown(&page[from..until], 600);
    (!text.is_empty()).then_some(text)
}

/// What a page shows a person: its text without its markup, in one line, and no longer
/// than this many letters.
fn shown(page: &str, at_most: usize) -> String {
    let mut text = String::with_capacity(page.len());
    let mut rest = page;
    while let Some(at) = rest.find('<') {
        text.push_str(&rest[..at]);
        text.push(' ');
        let lower = rest[at..].to_ascii_lowercase();
        // what scripts, styles and comments hold is not shown
        let skipped = [("<script", "</script>"), ("<style", "</style>"), ("<!--", "-->")]
            .iter()
            .find(|(begins, _)| lower.starts_with(begins))
            .map(|(_, ends)| lower.find(ends).map_or(lower.len(), |end| end + ends.len()));
        let passed = skipped.unwrap_or_else(|| lower.find('>').map_or(lower.len(), |end| end + 1));
        rest = &rest[at + passed..];
    }
    text.push_str(rest);
    let text = text.replace("&nbsp;", " ").replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match one_line.char_indices().nth(at_most) {
        Some((at, _)) => format!("{}…", &one_line[..at]),
        None => one_line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page as the authority of a real domain answered a request that it refused,
    /// with the authority's name changed.
    const REFUSED: &str = r##"
<HTML>
<Head>
    <Meta HTTP-Equiv="Content-Type" Content="text/html; charset=UTF-8">
    <Title>Microsoft Active Directory Certificate Services</Title>
</Head>
<Body BgColor=#FFFFFF Link=#0000FF VLink=#0000FF ALink=#0000FF><Font ID=locPageFont Face="Arial">

<Table Border=0 CellSpacing=0 CellPadding=4 Width=100% BgColor=#008080>
<TR>
	<TD><Font Color=#FFFFFF><LocID ID=locMSCertSrv><Font Face="Arial" Size=-1><B><I>Microsoft</I></B> Active Directory Certificate Services &nbsp;--&nbsp; corp-authority &nbsp;</Font></LocID></Font></TD>
	<TD ID=locHomeAlign Align=Right><A Href="/certsrv"><Font Color=#FFFFFF><LocID ID=locHomeLink><Font Face="Arial" Size=-1><B>Home</B></Font></LocID></Font></A></TD>
</TR>
</Table>

<P ID=locPageTitle> <B> Certificate Request Denied </B>
<!-- Green HR --><Table Border=0 CellSpacing=0 CellPadding=0 Width=100%><TR><TD BgColor=#008080><Img Src="certspc.gif" Alt="" Height=2 Width=1></TD></TR></Table>


<P ID=locDenied> Your certificate request was denied. </P>



<P ID=locInfoReqIDandReason>


Your Request Id is 452. The disposition message is "Denied by Policy Module  0x80094800, The request was for a certificate template that is not supported by the Active Directory Certificate Services policy: NoSuchKind.
".
</P>

<P ID=locContactAdmin>Contact your administrator for further information.
</P>

</Font>
<!-- ############################################################ -->
<!-- End of standard text. Scripts follow  -->

<!-- No scripts -->

</Body>
</HTML>
"##;

    #[test]
    fn a_request_goes_with_the_kind_of_certificate_that_is_asked_for() {
        let form = form(&[0x30, 0x03, 0x02, 0x01, 0x00], "CorpSignIn");
        assert_eq!(form[0], ("Mode", "newreq".to_string()));
        assert_eq!(form[1], ("CertRequest", "MAMCAQA=".to_string()));
        assert_eq!(form[2], ("CertAttrib", "CertificateTemplate:CorpSignIn".to_string()));
    }

    #[test]
    fn a_page_with_the_link_to_a_certificate_says_that_it_was_issued() {
        let page = r#"<P ID=locCertIssued>The certificate you requested was issued to you.</P>
            <A Href="certnew.cer?ReqID=449&amp;Enc=b64">Download certificate</A>"#;
        assert_eq!(read(page), Answer::Issued(449));
        assert_eq!(issued_at(449), "certnew.cer?ReqID=449&Enc=b64");
        // a link without a number is none
        assert!(matches!(read("certnew.cer?ReqID=&Enc=b64"), Answer::Other(_)));
    }

    #[test]
    fn a_refusal_is_given_in_the_authoritys_own_words() {
        let Answer::Refused(why) = read(REFUSED) else { panic!("the page is a refusal") };
        assert_eq!(
            why,
            "Your Request Id is 452. The disposition message is \"Denied by Policy Module 0x80094800, The request was for a \
             certificate template that is not supported by the Active Directory Certificate Services policy: NoSuchKind. \"."
        );
    }

    #[test]
    fn any_other_page_is_given_as_what_it_shows_and_no_longer_than_it_may_be() {
        let page = "<html><head><style>p { color: red }</style><script>var a = '<p>';</script></head>\
                    <body><!-- a note --><p>The service is busy.&nbsp;Try again &amp; wait.</p></body></html>";
        assert_eq!(read(page), Answer::Other("The service is busy. Try again & wait.".to_string()));
        let long = format!("<p>{}</p>", "word ".repeat(200));
        let Answer::Other(text) = read(&long) else { panic!("neither a certificate nor a refusal") };
        assert_eq!(text.chars().count(), 401);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn the_certificate_is_what_stands_between_the_two_lines_of_dashes() {
        let file = b"-----BEGIN CERTIFICATE-----\r\nMAMC\r\nAQA=\r\n-----END CERTIFICATE-----\r\n";
        assert_eq!(certificate(file), Some(vec![0x30, 0x03, 0x02, 0x01, 0x00]));
        assert_eq!(certificate(b"<html>no certificate</html>"), None);
        assert_eq!(certificate(b"-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----\n"), None);
    }
}
