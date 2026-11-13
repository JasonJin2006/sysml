//! `send-parameters` — a send action owns at least three input parameters.
//!
//! `validateSendActionParameters`: `inputParameters()->size() >= 3`, for the payload, the sender
//! and the receiver. `inputParameters()` counts the input parameters the action owns
//! (`input->select(f | f.owner = self)`, `generated.rs:9816-9819`).
//!
//! The notation decides the count (`SendNode`, `SenderReceiverPart`, `SysML.xtext:1499-1521`). A
//! written payload, `via` and `to` each own a parameter, and an `EmptyParameterMember` owns the
//! missing one when `to` is written without `via`, or `via` or `to` without a payload. So a send
//! that writes `to` owns three, and one that does not owns at most two, plus any input parameter
//! its body declares.
//!
//! ```text
//! send s to q;          // payload, empty sender, receiver: three
//! send s via p to q;    // three
//! send to q;            // empty payload, empty sender, receiver: three
//! send s via p;         // rejected: two
//! send s;               // rejected: one
//! ```
//!
//! The reference implementation does not check this count. It reports a send with no payload
//! (`validateSendActionUsagePayloadArgument`) and warns on a `to` that names a port. No official
//! model writes a send without `to`.
//!
//! The report code is `SEM123`.

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

use super::Violation;
use crate::derived;

const CODE: &str = "SEM123";

pub fn issues(roots: &[Element], index: &Index) -> Vec<Violation> {
    let mut out = Vec::new();
    super::walk(roots, |el| {
        if el.kind != Kind::SendActionUsage {
            return;
        }
        let written = if el.receiver.is_some() {
            3
        } else if el.sender.is_some() {
            2
        } else if el.has_payload {
            1
        } else {
            0
        };
        let owned = written + derived::input_parameters(index, el.id).len();
        if owned < 3 {
            out.push(Violation::new(
                CODE,
                el,
                index.qualified[el.id].clone(),
                format!(
                    "owns {owned} of the three input parameters a send action needs (payload, \
                     sender, receiver); name the receiver with `to`"
                ),
            ));
        }
    });
    out
}
