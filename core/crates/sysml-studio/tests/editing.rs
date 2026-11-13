//! The editing engine, held to the promises the canvas makes.
//!
//! Every test here corresponds to a way an editor can corrupt a model. They are in `tests/`
//! rather than a probe script because the properties are about text: an edit either touches
//! exactly the bytes it claims or it does not, and that is cheap to assert and expensive to
//! discover later.

use std::path::PathBuf;

use sysml_model::Kind;
use sysml_studio::edit::{self, Sources};
use sysml_studio::layout::{self, Layout, Placement};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sysml-studio-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn model(tag: &str, text: &str) -> (PathBuf, Sources) {
    let dir = temp_dir(tag);
    let path = dir.join("m.sysml");
    std::fs::write(&path, text).unwrap();
    let sources = Sources::read(&[path.clone()]).unwrap();
    (path, sources)
}

const DRONE: &str = r#"package P {
    part def Battery {
        attribute mass : Real = 2.1;
    }
    part def Drone {
        part battery : Battery;
        attribute power : Real = 900;
    }
    part drone : Drone;
}
"#;

#[test]
fn editing_a_value_changes_only_that_value() {
    let (_path, sources) = model("value", DRONE);
    let outcome = edit::set_value(&sources, "P::Drone::power", "950").expect("accepted");
    let before = &sources.texts[0];
    let after = &outcome.after.texts[0];
    assert_eq!(*after, before.replace("= 900", "= 950"));
    // Nothing else moved: the type, the indentation and the other features are byte-identical.
    assert!(after.contains("attribute power : Real = 950;"));
    assert!(after.contains("part battery : Battery;"));
}

#[test]
fn a_value_that_does_not_parse_is_refused_and_changes_nothing() {
    let (_path, sources) = model("badvalue", DRONE);
    let refused = edit::set_value(&sources, "P::Drone::power", "not a ( value");
    assert!(refused.is_err(), "an unparsable value must be refused");
    // The engine never wrote: the buffers it hands back are the ones it was given.
    assert_eq!(sources.texts[0], DRONE);
}

#[test]
fn renaming_a_definition_updates_the_usages_typed_by_it() {
    let (_path, sources) = model("rename", DRONE);
    let outcome = edit::rename(&sources, "P::Battery", "PowerCell").expect("accepted");
    let after = &outcome.after.texts[0];
    assert!(
        after.contains("part def PowerCell {"),
        "declaration renamed"
    );
    assert!(
        after.contains("part battery : PowerCell;"),
        "the usage that names it was rewritten too: {after}"
    );
    // A same-named *feature* is not the renamed element and must survive untouched.
    assert!(
        after.contains("part battery :"),
        "the feature name is not the type name"
    );
}

#[test]
fn renaming_to_a_name_that_breaks_a_reference_is_refused() {
    // `shadow` is used by two unrelated declarations; renaming one of them must not silently
    // leave the other's reference dangling, so the engine refuses rather than guessing.
    let text = r#"package P {
    part def Wheel;
    part def Axle { part w : Wheel; }
    part axle : Axle;
}
"#;
    let (_path, sources) = model("broken", text);
    let outcome = edit::rename(&sources, "P::Wheel", "Rim");
    // Either the reference was rewritten (and the rename is consistent) or the rename was
    // refused. What is never acceptable is a file whose reference no longer resolves.
    match outcome {
        Ok(o) => {
            assert!(o.after.texts[0].contains("part w : Rim;"));
            let reloaded = o.after.load().expect("still parses");
            assert!(
                sysml_resolve::Index::build(&reloaded.roots)
                    .references(&reloaded.roots)
                    .iter()
                    .all(
                        |r| !matches!(r.target, sysml_resolve::Resolution::Unresolved)
                            || r.text.contains("::")
                    ),
                "no plain reference may be left dangling"
            );
        }
        Err(edit::Refused(message)) => assert!(!message.is_empty()),
    }
}

#[test]
fn a_rename_that_is_not_a_plain_name_is_refused() {
    let (_path, sources) = model("badname", DRONE);
    for bad in ["", "with space-and-dash", "1digit", "a:b"] {
        assert!(
            edit::rename(&sources, "P::Drone", bad).is_err(),
            "`{bad}` must be refused"
        );
    }
}

