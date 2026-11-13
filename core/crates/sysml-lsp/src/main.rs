//! `sysml-lsp` — a Language Server for SysML v2.
//!
//! Reads messages on stdin, writes replies on stdout. Everything it knows comes from the kernel:
//! `sysml-syntax` parses, `sysml-resolve` binds names, `sysml-semantics` reports what is wrong.
//! This binary translates positions and shapes and nothing else.
//!
//! Diagnostics go to stderr. A server that printed anything else on stdout would corrupt the
//! protocol, and the failure looks like a client bug rather than a server one.

use std::io::{BufReader, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let mut server = sysml_lsp::Server::new();

    loop {
        match sysml_lsp::wire::read(&mut input) {
            Ok(Some(message)) => {
                // `exit` ends the session. The protocol says a server terminates on it, and a
                // client that sends `shutdown` then `exit` waits for the process to go; one that
                // keeps reading blocks that client for ever.
                let leaving = message
                    .body
                    .get("method")
                    .and_then(serde_json::Value::as_str)
                    == Some("exit");
                for reply in server.handle(&message.body) {
                    if let Err(e) = sysml_lsp::wire::write(&mut output, &reply) {
                        eprintln!("sysml-lsp: {e}");
                        return;
                    }
                }
                if leaving {
                    return;
                }
            }
            Ok(None) => return,
            Err(e) => {
                // A malformed message cannot be answered with an `id` it does not have, so it is
                // reported and the loop continues: one bad message should not end the session.
                eprintln!("sysml-lsp: {e}");
            }
        }
    }
}
