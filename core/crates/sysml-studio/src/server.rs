//! A small HTTP server over one project.
//!
//! Hand-rolled on `std::net` rather than pulling in a framework: the surface is a small set of
//! routes on loopback for one user, and a dependency that brings an async runtime to serve that is
//! a dependency the rest of this workspace would have to justify forever. The parser is already a
//! large tree; the wire format here is not the place to add another.
//!
//! Requests are answered one at a time, in the order they finish arriving. That is a decision, not
//! a limitation: two concurrent edits to one model would race between "verified by reparsing" and
//! "written to disk", which is exactly the window this design exists to close.
//!
//! Reading a request is not part of that queue. Each connection is read on a thread of its own and
//! handed over once its request is complete, because a connection that sends nothing must not hold
//! up the ones that do. A browser opens connections it does not use yet, and a tab that closes
//! leaves one behind. When the read was done in the answering loop, one such connection stopped
//! the server for good: the loop waited on it, and every later request waited on the loop.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use sysml_view::{Mode, View};

use crate::document::Session;
use crate::layout::Placement;

pub const ASSET_INDEX: &str = include_str!("../assets/index.html");
pub const ASSET_CANVAS: &str = include_str!("../assets/dist/bundle.js");
pub const ASSET_STYLE: &str = include_str!("../assets/dist/bundle.css");

/// How long a connection may take to send its request, and a client to take its answer.
///
/// Loopback delivers a request in well under a millisecond, so this only expires on a connection
/// that was opened and left idle. Ten seconds keeps such a connection from holding a thread for
/// long, without cutting off a request typed by hand into a terminal.
const IO_TIMEOUT: Duration = Duration::from_secs(10);

pub fn serve(paths: Vec<PathBuf>, port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("cannot listen on 127.0.0.1:{port}: {e}"))?;
    let bound = listener
        .local_addr()
        .map_err(|e| format!("cannot read the bound address: {e}"))?;
    println!("sysml studio: http://127.0.0.1:{}", bound.port());
    for path in &paths {
        println!("model: {}", path.display());
    }
    println!("ctrl-c to stop");
    serve_on(listener, paths)
}

/// Serve one project on a listener the caller already bound.
///
/// Split from [`serve`] so a caller can bind port 0 and read the port it was given before the
/// server starts answering, which is what a test needs.
pub fn serve_on(listener: TcpListener, paths: Vec<PathBuf>) -> Result<(), String> {
    let Some(model) = paths.first().cloned() else {
        return Err("no model file given".to_string());
    };
    let state = Mutex::new(State {
        model,
        session: Session::open(paths, View::General, Mode::Extended)?,
    });

    // Connections are read on their own threads and answered here, one at a time.
    let (sender, receiver) = mpsc::channel::<(Request, TcpStream)>();
    std::thread::Builder::new()
        .name("studio-accept".to_string())
        .spawn(move || accept(listener, sender))
        .map_err(|e| format!("cannot start the accept thread: {e}"))?;

    for (request, mut stream) in receiver {
        let response = route(
            &request.method,
            &request.path,
            &request.query,
            &request.body,
            &state,
        );
        // A failure on one connection is not a reason to stop serving: a browser closing a tab
        // mid-response is normal.
        if let Err(message) = write_response(&mut stream, response) {
            eprintln!("studio: {message}");
        }
    }
    Ok(())
}

struct State {
    model: PathBuf,
    session: Session,
}

/// One request, read whole.
struct Request {
    method: String,
    path: String,
    query: String,
    body: String,
}

/// Accept connections for as long as the listener lives, reading each on a thread of its own.
fn accept(listener: TcpListener, sender: Sender<(Request, TcpStream)>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let sender = sender.clone();
        let spawned = std::thread::Builder::new()
            .name("studio-read".to_string())
            .spawn(move || match read_request(&stream) {
                Ok(Some(request)) => {
                    // The send fails only when the answering loop has gone, which is shutdown.
                    let _ = sender.send((request, stream));
                }
                // The peer connected and sent nothing. Browsers do this routinely, so it is not
                // worth a line in the log.
                Ok(None) => {}
                Err(message) => eprintln!("studio: {message}"),
            });
        if let Err(e) = spawned {
            eprintln!("studio: cannot read a connection: {e}");
        }
    }
}