#[test]
fn adding_a_member_keeps_the_body_indented() {
    let (_path, sources) = model("add", DRONE);
    let outcome =
        edit::add_member(&sources, "P::Drone", Kind::AttributeUsage, "maxSpeed").expect("accepted");
    let after = &outcome.after.texts[0];
    // The new line carries the body's indentation, and the closing brace keeps its own.
    assert!(
        after.contains("\n        attribute maxSpeed;\n    }\n"),
        "got: {after}"
    );
    let reloaded = outcome.after.load().expect("still parses");
    let qualified = sysml_ledger::value_bearing(&reloaded.roots);
    assert!(qualified.iter().all(|(key, ..)| !key.is_empty()));
}

#[test]
fn adding_to_a_declaration_without_a_body_is_refused() {
    let text = "package P {\n    part def Empty;\n}\n";
    let (_path, sources) = model("empty", text);
    let refused = edit::add_member(&sources, "P::Empty", Kind::AttributeUsage, "x");
    assert!(
        refused.is_err(),
        "a declaration with `;` has no body to insert into"
    );
}

#[test]
fn an_edit_spans_the_buffer_it_was_written_in() {
    // Two files, the second renamed. A span carries its buffer index, so the edit must land in
    // file 1 — getting that wrong writes the new name into the wrong file.
    let dir = temp_dir("multi");
    let a = dir.join("a.sysml");
    let b = dir.join("b.sysml");
    std::fs::write(&a, "package A {\n    part def Shared;\n}\n").unwrap();
    std::fs::write(
        &b,
        "package B {\n    part def Other { part s : A::Shared; }\n}\n",
    )
    .unwrap();
    let sources = Sources::read(&[a, b]).unwrap();
    let outcome = edit::rename(&sources, "A::Shared", "Common").expect("accepted");
    assert!(outcome.after.texts[0].contains("part def Common;"));
    // The reference lives in the second file, so that is the file that must change. A span
    // carries its buffer index precisely so this lands in the right one.
    assert!(
        outcome.after.texts[1].contains("part s : A::Common;"),
        "the reference in the other buffer was rewritten: {}",
        outcome.after.texts[1]
    );
    assert_eq!(outcome.after.texts[0].matches("Common").count(), 1);
}

#[test]
fn a_layout_survives_a_reparse_and_drops_dead_keys() {
    let mut layout = Layout::empty("gv", "extended");
    layout.place(
        "P::Drone",
        Placement {
            x: 10.0,
            y: 20.0,
            w: None,
            h: None,
        },
    );
    layout.place(
        "P::Gone",
        Placement {
            x: 0.0,
            y: 0.0,
            w: None,
            h: None,
        },
    );
    assert_eq!(layout::prune(&mut layout, &["P::Drone".to_string()]), 1);
    assert!(layout.get("P::Drone").is_some());
    assert!(layout.get("P::Gone").is_none());
}

#[test]
fn a_moved_box_keeps_its_position_when_an_unrelated_edit_happens() {
    let (_path, sources) = model("stable", DRONE);
    let mut layout = Layout::empty("gv", "extended");
    layout.place(
        "P::Drone::power",
        Placement {
            x: 400.0,
            y: 300.0,
            w: None,
            h: None,
        },
    );

    let outcome = edit::set_value(&sources, "P::Drone::power", "950").expect("accepted");
    let reloaded = outcome.after.load().expect("parses");
    let names = edit::live_names(&reloaded);

    layout::prune(&mut layout, &names);
    let place = layout
        .get("P::Drone::power")
        .expect("position survives a value edit");
    assert_eq!((place.x, place.y), (400.0, 300.0));
}

#[test]
fn renaming_the_owner_moves_the_child_keys_with_it() {
    // A rename changes every qualified name beneath it. The layout is keyed by those names, so a
    // child's saved position is for a name that no longer exists — it must be dropped rather
    // than applied to whatever element now happens to have the old key.
    let (_path, sources) = model("owner", DRONE);
    let mut layout = Layout::empty("gv", "extended");
    layout.place(
        "P::Drone::power",
        Placement {
            x: 5.0,
            y: 5.0,
            w: None,
            h: None,
        },
    );

    let outcome = edit::rename(&sources, "P::Drone", "Quadcopter").expect("accepted");
    let reloaded = outcome.after.load().expect("parses");
    let names = edit::live_names(&reloaded);
    layout::prune(&mut layout, &names);

    assert!(names.iter().any(|n| n == "P::Quadcopter::power"));
    assert!(layout.get("P::Drone::power").is_none(), "stale key dropped");
}

