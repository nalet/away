//! Asks something that answers one request a connection, where each way a message
//! travels as its length in four bytes and then the message: the way a domain
//! controller is asked for a ticket (RFC 4120, section 7.2.2).

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// The longest answer that is taken.
const AT_MOST: usize = 1024 * 1024;

/// Sends one request, and gives what came back. The reason where nothing did.
pub async fn ask(host: &str, port: u16, request: &[u8], within: Duration) -> Result<Vec<u8>, String> {
    let asked = async {
        let mut connection = TcpStream::connect((host, port)).await.map_err(|error| format!("{host} could not be reached: {error}"))?;
        let length = u32::try_from(request.len()).map_err(|_| "the request is longer than a request may be".to_string())?;
        connection.write_all(&length.to_be_bytes()).await.map_err(|error| format!("{host} took no request: {error}"))?;
        connection.write_all(request).await.map_err(|error| format!("{host} took no request: {error}"))?;
        let closed = |_| format!("{host} closed the connection without an answer");
        let mut length = [0u8; 4];
        connection.read_exact(&mut length).await.map_err(closed)?;
        let length = u32::from_be_bytes(length) as usize;
        if length > AT_MOST {
            return Err(format!("{host} says its answer is longer than any answer is"));
        }
        let mut answer = vec![0u8; length];
        connection.read_exact(&mut answer).await.map_err(closed)?;
        Ok(answer)
    };
    match tokio::time::timeout(within, asked).await {
        Ok(answered) => answered,
        Err(_) => Err(format!("{host} gave no answer within {} seconds", within.as_secs())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// Something that answers each connection's request with this, behind its length.
    async fn answering(with: Option<&'static [u8]>, says_its_length_is: Option<u32>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut connection, _)) = listener.accept().await {
                let mut length = [0u8; 4];
                if connection.read_exact(&mut length).await.is_err() {
                    continue;
                }
                let mut request = vec![0u8; u32::from_be_bytes(length) as usize];
                let _ = connection.read_exact(&mut request).await;
                if let Some(with) = with {
                    let length = says_its_length_is.unwrap_or(with.len() as u32);
                    let _ = connection.write_all(&[&length.to_be_bytes()[..], &request, with].concat()).await;
                }
            }
        });
        port
    }

    #[tokio::test]
    async fn a_request_goes_out_behind_its_length_and_the_answer_comes_back_behind_its_own() {
        let port = answering(Some(b""), None).await;
        // the stand-in sends the request back: with nothing more, the answer is the request
        let port_that_echoes = answering(Some(b" and more"), Some(4 + 9)).await;
        assert_eq!(ask("127.0.0.1", port, b"", Duration::from_secs(5)).await.as_deref(), Ok(&b""[..]));
        assert_eq!(ask("127.0.0.1", port_that_echoes, b"this", Duration::from_secs(5)).await.as_deref(), Ok(&b"this and more"[..]));
    }

    #[tokio::test]
    async fn what_does_not_answer_is_said_with_its_reason() {
        let silent = answering(None, None).await;
        let why = ask("127.0.0.1", silent, b"this", Duration::from_secs(5)).await.unwrap_err();
        assert_eq!(why, "127.0.0.1 closed the connection without an answer");
        let too_long = answering(Some(b""), Some(64 * 1024 * 1024)).await;
        let why = ask("127.0.0.1", too_long, b"this", Duration::from_secs(5)).await.unwrap_err();
        assert_eq!(why, "127.0.0.1 says its answer is longer than any answer is");
        assert!(ask("127.0.0.1", 1, b"this", Duration::from_secs(5)).await.unwrap_err().starts_with("127.0.0.1 could not be reached: "));
        // something that takes the connection and then says nothing
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let waits = listener.local_addr().unwrap().port();
        let why = ask("127.0.0.1", waits, b"this", Duration::from_millis(200)).await.unwrap_err();
        assert_eq!(why, "127.0.0.1 gave no answer within 0 seconds");
    }
}