/// Read one request from a connection.
///
/// `None` means the peer sent nothing: it closed the connection, or stayed silent until the read
/// timed out.
fn read_request(stream: &TcpStream) -> Result<Option<Request>, String> {
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    match reader.read_line(&mut request_line) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        // An expired read timeout is `TimedOut` on Windows and `WouldBlock` on Unix.
        Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
            return Ok(None);
        }
        Err(e) => return Err(e.to_string()),
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    // Headers are read so the body can be found by Content-Length; nothing else in them matters.
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    Ok(Some(Request {
        method,
        path,
        query,
        body,
    }))
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: String,
}

impl Response {
    fn json(body: String) -> Self {
        Self {
            status: 200,
            content_type: "application/json; charset=utf-8",
            body,
        }
    }

    fn text(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body,
        }
    }

    fn asset(content_type: &'static str, body: &str) -> Self {
        Self {
            status: 200,
            content_type,
            body: body.to_string(),
        }
    }
}

fn write_response(stream: &mut TcpStream, response: Response) -> Result<(), String> {
    // A client that stops reading must not hold the answering loop: every later request waits
    // behind this write.
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.as_bytes().len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|e| e.to_string())?;
    stream
        .write_all(response.body.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn route(method: &str, path: &str, query: &str, body: &str, state: &Mutex<State>) -> Response {
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => {
            Response::asset("text/html; charset=utf-8", ASSET_INDEX)
        }
        ("GET", "/bundle.js") => Response::asset("text/javascript; charset=utf-8", ASSET_CANVAS),
        ("GET", "/bundle.css") => Response::asset("text/css; charset=utf-8", ASSET_STYLE),
        ("GET", "/api/fingerprint") => {
            // What the one-second poll asks: has the source changed since the page drew it? The
            // answer re-reads the files and digests them, and builds nothing. The poll used to ask
            // for the whole document, which parsed, checked and laid out the model every second to
            // compare one short string.
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            if let Err(e) = guard.session.reload() {
                return Response::text(500, e);
            }
            Response::json(
                serde_json::json!({ "fingerprint": guard.session.fingerprint() }).to_string(),
            )
        }
        ("GET", "/api/document") => {
            let params = query_map(query);
            let view = match params.get("view").map(String::as_str) {
                Some("iv") => View::Interconnection,
                Some("tv") => View::Traceability,
                Some("pv") => View::Packages,
                Some("stv") => View::StateTransition,
                Some("afv") => View::ActionFlow,
                Some("sv") => View::Sequence,
                _ => View::General,
            };
            let mode = match params.get("mode").map(String::as_str) {
                Some("compat") => Mode::Compat,
                _ => Mode::Extended,
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            guard.session.view = view;
            guard.session.mode = mode;
            // `only` names a model-declared view; its exposes then decide what is drawn.
            guard.session.model_view = params.get("only").filter(|k| !k.is_empty()).cloned();
            // Re-read the files: a person may have edited the model in their own editor, and
            // showing a stale picture after an external change is the failure mode this whole
            // design is supposed to avoid.
            if let Err(e) = guard.session.reload() {
                return Response::text(500, e);
            }
            // `filter` narrows the picture to what an annotation says; it is the same spelling the
            // CLI takes, because a filter that means one thing in a picture and another in a script
            // is worse than no filter.
            match guard
                .session
                .document_with(params.get("filter").map(String::as_str))
            {
                Ok(doc) => match serde_json::to_string(&doc) {
                    Ok(text) => Response::json(text),
                    Err(e) => Response::text(500, e.to_string()),
                },
                Err(e) => Response::text(400, e),
            }
        }
        ("POST", "/api/placements") => {
            // The batch form exists for alignment: moving eight boxes into a column is one
            // arrangement change, and eight round trips would save the layout file eight times
            // and leave seven of them in a state nobody asked for if one failed.
            #[derive(Deserialize)]
            struct Item {
                key: String,
                x: f32,
                y: f32,
                w: Option<f32>,
                h: Option<f32>,
            }
            #[derive(Deserialize)]
            struct Body {
                placements: Vec<Item>,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(400, "expected {placements:[{key,x,y,w?,h?}]}".to_string());
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            guard.session.select_picture();
            for item in &parsed.placements {
                guard.session.layout.place(
                    &item.key,
                    Placement {
                        x: item.x,
                        y: item.y,
                        w: item.w,
                        h: item.h,
                    },
                );
            }
            match guard.session.layout.save(&guard.session.layout_path) {
                Ok(()) => Response::json("{\"ok\":true}".to_string()),
                Err(e) => Response::text(500, e),
            }
        }
        ("POST", "/api/placement") => {
            #[derive(Deserialize)]
            struct Body {
                key: String,
                x: f32,
                y: f32,
                w: Option<f32>,
                h: Option<f32>,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(400, "expected {key,x,y,w?,h?}".to_string());
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            guard.session.select_picture();
            guard.session.layout.place(
                &parsed.key,
                Placement {
                    x: parsed.x,
                    y: parsed.y,
                    w: parsed.w,
                    h: parsed.h,
                },
            );
            match guard.session.layout.save(&guard.session.layout_path) {
                Ok(()) => Response::json("{\"ok\":true}".to_string()),
                Err(e) => Response::text(500, e),
            }
        }
        ("POST", "/api/reset-placement") => {
            #[derive(Deserialize)]
            struct Body {
                key: String,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(400, "expected {key}".to_string());
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            guard.session.select_picture();
            guard.session.layout.forget(&parsed.key);
            match guard.session.layout.save(&guard.session.layout_path) {
                Ok(()) => Response::json("{\"ok\":true}".to_string()),
                Err(e) => Response::text(500, e),
            }
        }
        ("POST", "/api/rename") => {
            #[derive(Deserialize)]
            struct Body {
                key: String,
                name: String,
                /// The fingerprint the page was showing. Absent means "do not check", which
                /// keeps a script or a test able to call the route without a document first.
                #[serde(default)]
                fingerprint: Option<String>,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(400, "expected {key,name}".to_string());
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            // Re-read, and refuse if the file is not what the page was looking at. The re-read
            // alone would make every edit look safe: by the time the edit runs, the buffer is
            // the file. What can be stale is the picture the person clicked on.
            if let Err(e) = guard
                .session
                .reload_expecting(parsed.fingerprint.as_deref())
            {
                return Response::json(
                    serde_json::json!({ "ok": false, "stale": true, "error": e }).to_string(),
                );
            }
            let sources = guard.session.sources.clone();
            match crate::edit::rename(&sources, &parsed.key, &parsed.name) {
                Ok(outcome) => {
                    // The layout is keyed by the old qualified name; carrying it to the new one
                    // keeps the box where the person put it instead of snapping back.
                    let old = parsed.key.clone();
                    let new = outcome.touched.first().cloned().unwrap_or_default();
                    // Carry the position in every picture, not just the one on screen: the
                    // arrangement is keyed by qualified name, so a rename orphans the entry in
                    // every view the person is not looking at.
                    guard.session.layout.rename(&old, &new);
                    let _ = guard.session.layout.save(&guard.session.layout_path);
                    match guard.session.commit(outcome) {
                        Ok(touched) => Response::json(
                            serde_json::json!({ "ok": true, "touched": touched }).to_string(),
                        ),
                        Err(e) => Response::text(500, e),
                    }
                }
                Err(crate::edit::Refused(message)) => {
                    Response::json(serde_json::json!({ "ok": false, "error": message }).to_string())
                }
            }
        }
        ("POST", "/api/set-value") => {
            #[derive(Deserialize)]
            struct Body {
                key: String,
                value: String,
                #[serde(default)]
                fingerprint: Option<String>,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(400, "expected {key,value}".to_string());
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            // Re-read, and refuse if the file is not what the page was looking at. A bare
            // re-read makes every edit look safe: by the time the edit runs, the buffer *is*
            // the file. What goes stale is the picture the person clicked on.
            if let Err(e) = guard
                .session
                .reload_expecting(parsed.fingerprint.as_deref())
            {
                return Response::json(
                    serde_json::json!({ "ok": false, "stale": true, "error": e }).to_string(),
                );
            }
            let sources = guard.session.sources.clone();
            match crate::edit::set_value(&sources, &parsed.key, &parsed.value) {
                Ok(outcome) => match guard.session.commit(outcome) {
                    Ok(touched) => Response::json(
                        serde_json::json!({ "ok": true, "touched": touched }).to_string(),
                    ),
                    Err(e) => Response::text(500, e),
                },
                Err(crate::edit::Refused(message)) => {
                    Response::json(serde_json::json!({ "ok": false, "error": message }).to_string())
                }
            }
        }
        ("POST", "/api/undo") => {
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            history_reply(guard.session.undo(), "undone")
        }
        ("POST", "/api/redo") => {
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            history_reply(guard.session.redo(), "redone")
        }
        ("POST", "/api/add-member") => {
            #[derive(Deserialize)]
            struct Body {
                owner: String,
                kind: String,
                name: String,
                #[serde(default)]
                fingerprint: Option<String>,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(400, "expected {owner,kind,name}".to_string());
            };
            let kind = match parsed.kind.as_str() {
                "part" => sysml_model::Kind::PartUsage,
                "attribute" => sysml_model::Kind::AttributeUsage,
                "requirement" => sysml_model::Kind::RequirementUsage,
                "port" => sysml_model::Kind::PortUsage,
                other => {
                    return Response::text(400, format!("cannot add a `{other}`"));
                }
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            // Re-read, and refuse if the file is not what the page was looking at. See the
            // rename route for why the re-read alone cannot catch this.
            if let Err(e) = guard
                .session
                .reload_expecting(parsed.fingerprint.as_deref())
            {
                return Response::json(
                    serde_json::json!({ "ok": false, "stale": true, "error": e }).to_string(),
                );
            }
            let sources = guard.session.sources.clone();
            match crate::edit::add_member(&sources, &parsed.owner, kind, &parsed.name) {
                Ok(outcome) => match guard.session.commit(outcome) {
                    Ok(touched) => Response::json(
                        serde_json::json!({ "ok": true, "touched": touched }).to_string(),
                    ),
                    Err(e) => Response::text(500, e),
                },
                Err(crate::edit::Refused(message)) => {
                    Response::json(serde_json::json!({ "ok": false, "error": message }).to_string())
                }
            }
        }
        // Ledger verbs the canvas offers. They go through the same rules as the CLI: an
        // assumption needs a falsifier, and only a person can approve one. Reimplementing the
        // checks here rather than shelling out keeps the two in step by construction.
        ("POST", "/api/decide") => {
            #[derive(Deserialize)]
            struct Body {
                key: String,
                value: String,
                origin: String,
                #[serde(default)]
                why: String,
                #[serde(default)]
                falsifier: String,
                #[serde(default)]
                by: Option<String>,
                #[serde(default)]
                cite: Option<String>,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(
                    400,
                    "expected {key,value,origin,why?,falsifier?}".to_string(),
                );
            };
            let Some(origin) = sysml_ledger::Origin::parse(&parsed.origin) else {
                return Response::text(400, format!("unknown origin `{}`", parsed.origin));
            };
            let ledger_path = match state.lock() {
                Ok(g) => sysml_ledger::Ledger::path_for(&g.model),
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            let mut ledger = match sysml_ledger::Ledger::load(&ledger_path) {
                Ok(l) => l,
                Err(e) => return Response::text(500, e),
            };
            if origin == sysml_ledger::Origin::Assumed && parsed.falsifier.trim().is_empty() {
                return Response::json(
                    serde_json::json!({
                        "ok": false,
                        "error": "an assumption needs a falsifier: what observation would show it wrong"
                    })
                    .to_string(),
                );
            }
            if origin == sysml_ledger::Origin::Spec
                && parsed.cite.as_deref().unwrap_or("").is_empty()
            {
                return Response::json(
                    serde_json::json!({
                        "ok": false,
                        "error": "a spec value needs a registered source; add one first"
                    })
                    .to_string(),
                );
            }
            let id = ledger.next_decision_id();
            ledger.decisions.push(sysml_ledger::Decision {
                id: id.clone(),
                element: parsed.key.clone(),
                value: parsed.value.clone(),
                origin,
                why: parsed.why,
                falsifier: parsed.falsifier,
                inputs: Vec::new(),
                question: None,
                decided_by: parsed.by.unwrap_or_else(|| "human:canvas".to_string()),
                approved_by: None,
                cites: parsed.cite.into_iter().collect(),
            });
            ledger.model = ledger_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("model")
                .to_string();
            match ledger.save(&ledger_path) {
                Ok(()) => Response::json(serde_json::json!({ "ok": true, "id": id }).to_string()),
                Err(e) => Response::text(500, e),
            }
        }
        ("POST", "/api/approve") => {
            #[derive(Deserialize)]
            struct Body {
                id: String,
                by: String,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(400, "expected {id,by}".to_string());
            };
            if !parsed.by.starts_with("human:") {
                return Response::json(
                    serde_json::json!({
                        "ok": false,
                        "error": "approval must name a person as human:<name>"
                    })
                    .to_string(),
                );
            }
            let ledger_path = match state.lock() {
                Ok(g) => sysml_ledger::Ledger::path_for(&g.model),
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            let mut ledger = match sysml_ledger::Ledger::load(&ledger_path) {
                Ok(l) => l,
                Err(e) => return Response::text(500, e),
            };
            let Some(decision) = ledger.decisions.iter_mut().find(|d| d.id == parsed.id) else {
                return Response::json(
                    serde_json::json!({ "ok": false, "error": format!("no decision {}", parsed.id) })
                        .to_string(),
                );
            };
            if decision.approved_by.is_some() {
                return Response::json(
                    serde_json::json!({
                        "ok": false,
                        "error": "this one is already approved; a second approval would rewrite who accepted the risk"
                    })
                    .to_string(),
                );
            }
            decision.approved_by = Some(parsed.by);
            match ledger.save(&ledger_path) {
                Ok(()) => Response::json("{\"ok\":true}".to_string()),
                Err(e) => Response::text(500, e),
            }
        }
        ("GET", "/api/views") => {
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            if let Err(e) = guard.session.reload() {
                return Response::text(500, e);
            }
            match guard.session.views() {
                Ok(entries) => match serde_json::to_string(&entries) {
                    Ok(text) => Response::json(text),
                    Err(e) => Response::text(500, e.to_string()),
                },
                Err(e) => Response::text(400, e),
            }
        }
        ("POST", "/api/judge") => {
            #[derive(Deserialize)]
            struct Body {
                requirement: Option<String>,
            }
            // An empty body means every requirement; `{}` is a valid request.
            let parsed = if body.trim().is_empty() {
                Body { requirement: None }
            } else {
                match serde_json::from_str::<Body>(body) {
                    Ok(b) => b,
                    Err(e) => {
                        return Response::text(400, format!("expected {{requirement?}}: {e}"))
                    }
                }
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            if let Err(e) = guard.session.reload() {
                return Response::text(500, e);
            }
            match guard.session.judge(parsed.requirement.as_deref()) {
                Ok(written) => Response::json(format!("{{\"ok\":true,\"written\":{written}}}")),
                Err(e) => Response::text(400, e),
            }
        }
        ("GET", "/api/next") => {
            let params = query_map(query);
            let lang = match params.get("lang").map(String::as_str) {
                None => crate::translate::Lang::En,
                Some(code) => match crate::translate::Lang::parse(code) {
                    Some(lang) => lang,
                    None => {
                        return Response::text(
                            400,
                            format!("unknown language {code}: use en or zh"),
                        )
                    }
                },
            };
            let guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            match guard.session.next(lang) {
                Ok(plan) => Response::json(plan.to_string()),
                Err(e) => Response::text(500, e),
            }
        }
        ("GET", "/api/outline") => {
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            // Re-read first: the navigator is the panel a person trusts to list the model, so it
            // must not show the tree from before their last edit.
            if let Err(e) = guard.session.reload() {
                return Response::text(500, e);
            }
            match guard.session.outline() {
                Ok(entries) => match serde_json::to_string(&entries) {
                    Ok(text) => Response::json(text),
                    Err(e) => Response::text(500, e.to_string()),
                },
                Err(e) => Response::text(400, e),
            }
        }
        ("GET", "/api/file") => {
            let params = query_map(query);
            let want = params.get("path").cloned().unwrap_or_default();
            let guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            // The editor holds one file at a time, addressed by the path the document listed.
            // Answering by index would be cheaper but the page reconnects with a path it read
            // earlier, and an index means nothing after a reload that reordered the project.
            let sources = &guard.session.sources;
            let found = sources
                .paths
                .iter()
                .position(|p| p.display().to_string() == want);
            match found {
                Some(index) => Response::json(
                    serde_json::json!({
                        "ok": true,
                        "path": want,
                        "index": index,
                        "text": sources.texts[index],
                        "files": sources
                            .paths
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>(),
                    })
                    .to_string(),
                ),
                None => Response::json(
                    serde_json::json!({ "ok": false, "error": "no such file" }).to_string(),
                ),
            }
        }
        ("POST", "/api/file") => {
            #[derive(Deserialize)]
            struct Body {
                path: String,
                text: String,
                /// The document the page was looking at. Absent means "do not check".
                #[serde(default)]
                fingerprint: Option<String>,
            }
            let Ok(parsed) = serde_json::from_str::<Body>(body) else {
                return Response::text(400, "expected {path,text}".to_string());
            };
            let mut guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            if let Err(e) = guard
                .session
                .reload_expecting(parsed.fingerprint.as_deref())
            {
                return Response::json(
                    serde_json::json!({ "ok": false, "stale": true, "error": e }).to_string(),
                );
            }
            let sources = guard.session.sources.clone();
            let Some(index) = sources
                .paths
                .iter()
                .position(|p| p.display().to_string() == parsed.path)
            else {
                return Response::json(
                    serde_json::json!({ "ok": false, "error": "no such file" }).to_string(),
                );
            };
            match crate::edit::replace_buffer(&sources, index, parsed.text) {
                Ok(outcome) => {
                    if let Err(e) = crate::edit::commit(&outcome) {
                        return Response::json(
                            serde_json::json!({ "ok": false, "error": e }).to_string(),
                        );
                    }
                    guard.session.sources = outcome.after;
                    // The picture is now about a different model, so the page asks for it again.
                    Response::json(serde_json::json!({ "ok": true }).to_string())
                }
                Err(refused) => Response::json(
                    serde_json::json!({ "ok": false, "error": refused.0 }).to_string(),
                ),
            }
        }
        ("GET", "/api/source") => {
            let params = query_map(query);
            let want = params.get("key").cloned().unwrap_or_default();
            let guard = match state.lock() {
                Ok(g) => g,
                Err(_) => return Response::text(500, "server state is poisoned".to_string()),
            };
            match source_view(&guard.session, &want) {
                Some(view) => {
                    let (name_start, name_end) = view.name.unwrap_or((0, 0));
                    Response::json(
                        serde_json::json!({
                            "ok": true,
                            "file": view.file,
                            "path": view.path,
                            "text": view.text,
                            "line": view.line,
                            "endLine": view.end_line,
                            "nameStart": name_start,
                            "nameEnd": name_end,
                        })
                        .to_string(),
                    )
                }
                None => Response::json(
                    serde_json::json!({ "ok": false, "error": "no such element" }).to_string(),
                ),
            }
        }
        _ => Response::text(404, format!("no route for {method} {path}")),
    }
}

/// Everything a text panel needs to show one element in its file.
///
/// The old route returned 24 lines from the first buffer starting at the element's line. Three
/// things were wrong with that, and a reader notices all three: the extent was a guess rather than
/// the declaration's own, the file was always buffer 0 even in a multi-file project, and there was
/// nothing to point at inside the text.
struct SourceView {
    file: usize,
    path: String,
    /// The whole file, so the panel scrolls with real context instead of a fixed window.
    text: String,
    /// 1-based line the declaration starts on.
    line: u32,
    /// Byte range of the element's name inside `text`, when the loader kept one.
    name: Option<(usize, usize)>,
    /// 1-based line the declaration ends on.
    end_line: u32,
}

/// The reply for a history step: what moved, or that nothing did.
///
/// Nothing to move is not an error. The button was pressed, the model is where it was, and a red
/// toast would say something went wrong when nothing did. `verb` names which way the step went, so a
/// caller reads one field rather than guessing from the route it called.
fn history_reply(step: Result<Option<Vec<String>>, String>, verb: &str) -> Response {
    match step {
        Ok(Some(touched)) => Response::json(
            serde_json::json!({ "ok": true, verb: true, "touched": touched }).to_string(),
        ),
        Ok(None) => Response::json(serde_json::json!({ "ok": true, verb: false }).to_string()),
        Err(e) => Response::text(500, e),
    }
}

fn source_view(session: &Session, key: &str) -> Option<SourceView> {
    let loaded = session.sources.load().ok()?;
    fn walk<'a>(
        el: &'a sysml_model::Element,
        path: &mut Vec<String>,
        want: &str,
    ) -> Option<&'a sysml_model::Element> {
        path.push(el.name.clone());
        if path.join("::") == want {
            return Some(el);
        }
        for child in &el.children {
            if let Some(found) = walk(child, path, want) {
                return Some(found);
            }
        }
        path.pop();
        None
    }
    let mut element = None;
    for root in &loaded.roots {
        let mut path = Vec::new();
        if let Some(found) = walk(root, &mut path, key) {
            element = Some(found);
            break;
        }
    }
    let element = element?;
    // The span carries the buffer index, which is what makes a multi-file project show the right
    // file rather than whatever happened to be loaded first.
    let span = element.name_span?;
    let text = session.sources.texts.get(span.file)?.clone();
    let path = session
        .sources
        .paths
        .get(span.file)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    // The declaration's extent comes from its own subtree: a `part def` owns the lines its members
    // occupy, and a requirement's clauses are further down than its name. A fixed window is wrong
    // in both directions — too short for a container, too long for an `attribute`.
    let end_line = element
        .children
        .iter()
        .map(deepest_line)
        .max()
        .unwrap_or(element.line)
        .max(element.line);
    Some(SourceView {
        file: span.file,
        path,
        text,
        line: element.line,
        name: Some((span.start, span.end)),
        end_line,
    })
}

/// The last line this element or any descendant starts on.
fn deepest_line(el: &sysml_model::Element) -> u32 {
    el.children
        .iter()
        .map(deepest_line)
        .fold(el.line, |deepest, child| deepest.max(child))
}

fn query_map(query: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            out.insert(k.to_string(), percent_decode(v));
        }
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