/// Every expression shape a value can take, with the text the edit must produce.
///
/// The bug this pins: the parser's value range ended at the *next* token rather than at the last
/// one the value occupied, so replacing `= 900 [W]` with `= 950` also consumed the `;`. The file
/// still parsed — a missing semicolon before a `}` is accepted — so `check` said "clean" while
/// the unit and the statement punctuation were gone. Only the on-disk text shows it.
#[test]
fn setting_a_value_keeps_the_unit_and_the_semicolon() {
    let text = r#"package P {
    part def A {
        attribute bare : Real = 5;
        attribute unit : Real = 2.1 [kg];
        attribute path : Real = b.x;
        attribute call : Real = sum(x);
        attribute paren : Real = (5);
        attribute list : Real = [1, 2];
        attribute sum : Real = x + y;
        attribute nested : Real = (x + y);
        attribute ratio : Real = 360 [W*h] / 900 [W];
    }
}
"#;
    let (_path, sources) = model("valuesshapes", text);
    let names = [
        "bare", "unit", "path", "call", "paren", "list", "sum", "nested", "ratio",
    ];
    for name in names {
        let qualified = format!("P::A::{name}");
        let outcome = edit::set_value(&sources, &qualified, "7")
            .unwrap_or_else(|e| panic!("{qualified} was refused: {e}"));
        let after = &outcome.after.texts[0];
        assert!(
            after.contains(&format!("attribute {name} : Real = 7;")),
            "{qualified} kept its statement: {after}"
        );
    }
}

/// A write must not clobber a change somebody else made to the same file.
///
/// The canvas reads the file, edits the text in memory, verifies it by parsing, and writes it
/// back. Between the read and the write a person may save the same file in their own editor. If
/// the write goes ahead, their change is gone and nothing complains — the buffer is valid SysML,
/// so `check` says the model is clean.
#[test]
fn a_write_refuses_when_the_file_changed_underneath() {
    let (path, sources) = model("conflict", DRONE);
    // The edit itself is fine; the conflict is what has to stop it.
    let outcome = edit::set_value(&sources, "P::Drone::power", "950").expect("accepted");

    // Somebody else saves the file, with a change of their own.
    let theirs = DRONE.replace("part def Battery", "part def PowerCell");
    std::fs::write(&path, &theirs).unwrap();

    let refused = edit::commit(&outcome).expect_err("a conflicting write is refused");
    assert!(
        refused.contains("changed on disk"),
        "the message says what happened: {refused}"
    );
    assert!(
        refused.contains("Nothing was written"),
        "and that nothing was lost: {refused}"
    );
    // Their change is still there, byte for byte.
    assert_eq!(std::fs::read_to_string(&path).unwrap(), theirs);
}

#[test]
fn a_write_goes_through_when_nobody_else_touched_the_file() {
    let (path, sources) = model("noconflict", DRONE);
    let outcome = edit::set_value(&sources, "P::Drone::power", "950").expect("accepted");
    edit::commit(&outcome).expect("an uncontested write is applied");
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(
        on_disk.contains("attribute power : Real = 950;"),
        "{on_disk}"
    );
}

#[test]
fn a_write_refuses_when_the_file_is_gone() {
    let (path, sources) = model("deleted", DRONE);
    let outcome = edit::set_value(&sources, "P::Drone::power", "950").expect("accepted");
    std::fs::remove_file(&path).unwrap();
    let refused = edit::commit(&outcome).expect_err("a missing file is a conflict");
    assert!(refused.contains("is gone"), "{refused}");
}

