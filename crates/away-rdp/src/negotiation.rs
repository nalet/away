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
    let mut asked = Vec::with_capacity(15);
    // the connection request: its length after this byte, what it is, no references, class 0
    asked.extend_from_slice(&[14, 0xe0, 0, 0, 0, 0, 0]);
    // the negotiation request: its kind, no flags, its length, what is asked for
    asked.extend_from_slice(&[1, 0, 8, 0]);
    asked.extend_from_slice(&ASKED_FOR.to_le_bytes());
    tpkt::frame(&asked)
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
