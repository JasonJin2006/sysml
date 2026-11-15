//! What to offer when someone starts typing.
//!
//! Three kinds of answer, because a SysML file is written in three registers and the caret does not
//! say which one it is in:
//!
//! * **A keyword.** `par` at the start of a line is `part`. The list is the lexer's own, so a word
//!   is offered exactly when the grammar would accept it as a keyword.
//! * **An element in scope.** After `: ` the answer is a type; after a `.` it is a member of what
//!   precedes it. The resolver already knows both, so this asks it rather than re-deriving scopes.
//! * **A package after `import`.** The one place Syside adds something a plain list cannot: the
//!   package name, quoted when the name needs it.
//!
//! Everything here is a suggestion. Nothing is inserted that a person did not ask for, and a
//! completion that turns out to be wrong costs one keystroke — which is why the list leans towards
//! offering more and ordering well rather than filtering hard.

use std::collections::BTreeSet;

use sysml_model::{Element, Kind};
use sysml_resolve::Index;

/// One thing the editor may insert.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// What is inserted.
    pub insert: String,
    /// What the list shows, when that differs from what is inserted.
    pub label: String,
    /// The protocol's completion item kind, so the client picks an icon.
    pub kind: u8,
    /// A line under the item: the qualified name, or what kind of element it is.
    pub detail: String,
    /// Sorts earlier than anything with a lower value. The protocol orders by this.
    pub rank: String,
}

/// Protocol completion kinds (`CompletionItemKind`).
mod item {
    pub const KEYWORD: u8 = 14;
    pub const CLASS: u8 = 7;
    pub const FIELD: u8 = 5;
    pub const MODULE: u8 = 9;
}

/// Everything the completion handler needs to know about the caret.
pub struct Context<'a> {
    /// The buffer, as the client holds it.
    pub text: &'a str,
    /// Byte offset of the caret.
    pub offset: usize,
    /// The element the caret is inside, if any.
    pub inside: Option<&'a Element>,
    /// The project's element index.
    pub index: &'a Index,
    /// Every root of the project, for the walks that need the tree rather than the index.
    pub roots: &'a [Element],
}

/// What a caret at this position is in the middle of.
enum Place<'a> {
    /// Typing a dotted path: complete the members of an element.
    Member(&'a str),
    /// After `import` or `expose`: complete a package path.
    Path,
    /// After `:` or `:>`: complete a type.
    Type,
    /// Anywhere else: a declaration name or a keyword.
    Free,
}

/// The completions for one position.
pub fn complete(context: &Context) -> Vec<Suggestion> {
    let before = &context.text[..context.offset.min(context.text.len())];
    let place = classify(before);
    let mut out = match place {
        Place::Member(path) => members_of(context, path),
        Place::Path => paths(context),
        Place::Type => types_in_scope(context),
        Place::Free => {
            let mut items = keywords();
            items.extend(declarations(context));
            items
        }
    };
    // The last word before the caret is what the client filters on, so a prefix that does not
    // match is not filtered out again here: the client already knows the word, and repeating the
    // filter would hide a completion it would have matched by fuzzy search.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    out.retain(|item| seen.insert(item.insert.clone()));
    out.sort_by(|a, b| a.rank.cmp(&b.rank).then(a.label.cmp(&b.label)));
    out
}

/// Read the text before the caret and decide what kind of completion it wants.
///
/// The test is on the *line* and on the token immediately before the caret, not on a parse: a
/// buffer is nearly always mid-sentence while it is being typed, and a completion that only worked
/// on a file that parses would be a completion that never worked.
fn classify(before: &str) -> Place<'_> {
    // The word being typed, and the word before it.
    let (_prefix, rest) = split_word(before);
    let trimmed = rest.trim_end();
    // A dotted path: `engine.po` completes members of `engine`.
    if let Some(dot) = trimmed.rfind('.') {
        let head = &trimmed[..dot];
        let start = head
            .rfind(|c: char| c.is_whitespace() || "{(;,".contains(c))
            .map(|i| i + 1)
            .unwrap_or(0);
        let path = head[start..].trim();
        if !path.is_empty()
            && path
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        {
            return Place::Member(path);
        }
    }
    let last = trimmed.split_whitespace().next_back().unwrap_or("");
    // `import B::*` writes a path; `import ` with nothing after it starts one.
    if last == "import" || trimmed.ends_with("::") || last == "expose" {
        return Place::Path;
    }
    if last == ":" || last == ":>" || last == ":>>" || trimmed.ends_with(':') {
        return Place::Type;
    }
    Place::Free
}