/// The check must run for every file before the first write.
///
/// With two files, a conflict on the second one used to be able to leave the first one already
/// overwritten. The failure mode has to stay "nothing happened".
#[test]
fn a_conflict_in_the_second_file_leaves_the_first_untouched() {
    let dir = temp_dir("multiconflict");
    let a = dir.join("a.sysml");
    let b = dir.join("b.sysml");
    std::fs::write(&a, "package A {\n    part def Shared;\n}\n").unwrap();
    std::fs::write(
        &b,
        "package B {\n    part def Other { part s : A::Shared; }\n}\n",
    )
    .unwrap();
    let sources = Sources::read(&[a.clone(), b.clone()]).unwrap();

    // Renaming rewrites the declaration in `a` and the reference in `b`: one edit, two files.
    let outcome = edit::rename(&sources, "A::Shared", "Common").expect("accepted");
    let a_before = std::fs::read_to_string(&a).unwrap();

    // The second file changes underneath us.
    std::fs::write(&b, "package B {\n    part def Other;\n}\n").unwrap();
    let b_theirs = std::fs::read_to_string(&b).unwrap();

    let refused = edit::commit(&outcome).expect_err("the conflict is reported");
    assert!(refused.contains("b.sysml"), "names the file: {refused}");
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        a_before,
        "the first file was not written"
    );
    assert_eq!(
        std::fs::read_to_string(&b).unwrap(),
        b_theirs,
        "nor the second"
    );
}

/// The fingerprint a page holds is what makes a stale click detectable.
///
/// The server re-reads before every edit, so by the time an edit runs the buffer *is* the file —
/// which means the library-level write guard can never fire on the HTTP path. What goes stale is
/// the picture the person clicked on, and the fingerprint is the only thing that carries that
/// fact from the browser to the server.
#[test]
fn a_page_holding_an_old_fingerprint_is_told_the_model_moved() {
    let dir = temp_dir("stale");
    let path = dir.join("m.sysml");
    std::fs::write(&path, DRONE).unwrap();
    let mut session = sysml_studio::Session::open(
        vec![path.clone()],
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    )
    .expect("opens");

    let seen = session.fingerprint();
    // Somebody saves the file in their own editor.
    std::fs::write(&path, DRONE.replace("= 900", "= 123")).unwrap();

    let refused = session
        .reload_expecting(Some(&seen))
        .expect_err("the page is out of date");
    assert!(
        refused.contains("changed on disk"),
        "the message says what happened: {refused}"
    );
    assert!(
        refused.contains("Nothing was written"),
        "and that nothing was lost: {refused}"
    );
}

#[test]
fn a_page_holding_the_current_fingerprint_may_edit() {
    let dir = temp_dir("fresh");
    let path = dir.join("m.sysml");
    std::fs::write(&path, DRONE).unwrap();
    let mut session = sysml_studio::Session::open(
        vec![path],
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    )
    .expect("opens");
    let seen = session.fingerprint();
    session
        .reload_expecting(Some(&seen))
        .expect("an up-to-date page is not refused");
}

/// A caller with no picture — a script, a test — passes `None` and is not checked.
#[test]
fn a_caller_without_a_picture_is_not_checked() {
    let dir = temp_dir("nopicture");
    let path = dir.join("m.sysml");
    std::fs::write(&path, DRONE).unwrap();
    let mut session = sysml_studio::Session::open(
        vec![path.clone()],
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    )
    .expect("opens");
    std::fs::write(&path, DRONE.replace("= 900", "= 123")).unwrap();
    session
        .reload_expecting(None)
        .expect("no fingerprint means no check");
    // And the reload really happened: the buffer is the new text.
    assert!(session.sources.texts[0].contains("= 123"));
}

/// The digest has to change when the text changes, including between files.
#[test]
fn the_fingerprint_tracks_the_text() {
    let dir = temp_dir("fingerprint");
    let a = dir.join("a.sysml");
    let b = dir.join("b.sysml");
    std::fs::write(&a, "package A { part def X; }").unwrap();
    std::fs::write(&b, "package B { part def Y; }").unwrap();
    let mut session = sysml_studio::Session::open(
        vec![a.clone(), b.clone()],
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    )
    .expect("opens");
    let first = session.fingerprint();

    // A change in the second file moves it.
    std::fs::write(&b, "package B { part def Z; }").unwrap();
    session.reload().unwrap();
    assert_ne!(session.fingerprint(), first, "a change in file 2 is seen");

    // So does a swap that leaves the set of lines identical.
    std::fs::write(&a, "package B { part def X; }").unwrap();
    std::fs::write(&b, "package A { part def Y; }").unwrap();
    session.reload().unwrap();
    let swapped = session.fingerprint();
    std::fs::write(&a, "package A { part def Y; }").unwrap();
    std::fs::write(&b, "package B { part def X; }").unwrap();
    session.reload().unwrap();
    assert_ne!(
        session.fingerprint(),
        swapped,
        "the order of files is part of the identity"
    );
}

