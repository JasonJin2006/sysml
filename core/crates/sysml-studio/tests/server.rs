//! The server as a browser meets it: over a socket.
//!
//! Two things are pinned here that no in-process test can see. A connection that sends nothing must
//! not stop the server from answering the next one, and the route the page polls must answer with
//! the digest a document carries.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

const MODEL: &str = "package P {\n    private import ScalarValues::*;\n    part def Thing { attribute x : Real = 1; }\n}\n";

/// A model in a directory of its own, and a server on a port the system chose.
fn serve(tag: &str) -> (PathBuf, u16) {
    let dir = std::env::temp_dir().join(format!("sysml-server-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("m.sysml");
    std::fs::write(&path, MODEL).expect("write model");
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("address").port();
    let model = path.clone();
    // The server runs until the process ends, and the test process ending is what stops it.
    std::thread::spawn(move || {
        let _ = sysml_studio::serve_on(listener, vec![model]);
    });
    (path, port)
}

/// One request over a fresh connection: the status line and the body.
fn get(port: u16, target: &str) -> (String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    // A server that never answers must fail the test, not hang it.
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("read timeout");
    write!(stream, "GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").expect("send");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the server answers and closes the connection");
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    (
        head.lines().next().unwrap_or("").to_string(),
        body.to_string(),
    )
}

fn fingerprint(port: u16) -> String {
    let (status, body) = get(port, "/api/fingerprint");
    assert!(status.contains("200"), "{status}");
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
    parsed["fingerprint"]
        .as_str()
        .expect("a fingerprint")
        .to_string()
}

#[test]
fn a_connection_that_sends_nothing_does_not_block_the_next_request() {
    let (_path, port) = serve("idle");
    // A browser opens connections it does not use yet, and a closed tab leaves one behind. The
    // server used to read each connection to its end before it accepted the next, so one silent
    // connection stopped it for good.
    let idle = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let (status, _) = get(port, "/api/fingerprint");
    assert!(
        status.contains("200"),
        "the request behind a silent connection was not answered: {status}"
    );
    drop(idle);
}

#[test]
fn the_fingerprint_route_answers_the_digest_a_document_carries() {
    let (path, port) = serve("fingerprint");
    let polled = fingerprint(port);
    let (status, body) = get(port, "/api/document?view=gv&mode=extended");
    assert!(status.contains("200"), "{status}");
    let document: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(
        document["fingerprint"].as_str(),
        Some(polled.as_str()),
        "the page compares the two, so they must be the same digest"
    );

    // An edit made in another editor is what the poll exists to see.
    std::fs::write(&path, MODEL.replace("= 1;", "= 2;")).expect("rewrite the model");
    assert_ne!(
        fingerprint(port),
        polled,
        "a changed file must change the fingerprint"
    );
}

#[test]
fn requests_sent_at_once_are_all_answered() {
    // A page that opens asks for the document, the ladder, the outline and the views together, on
    // a connection each.
    let (_path, port) = serve("burst");
    let handles: Vec<_> = [
        "/api/document?view=gv&mode=extended",
        "/api/next",
        "/api/outline",
        "/api/views",
        "/api/fingerprint",
    ]
    .into_iter()
    .map(|target| std::thread::spawn(move || (target, get(port, target).0)))
    .collect();
    for handle in handles {
        let (target, status) = handle.join().expect("the request thread");
        assert!(status.contains("200"), "{target}: {status}");
    }
}

/// One POST over a fresh connection: the status line and the body.
fn post(port: u16, target: &str, body: &str) -> (String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("read timeout");
    write!(
        stream,
        "POST {target} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("send");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the server answers and closes the connection");
    let (head, rest) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    (
        head.lines().next().unwrap_or("").to_string(),
        rest.to_string(),
    )
}

#[test]
fn a_decide_body_that_is_not_json_is_a_400() {
    let (_model, port) = serve("decide-bad-json");
    let (status, _body) = post(port, "/api/decide", "not json");
    assert!(status.contains("400"), "{status}");
}

#[test]
fn a_decide_body_with_an_unknown_origin_is_a_400() {
    let (_model, port) = serve("decide-bad-origin");
    let (status, body) = post(
        port,
        "/api/decide",
        r#"{"key":"P::Thing::x","value":"1","origin":"invented"}"#,
    );
    assert!(status.contains("400"), "{status} {body}");
}

#[test]
fn an_assumption_with_no_falsifier_is_refused() {
    // The same rule the command line enforces. Without it the canvas would be a second way to
    // write an assumption nothing could ever falsify.
    let (_model, port) = serve("decide-no-falsifier");
    let (status, body) = post(
        port,
        "/api/decide",
        r#"{"key":"P::Thing::x","value":"2","origin":"assumed","why":"because"}"#,
    );
    assert!(status.contains("200"), "{status} {body}");
    assert!(body.contains("\"ok\":false"), "{body}");
    assert!(body.contains("falsifier"), "{body}");
}

#[test]
fn an_approval_that_is_not_a_person_is_refused() {
    let (_model, port) = serve("approve-agent");
    let (status, body) = post(port, "/api/approve", r#"{"id":"D001","by":"agent"}"#);
    assert!(status.contains("200"), "{status} {body}");
    assert!(body.contains("\"ok\":false"), "{body}");
    assert!(body.contains("human:"), "{body}");
}
