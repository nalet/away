//! The first exchange of a connection: Away asks a machine which security it takes, and
//! the machine chooses one or refuses (MS-RDPBCGR 2.2.1.1 and 2.2.1.2).
//!
//! Away asks for TLS, and for the sign-in before the desktop in both its forms. It never
//! asks for RDP's own old encryption, and refuses a machine that offers nothing else.

use crate::{tpkt, Error, Refusal};

/// TLS, and the sign-in on the machine's own screen after it.
pub const TLS: u32 = 0x1;
/// TLS, and the sign-in before the desktop.
pub const SIGN_IN_FIRST: u32 = 0x2;
/// The same, with the machine saying whether the person may come in before it goes on.
pub const SIGN_IN_FIRST_WITH_RESULT: u32 = 0x8;
/// What Away asks for.
pub const ASKED_FOR: u32 = TLS | SIGN_IN_FIRST | SIGN_IN_FIRST_WITH_RESULT;

/// The security a machine chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Security {
    /// TLS, with the sign-in on the machine's own screen.
    Tls,
    /// TLS, with the sign-in before the desktop.
    SignInFirst {
        /// Whether the machine says if the person may come in, before it goes on.
        tells_result: bool,
    },
}

impl Security {
    /// The number the machine gave for it, which the client says back later.
    pub fn number(self) -> u32 {
        match self {
            Security::Tls => TLS,
            Security::SignInFirst { tells_result: false } => SIGN_IN_FIRST,
            Security::SignInFirst { tells_result: true } => SIGN_IN_FIRST_WITH_RESULT,
        }
    }
}

/// A machine's answer to the first request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Answer {
    /// The security it chose.
    pub security: Security,
    /// Whether it has the graphics pipeline.
    pub graphics_pipeline: bool,
    /// Whether it takes the longer form of what a client says of itself.
    pub extended_client_data: bool,
    /// Whether it takes a sign-in that leaves no credential on it, meant for administrators.
    pub restricted_admin: bool,
    /// Whether it takes a sign-in that the person's own device answers for.
    pub redirected_sign_in: bool,
}

/// The first bytes of a connection: the request, asking for what Away takes.
pub fn request() -> Vec<u8> {
    request_with(None).expect("a request that says nothing first is as long as it may be")
}

/// The longest thing a client may say first to a farm.
pub const LONGEST_TOKEN: usize = 200;

/// The same request, with what the client says first to a farm: the collection it wants
/// of a broker, or what a broker gave it for the machine it was sent on to. A farm reads
/// it as one line.
pub fn request_with(token: Option<&[u8]>) -> Result<Vec<u8>, Error> {
    request_mode(token, false)
}

/// Requires Restricted Admin and CredSSP. TLS-only logon is never offered.
pub fn request_restricted_admin() -> Vec<u8> {
    request_mode(None, true).expect("a request without a routing token fits")
}

fn request_mode(token: Option<&[u8]>, restricted: bool) -> Result<Vec<u8>, Error> {
    let token = token.map(|token| token.strip_suffix(b"\r\n").unwrap_or(token));
    if token.is_some_and(|token| token.len() > LONGEST_TOKEN || token.iter().any(|byte| matches!(byte, b'\r' | b'\n'))) {
        return Err(Error::NotRdp("what is said first to a farm is one line, of 200 bytes at most"));
    }
    let line = token.map_or(0, |token| token.len() + 2);
    let mut asked = Vec::with_capacity(15 + line);
    // the connection request: its length after this byte, what it is, no references, class 0
    asked.extend_from_slice(&[(14 + line) as u8, 0xe0, 0, 0, 0, 0, 0]);
    if let Some(token) = token {
        asked.extend_from_slice(token);
        asked.extend_from_slice(b"\r\n");
    }
    // the negotiation request: its kind, no flags, its length, what is asked for
    asked.extend_from_slice(&[1, u8::from(restricted), 8, 0]);
    let protocols = if restricted { SIGN_IN_FIRST | SIGN_IN_FIRST_WITH_RESULT } else { ASKED_FOR };
    asked.extend_from_slice(&protocols.to_le_bytes());
    Ok(tpkt::frame(&asked))
}

/// What a client said first to a farm, read from what its request's frame carries: for
/// the tests' stand-in for a machine.
#[cfg(any(test, feature = "stand-in"))]
pub fn read_token(carried: &[u8]) -> Option<Vec<u8>> {
    let said = carried.get(7..carried.len().checked_sub(8)?)?;
    said.strip_suffix(b"\r\n").map(<[u8]>::to_vec)
}