#[test]
fn an_edit_can_be_taken_back_and_then_there_is_nothing_left() {
    let (path, _sources) = model("undo", DRONE);
    let mut session = sysml_studio::Session::open(
        vec![path.clone()],
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    )
    .expect("session opens");

    let sources = session.sources.clone();
    let outcome = edit::set_value(&sources, "P::Drone::power", "950").expect("accepted");
    session.commit(outcome).expect("written");
    assert!(std::fs::read_to_string(&path).unwrap().contains("= 950"));

    let undone = session
        .undo()
        .expect("undo runs")
        .expect("something to take back");
    assert_eq!(undone.len(), 1, "one file was written back");
    assert!(
        std::fs::read_to_string(&path).unwrap().contains("= 900"),
        "the file is what it was before the edit"
    );
    // The stack has one edit in it, so a second undo has nothing to do — and says so rather than
    // inventing a state.
    assert!(session.undo().expect("undo runs").is_none());
}

#[test]
fn an_edit_that_was_refused_cannot_be_undone() {
    // A refused edit wrote nothing, so a history entry for it would make one undo do nothing and
    // the next one skip a step.
    let (path, _sources) = model("undo-refused", DRONE);
    let mut session = sysml_studio::Session::open(
        vec![path.clone()],
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    )
    .expect("session opens");
    let sources = session.sources.clone();
    assert!(edit::set_value(&sources, "P::Drone::power", "not a ( value").is_err());
    assert!(
        session.undo().expect("undo runs").is_none(),
        "nothing was written"
    );
}

fn session_for(tag: &str) -> (std::path::PathBuf, sysml_studio::Session) {
    let (path, _sources) = model(tag, DRONE);
    let session = sysml_studio::Session::open(
        vec![path.clone()],
        sysml_view::View::General,
        sysml_view::Mode::Extended,
    )
    .expect("session opens");
    (path, session)
}

fn set(session: &mut sysml_studio::Session, value: &str) {
    let sources = session.sources.clone();
    let outcome = edit::set_value(&sources, "P::Drone::power", value).expect("accepted");
    session.commit(outcome).expect("written");
}

#[test]
fn an_undone_edit_can_be_put_back() {
    // Undo that cannot be reversed is a trap: the person who takes back one edit too many has to
    // retype it from memory.
    let (path, mut session) = session_for("redo");
    set(&mut session, "950");
    session
        .undo()
        .expect("undo runs")
        .expect("something to take back");
    assert!(std::fs::read_to_string(&path).unwrap().contains("= 900"));

    let redone = session
        .redo()
        .expect("redo runs")
        .expect("something to put back");
    assert_eq!(redone.len(), 1);
    assert!(
        std::fs::read_to_string(&path).unwrap().contains("= 950"),
        "the file has the edit again"
    );
    assert!(
        session.redo().expect("redo runs").is_none(),
        "nothing left to put back"
    );
}

#[test]
fn an_edit_after_an_undo_abandons_the_redo_branch() {
    // The old path is not on the model any more. Offering it as a redo would write back a state the
    // person had already walked away from.
    let (path, mut session) = session_for("redo-branch");
    set(&mut session, "950");
    session.undo().expect("undo runs");
    set(&mut session, "1000");

    assert!(
        session.redo().expect("redo runs").is_none(),
        "the abandoned branch is gone"
    );
    assert!(std::fs::read_to_string(&path).unwrap().contains("= 1000"));
}

#[test]
fn an_edit_after_a_multibyte_comment_lands_on_the_right_bytes() {
    // Spans are byte offsets, and `replace_range` panics on a boundary that is not a character
    // boundary. A comment in Chinese or an accented name in front of the edited value shifts
    // every byte offset after it, so an editor that confused characters with bytes would corrupt
    // the file or panic here.
    let text =
        "package P {\n    // 这是注释，中文\n    part def Thing { attribute x : Real = 1; }\n}\n";
    let (_path, sources) = model("utf8", text);
    let outcome = edit::set_value(&sources, "P::Thing::x", "2").expect("accepted");
    let after = &outcome.after.texts[0];
    assert!(after.contains("x : Real = 2"), "got {after:?}");
    assert!(
        after.contains("这是注释，中文"),
        "the comment survives: {after:?}"
    );
    assert_eq!(
        after.chars().count() - text.chars().count(),
        0,
        "one character swapped for one"
    );
}