/// The identifier being typed at the end of `text`, and what came before it.
fn split_word(text: &str) -> (&str, &str) {
    let end = text.len();
    let mut start = end;
    for (i, c) in text.char_indices().rev() {
        if c.is_alphanumeric() || c == '_' {
            start = i;
        } else {
            break;
        }
    }
    (&text[start..end], &text[..start])
}

/// The words the grammar reserves, in the order a person is most likely to want them.
///
/// Read from the lexer's own table so the two cannot disagree: a word offered here is one the
/// parser would accept as a keyword, and one it would not is not offered.
fn keywords() -> Vec<Suggestion> {
    let mut out = Vec::new();
    // The declaration keywords come first, because the most common thing to type at the start of a
    // line is a declaration. `kind` is the protocol's keyword kind, which the client draws plainly.
    for word in sysml_parser::keywords() {
        out.push(Suggestion {
            insert: word.to_string(),
            label: word.to_string(),
            kind: item::KEYWORD,
            detail: "keyword".to_string(),
            rank: format!("2{word}"),
        });
    }
    out
}

/// Every element the caret could name, nearest scope first.
fn declarations(context: &Context) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let mut push = |el: &Element, qualified: &str| {
        if el.name.is_empty() || el.name == "<anonymous>" {
            return;
        }
        let kind = if el.kind.is_definition() {
            item::CLASS
        } else {
            item::FIELD
        };
        out.push(Suggestion {
            insert: quote_if_needed(&el.name),
            label: el.name.clone(),
            kind,
            detail: qualified.to_string(),
            // The nearest scope sorts first: a member of the enclosing definition is a likelier
            // answer than one from three packages away.
            rank: format!("1{}", 99 - qualified.split("::").count().min(98)),
        });
    };
    for root in context.roots {
        walk_names(root, &mut Vec::new(), &mut push);
    }
    out
}

fn walk_names(el: &Element, path: &mut Vec<String>, push: &mut impl FnMut(&Element, &str)) {
    // A parse failure lowers to an element named `syntax error`. Offering it as a completion
    // suggests the file contains something called that, which is the opposite of what it means.
    if el.kind == Kind::Unsupported {
        return;
    }
    let anonymous = el.name == "<anonymous>";
    if !anonymous {
        path.push(el.name.clone());
    }
    if !anonymous && !el.name.is_empty() {
        push(el, &path.join("::"));
    }
    for child in &el.children {
        walk_names(child, path, push);
    }
    if !anonymous {
        path.pop();
    }
}

/// The element a qualified name denotes.
fn find<'a>(roots: &'a [Element], qualified: &str) -> Option<&'a Element> {
    fn walk<'a>(el: &'a Element, path: &mut Vec<&'a str>, want: &str) -> Option<&'a Element> {
        let anonymous = el.name == "<anonymous>";
        if !anonymous {
            path.push(&el.name);
        }
        if path.join("::") == want {
            return Some(el);
        }
        for child in &el.children {
            if let Some(found) = walk(child, path, want) {
                return Some(found);
            }
        }
        if !anonymous {
            path.pop();
        }
        None
    }
    roots
        .iter()
        .find_map(|root| walk(root, &mut Vec::new(), qualified))
}

/// Elements a type position could name: every definition, and every package.
fn types_in_scope(context: &Context) -> Vec<Suggestion> {
    let mut out = Vec::new();
    for (id, qualified) in context.index.qualified.iter().enumerate() {
        if qualified.is_empty() {
            continue;
        }
        let Some(kind) = context.index.kind.get(id) else {
            continue;
        };
        let is_type = kind.is_definition() || *kind == Kind::Package;
        if !is_type {
            continue;
        }
        // The last segment is the name a reference writes; the whole path is what the list shows
        // under it, so a person can tell two same-named definitions apart.
        let Some(leaf) = qualified.rsplit("::").next() else {
            continue;
        };
        if leaf.is_empty() {
            continue;
        }
        out.push(Suggestion {
            insert: quote_if_needed(leaf),
            label: leaf.to_string(),
            kind: item::CLASS,
            detail: qualified.clone(),
            rank: format!("0{qualified}"),
        });
    }
    out
}

