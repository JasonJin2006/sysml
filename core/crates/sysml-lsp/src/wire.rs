//! Message framing for the Language Server Protocol.
//!
//! Every message is a header block and a JSON body:
//!
//! ```text
//! Content-Length: 42\r\n
//! \r\n
//! {"jsonrpc":"2.0", ...}
//! ```
//!
//! `Content-Length` is bytes, not characters. A model is written in several scripts, and a reader
//! that counted characters would split a message in the middle of a multi-byte one — the failure
//! shows up as a parse error far from its cause.

use std::io::{BufRead, Read, Write};

/// One message, read from the wire.
#[derive(Debug, Clone)]
pub struct Message {
    pub body: serde_json::Value,
}

/// Read one message from `input`, or `None` at end of stream.
///
/// # Errors
///
/// Returns the operating system's message when the stream cannot be read, and the header's when it
/// is malformed. A missing `Content-Length` is an error rather than a guess: an editor that sends
/// one is defective, and guessing the length would desynchronise every message after it.
pub fn read(input: &mut impl BufRead) -> Result<Option<Message>, String> {
    let mut length: Option<usize> = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let read = input
            .read_line(&mut line)
            .map_err(|e| format!("reading a header: {e}"))?;
        if read == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if !saw_header {
                // A blank line before any header: tolerate it. Some clients send a trailing CRLF.
                continue;
            }
            break;
        }
        saw_header = true;
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = Some(
                    value
                        .trim()
                        .parse()
                        .map_err(|e| format!("Content-Length {value:?}: {e}"))?,
                );
            }
        }
    }
    let Some(length) = length else {
        return Err("a message arrived with no Content-Length".to_string());
    };
    let mut body = vec![0u8; length];
    input
        .read_exact(&mut body)
        .map_err(|e| format!("reading {length} bytes of a message: {e}"))?;
    let text = String::from_utf8(body).map_err(|e| {
        format!(
            "a message body is not UTF-8 at byte {}",
            e.utf8_error().valid_up_to()
        )
    })?;
    let body =
        serde_json::from_str(&text).map_err(|e| format!("a message body is not JSON: {e}"))?;
    Ok(Some(Message { body }))
}

/// Write one message.
///
/// # Errors
///
/// Returns the operating system's message when the stream cannot be written.
pub fn write(output: &mut impl Write, body: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string(body).map_err(|e| format!("serialising a response: {e}"))?;
    // The header counts bytes. `text.len()` on a `String` is already the byte length, which is why
    // the body is serialised to a `String` rather than to a writer directly.
    write!(output, "Content-Length: {}\r\n\r\n{}", text.len(), text)
        .map_err(|e| format!("writing a message: {e}"))?;
    output
        .flush()
        .map_err(|e| format!("flushing a message: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_round_trips() {
        let body = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
        let mut buffer = Vec::new();
        write(&mut buffer, &body).expect("write");
        let mut cursor = std::io::Cursor::new(buffer);
        let read_back = read(&mut cursor).expect("read").expect("a message");
        assert_eq!(read_back.body, body);
    }

    #[test]
    fn the_length_counts_bytes_not_characters() {
        // `温度` is six bytes in UTF-8 and two characters. A reader that trusted character counts
        // would stop in the middle of the second one.
        let body = serde_json::json!({ "text": "温度" });
        let mut buffer = Vec::new();
        write(&mut buffer, &body).expect("write");
        let header = String::from_utf8_lossy(&buffer).to_string();
        //  is 18 bytes: 9 for the ASCII skeleton and 6 for the two 3-byte
        // characters, where a character count would say 14.
        assert!(header.contains("Content-Length: 17"), "{header}");
        let mut cursor = std::io::Cursor::new(buffer);
        let read_back = read(&mut cursor).expect("read").expect("a message");
        assert_eq!(read_back.body["text"], "温度");
    }

    #[test]
    fn a_message_without_a_length_is_an_error() {
        let mut cursor = std::io::Cursor::new(b"X-Thing: 1\r\n\r\n{}".to_vec());
        assert!(read(&mut cursor).is_err());
    }

    #[test]
    fn two_messages_in_one_stream_are_read_in_order() {
        let mut buffer = Vec::new();
        write(&mut buffer, &serde_json::json!({ "n": 1 })).expect("write one");
        write(&mut buffer, &serde_json::json!({ "n": 2 })).expect("write two");
        let mut cursor = std::io::Cursor::new(buffer);
        let first = read(&mut cursor).expect("read").expect("first");
        let second = read(&mut cursor).expect("read").expect("second");
        assert_eq!(first.body["n"], 1);
        assert_eq!(second.body["n"], 2);
        assert!(read(&mut cursor).expect("read").is_none(), "then the end");
    }
}