/// Reads what the frame of a machine's answer carries.
pub fn read_answer(carried: &[u8]) -> Result<Answer, Error> {
    // the connection confirm: its length after this byte, what it is, references, class
    let [length, kind, _, _, _, _, _, rest @ ..] = carried else {
        return Err(Error::NotRdp("the answer to the first request is too short"));
    };
    if kind & 0xf0 != 0xd0 {
        return Err(Error::NotRdp("the answer to the first request is no confirmation"));
    }
    if usize::from(*length) != carried.len() - 1 {
        return Err(Error::NotRdp("the answer to the first request says another length than it has"));
    }
    if rest.is_empty() {
        return Err(Error::OldSecurityOnly);
    }
    let [kind, flags, 8, 0, value @ ..] = rest else {
        return Err(Error::NotRdp("the answer names no security"));
    };
    let Ok(value) = <[u8; 4]>::try_from(value) else {
        return Err(Error::NotRdp("the answer names no security"));
    };
    let value = u32::from_le_bytes(value);
    match kind {
        2 => {
            let security = match value {
                0 => return Err(Error::OldSecurityOnly),
                TLS => Security::Tls,
                SIGN_IN_FIRST => Security::SignInFirst { tells_result: false },
                SIGN_IN_FIRST_WITH_RESULT => Security::SignInFirst { tells_result: true },
                other => return Err(Error::NotAskedFor(other)),
            };
            Ok(Answer {
                security,
                extended_client_data: flags & 0x01 != 0,
                graphics_pipeline: flags & 0x02 != 0,
                restricted_admin: flags & 0x08 != 0,
                redirected_sign_in: flags & 0x10 != 0,
            })
        }
        3 => Err(Error::Refused(match value {
            1 => Refusal::TlsRequired,
            2 => Refusal::TlsNotAllowed,
            3 => Refusal::NoCertificate,
            4 => Refusal::InconsistentRequest,
            5 => Refusal::SignInFirstRequired,
            6 => Refusal::SignInFirstOverTlsRequired,
            other => Refusal::Other(other),
        })),
        _ => Err(Error::NotRdp("the answer is neither a choice nor a refusal")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tpkt::Frames;

    fn carried(bytes: &[u8]) -> Vec<u8> {
        let mut frames = Frames::new();
        frames.push(bytes);
        frames.next_frame().unwrap().unwrap()
    }

    #[test]
    fn restricted_admin_requires_credentialless_credssp() {
        let asked = carried(&request_restricted_admin());
        assert_eq!(&asked[7..], &[1, 1, 8, 0, 10, 0, 0, 0]);
    }

    #[test]
    fn the_request_asks_for_tls_and_both_forms_of_the_sign_in_first() {
        assert_eq!(
            request(),
            [0x03, 0x00, 0x00, 0x13, 0x0e, 0xe0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x08, 0x00, 0x0b, 0x00, 0x00, 0x00]
        );
    }

    /// What a Windows 11 machine answered on 14 November 2026.
    #[test]
    fn a_windows_machine_chooses_the_sign_in_first_and_says_what_else_it_takes() {
        let answer = [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x02, 0x3f, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00];
        assert_eq!(
            read_answer(&carried(&answer)),
            Ok(Answer {
                security: Security::SignInFirst { tells_result: true },
                graphics_pipeline: true,
                extended_client_data: true,
                restricted_admin: true,
                redirected_sign_in: true,
            })
        );
    }

    /// What the same machine answered when it was asked for TLS alone.
    #[test]
    fn a_machine_that_wants_the_sign_in_first_refuses_tls_alone() {
        let answer = [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x03, 0x00, 0x08, 0x00, 0x05, 0x00, 0x00, 0x00];
        assert_eq!(read_answer(&carried(&answer)), Err(Error::Refused(Refusal::SignInFirstRequired)));
    }

    #[test]
    fn a_machine_can_choose_tls_alone() {
        let answer = [0x03, 0x00, 0x00, 0x13, 0x0e, 0xd0, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x08, 0x00, 0x01, 0x00, 0x00, 0x00];
        let answer = read_answer(&carried(&answer)).unwrap();
        assert_eq!(answer.security, Security::Tls);
        assert!(!answer.graphics_pipeline);
    }

    #[test]
    fn a_machine_with_only_the_old_encryption_is_refused() {
        // a confirmation with nothing after it, and one that chooses the old encryption
        assert_eq!(read_answer(&[0x06, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00]), Err(Error::OldSecurityOnly));
        let chosen = [0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x02, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(read_answer(&chosen), Err(Error::OldSecurityOnly));
    }

    #[test]
    fn what_is_no_answer_is_refused_and_never_read_past_its_end() {
        assert!(matches!(read_answer(&[]), Err(Error::NotRdp(_))));
        assert!(matches!(read_answer(&[0x06, 0xe0, 0, 0, 0, 0, 0]), Err(Error::NotRdp(_))), "a request is no answer");
        assert!(matches!(read_answer(&[0x0e, 0xd0, 0, 0, 0, 0, 0, 2, 0, 8, 0, 8]), Err(Error::NotRdp(_))), "cut short");
        assert!(matches!(read_answer(&[0x09, 0xd0, 0, 0, 0, 0, 0, 2, 0, 8, 0, 8, 0, 0, 0]), Err(Error::NotRdp(_))), "a wrong length");
        let unknown = [0x0e, 0xd0, 0, 0, 0, 0, 0, 2, 0, 8, 0, 4, 0, 0, 0];
        assert_eq!(read_answer(&unknown), Err(Error::NotAskedFor(4)));
        // every beginning of a real answer is refused, none panics
        let real = [0x0e, 0xd0, 0x00, 0x00, 0x12, 0x34, 0x00, 0x02, 0x3f, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00];
        for cut in 0..real.len() {
            assert!(read_answer(&real[..cut]).is_err(), "cut at {cut}");
        }
    }
}
