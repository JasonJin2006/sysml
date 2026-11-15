//! Re-indenting a model without losing anything the author wrote.
//!
//! The formatter is judged on two things at once, and either one alone is a failure: the layout has
//! to come out consistent, and everything that is not layout — comments, blank-line grouping, the
//! spelling of a number, the line a `doc` sits on — has to survive unchanged. A formatter that got
//! the second wrong would be a formatter nobody runs twice.

use sysml_syntax::format::format;

#[test]
fn a_body_is_indented_one_level() {
    let src = "package P {\npart def A {\nattribute x : Real;\n}\n}\n";
    let want = "package P {\n    part def A {\n        attribute x : Real;\n    }\n}\n";
    assert_eq!(format(src), want);
}

#[test]
fn an_already_formatted_file_is_left_alone() {
    // Running a formatter twice has to be the same as running it once, or every save produces a
    // diff and the formatter becomes the thing people turn off.
    let src = "package P {\n    part def A {\n        attribute x : Real;\n    }\n}\n";
    assert_eq!(format(src), src);
}

#[test]
fn over_indented_input_is_pulled_back() {
    let src = "package P {\n            part def A;\n}\n";
    assert_eq!(format(src), "package P {\n    part def A;\n}\n");
}

#[test]
fn a_closing_brace_lines_up_with_what_it_closed() {
    let src = "package P {\n    part def A {\n        attribute x;\n        }\n}\n";
    assert_eq!(
        format(src),
        "package P {\n    part def A {\n        attribute x;\n    }\n}\n"
    );
}

#[test]
fn a_line_comment_is_kept_and_indented() {
    let src = "package P {\n// a note\npart def A;\n}\n";
    let after = format(src);
    assert!(after.contains("// a note"), "{after}");
    assert!(after.contains("    // a note"), "{after}");
}

#[test]
fn a_block_comment_between_lines_survives() {
    let src = "package P {\n/* why this exists */\npart def A;\n}\n";
    let after = format(src);
    assert!(after.contains("/* why this exists */"), "{after}");
}

#[test]
fn a_multi_line_doc_keeps_its_own_layout() {
    // A `doc` written across lines is prose. Its inner indentation is how the author laid the
    // paragraph out, and moving it would rewrap text that was written to be read.
    let src = "package P {\n    part def A {\n        doc /*\n         * First line\n         *     and an indented continuation\n         */\n    }\n}\n";
    let after = format(src);
    assert!(
        after.contains(" *     and an indented continuation"),
        "the prose keeps its own indentation: {after}"
    );
}

#[test]
fn braces_inside_a_string_are_not_counted() {
    // A string is data. Counting its braces would open a block that never closes, and every line
    // after it would be indented one level too deep.
    let src = "package P {\n    part def A;\n    attribute note : String = \"a { brace\";\n}\n";
    assert_eq!(format(src), src);
}

#[test]
fn braces_inside_a_comment_are_not_counted() {
    let src = "package P {\n    // this { is not a block\n    part def A;\n}\n";
    assert_eq!(format(src), src);
}

#[test]
fn a_name_in_quotes_is_one_token() {
    // `'vehicle-trailer system'` holds no braces; a quoted name with an apostrophe inside must not
    // end early and turn the rest of the line into code.
    let src = "package P {\n    part def 'it\\'s a name';\n}\n";
    assert_eq!(format(src), src);
}

#[test]
fn a_run_of_blank_lines_becomes_one() {
    let src = "package P {\n    part def A;\n\n\n\n    part def B;\n}\n";
    assert_eq!(
        format(src),
        "package P {\n    part def A;\n\n    part def B;\n}\n"
    );
}

#[test]
fn a_blank_line_just_inside_a_body_goes_away() {
    // A row that holds nothing is a row the reader has to look at and dismiss.
    let src = "package P {\n\n    part def A;\n\n}\n";
    assert_eq!(format(src), "package P {\n    part def A;\n}\n");
}

#[test]
fn trailing_whitespace_goes_away() {
    let src = "package P {   \n    part def A;  \n}\n";
    assert_eq!(format(src), "package P {\n    part def A;\n}\n");
}

#[test]
fn exactly_one_newline_ends_the_file() {
    let src = "package P {\n    part def A;\n}";
    assert_eq!(format(src), "package P {\n    part def A;\n}\n");
    let src = "package P {\n    part def A;\n}\n\n\n";
    assert_eq!(format(src), "package P {\n    part def A;\n}\n");
}

#[test]
fn an_unbalanced_brace_leaves_the_file_alone() {
    // Guessing at a buffer mid-edit would move code into the wrong block, and the file would still
    // parse — the one outcome worse than leaving it alone.
    let src = "package P {\n    part def A {\n        attribute x;\n    }\n";
    assert_eq!(format(src), src);
}

#[test]
fn an_unterminated_comment_leaves_the_file_alone() {
    let src = "package P {\n    doc /* never closed\n}\n";
    assert_eq!(format(src), src);
}

#[test]
fn an_empty_file_stays_empty() {
    assert_eq!(format(""), "");
    assert_eq!(format("\n\n"), "\n");
}

#[test]
fn a_one_line_body_is_kept_on_one_line() {
    // The formatter does not split declarations. A member written on one line was written that
    // way on purpose, and expanding it would turn every save into a reflow nobody asked for.
    let src = "package P { part def A; }\n";
    assert_eq!(format(src), src);
}

#[test]
fn the_repository_examples_keep_their_comments() {
    // The real files are the test that matters: every one of them already carries comments, and a
    // formatter that dropped one would be caught here rather than in somebody's diff.
    for path in [
        "../examples/drone.sysml",
        "../examples/rover.sysml",
        "../examples/fuel.sysml",
    ] {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        let after = format(&src);
        let comments_before = src.matches("//").count() + src.matches("/*").count();
        let comments_after = after.matches("//").count() + after.matches("/*").count();
        assert_eq!(
            comments_before, comments_after,
            "{path} lost a comment:\n{after}"
        );
        assert_eq!(
            format(&after),
            after,
            "{path} is not stable under formatting"
        );
    }
}