/// Members of the element a dotted path names.
///
/// `battery.` completes the members of the *type* of `battery`, which is what the notation means by
/// a dot: it reaches into what the feature holds, not into the declaration that wrote it. So the
/// path is resolved to an element, and when that element is a usage its type is followed one step.
fn members_of(context: &Context, path: &str) -> Vec<Suggestion> {
    let leaf = path.rsplit('.').next().unwrap_or(path);
    let leaf = leaf.rsplit("::").next().unwrap_or(leaf);
    let Some(element) = find(context.roots, path).or_else(|| find_by_leaf(context.roots, leaf))
    else {
        return Vec::new();
    };
    // A usage holds its members through its type. Following one step is what makes `battery.energy`
    // work; more than one is the resolver's job, and it is not asked here because a completion list
    // that waited for a full resolution would arrive after the person had typed the whole name.
    let target = match element.type_ref.as_deref() {
        Some(ty) => {
            find_by_leaf(context.roots, ty.rsplit("::").next().unwrap_or(ty)).unwrap_or(element)
        }
        None => element,
    };
    let mut out = Vec::new();
    for child in &target.children {
        if child.name.is_empty() || child.name == "<anonymous>" {
            continue;
        }
        let kind = if child.kind.is_definition() {
            item::CLASS
        } else {
            item::FIELD
        };
        out.push(Suggestion {
            insert: quote_if_needed(&child.name),
            label: child.name.clone(),
            kind,
            detail: format!("{}::{}", target.name, child.name),
            rank: format!("0{}", child.name),
        });
    }
    out
}

/// The element whose own name is `leaf`, when no qualified path matches.
///
/// A dotted path names a feature, and a feature is reached by its own name rather than by a
/// qualified one: `engine.po` has no `P::engine` to find, only the `engine` declared inside a
/// definition. The first such declaration is the answer — a better one would need the type of the
/// feature, which is what the resolver does for a reference and not for a path fragment.
fn find_by_leaf<'a>(roots: &'a [Element], leaf: &str) -> Option<&'a Element> {
    fn walk<'a>(el: &'a Element, leaf: &str) -> Option<&'a Element> {
        if el.name == leaf {
            return Some(el);
        }
        el.children.iter().find_map(|child| walk(child, leaf))
    }
    roots.iter().find_map(|root| walk(root, leaf))
}

/// Every package path in the project, for an `import`.
fn paths(context: &Context) -> Vec<Suggestion> {
    fn walk(el: &Element, path: &mut Vec<String>, out: &mut Vec<Suggestion>) {
        let anonymous = el.name == "<anonymous>";
        if !anonymous {
            path.push(el.name.clone());
        }
        if el.kind == Kind::Package && !anonymous {
            let qualified = path.join("::");
            // An `import` writes the package and a wildcard; offering the bare path lets the
            // client add `::*` or let the person type it, and offering `P::*` would insert a
            // wildcard into an `expose` that wanted the package itself.
            out.push(Suggestion {
                insert: qualified.clone(),
                label: qualified.clone(),
                kind: item::MODULE,
                detail: "package".to_string(),
                rank: format!("0{qualified}"),
            });
        }
        for child in &el.children {
            walk(child, path, out);
        }
        if !anonymous {
            path.pop();
        }
    }
    let mut out = Vec::new();
    for root in context.roots {
        walk(root, &mut Vec::new(), &mut out);
    }
    out
}

/// A name written with quotes when the bare spelling would not parse.
///
/// SysML reserves a bare name to `UnrestrictedName`: letters, digits and underscores, not starting
/// with a digit. `'vehicle-trailer system'` and `'frame'` need the quotes, and an editor that
/// inserted the string without them would produce a file that does not compile — from a completion
/// that was supposed to help.
fn quote_if_needed(name: &str) -> String {
    let plain = {
        let mut chars = name.chars();
        matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_')
            && chars.all(|c| c.is_alphanumeric() || c == '_')
    };
    if plain {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "\\'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(text: &'a str, index: &'a Index, roots: &'a [Element]) -> Context<'a> {
        Context {
            text,
            offset: text.len(),
            inside: None,
            index,
            roots,
        }
    }

    fn inserts(items: &[Suggestion]) -> Vec<&str> {
        items.iter().map(|i| i.insert.as_str()).collect()
    }

    fn model(src: &str) -> (Vec<Element>, Index) {
        let loaded = sysml_syntax::load(src).expect("parses");
        let index = Index::build(&loaded.roots);
        (loaded.roots, index)
    }

    #[test]
    fn a_declaration_keyword_is_offered_at_the_start_of_a_line() {
        let (roots, index) = model("package P {\n}\n");
        let text = "package P {\n  par";
        let items = complete(&context(text, &index, &roots));
        assert!(inserts(&items).contains(&"part"), "{:?}", inserts(&items));
    }

    #[test]
    fn an_element_in_the_model_is_offered() {
        let (roots, index) = model("package P { part def Battery; }");
        let text = "package P {\n  part def Car {\n    part b : \n";
        let items = complete(&context(text, &index, &roots));
        assert!(
            inserts(&items).contains(&"Battery"),
            "Battery is in scope: {:?}",
            inserts(&items)
        );
    }

    #[test]
    fn a_dotted_path_completes_the_members_of_what_precedes_it() {
        // The model has to declare `battery` *and* its type: a dotted path reaches into what the
        // feature holds, and a model without the feature would make this test answer a question
        // about nothing.
        let (roots, index) = model(
            "package P {\n  part def Battery { attribute energy : Real; }\n  part def A { part battery : Battery; }\n}\n",
        );
        let text = "package P {\n  part def Battery { attribute energy : Real; }\n  part def A {\n    part battery : Battery;\n    attribute x = battery.\n";
        let items = complete(&context(text, &index, &roots));
        assert!(
            inserts(&items).contains(&"energy"),
            "a member of Battery: {:?}",
            inserts(&items)
        );
    }

    #[test]
    fn a_parse_failure_is_not_offered_as_a_name() {
        // A buffer that does not parse lowers `syntax error` as an element. Offering it would tell
        // the person their file contains something called that.
        let (roots, index) = model("package P {\n  this is not sysml\n  part def A;\n}\n");
        let text =
            "package P {\n  this is not sysml\n  part def A;\n  part def B {\n    part x : \n";
        let items = complete(&context(text, &index, &roots));
        assert!(
            !inserts(&items).iter().any(|i| i.contains("syntax error")),
            "{:?}",
            inserts(&items)
        );
    }

    #[test]
    fn an_import_offers_package_paths() {
        let (roots, index) = model("package P { package Sub { } }");
        let text = "package Q {\n  private import \n";
        let items = complete(&context(text, &index, &roots));
        assert!(
            inserts(&items).contains(&"P"),
            "the package is offered: {:?}",
            inserts(&items)
        );
        assert!(
            inserts(&items).contains(&"P::Sub"),
            "and its members too: {:?}",
            inserts(&items)
        );
    }

    #[test]
    fn a_name_that_needs_quotes_is_inserted_with_them() {
        // `'vehicle-trailer system'` is a legal name; inserting it bare is not.
        let (roots, index) = model("package P { part def 'vehicle-trailer system'; }");
        let text = "package P {\n  part def A {\n    part v : \n";
        let items = complete(&context(text, &index, &roots));
        assert!(
            inserts(&items).contains(&"'vehicle-trailer system'"),
            "{:?}",
            inserts(&items)
        );
    }

    #[test]
    fn a_plain_name_is_inserted_without_quotes() {
        let (roots, index) = model("package P { part def Battery; }");
        let text = "package P {\n  part def A {\n    part b : \n";
        let items = complete(&context(text, &index, &roots));
        assert!(inserts(&items).contains(&"Battery"));
        assert!(!inserts(&items).contains(&"'Battery'"));
    }

    #[test]
    fn nothing_is_offered_twice() {
        let (roots, index) = model("package P { part def Battery; part def Battery2; }");
        let text = "package P {\n  part def A {\n    part b : \n";
        let items = complete(&context(text, &index, &roots));
        let mut seen = BTreeSet::new();
        for item in &items {
            assert!(seen.insert(item.insert.clone()), "{} twice", item.insert);
        }
    }

    #[test]
    fn quoting_leaves_a_plain_name_alone() {
        assert_eq!(quote_if_needed("Battery"), "Battery");
        assert_eq!(quote_if_needed("_x1"), "_x1");
        assert_eq!(quote_if_needed("frame"), "frame");
        assert_eq!(quote_if_needed("a b"), "'a b'");
        assert_eq!(quote_if_needed("1st"), "'1st'");
        assert_eq!(quote_if_needed("it's"), "'it\\'s'");
    }
}
