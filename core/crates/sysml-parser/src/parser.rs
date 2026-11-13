//! Recursive-descent parser for SysML v2 and KerML textual notation.
//!
//! # Structure
//!
//! The parser is one struct over a token slice. Each production is a method, and the top-level
//! entry [`Parser::document`] loops over body members until the input ends.
//!
//! # Error recovery
//!
//! Two rules, both learned from what goes wrong in practice:
//!
//! 1. **A body member that fails is recorded and skipped to the next member boundary**, so one
//!    bad statement does not cost the rest of the file. The boundary is the next token that
//!    starts a member at the same brace depth.
//! 2. **A failed member never consumes the closing brace of its parent.** Without that rule a
//!    runaway parse eats the whole file and reports one error at the top, which hides where the
//!    problem actually is.
//!
//! # Reserved words in name position
//!
//! `step entry[1];` declares a step named `entry`. The grammar allows it because `entry` is only
//! a keyword inside a state body. The parser therefore asks "is a name expected here" before it
//! asks "what keyword is this", rather than dispatching on the token alone — which is the single
//! change that makes the state and transition library files parse.

use crate::ast::{
    Annotation, BodyMember, Declaration, Direction, Document, Expr, Head, ImportShape, Kind,
    Multiplicity, Name, Prefixes, Relation, RelationOp, Relationship, Span, Statement, TypeRef,
    TypeTestKind, Visibility,
};
use crate::lexer::{lex, Token, TokenKind};

/// A parse failure severe enough to abandon the file.
///
/// Body-level problems do not become this: they are recorded as [`BodyMember::Error`] and the
/// scan continues. This is only for a buffer the parser cannot make head or tail of at all.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: u32,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (line {})", self.message, self.line)
    }
}

impl std::error::Error for ParseError {}

/// A parsed file plus the source it came from, so a span is resolvable.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub document: Document,
    pub source: String,
}

impl ParsedDocument {
    /// The authored text inside `span`.
    pub fn slice(&self, span: Span) -> Option<&str> {
        self.source.get(span.start..span.end)
    }
}

/// Parse one source buffer.
///
/// # Errors
///
/// Returns [`ParseError`] with the line and what was expected when the text is not valid SysML:
/// the lexer's own error passes through first, and a construct the grammar does not accept
/// produces one after it. Valid SysML that this parser does not model is not an error here; it
/// becomes a visible marker downstream.
pub fn parse(src: &str) -> Result<ParsedDocument, ParseError> {
    let tokens = lex(src).map_err(|e| ParseError {
        message: e.message,
        line: e.line,
    })?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
        pending_parameters: Vec::new(),
        pending_cross_multiplicity: None,
        pending_cross_name: None,
        pending_cross_relations: Vec::new(),
        prefix_annotated: false,
        pending_members: Vec::new(),
        pending_trigger_type: None,
        pending_trigger_via: None,
        pending_visibility: None,
    };
    let members = parser.document()?;
    Ok(ParsedDocument {
        document: Document { members },
        source: src.to_string(),
    })
}

/// The nesting limit for bodies and expressions.
///
/// A model is data, and data can be hostile or merely broken. A recursive-descent parser with no
/// limit turns a file of ten thousand `(` into a stack overflow, which is a crash rather than a
/// diagnostic. This cap is far above anything a person writes and far below the stack limit.
const MAX_DEPTH: usize = 256;

pub(crate) struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
    /// Parameter members a declaration head collected, waiting for the declaration to take them.
    ///
    /// `ref action X (in a = 1) :> B` writes the parameter list between the name and the
    /// specialization clause, so `head` has to read it before `relations` — and `Head` has no
    /// field for members. Draining this right after `head` returns keeps the two halves together
    /// without threading a second return value through every caller.
    pending_parameters: Vec<BodyMember>,
    /// The bound written between `end` and the kind keyword, waiting for the declaration head.
    ///
    /// `end [0..1] part hub : Hub` puts it where only the prefix run can see it: the head runs
    /// after the kind keyword, and by then the bracket is behind. The head takes it here and files
    /// it as `Head::cross_multiplicity`, never as the end's own bound — the specification fixes an
    /// end at 1..1 and puts this number on the owned cross feature.
    pending_cross_multiplicity: Option<Multiplicity>,
    /// The name the model gave the owned cross feature, read in the same prefix run.
    pending_cross_name: Option<String>,
    /// The specializations written on the owned cross feature (`end e subsets s feature …`).
    pending_cross_relations: Vec<Relation>,
    /// Members a member brought with it, to be placed before it: `then action b` is the succession
    /// into `b` and then `b`.
    pending_members: Vec<BodyMember>,
    /// The type and the `via` port of the payload a transition's `accept` just read.
    pending_trigger_type: Option<TypeRef>,
    pending_trigger_via: Option<Expr>,
    /// Whether the member just read was a prefix annotation, which makes a following
    /// declaration with no kind keyword an extended definition or usage.
    prefix_annotated: bool,
    /// The visibility keyword the member run just consumed, for the declaration it introduces.
    ///
    /// `private attribute b;` states a visibility on the *membership*, and the declaration is
    /// built after it was read; without a place to keep it the word was dropped and every member
    /// looked public (`AssignmentActionUsage_invalid.sysml.xt:41`).
    pending_visibility: Option<Visibility>,
}

impl Parser {
    // -- token access --------------------------------------------------------

    fn at(&self, offset: usize) -> &Token {
        let i = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[i]
    }

    fn cur(&self) -> &Token {
        self.at(0)
    }

    fn kind(&self) -> &TokenKind {
        &self.cur().kind
    }

    fn is_eof(&self) -> bool {
        matches!(self.kind(), TokenKind::Eof)
    }

    fn text(&self) -> &str {
        &self.cur().text
    }

    fn line(&self) -> u32 {
        self.cur().line
    }

    fn bump(&mut self) -> Token {
        let t = self.cur().clone();
        if !matches!(t.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        t
    }

    /// Consume the current token when it is exactly `text`.
    fn eat(&mut self, text: &str) -> bool {
        if self.text() == text {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Consume the current token when it is one of `texts`.
    fn eat_any(&mut self, texts: &[&str]) -> Option<String> {
        let t = self.text().to_string();
        if texts.contains(&t.as_str()) {
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    fn expect(&mut self, text: &str) -> Result<Token, String> {
        if self.text() == text {
            Ok(self.bump())
        } else {
            Err(format!(
                "expected `{text}`, found {}",
                self.describe_current()
            ))
        }
    }

    /// A human-readable description of the current token, for a diagnostic.
    fn describe_current(&self) -> String {
        if self.is_eof() {
            "end of file".to_string()
        } else {
            format!("`{}`", self.text())
        }
    }

    fn span_here(&self) -> Span {
        Span::new(self.cur())
    }

    /// The span of the token just consumed.
    ///
    /// `span_here` names the token that is *next*, which is the wrong end for a range: an
    /// expression that stops at `;` would report a range ending after the semicolon. Anything an
    /// editor splices back into the source has to end at the last character the construct
    /// actually occupies, so the closing token is asked for by position rather than by peeking.
    fn span_prev(&self) -> Span {
        let i = self.pos.saturating_sub(1);
        Span::new(&self.tokens[i.min(self.tokens.len().saturating_sub(1))])
    }

    /// True when the current token can introduce a name.
    ///
    /// A reserved word counts: the grammar allows `step entry;` and `part 'name'`. What is *not*
    /// a name is punctuation, a number, or a string.
    fn at_name(&self) -> bool {
        matches!(
            self.kind(),
            TokenKind::Ident | TokenKind::Keyword | TokenKind::QuotedName
        )
    }

    /// Guard against a hostile or merely broken input overflowing the stack.
    ///
    /// Returns the message as a `String` so it composes with the member-level `Result<_, String>`
    /// used throughout; only the top-level entry converts to a [`ParseError`].
    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(format!(
                "nesting deeper than {MAX_DEPTH} levels (line {})",
                self.line()
            ));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    // -- names ---------------------------------------------------------------

    /// Parse a declared name: an optional `<short>` then the simple name.
    ///
    /// Both forms may appear together (`<B> byte`), and either may be absent when the declaration
    /// is anonymous. The returned `Name` is empty when nothing was written.
    fn name(&mut self) -> Name {
        let start = self.span_here();
        let mut short = None;
        let mut text = String::new();
        let mut end = start;

        if matches!(self.kind(), TokenKind::ShortName) {
            short = Some(self.cur().name_text());
            end = self.span_here();
            self.bump();
        }
        if self.at_name() && !self.is_member_boundary() {
            text = self.cur().name_text();
            end = self.span_here();
            self.bump();
        }
        Name {
            text,
            short,
            span: start.to(end),
        }
    }

    /// True when the current token ends a name rather than continuing one.
    ///
    /// Used to keep `name()` from swallowing the keyword that follows an anonymous declaration.
    /// The relationship keywords are included: `feature redefines A::B` names no feature, it
    /// redefines `A::B`, and reading `redefines` as a name left the rest of the clause to be
    /// parsed as a value.
    fn is_member_boundary(&self) -> bool {
        matches!(
            self.text(),
            ";" | "{"
                | "}"
                | ":"
                | ":>"
                | ":>>"
                | "::>"
                | "="
                | ":="
                | ","
                | ")"
                | "redefines"
                | "specializes"
                | "subsets"
                | "subset"
                | "references"
                | "crosses"
                | "typed"
                | "defined"
                | "default"
        )
    }

    /// True when the token at `offset` is a relationship operator, so the word before it was a
    /// prefix rather than a name.
    ///
    /// `ref :>> x` is an anonymous reference feature redefining `x`: the grammar puts `ref` in
    /// `FeaturePrefix` and lets the feature name be absent (`KerML.xtext`, `FeatureDeclaration`),
    /// so the `:>>` follows the prefix directly. Reading `ref` as the name turned every such
    /// declaration into an element called `ref` — twelve of them across the official validation
    /// models, and two in one body collided into an SEM091 duplicate-name warning.
    ///
    /// `:` is deliberately absent. `part ref : A` writes a feature *named* `ref`, and `ref : T`
    /// — an anonymous reference feature typed directly — is a spelling no model in the corpus
    /// uses; accepting the colon would trade a silent wrong name for a silent missing one.
    fn at_relation_op(&self, offset: usize) -> bool {
        matches!(self.at(offset).text.as_str(), ":>" | ":>>" | "::>")
    }

    /// The run of modifiers that may precede a kind keyword.
    ///
    /// `in`/`out`/`inout`, `abstract`, `variation`, `individual`, `derived`, `constant`,
    /// `readonly`, `end`, `ref`, the portion kinds, and `#Tag` extensions. Collected in one pass
    /// because the grammar treats them as one optional run and the lowering layer reads them as
    /// one set of flags.
    fn prefixes(&mut self) -> Prefixes {
        let mut p = Prefixes::default();
        loop {
            match self.text() {
                "in" => {
                    self.bump();
                    p.direction = Some(Direction::In);
                }
                "out" => {
                    self.bump();
                    p.direction = Some(Direction::Out);
                }
                "inout" => {
                    self.bump();
                    p.direction = Some(Direction::InOut);
                }
                "abstract" => {
                    self.bump();
                    p.is_abstract = true;
                }
                // `all conn def C;` — `all` marks the type sufficient. Without this arm the word
                // and the keyword after it became two stray attributes and the declaration
                // vanished.
                "all" if !matches!(self.at(1).kind, TokenKind::Punct) => {
                    self.bump();
                    p.is_sufficient = true;
                }
                "variation" => {
                    self.bump();
                    p.is_variation = true;
                }
                "individual" => {
                    // `individual` is also a kind keyword (an individual definition); it is a
                    // prefix only when a kind follows it.
                    if matches!(
                        self.at(1).text.as_str(),
                        "part" | "attribute" | "port" | "item" | "action" | "state"
                    ) {
                        self.bump();
                        p.is_individual = true;
                    } else {
                        break;
                    }
                }
                "derived" => {
                    self.bump();
                    p.is_derived = true;
                }
                "member" if self.at(1).text == "feature" => {
                    self.bump();
                    p.is_member = true;
                }
                // `constant` is the SysML spelling (`SysML.xtext:560`) and `const` the KerML
                // one (`KerML.xtext:511`, `:519`). Only the first was read, so every KerML
                // `const feature f;` — the spelling the pilot uses — lost the flag and the
                // subsetting-constant rule had nothing to compare against
                // (`Subsetting_constant_invalid.kerml.xt:24`).
                "constant" | "const" => {
                    self.bump();
                    p.is_constant = true;
                }
                "readonly" => {
                    self.bump();
                    p.is_readonly = true;
                }
                // `var x : Real` — a variable feature. It was a keyword in the lexer but had no
                // arm here, so the word became the member's name and the feature it introduced
                // was lost. Four rules read isVariable.
                "var" if !matches!(self.at(1).kind, TokenKind::Punct) => {
                    self.bump();
                    p.is_variable = true;
                }
                // `end : A` is an *anonymous* end: the keyword is the whole prefix and the
                // declaration starts at the colon. The guard used to require a name after the word,
                // so this spelling took `end` for the member's name and gave the association two
                // members called `end`.
                // `end`, and whatever cross feature it names or bounds. Both the anonymous form
                // (`end [0..1] part hub`) and the named one (`end connectingHub [0..1] part hub`)
                // put that material before the kind keyword, where only this run can see it.
                "end"
                    if !matches!(self.at(1).kind, TokenKind::Punct)
                    || self.at(1).text == ":"
                    || self.at(1).text == "["
                    // `connection { end ::> b0; }` — an anonymous end whose identity is the
                    // redefinition it writes. The run stopped at the operator, so `end` became the
                    // member's name and the connection ended up with one member called `end` and
                    // no ends at all (`Relationship_invalid_relatedElement0.sysml.xt:33`).
                    || self.at_relation_op(1) =>
                {
                    self.bump();
                    p.is_end = true;
                    self.read_cross_feature();
                }
                "composite" | "portion" | "snapshot" | "timeslice" => {
                    p.portion = Some(self.bump().text);
                }
                // `abstract ref :>> trailerHitch[1]` — the prefix run stops at a relationship
                // operator, where the declaration has no name by design. The guard used to reject
                // every punctuation, so `ref` became the member's name and the redefinition it
                // introduces lost its identity (`3c-Function-based Behavior-structure mod-2.sysml:38`).
                "ref" if !matches!(self.at(1).kind, TokenKind::Punct) || self.at_relation_op(1) => {
                    self.bump();
                    p.is_ref = true;
                }
                // `render r : R` is a *view rendering membership*, not a keyword that names the
                // member's kind (`SysML.xtext:2335-2343`). `rendering r : R` is the usage form
                // and stays a kind keyword. Folding the two made a view body with one of each
                // count as two renderings when it states one.
                "render" if !matches!(self.at(1).kind, TokenKind::Punct) => {
                    self.bump();
                    p.is_render = true;
                }
                "#" => {
                    self.bump();
                    if self.at_name() {
                        p.extensions.push(self.bump().name_text());
                    }
                }
                _ => break,
            }
        }
        p
    }

    /// Read the owned cross feature that may follow `end`.
    ///
    /// Two spellings, both of which put the material before the kind keyword:
    ///
    /// ```text
    /// end [0..1] part hub : Hub;              // anonymous cross feature
    /// end connectingHub [0..1] ordered part hub : Hub;   // named
    /// ```
    ///
    /// The name and the bound belong to the cross feature, not to the end: the specification fixes
    /// an end at 1..1 and gives this number to the cross feature (2024-12 pilot release notes).
    /// They are stashed for the head, which runs after the kind keyword and cannot see back.
    ///
    /// The lookahead is what keeps an ordinary end out of this path. `end x [0..1] : T` also writes
    /// a name and a bracket, but what follows is its own typing clause rather than a kind keyword,
    /// so the position is restored and the head reads it as the end's own bound.
    fn read_cross_feature(&mut self) {
        if self.text() == "[" {
            self.pending_cross_multiplicity = self.multiplicity();
            self.skip_ordering();
            return;
        }
        if !self.at_name() {
            return;
        }
        let save = self.pos;
        let name = self.name().text;
        let bound = self.multiplicity();
        let ordered = matches!(self.text(), "ordered" | "nonunique");
        self.skip_ordering();
        // `OwnedCrossingFeature : BasicFeaturePrefix FeatureDeclaration` (`KerML.xtext:533-535`):
        // the cross feature may specialize as well as bound, as in
        // `end happensWhile [1..*] subsets timeCoincidentOccurrences feature thatOccurrence …`
        // (`Occurrences.kerml:767`). Reading only a bound left `subsets …` to end the declaration,
        // and the real end became a phantom sibling.
        let relations = self.relations(false);
        let bound = bound.or_else(|| self.multiplicity());
        if bound.is_none() && !ordered && relations.is_empty() {
            self.pos = save;
            return;
        }
        // The end's own prefix may follow the cross feature before the kind keyword:
        // `end cross [0..1] derived part hub : Hub` makes `hub` the derived end. Read as part of
        // the declaration, `derived` ended `cross` and `hub` became a separate member, so the
        // rule against a derived end saw neither element carrying both.
        let mut i = 0usize;
        while matches!(
            self.at(i).text.as_str(),
            "derived"
                | "abstract"
                | "constant"
                | "const"
                | "readonly"
                | "ref"
                | "var"
                | "composite"
                | "portion"
                | "in"
                | "out"
                | "inout"
                | "individual"
                | "variation"
        ) {
            i += 1;
        }
        if !matches!(
            self.at(i).text.as_str(),
            "part"
                | "port"
                | "attribute"
                | "item"
                | "occurrence"
                | "action"
                | "state"
                | "feature"
                | "connection"
                | "interface"
                | "flow"
                | "allocation"
                | "event"
        ) {
            self.pos = save;
            return;
        }
        self.pending_cross_name = Some(name);
        self.pending_cross_multiplicity = bound;
        self.pending_cross_relations = relations;
    }

    /// `ordered` / `nonunique` name a property of the multiplicity, so they travel with it.
    fn skip_ordering(&mut self) {
        while matches!(self.text(), "ordered" | "nonunique") {
            self.bump();
        }
    }

    /// Parse a qualified reference as written: `A::B::C`, `'A B'::C`, `x.y.z`.
    ///
    /// Dots and double colons are both kept in the text, because a feature chain (`vehicle.engine`)
    /// and a namespace path (`ISQ::mass`) differ only in the separator, and the resolver needs to
    /// tell them apart.
    fn type_ref(&mut self) -> Option<TypeRef> {
        if !self.at_name() {
            return None;
        }
        let start = self.span_here();
        let mut text = String::new();
        let mut end = start;
        loop {
            if !self.at_name() {
                break;
            }
            text.push_str(&self.cur().name_text());
            end = self.span_here();
            self.bump();
            // A multiplicity on the type itself (`ScalarValue[1]`) belongs to the reference,
            // because that is how the authored text reads and how the units/multiplicity layers
            // need to see it.
            if self.text() == "[" {
                let mut depth = 0usize;
                while !self.is_eof() {
                    if self.text() == "[" {
                        depth += 1;
                    } else if self.text() == "]" {
                        depth -= 1;
                        if depth == 0 {
                            text.push_str(self.text());
                            end = self.span_here();
                            self.bump();
                            break;
                        }
                    }
                    text.push_str(self.text());
                    end = self.span_here();
                    self.bump();
                }
                break;
            }
            // A path continues across `::` or `.`.
            if (self.text() == "::" || self.text() == ".")
                && !matches!(self.at(1).kind, TokenKind::Punct | TokenKind::Eof)
            {
                text.push_str(self.text());
                self.bump();
                // A `::*` in this position is an import suffix, not a path segment.
                if self.text() == "*" {
                    text.push('*');
                    end = self.span_here();
                    self.bump();
                    break;
                }
                continue;
            }
            break;
        }
        Some(TypeRef {
            text,
            span: start.to(end),
        })
    }
}

// -- document and members ----------------------------------------------------

impl Parser {
    /// Parse a whole file: body members until the input ends.
    pub(crate) fn document(&mut self) -> Result<Vec<BodyMember>, ParseError> {
        self.enter().map_err(|message| ParseError {
            message,
            line: self.line(),
        })?;
        let mut members = Vec::new();
        while !self.is_eof() {
            let before = self.pos;
            let member = self.member(true);
            members.append(&mut self.pending_members);
            if let Some(m) = member {
                members.push(m);
            }
            // No progress means no rule matched and recovery did not move either. Skip one token
            // so the loop terminates; the member-level recovery normally prevents this.
            //
            // TODO: a genuinely unmatched top-level `}` is still skipped here, so
            // `package P { } }` is accepted. This is independent of Import_Visibility_Invalid:26:
            // that brace closes the source package and is only extraneous in the reference parser's
            // recovered tree after its invalid bare import. It must not receive a second error just
            // to reproduce recovery. Strictness changes also need an exact source-defect allowance
            // for the old upstream Interfaces.sysml, whose `//*` comment hides an opening brace;
            // the runtime's vendored standard library is a separate, balanced copy.
            if self.pos == before {
                self.bump();
            }
        }
        self.leave();
        Ok(members)
    }

    /// `{ … }` in value position: an `ExpressionBody` (`KerML.xtext:1121-1123`).
    ///
    /// The body holds members like any other, and the expression a reader means is the one it
    /// states — its last expression member, which is how a calculation writes its result. A body
    /// that states none (`default { }`) has no operand, and `None` says so rather than inventing
    /// one.
    fn braced_expression(&mut self) -> Result<Option<Expr>, String> {
        let members = self.body();
        for member in members.iter().rev() {
            if let BodyMember::Statement(Statement::Expression { expr, .. }) = member {
                return Ok(Some(expr.clone()));
            }
        }
        Ok(None)
    }

    /// Parse a braced body: `{ ... }`, or nothing after `;`.
    ///
    /// Returns the members. An unterminated body is recorded as an error member inside the body
    /// it belongs to, so the diagnostic points at the place the brace should have been.
    fn body(&mut self) -> Vec<BodyMember> {
        if self.eat(";") {
            return Vec::new();
        }
        if !self.eat("{") {
            // A declaration ends at `;` or at its body. Returning quietly on anything else let the
            // unread text start a new sibling member, which is how `= f meta T` and `actor #B a`
            // turned into phantom members while the file still counted as parsed. A comma or a
            // closing parenthesis ends a parameter, and a closing brace ends the owner.
            if self.is_eof() || matches!(self.text(), "," | ")" | "}") {
                return Vec::new();
            }
            // Queued rather than returned: 36 callers discard what `body()` returns, and an error
            // inside a discarded vector is the same silent split this check exists to end —
            // `return in r : Real;` lost `r` that way with no report at all.
            self.pending_members.push(BodyMember::Error {
                message: format!(
                    "expected `;` or a body to end the declaration, found {} (line {})",
                    self.describe_current(),
                    self.line()
                ),
                text: String::new(),
                span: self.span_here(),
            });
            return Vec::new();
        }
        if self.depth > MAX_DEPTH {
            // Bail rather than recurse; the caller's error member carries the message.
            self.skip_balanced_brace();
            return Vec::new();
        }
        self.depth += 1;
        let mut members = Vec::new();
        loop {
            if self.is_eof() {
                members.push(BodyMember::Error {
                    message: "unterminated body: `}` not found".to_string(),
                    text: String::new(),
                    span: self.span_here(),
                });
                break;
            }
            if self.text() == "}" {
                self.bump();
                // A prefix annotation with nothing after it in this body annotates nothing outside.
                self.prefix_annotated = false;
                break;
            }
            let before = self.pos;
            let member = self.member(false);
            // A member may bring the succession its `then` states, or an error a discarded body
            // reported; both precede it.
            members.append(&mut self.pending_members);
            if let Some(m) = member {
                members.push(m);
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.depth -= 1;
        members
    }

    /// Consume tokens through the matching `}` without parsing them.
    fn skip_balanced_brace(&mut self) {
        if self.text() != "{" {
            return;
        }
        let mut depth = 0usize;
        while !self.is_eof() {
            match self.text() {
                "{" => depth += 1,
                "}" => {
                    depth -= 1;
                    if depth == 0 {
                        self.bump();
                        return;
                    }
                }
                _ => {}
            }
            self.bump();
        }
    }

    /// Parse one body member, or recover from one that does not parse.
    ///
    /// `top` says whether this is the file's top level, which decides whether a `package` is
    /// allowed here.
    fn member(&mut self, top: bool) -> Option<BodyMember> {
        let start = self.span_here();
        let parsed = self.parse_member(top);
        // A visibility keyword belongs to the one membership it is written on. Only an annotation
        // hands it on, because `private #Safety part def B2;` returns the annotation first and the
        // declaration next. Everything else that read one and did not consume it — an import, an
        // alias, a failed member — used to leave it pending, so `private import ScalarValues::*;`
        // made the next declaration private as well.
        if !matches!(parsed, Ok(Some(BodyMember::Annotation(_)))) {
            self.pending_visibility = None;
        }
        match parsed {
            Ok(Some(m)) => Some(m),
            // A member that parsed to nothing (a doc comment consumed by its owner, say).
            Ok(None) => None,
            Err(message) => {
                // Recovery: consume through the end of this member and record it. The limit is
                // the parent's closing brace, so a runaway never eats the enclosing body.
                let text = self.skip_member();
                Some(BodyMember::Error {
                    message,
                    text,
                    span: start,
                })
            }
        }
    }

    /// Skip to the end of the current member and return its text.
    ///
    /// Stops at `;` (consumed), or at a `}` that closes the parent (not consumed), or at a `{`
    /// whose body is skipped as a unit.
    fn skip_member(&mut self) -> String {
        let start = self.cur().start;
        let mut depth = 0usize;
        let mut end = start;
        let mut skipped = false;
        while !self.is_eof() {
            match self.text() {
                "{" => {
                    depth += 1;
                    end = self.cur().end;
                    self.bump();
                }
                "}" => {
                    if depth == 0 {
                        // The parent's brace: leave it for the body loop.
                        return self.source_between(start, end);
                    }
                    depth -= 1;
                    end = self.cur().end;
                    self.bump();
                }
                ";" if depth == 0 => {
                    end = self.cur().end;
                    self.bump();
                    return self.source_between(start, end);
                }
                // A runaway that swallowed the next declaration is worse than the error it is
                // recovering from: `??? part def B;` used to consume `part def B;` and report one
                // error for two problems, with B missing from the outline entirely. Once at least
                // one token is behind us, a word that starts a declaration ends the skipped run.
                word if depth == 0 && skipped && Self::starts_declaration_str(word) => {
                    return self.source_between(start, end);
                }
                _ => {
                    end = self.cur().end;
                    self.bump();
                    skipped = true;
                }
            }
        }
        self.source_between(start, end)
    }

    /// Whether a token can begin a declaration, for the recovery scan above.
    fn starts_declaration_str(word: &str) -> bool {
        matches!(
            word,
            "part"
                | "attribute"
                | "port"
                | "item"
                | "action"
                | "state"
                | "requirement"
                | "interface"
                | "connection"
                | "flow"
                | "package"
                | "import"
                | "alias"
                | "ref"
                | "private"
                | "public"
                | "protected"
        )
    }

    fn source_between(&self, start: usize, end: usize) -> String {
        // The parser does not keep the buffer; the caller fills the text in from the span. An
        // empty string here keeps the span meaningful, which is what a report quotes.
        let _ = (start, end);
        String::new()
    }
}

// -- member dispatch ---------------------------------------------------------

impl Parser {
    /// Parse one member. Returns `Ok(None)` when the member was fully consumed but produces no
    /// node of its own (a `doc` comment attached to the next element, for instance).
    fn parse_member(&mut self, _top: bool) -> Result<Option<BodyMember>, String> {
        // A leading multiplicity is an occurrence prefix on the member that follows
        // (`[0..*] part x : T;`), not an expression. It is consumed here so the member is read
        // as the declaration it is.
        let leading_multiplicity = self.multiplicity();
        // Annotations come first: they precede the member they document.
        if let Some(annotation) = self.annotation()? {
            return Ok(Some(BodyMember::Annotation(annotation)));
        }
        // Visibility prefix belongs to the membership, not to the element.
        let visibility = self.visibility();
        // Keep what an earlier member already recorded: `private #Safety part def B2;` reads the
        // visibility here, then returns the annotation as its own member, and the declaration comes
        // in the next call with no visibility written. Overwriting with `None` there would lose the
        // `private` (measured: `sysml outline` showed `part def B2` without the marker).
        if visibility.is_some() {
            self.pending_visibility = visibility;
        }
        // An annotation may also follow the visibility: `private #Safety part def B2;` declares the
        // same annotation as `#Safety private part def B2;`, because visibility belongs to the
        // membership and the annotation to the element. Only the second spelling was read — the
        // first dropped the annotation entirely (measured: `sysml outline` showed `part def B2` with
        // no metadata), which left §7.27.3 nothing to derive from.
        if let Some(annotation) = self.annotation()? {
            return Ok(Some(BodyMember::Annotation(annotation)));
        }
        // `standard library package` is a package with a flag.
        if self.text() == "standard" && self.at(1).text == "library" {
            self.bump();
            self.bump();
            // `standard` is not part of the kind, and the reference warns about the spelling: a
            // user's library is not the standard one
            // (`LibraryPackage_invalid_notStandard.kerml.xt:15`). Dropping the word left the
            // declaration indistinguishable from `library package`.
            let d = self.package_decl_with(Kind::LibraryPackage, true)?;
            return Ok(Some(BodyMember::Declaration(Box::new(d))));
        }
        if self.text() == "library" && self.at(1).text == "package" {
            self.bump();
            let d = self.package_decl(Kind::LibraryPackage)?;
            return Ok(Some(BodyMember::Declaration(Box::new(d))));
        }
        if self.text() == "package" {
            let d = self.package_decl(Kind::Package)?;
            return Ok(Some(BodyMember::Declaration(Box::new(d))));
        }
        if self.text() == "namespace" {
            let d = self.package_decl(Kind::Namespace)?;
            return Ok(Some(BodyMember::Declaration(Box::new(d))));
        }

        // A directed parameter: `in x : T;` / `out y : T;`. It is a declaration whose kind is
        // the direction, and it appears in every action, calculation and function signature.
        if matches!(self.text(), "in" | "out" | "inout")
            && matches!(
                self.at(1).kind,
                TokenKind::Ident | TokenKind::QuotedName | TokenKind::ShortName
            )
            && !matches!(self.at(1).text.as_str(), "ref" | "for")
            // `in action thenClause[0..1];` (`Actions.sysml:399`) names a kind after the
            // direction. This shortcut reads only attribute-like words there, so the kind became
            // the parameter's name; the general prefix path keeps both the direction and the kind.
            && !matches!(
                self.at(1).text.as_str(),
                "action" | "part" | "port" | "occurrence" | "state" | "calc" | "constraint"
                    | "requirement" | "connection" | "interface" | "allocation" | "flow" | "event"
                    | "analysis" | "verification" | "case" | "use" | "view" | "viewpoint"
                    | "rendering" | "concern" | "metadata" | "enum" | "individual"
            )
        {
            let direction = match self.text() {
                "in" => Direction::In,
                "out" => Direction::Out,
                _ => Direction::InOut,
            };
            self.bump();
            let mut p = Prefixes {
                direction: Some(direction),
                ..Prefixes::default()
            };
            // `in ref x : T` and `in attribute x : T` are both legal: the modifier run after the
            // direction may name the feature kind. Leaving `attribute` in place made the whole
            // declaration parse as a parameter called `attribute`.
            // Whether the run named a kind word. `attribute`, `item` and `feature` do, and a
            // declaration that names none is a bare feature (`bare` on the declaration).
            let mut named_kind = false;
            while matches!(
                self.text(),
                "ref" | "attribute" | "feature" | "item" | "var"
            ) {
                match self.text() {
                    "ref" => p.is_ref = true,
                    // `attribute var y : Integer` — the modifier run names the kind and the
                    // variable-ness together. Bumping `var` without recording it left the flag
                    // unset, and a rule that reads isVariable saw a constant.
                    "var" => p.is_variable = true,
                    "attribute" | "feature" | "item" => named_kind = true,
                    _ => {}
                }
                self.bump();
            }
            let head = self.head(p, None)?;
            let body = self.body();
            return Ok(Some(BodyMember::Declaration(Box::new(Declaration {
                kind: Kind::Attribute,
                is_definition: false,
                bare: !named_kind,
                head,
                doc: extract_doc(&body),
                body,
            }))));
        }

        // Relationships.
        if self.text() == "import" {
            // `ImportPrefix` is `visibility = VisibilityIndicator 'import'` in both grammars
            // (`SysML.xtext:239-241`, `KerML.xtext:168-170`): the visibility keyword is not
            // optional. `import ScalarValues;` after three imports that state one is a syntax
            // error, and the model that states it says so
            // (`Import_Visibility_Invalid.sysml.xt:23`). Accepting it made this parser wider than
            // the grammar it claims — the one direction a conformance parser must not drift.
            if visibility.is_none() {
                return Err(format!(
                    "an `import` needs a visibility keyword before it, found `import` (line {})",
                    self.line()
                ));
            }
            return Ok(Some(BodyMember::Relationship(self.import(visibility)?)));
        }
        if self.text() == "alias" {
            return Ok(Some(BodyMember::Relationship(self.alias()?)));
        }
        if self.text() == "expose" {
            // `ExposePrefix` is `visibility = ExposeVisibilityKind`, and the only value it takes is
            // the keyword `expose` (`SysML.xtext:2366-2372`). A visibility keyword before it is not
            // a spelling the grammar admits, and the tree has no field to keep one.
            if visibility.is_some() {
                self.pending_visibility = None;
                return Err(format!(
                    "an `expose` takes no visibility keyword before it (line {})",
                    self.line()
                ));
            }
            return Ok(Some(BodyMember::Relationship(self.expose()?)));
        }
        if self.text() == "dependency" {
            return Ok(Some(BodyMember::Relationship(self.dependency()?)));
        }
        // `subtype C specializes B;` — a specialization stated on its own. The word alone is not
        // enough to claim it, since a feature may be called `subtype`.
        if self.at_specialization() {
            return Ok(Some(BodyMember::Relationship(self.specialization()?)));
        }
        // `disjoint earlierOccurrence.successors from laterOccurrence.predecessors;` — a disjoining
        // stated on its own (`Occurrences.kerml:125`). `disjoint from` without a first operand is
        // the clause form, which `relations` reads on a declaration.
        if self.text() == "disjoint" && self.at(1).text != "from" && self.at_name_at(1) {
            let start = self.span_here();
            self.bump();
            let disjoined = self
                .type_ref()
                .ok_or("`disjoint` needs a type or feature")?;
            self.expect("from")?;
            let disjoining = self
                .type_ref()
                .ok_or("`disjoint … from` needs a type or feature")?;
            self.body();
            return Ok(Some(BodyMember::Relationship(Relationship::Disjoining {
                disjoined,
                disjoining,
                span: start.to(self.span_here()),
            })));
        }
        if self.text() == "featuring" {
            return Ok(Some(BodyMember::Relationship(self.featuring()?)));
        }
        if self.text() == "satisfy" {
            return Ok(Some(BodyMember::Relationship(self.satisfy()?)));
        }
        if self.text() == "filter" {
            let start = self.span_here();
            self.bump();
            let expr = self.filter_expr()?.ok_or_else(|| {
                format!(
                    "`filter` needs an expression, found {}",
                    self.describe_current()
                )
            })?;
            let end = self.span_here();
            self.eat(";");
            return Ok(Some(BodyMember::Filter {
                expr,
                span: start.to(end),
            }));
        }

        // Definitions and usages.
        let mut m = self.declaration()?;
        if let Some(multi) = leading_multiplicity {
            if let Some(BodyMember::Declaration(d)) = &mut m {
                if d.head.multiplicity.is_none() {
                    d.head.multiplicity = Some(multi);
                }
            }
        }
        Ok(m)
    }

    /// A filter's expression, which may name a metadata tag (`@Safety`) as an operand.
    fn filter_expr(&mut self) -> Result<Option<Expr>, String> {
        if self.text() == "@" {
            let start = self.span_here();
            self.bump();
            let Some(name) = self.type_ref() else {
                return Err("@ needs a metadata name".to_string());
            };
            let base = Expr::Opaque {
                text: format!("@{}", name.text),
                children: Vec::new(),
                span: start.to(name.span),
            };
            // The rest of the filter (`| @Security`, `.isMandatory`) is an ordinary operator run
            // whose left operand is the tag.
            return self.expr_from(base).map(Some);
        }
        self.expr()
    }

    /// Whether the `then` at the current position introduces a declaration rather than a
    /// succession target or an action node (`then assign …`, `then accept …`), which the
    /// statement path reads.
    fn then_introduces_declaration(&self) -> bool {
        let mut i = 1;
        if matches!(self.at(i).text.as_str(), "private" | "protected" | "public") {
            i += 1;
        }
        matches!(
            self.at(i).text.as_str(),
            "action"
                | "state"
                | "part"
                | "item"
                | "occurrence"
                | "calc"
                | "constraint"
                | "analysis"
                | "verification"
                | "use"
                | "event"
        )
    }

    fn visibility(&mut self) -> Option<Visibility> {
        match self.text() {
            "private" => {
                self.bump();
                Some(Visibility::Private)
            }
            "protected" => {
                self.bump();
                Some(Visibility::Protected)
            }
            "public" => {
                self.bump();
                Some(Visibility::Public)
            }
            _ => None,
        }
    }

    /// `doc /* ... */`, `comment /* ... */`, `@Tag { ... }`, `metadata T { ... }`.
    ///
    /// Returns `None` (having consumed nothing) when the current token does not start an
    /// annotation.
    fn annotation(&mut self) -> Result<Option<Annotation>, String> {
        let start = self.span_here();
        match self.text() {
            "doc" => {
                self.bump();
                let text = self.prose_after_keyword();
                Ok(Some(Annotation::Doc {
                    text,
                    span: start.to(self.span_here()),
                }))
            }
            "comment" | "rep" => {
                let keyword = self.bump().text;
                // `comment c about A, B /* … */` — the comment may be named and may say what it is
                // about (`Comment`, KerML 1.0 §8.2.3.5). The name and the targets are kept: a named
                // comment with an `about` clause is an element of its own, and its targets are the
                // annotations it states. The prose is not part of the model's meaning here.
                let name =
                    if self.at_name() && !matches!(self.text(), "about" | "locale" | "language") {
                        Some(self.name())
                    } else {
                        None
                    };
                let mut about = Vec::new();
                if self.eat("about") {
                    loop {
                        match self.type_ref() {
                            Some(target) => about.push(target),
                            None => break,
                        }
                        if !self.eat(",") {
                            break;
                        }
                    }
                }
                let text = self.prose_after_keyword();
                Ok(Some(Annotation::Prose {
                    keyword,
                    name,
                    about,
                    text,
                    span: start.to(self.span_here()),
                }))
            }
            "@" => {
                self.bump();
                let type_ref = self.type_ref().ok_or("`@` needs a metadata type name")?;
                self.metadata_rest(type_ref, start).map(Some)
            }
            // `#Tag` on its own is a keyword annotation with no body. It prefixes the member
            // that follows, so it contributes no element of its own: an annotation named
            // `fmeaspec` would otherwise shadow the `fmeaspec` short name the metadata
            // definition declares and make both references ambiguous.
            // `#Tag` is a metadata annotation written with the keyword form of `@` (KerML.xtext:1069
            // `PrefixMetadataAnnotation : '#' ownedRelatedElement += PrefixMetadataFeature`, and
            // `:1083` shows `'@' | 'metadata'` as the two other spellings of the same element). It
            // was lowered to prose, which dropped the type entirely: `#Classified #Security part z1;`
            // left `part z1` with no annotation at all (measured: `sysml outline`), so the §7.27.3
            // derivation had nothing to read and `SemanticMetadata_valid.sysml.xt:73,77,78` — a
            // `noErrors` model — reported three unresolved redefinitions.
            "#" => {
                self.bump();
                let type_ref = self.type_ref().ok_or("`#` needs a metadata type name")?;
                self.metadata_rest(type_ref, start).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// The `about ... { ... }` tail of a metadata annotation.
    fn metadata_rest(&mut self, type_ref: TypeRef, start: Span) -> Result<Annotation, String> {
        let mut about = Vec::new();
        if self.eat("about") {
            loop {
                match self.type_ref() {
                    Some(t) => about.push(t),
                    None => break,
                }
                if !self.eat(",") {
                    break;
                }
            }
        }
        let peek_before_body = self.text().to_string();
        // `#fmeaspec requirement req1 { … }` — a prefix annotation is followed by the declaration
        // it annotates, not by a terminator, so only `;` or a body belongs to the annotation.
        let args = if matches!(self.text(), ";" | "{") {
            self.body()
        } else {
            Vec::new()
        };
        // `body()` consumes a `;` and returns no members, so the terminator is gone by now. Recorded
        // here because it decides who is annotated: `@A;` is a member of its body and annotates the
        // containing element (SysML §7.27.2), while `@Safety { … } part def Brake;` is a
        // `MemberPrefix` of the declaration that follows (`KerML.xtext:384`).
        let self_terminated = peek_before_body == ";";
        // A `#B` that is not its own member prefixes the declaration that follows.
        self.prefix_annotated = !self_terminated;
        // `@Safety { level = 3 } about Brake` reads as the metadata annotation followed by two
        // declarations — `attribute about` and `attribute Brake` — because the grammar puts `about`
        // *before* the body. Silently inventing two members out of a misplaced clause is worse than
        // refusing the clause: the model then has attributes nobody wrote.
        if self.text() == "about" {
            return Err(format!(
                "`about` comes before the annotation's body: `@{} about … {{ … }}` (line {})",
                type_ref.text, start.line
            ));
        }
        Ok(Annotation::Metadata {
            type_ref,
            about,
            args,
            self_terminated,
            span: start.to(self.span_here()),
        })
    }

    /// The prosaic body of a `doc`/`comment`/`rep`: either a `/* ... */` block that the lexer
    /// skipped, a quoted string, or `language "..."`.
    ///
    /// The lexer drops comments, so the text here comes from a string literal when the author
    /// wrote one. A comment-only doc yields an empty string, which the lowering layer turns into
    /// `None` — the prose itself is not part of the model's meaning.
    fn prose_after_keyword(&mut self) -> String {
        if let Some(lang) = self.eat_any(&["language", "locale"]) {
            let _ = lang;
        }
        if matches!(self.kind(), TokenKind::String) {
            let t = self.bump().text;
            return t.trim_matches('"').to_string();
        }
        String::new()
    }
}

// -- declarations ------------------------------------------------------------

impl Parser {
    fn package_decl(&mut self, kind: Kind) -> Result<Declaration, String> {
        self.package_decl_with(kind, false)
    }

    /// `package_decl`, with the `standard` word recorded for `standard library package`.
    fn package_decl_with(&mut self, kind: Kind, is_standard: bool) -> Result<Declaration, String> {
        let start = self.span_here();
        // The keyword itself.
        match kind {
            Kind::Namespace => {
                self.bump();
            }
            Kind::LibraryPackage => {
                // Either `library package` (library consumed by caller) or plain `package`.
                if self.text() == "package" {
                    self.bump();
                }
            }
            _ => {
                self.bump();
            }
        }
        let name = self.name();
        let body = self.body();
        Ok(Declaration {
            kind,
            is_definition: true,
            bare: false,
            head: Head {
                name,
                prefixes: Prefixes {
                    is_standard,
                    ..Prefixes::default()
                },
                multiplicity: None,
                cross_name: None,
                cross_multiplicity: None,
                cross_relations: Vec::new(),
                relations: Vec::new(),
                from: None,
                to: None,
                extra_ends: Vec::new(),
                value: None,
                value_text: None,
                value_span: None,
                // No authored `default`: this `Head` is built for a form that writes no value clause.
                is_default: false,
                is_initial: false,
                span: start.to(self.span_here()),
            },
            doc: extract_doc(&body),
            body,
        })
    }

    /// Parse a definition or usage, or a statement that is not a declaration.
    fn declaration(&mut self) -> Result<Option<BodyMember>, String> {
        let start = self.span_here();
        // Where the member began, so an empty result can tell "consumed, produced nothing" (a doc
        // comment taken by its owner) from "nothing here is a member" (`???`). The second used to
        // return `Ok(None)` as well, which dropped the text silently: `sysml check` reported no
        // error for a model holding a token no production accepts.
        let pos_before = self.pos;

        // `then action b { … }` and `then private action d assign …` declare the action that
        // follows the previous one. The `then` statement used to read only a name here — taking
        // the kind keyword itself for it — and returned a reference to `action`, so the declared
        // action became an attribute beside a phantom reference and its body lost its owner.
        //
        // The `then` itself is an `EmptySuccession` (`SysML.xtext:874-881`): a succession with no
        // written ends, whose source is the feature before it and whose target is the declaration
        // after it. It goes ahead of the declaration as its own member; the lowering fills both ends
        // from the neighbours.
        if self.text() == "then" && self.then_introduces_declaration() {
            let then_span = self.span_here();
            self.bump();
            if let Some(visibility) = self.visibility() {
                self.pending_visibility = Some(visibility);
            }
            let declared = self.declaration();
            // Queued only after the declaration is read: queued before, the first member of the
            // declaration's own body took it, and the succession landed inside `b`. A declaration
            // that failed becomes an error member, and a succession into it would name nothing.
            if declared.is_err() {
                return declared;
            }
            self.pending_members
                .push(BodyMember::Statement(Statement::Succession {
                    name: Name::default(),
                    first: None,
                    then: None,
                    trigger: None,
                    trigger_kind: None,
                    trigger_type: None,
                    trigger_via: None,
                    guard: None,
                    effect: None,
                    implicit_source: true,
                    span: then_span,
                }));
            return declared;
        }

        // `variant part x : T` / `variant def V :> B` — the variant keyword introduces one
        // alternative of a variability point. It is a membership prefix, and the kind keyword
        // after it says what the alternative is.
        let is_variant = self.text() == "variant";
        if is_variant {
            self.bump();
        }
        // A leading run of prefixes, then the kind keyword (or none, for a default reference).
        let mut prefixes = self.prefixes();
        // The `variant` word belongs to the membership, not to the kind: `variant part a` is a
        // part usage, and the lowering reads the flag.
        prefixes.is_variant = is_variant;
        let is_definition = self.text() == "def" || (self.at(1).text == "def");
        // `#B def C1 { … }` — an `ExtendedDefinition`: extension keywords, then `def` with no kind
        // (`SysML.xtext:724-725`). The prefix annotation was read as the previous member.
        let bare_def = self.text() == "def";
        // `#B f1 { … }` — an `ExtendedUsage` (`SysML.xtext:728-729`): a usage with no kind keyword
        // whose prefix annotation is its extension keyword.
        let extended = std::mem::take(&mut self.prefix_annotated);
        if self.text() == "def" {
            // `individual def X;` puts `def` first.
            self.bump();
        }

        // Statements that live in bodies and are not declarations.
        //
        // `end` forces a feature declaration, so a name that also begins a statement is still the
        // end's own name: `end first : T :>> left` writes an end called `first`, not a `first …
        // then …` succession. Dispatching the statement run first read it as the latter, and the
        // owned end disappeared from the association.
        if !prefixes.is_end {
            if let Some(stmt) = self.statement(&prefixes, start)? {
                return Ok(Some(BodyMember::Statement(stmt)));
            }
        }

        // A quoted operator name (`function '+'`) is a declaration whose kind keyword was already
        // consumed. A bare quoted name is a default reference.
        // `variant highPerformanceRequirement;` — a variant with no kind keyword. The word names
        // the membership, not the feature's kind, so the member keeps whatever kind the enclosing
        // declaration implies; landing in the no-kind path made it an ordinary attribute and a
        // variation body then looked like it held a non-variant.
        // The same spelling with a body (`variant b2 { ... }`) is the other shape and used to
        // fall through to the no-kind branch, which made it an ordinary attribute — so the
        // variation read as holding a non-variant and the rule that says otherwise fired on a
        // conforming model (`valid_Variability.sysml.xt:61`).
        // A kind keyword after `variant` (`variant part a : B`) is not this shape: the word that
        // follows names the member's kind and the member's own name comes after it.
        if is_variant && self.at_name() && !self.at_name_at(1) {
            let name = self.name();
            // `variant b1;` ends here; `variant b2 { ... }` carries the variant's own members,
            // and reading the `;` unconditionally left the brace to the enclosing body.
            let body = self.body();
            return Ok(Some(BodyMember::Declaration(Box::new(Declaration {
                kind: Kind::Kerml("variant".to_string()),
                is_definition: false,
                bare: false,
                doc: extract_doc(&body),
                head: Head {
                    name,
                    prefixes,
                    multiplicity: None,
                    cross_name: None,
                    cross_multiplicity: None,
                    cross_relations: Vec::new(),
                    relations: Vec::new(),
                    from: None,
                    to: None,
                    extra_ends: Vec::new(),
                    value: None,
                    value_text: None,
                    value_span: None,
                    // No authored `default`: this `Head` is built for a form that writes no value clause.
                    is_default: false,
                    is_initial: false,
                    span: start.to(self.span_here()),
                },
                body,
            }))));
        }
        let Some(kind) = self.declaration_kind() else {
            // No kind keyword. Three shapes reach here: a default reference
            // (`mass : MassValue;`), a bare expression that is a body's terminal value, and a
            // member this grammar does not know. A name followed by `:`/`=` is a reference; a
            // name followed by `;` or an operator is an expression.
            // A single name followed by `;` is a declaration, not an expression. `enum def M {
            // a; b; }` declares two values; reading them as expressions left the enumeration
            // unable to name any of its own values.
            if self.at_expr_start()
                && !(self.at_name() && self.at(1).text == ";")
                && !matches!(self.at(1).text.as_str(), ":" | ":>" | ":>>" | "=")
            {
                let before = self.pos;
                if let Some(expr) = self.expr()? {
                    // A trailing operator run means this was not a complete expression.
                    if self.text() == ";" || self.text() == "}" || self.is_eof() {
                        self.eat(";");
                        return Ok(Some(BodyMember::Statement(Statement::Expression {
                            expr,
                            span: start.to(self.span_here()),
                        })));
                    }
                }
                self.pos = before;
            }
            let mut head = self.head(prefixes, None)?;
            if head.name.is_empty() && head.relations.is_empty() && head.value.is_none() {
                // A stray `;` ends an empty member; it is not unreadable input. Consuming it here
                // is what lets the recovery in `member()` tell the two apart.
                if self.text() == ";" {
                    self.bump();
                    return Ok(None);
                }
                // Nothing was recognised and nothing was consumed: this token starts no member.
                // Reporting it is what keeps the module's promise that parsing never silently
                // drops input; `member()` turns the error into a visible marker.
                if self.pos == pos_before && !self.is_eof() && self.text() != "}" {
                    return Err(format!(
                        "expected a declaration, found `{}` (line {})",
                        self.text(),
                        self.cur().line
                    ));
                }
                return Ok(None);
            }
            // `render X` is an `OwnedReferenceSubsetting`, not a declaration: `ViewRenderingUsage`
            // is `ownedRelationship += OwnedReferenceSubsetting FeatureSpecializationPart?
            // UsageBody` (`SysML.xtext:2340-2343`), and only the other alternative (`rendering
            // Usage`) declares a name — that one arrives through the kind keyword above, not here.
            //
            // The edge matters because the body redefines what the referenced rendering provides:
            // `render asElementTable { view :>> columnView[1] { … } }` names `columnView`, which the
            // library's `rendering asElementTable : TabularRendering` declares.
            if head.prefixes.is_render
                && !head.name.is_empty()
                && !head.relations.iter().any(|r| r.op == RelationOp::Subsets)
            {
                let text = head.name.display();
                let span = head.name.span;
                head.relations.push(Relation {
                    op: RelationOp::Subsets,
                    targets: vec![TypeRef { text, span }],
                    span,
                });
            }
            let body = self.body();
            // Keep a bare end distinct from an explicitly written `end port` or `end feature`.
            // Its owner decides the metaclass during lowering: interface ends are PortUsages,
            // whereas general connector and association ends are Features. Guessing Port here
            // made connection ends inherit Ports::Port even though no port keyword was written.
            // `render r;` writes no kind word either, but its prefix fixes the rendering kind.
            //
            // An extended definition or usage is a plain `Definition` or `Usage`. Read as an
            // attribute it gained `Base::DataValue` as a default supertype, which beside the
            // semantic-metadata base type made `self` look inherited twice
            // (`SemanticMetadata_valid.sysml.xt:72,76`, a `noErrors` model the kernel warned on).
            let bare_kind = if head.prefixes.is_end {
                Kind::Kerml("end".to_string())
            } else if head.prefixes.is_render {
                Kind::Rendering
            } else if bare_def {
                Kind::Kerml("classifier".to_string())
            } else if extended {
                Kind::Kerml("feature".to_string())
            } else {
                Kind::Attribute
            };
            return Ok(Some(BodyMember::Declaration(Box::new(Declaration {
                bare: matches!(bare_kind, Kind::Attribute),
                kind: bare_kind,
                is_definition: bare_def,
                head,
                doc: extract_doc(&body),
                body,
            }))));
        };

        // `actor #B a;` — extension keywords may follow the kind keyword (`'actor'
        // UsageExtensionKeyword* Usage`, `SysML.xtext:2095`). Read as the end of the declaration
        // they left `a` as a phantom attribute. They annotate the declaration, so they go into its
        // body the way a prefix annotation's lowering puts them.
        let mut extension_annotations = Vec::new();
        while self.text() == "#" {
            if let Some(annotation) = self.annotation()? {
                extension_annotations.push(BodyMember::Annotation(annotation));
            }
        }
        self.prefix_annotated = false;

        // `enum nominal;` inside an enum definition declares a value. The `enum` keyword here is
        // the value's introducer, not a nested enumeration definition.
        let _is_enum_value = matches!(kind, Kind::Enumeration) && !is_definition;
        // `part def X` — the `def` follows the kind keyword.
        let is_def = is_definition
            || self.text() == "def"
            || matches!(&kind, Kind::Kerml(keyword) if keyword == "assoc struct");
        if self.text() == "def" {
            self.bump();
        }

        // `EnumerationDefinition` takes no `BasicDefinitionPrefix` (`SysML.xtext:767-770`), so its
        // only prefixes are extension keywords. `abstract enum def` and `variation enum def` are not
        // spellings the grammar admits. They are a syntax error here, not a flag for the lowering
        // to drop. An enumeration usage still takes both (`UsagePrefix`).
        if matches!(kind, Kind::Enumeration) && is_def {
            let word = if prefixes.is_abstract {
                Some("abstract")
            } else if prefixes.is_variation {
                Some("variation")
            } else {
                None
            };
            if let Some(word) = word {
                self.pending_visibility = None;
                return Err(format!(
                    "an enumeration definition takes no `{word}` prefix (line {})",
                    self.line()
                ));
            }
        }
        // `interface a.b to c.d;` is the anonymous spelling of a connection: two feature paths
        // joined by `to`, with neither a name nor a type. Reading it as a declaration took the
        // first path segment for the element's name and left every later segment to be read as
        // a separate member, so `rearAxle` inherited a phantom `attribute rearAxle` and the
        // connection end resolved as ambiguous.
        if matches!(kind, Kind::Interface) && !is_def && self.at_name() && self.at(1).text == "." {
            let from = self.type_ref().map(|t| t.text).unwrap_or_default();
            if self.eat("to") {
                let to = self.type_ref().map(|t| t.text).unwrap_or_default();
                self.body();
                self.eat(";");
                return Ok(Some(BodyMember::Statement(Statement::Connect {
                    name: Name::default(),
                    type_ref: None,
                    from,
                    to,
                    extra: Vec::new(),
                    span: start.to(self.span_here()),
                })));
            }
            // No `to`: not the shorthand after all. Put the path back is not possible, so the
            // caller's error member reports it with the position it reached.
            return Err(format!(
                "expected `to` after an interface end, found {}",
                self.describe_current()
            ));
        }
        // `flow a.b to c.d;` — the flow's own name is optional and neither end is it. Reading
        // the first segment as the declaration's name put a `Flow` element under the name of the
        // action it starts from, so `'assemble transmission into vehicle'.transmission` resolved
        // to both the action and the flow.
        //
        // A *named* flow (`abstract flow flowTransfers: FlowTransfer[0..*] ... { ... }`) is an
        // ordinary declaration and keeps its `:` curve; only the connector form, where a name is
        // followed directly by `to`, is a statement.
        if matches!(kind, Kind::Flow) && !is_def && self.is_flow_connector() {
            let mut prefixes = prefixes;
            prefixes.visibility = self.pending_visibility.take();
            return self
                .flow_statement(start, prefixes)
                .map(|s| Some(BodyMember::Statement(s)));
        }
        // `event a;` / `event a.areal;` — no `occurrence` after the keyword means the reference
        // form, which declares nothing: `EventOccurrenceUsage` is
        // `'event' ( OwnedReferenceSubsetting FeatureSpecializationPart? |
        // OccurrenceUsageKeyword UsageDeclaration? )` (`SysML.xtext:865-870`), and
        // `OccurrenceUsageKeyword` is the single word `occurrence` (`SysML.xtext:832-834`). So
        // `event occurrence e : E;` is a declaration and `event e;` is a reference to `e`.
        if matches!(kind, Kind::Event) && !is_def {
            if self.text() == "occurrence" {
                // The second alternative states the kind again before the declaration. Read as
                // the name, the word gave every such event the name `occurrence` and left the
                // one it declares to become a member of its own — `17a-Sequence-Modeling.sysml`
                // gained an `attribute publish_source_event` beside each event.
                self.bump();
            } else {
                return self
                    .event_reference(start)
                    .map(|s| Some(BodyMember::Statement(s)));
            }
        }
        // `variant part a` keeps `Kind::Part`, and the `is_variant` prefix above records the
        // membership. Overwriting the kind here was what lost the alternative's own kind.
        // `assoc all BinaryLink specializes Link` — `all` *after* the kind keyword marks the type
        // sufficient too (`isSufficient ?= 'all'`, `KerML.xtext:324`, `:468`). The prefix run at the
        // top of this function accepts `all` before the keyword (`all conn def C;`); after it the
        // word was read as the declaration's name, so each of the kernel library's `assoc all` /
        // `struct all` / `class all` / `feature all` declarations arrived as an element called
        // `all` beside a nameless one — `Links::BinaryLink`, `Objects::Body`, `Point`, `Surface`,
        // `Curve` and `Occurrences::Life` all lost the kind and the supertype they name.
        if self.text() == "all" && !matches!(self.at(1).kind, TokenKind::Punct) {
            self.bump();
            prefixes.is_sufficient = true;
        }
        // The visibility keyword was read before the kind keyword; the declaration carries it.
        let mut prefixes = prefixes;
        prefixes.visibility = self.pending_visibility.take();
        // `connector [0..1] transitionLink to [1..*] trigger;` writes only the two ends: the
        // declaration before `from` is optional and so is `from` itself (`BinaryConnectorDeclaration`,
        // `KerML.xtext`). Read as a head, the first end's bound became the connector's and both ends
        // became phantom members (`TransitionPerformances.kerml:35`).
        // `then action accept engineOff : EngineOff;` — an action node with no declaration of its
        // own (`ActionNodeUsageDeclaration?`). Read as a head, `accept` became the action's name.
        let anonymous_node = matches!(kind, Kind::Action)
            && !is_def
            && matches!(self.text(), "accept" | "send" | "assign");
        let head = if anonymous_node {
            Head {
                prefixes,
                span: self.span_here(),
                ..Head::default()
            }
        } else if matches!(&kind, Kind::Kerml(k) if k == "connector")
            && !is_def
            && self.at_bare_connector_ends()
        {
            let head_start = self.span_here();
            let from = self.connector_end()?;
            self.expect("to")?;
            let to = self.connector_end()?;
            Head {
                prefixes,
                from: Some(from),
                to: Some(to),
                span: head_start.to(self.span_here()),
                ..Head::default()
            }
        } else {
            self.head(prefixes, if is_def { Some(kind.clone()) } else { None })?
        };
        let mut body = self.pending_parameters.drain(..).collect::<Vec<_>>();
        body.extend(extension_annotations);
        // `action engineStarted accept engineStart : EngineStart;`, `action a assign x := 1;` and
        // `action loop while c { … }` are action nodes whose declaration comes first
        // (`ActionNodeUsageDeclaration? 'accept' …`, `SysML.xtext:1434-1447`, `:1536-1540`,
        // `:1642`). The node is kept as the action's own content; reading only the declaration made
        // the node a sibling named after its keyword.
        let node_start = self.span_here();
        // `metadata t : T about t;` — a metadata usage names what it is about before its body
        // (`SysML.xtext:141-148`). The targets become the first member of the body, where lowering
        // reads them as the element's `to`.
        if matches!(kind, Kind::Metadata) && self.text() == "about" {
            let about_start = self.span_here();
            self.bump();
            let mut targets = Vec::new();
            while let Some(target) = self.type_ref() {
                targets.push(target);
                if !self.eat(",") {
                    break;
                }
            }
            body.insert(
                0,
                BodyMember::Annotation(Annotation::About {
                    targets,
                    span: about_start.to(self.span_here()),
                }),
            );
        }
        if matches!(kind, Kind::Action) && !is_def && matches!(self.text(), "accept" | "send") {
            body.push(BodyMember::Statement(self.message(node_start, true)?));
        } else if matches!(kind, Kind::Action) && !is_def && self.text() == "assign" {
            body.push(BodyMember::Statement(self.assign_statement(node_start)?));
        } else if matches!(kind, Kind::Action)
            && !is_def
            && matches!(self.text(), "if" | "while" | "loop" | "for")
        {
            body.push(BodyMember::Statement(self.control_flow(node_start)?));
        } else {
            body.extend(self.body());
        }
        Ok(Some(BodyMember::Declaration(Box::new(Declaration {
            kind,
            is_definition: is_def,
            bare: false,
            head,
            doc: extract_doc(&body),
            body,
        }))))
    }

    /// The declaration's body, including a parenthesised parameter list before it.
    ///
    /// `ref action 'provide power' (in fuelCmd = ...) :> ...` writes its parameters between the
    /// name and the specialization clause. The grammar states no production for that form, but
    /// the official validation corpus writes it, and leaving the group unread put every parameter
    /// into the enclosing body as a sibling: `action X (in f = 1)` declared `f` on the package.
    /// Read here, they become what they are — members of the declaration.
    fn parameter_members(&mut self) -> Vec<BodyMember> {
        if self.text() != "(" {
            return Vec::new();
        }
        self.bump();
        let mut members = Vec::new();
        while !self.is_eof() {
            if self.text() == ")" {
                self.bump();
                break;
            }
            // Parameters are comma separated: `(in a = 1, out b)`. The comma belongs to
            // the list, not to a member, so it is consumed here.
            if self.text() == "," {
                self.bump();
                continue;
            }
            let before = self.pos;
            let member = self.member(false);
            members.append(&mut self.pending_members);
            match member {
                Some(member) => members.push(member),
                None => break,
            }
            if self.pos == before {
                break;
            }
        }
        members
    }

    /// The kind keyword at the current position, consuming it.
    ///
    /// Multi-word kinds (`analysis case`, `use case`, `verification`, `individual`) are matched
    /// before their prefixes so `use case def` is one kind, not `use` plus `case`.
    fn declaration_kind(&mut self) -> Option<Kind> {
        // Multi-word spellings first. `assoc struct` is one AssociationStructure,
        // not an association named `struct` followed by a second declaration.
        if self.text() == "assoc" && self.at(1).text == "struct" {
            self.bump();
            self.bump();
            return Some(Kind::Kerml("assoc struct".to_string()));
        }
        if self.text() == "analysis" {
            self.bump();
            // `analysis case` is the two-word spelling; a bare `analysis def` is the same kind.
            self.eat("case");
            return Some(Kind::AnalysisCase);
        }
        if self.text() == "use" && self.at(1).text == "case" {
            self.bump();
            self.bump();
            return Some(Kind::UseCase);
        }
        let kind = match self.text() {
            "part" => Kind::Part,
            // `actor driver : Person;` — a participant of a use case.
            "actor" => Kind::Actor,
            // `stakeholder owner;` — the word introduces a membership, and without an arm the
            // word and the name after it fell into the body as two attributes.
            "stakeholder" => Kind::Stakeholder,
            "attribute" => Kind::Attribute,
            "port" => Kind::Port,
            "item" => Kind::Item,
            "connection" => Kind::Connection,
            "interface" => Kind::Interface,
            "flow" => Kind::Flow,
            // `message m : F from b to c` is a `FlowUsage` in the metamodel
            // (`Message returns SysML::FlowUsage`, `SysML.xtext:1245-1247`), not a new keyword
            // class. Without this arm the word became a member name and its ends spilled into
            // the body as `from`, `b`, `to`.
            "message" => Kind::Flow,
            "allocation" => Kind::Allocation,
            "action" => Kind::Action,
            "state" => Kind::State,
            "calc" | "calculation" => Kind::Calculation,
            "constraint" => Kind::Constraint,
            "requirement" => Kind::Requirement,
            // `verify r1 : R;` — a requirement verification. The word was unknown, so the whole
            // declaration fell apart into two attributes and the requirement it verifies was lost.
            "verify" => Kind::Verification,
            "concern" => Kind::Concern,
            "case" => Kind::Case,
            "verification" => Kind::VerificationCase,
            "view" => Kind::View,
            "viewpoint" => Kind::Viewpoint,
            "rendering" => Kind::Rendering,
            "metadata" => Kind::Metadata,
            "enum" => Kind::Enumeration,
            "occurrence" => Kind::Occurrence,
            "event" => Kind::Event,
            "individual" => Kind::Individual,
            "dependency" => Kind::Dependency,
            // KerML classifiers.
            "class" | "datatype" | "struct" | "assoc" | "association" | "metaclass"
            | "behavior" | "function" | "predicate" | "interaction" | "multiplicity"
            | "classifier" | "step" | "feature" | "expr" | "bool" | "connector" | "binding" => {
                let k = self.text().to_string();
                Kind::Kerml(k)
            }
            _ => return None,
        };
        self.bump();
        Some(kind)
    }

    /// The declaration half of a definition or usage: name, multiplicity, relations, value.
    ///
    /// `definition_kind` being `Some` means this is a definition, where a bare name and a
    /// specialization clause are legal and a typing clause is not.
    fn head(&mut self, prefixes: Prefixes, definition_kind: Option<Kind>) -> Result<Head, String> {
        let start = self.span_here();
        // A `ref`/`end` prefix can also appear after the kind keyword (`end occurrence source`).
        let mut prefixes = prefixes;
        loop {
            match self.text() {
                // Same reason as the run above the kind keyword: `ref :>> x` has no name, and the
                // operator is what says so.
                "ref" if !matches!(self.at(1).kind, TokenKind::Punct) || self.at_relation_op(1) => {
                    prefixes.is_ref = true;
                    self.bump();
                }
                "end" if !matches!(self.at(1).kind, TokenKind::Punct) || self.at(1).text == ":" => {
                    prefixes.is_end = true;
                    self.bump();
                }
                // `attribute var y : Integer` — the modifier may follow the kind keyword as well
                // as precede it. Without this arm `var` was taken for the name, and the feature it
                // introduced was lost: two elements where the model wrote one.
                "var" if !matches!(self.at(1).kind, TokenKind::Punct) => {
                    prefixes.is_variable = true;
                    self.bump();
                }
                // `part snapshot p : A` — the portion word follows the kind keyword too, and
                // without this arm it became a member of its own with the real feature beside it.
                // `verify requirement mr : R` — `requirement` names the kind the verification is
                // about; the member's own name comes after it. Taking the word for the name left
                // every verification called `requirement` and lost the one it verifies.
                "requirement" if self.at(1).text != ":" && self.at_name_at(1) => {
                    self.bump();
                }
                "snapshot" | "timeslice" if !matches!(self.at(1).kind, TokenKind::Punct) => {
                    prefixes.portion = Some(self.bump().text);
                }
                // `state parallel s` — how the state's substates run. The word sits between the
                // kind keyword and the name; without this arm it became the state's name.
                "parallel" if !matches!(self.at(1).kind, TokenKind::Punct) => {
                    prefixes.is_parallel = true;
                    self.bump();
                }
                _ => break,
            }
        }
        // `interface connect a.p to b.q;` declares no name: `connect` opens the connector part
        // (`SysML.xtext:1052-1062`). Taken as the name, it left both ends as phantom members.
        let mut name = if matches!(self.text(), "connect" | "allocate")
            && (self.at_name_at(1)
                || self.at(1).text == "["
                || (self.text() == "connect" && self.at(1).text == "("))
        {
            Name::default()
        } else {
            self.name()
        };
        // `part def ::` — a `::` where a name belongs is a broken declaration, not an anonymous
        // one. Reporting it keeps the member from being silently read as something it is not.
        //
        // `::>` is the exception: an unnamed end that redefines something writes it directly
        // (`end part ::> tankAssy.fuel;`, `InterfaceUsage_Invalid.sysml.xt:79`). The name is
        // genuinely absent there — the feature is the redefined one — so the declaration goes on
        // and the redefinition supplies its identity.
        if self.text() == "::" && self.at(1).text != ">" {
            return Err(format!(
                "expected a name, found `::` (line {})",
                self.line()
            ));
        }
        // `end [0..1] part hub` / `end connectingHub [0..1] part hub` — the prefix run read the
        // cross feature's name and bound and left them here.
        let cross_name = self.pending_cross_name.take();
        let cross_multiplicity = self.pending_cross_multiplicity.take();
        let cross_relations = std::mem::take(&mut self.pending_cross_relations);
        let multiplicity = self.multiplicity();
        // `ordered` / `nonunique` modify the multiplicity and belong to the declaration. Reading
        // them as separate members put a stray `ordered` attribute into every body that used one.
        // `parallel` / `exclusive` are the same kind of modifier on a state: they say how its
        // substates run, and leaving them out put a stray `parallel` attribute in the body.
        while matches!(
            self.text(),
            "ordered" | "nonunique" | "parallel" | "exclusive"
        ) {
            // `state parallel s` — how a state's substates run. `real[2] nonunique` — whether a
            // feature's values may repeat. Both were consumed without a record, so no rule could
            // ask about either.
            match self.text() {
                "parallel" => prefixes.is_parallel = true,
                "ordered" => prefixes.is_ordered = true,
                "nonunique" => prefixes.is_nonunique = true,
                _ => {}
            }
            self.bump();
        }
        // `ref action X (in a = 1) :> B` writes its parameters between the name and the
        // specialization clause. Reading them here, before `relations`, is what keeps the `:>`
        // clause attached to the declaration: left until after, the clause had no owner and its
        // targets became an anonymous member of the enclosing body.
        self.pending_parameters = self.parameter_members();
        let mut relations = self.relations(definition_kind.is_some());
        // `connector a : A from x to y;` and the named flow `flow f : F from a to b { … }` write
        // their ends after the head, not after a `connect` keyword. Left to the body they became
        // four members — `from`, `x`, `to`, `y` — and the two ends then counted as declared twice
        // in the enclosing namespace
        // (`ConnectorTest_NestedConnectorConnectingEndsGoodCase.kerml.xt:34`).
        // `connection bc : BC connect b to c;` — the `connect` clause belongs to the declaration
        // head (`SysML.xtext:1052-1062`: `ConnectionUsageKeyword UsageDeclaration? ValuePart?
        // ( ConnectorKeyword ConnectorPart )?`), and `allocation :P allocate b to c;` is the same
        // shape with `allocate` (`SysML.xtext:1218-1221`). Left unread the clause became a
        // *sibling* of the declaration — a phantom member named after the keyword, and a
        // connector or allocation with no ends at all (`ConnectionUsage_Invalid.sysml.xt:54`,
        // `AllocationUsage_Invalid.sysml.xt:53`).
        let (from, to, extra_ends) = if matches!(self.text(), "connect" | "allocate") {
            let connect = self.text() == "connect";
            self.bump();
            if connect && self.text() == "(" {
                let mut ends = self.nary_connector_ends()?.into_iter();
                (ends.next(), ends.next(), ends.collect::<Vec<String>>())
            } else {
                let first = self.connector_head_end();
                let second = if self.eat("to") {
                    self.connector_head_end()
                } else {
                    None
                };
                (first, second, Vec::new())
            }
        } else if matches!(self.text(), "from" | "to") {
            let explicit_from = self.eat("from");
            let first = self.connector_head_end();
            let second = if self.eat("to") {
                self.connector_head_end()
            } else {
                None
            };
            // `flow a.b to c.d` writes only the target; `flow from a.b to c.d` writes both.
            if explicit_from {
                (first, second, Vec::new())
            } else {
                (None, first, Vec::new())
            }
        } else {
            (None, None, Vec::new())
        };
        // `Real[2] ordered` — the modifiers follow the type clause.
        while matches!(
            self.text(),
            "ordered" | "nonunique" | "parallel" | "exclusive"
        ) {
            match self.text() {
                "parallel" => prefixes.is_parallel = true,
                "ordered" => prefixes.is_ordered = true,
                "nonunique" => prefixes.is_nonunique = true,
                _ => {}
            }
            self.bump();
        }
        // `FeatureSpecializationPart` is `(FeatureSpecialization)+ MultiplicityPart?
        // FeatureSpecialization*` (`KerML.xtext:573-576`): a clause written *after* the
        // multiplicity part is another specialization, not a body member.
        // `part rearWheel_1 : Wheel[2] nonunique subsets rearWheel;` read as a typing plus a
        // modifier, and `subsets rearWheel` then fell into the enclosing body — the feature had no
        // subsetting at all, so the uniqueness rule had nothing to compare and stayed silent
        // (`Subsetting_UniquenessConformance_Invalid.sysml.xt:39`). The same line with the
        // modifier first (`part rearWheel_2[0..1] nonunique subsets rearWheel;`) was already read,
        // which is why one line of that model matched and the other did not.
        relations.extend(self.relations(definition_kind.is_some()));
        // `= v`, `default = v` and `default v` all give a feature its value; `default` is the
        // spelling a redefinition uses to say "this value applies unless overridden".
        // `port :>> fp;` redefines an existing feature and takes its name from the redefinition.
        // The declaration is anonymous on the left by design; leaving the name empty made the
        // redefined feature unreachable from any predicate.
        if name.is_empty() {
            if let Some(rel) = relations.iter().find(|r| r.op == RelationOp::Redefines) {
                if let Some(target) = rel.targets.first() {
                    // The target may carry a multiplicity (`part :>> part3[0..1]`). That bound
                    // belongs to the feature, not to the name it takes: keeping it made the
                    // redefined part reachable under `part3[0..1]` and under no other spelling.
                    let leaf = target
                        .text
                        .rsplit("::")
                        .next()
                        .unwrap_or(&target.text)
                        .to_string();
                    name = Name {
                        text: leaf
                            .split('[')
                            .next()
                            .unwrap_or(&leaf)
                            .trim_end()
                            .to_string(),
                        short: None,
                        span: target.span,
                    };
                }
            }
        }
        let mut head = Head {
            name,
            prefixes,
            multiplicity,
            cross_name,
            cross_multiplicity,
            cross_relations,
            relations,
            from,
            to,
            extra_ends,
            ..Head::default()
        };
        self.value_part(&mut head)?;
        head.span = start.to(self.span_here());
        Ok(head)
    }

    fn value_part(&mut self, head: &mut Head) -> Result<bool, String> {
        if !matches!(self.text(), "=" | ":=" | "default") {
            return Ok(false);
        }
        let start = self.span_here();
        head.is_default = self.eat("default");
        head.is_initial = self.eat(":=");
        if !head.is_initial {
            self.eat("=");
        }
        // A value beginning at `{` is an expression body, not the feature's own body.
        let value = if self.text() == "{" {
            self.braced_expression()?
        } else {
            self.expr()?
        }
        .ok_or("expected an expression in a value clause")?;
        head.value_span = Some(start.to(value.span()));
        head.value_text = Some(format!("= {}", render_expr(&value)));
        head.value = Some(value);
        Ok(true)
    }

    fn multiplicity(&mut self) -> Option<Multiplicity> {
        if self.text() != "[" {
            return None;
        }
        let start = self.span_here();
        self.bump();
        let mut text = String::from("[");
        while !self.is_eof() && self.text() != "]" {
            text.push_str(self.text());
            self.bump();
        }
        self.eat("]");
        text.push(']');
        Some(Multiplicity {
            text,
            span: start.to(self.span_here()),
        })
    }

    /// Every relationship clause on a declaration, in authored order.
    fn relations(&mut self, is_definition: bool) -> Vec<Relation> {
        let mut out = Vec::new();
        loop {
            let start = self.span_here();

            let op = match self.text() {
                ":" => RelationOp::Typing,
                ":>" => RelationOp::Specializes,
                ":>>" => RelationOp::Redefines,
                "::>" => RelationOp::References,
                "crosses" => RelationOp::Crosses,
                "typed" if self.at(1).text == "by" => {
                    self.bump();
                    RelationOp::Typing
                }
                "defined" if self.at(1).text == "by" => {
                    self.bump();
                    RelationOp::Typing
                }
                "specializes" => RelationOp::Specializes,
                "subsets" | "subset" => RelationOp::Subsets,
                "redefines" => RelationOp::Redefines,
                "references" => RelationOp::References,
                // The type-relationship parts (`KerML.xtext:340-365`). They follow the
                // specialization clause as part of the declaration head, not as body members, so
                // leaving them out put `unions` and its target into the body as two stray
                // attributes — a model gained two members it never wrote.
                "unions" => RelationOp::Unions,
                "intersects" => RelationOp::Intersects,
                "differences" => RelationOp::Differences,
                "disjoint" if self.at(1).text == "from" => {
                    self.bump();
                    RelationOp::Disjoints
                }
                "conjugates" => RelationOp::Conjugates,
                // `feature f chains a.b` — the feature chain a feature reaches through
                // (`KerML.xtext:560`). Part of the declaration head, and without it the word and
                // its path fell into the body as separate members.
                "chains" => RelationOp::Chains,
                // `feature f inverse of B::g` and `feature gg featured by B` — the two KerML
                // feature-relationship parts (`KerML.xtext:563-570`). Without them the words and
                // their paths fell into the body as separate members, which made the enclosing
                // name look declared several times over.
                "inverse" if self.at(1).text == "of" => {
                    self.bump();
                    self.bump();
                    let _ = self.type_ref();
                    continue;
                }
                "featured" if self.at(1).text == "by" => {
                    self.bump();
                    self.bump();
                    while self.type_ref().is_some() && self.eat(",") {}
                    continue;
                }
                "~" if self.at_name_at(1) => RelationOp::Conjugates,
                _ => break,
            };
            // A definition's `:` clause names a supertype; a usage's names a type.
            let op = if is_definition && op == RelationOp::Typing {
                RelationOp::Specializes
            } else {
                op
            };
            self.bump();
            // `: ~FuelPort` — the conjugation sits between the operator and the target. It is a
            // statement about the type, not about the operator, so it becomes its own relation:
            // eating it silently left `port p : ~FuelPort` indistinguishable from `port p :
            // FuelPort`, and the canvas drew the conjugated port as an ordinary one.
            let conjugated = self.eat("~");
            let mut targets = Vec::new();
            loop {
                match self.type_ref() {
                    Some(t) => targets.push(t),
                    None => break,
                }
                if !self.eat(",") {
                    break;
                }
            }
            if targets.is_empty() {
                break;
            }
            if conjugated {
                out.push(Relation {
                    op: RelationOp::Conjugates,
                    targets: targets.clone(),
                    span: start.to(self.span_here()),
                });
            }
            out.push(Relation {
                op,
                targets,
                span: start.to(self.span_here()),
            });
        }
        out
    }
}

/// The documentation attached to an element: the first `doc` member's non-empty text.
fn extract_doc(body: &[BodyMember]) -> Option<String> {
    body.iter().find_map(|m| match m {
        BodyMember::Annotation(Annotation::Doc { text, .. }) if !text.is_empty() => {
            Some(text.clone())
        }
        _ => None,
    })
}

// -- expressions -------------------------------------------------------------

impl Parser {
    /// Parse an expression, or nothing when the current token cannot start one.
    ///
    /// Precedence follows KerML: `implies` < `or`/`xor` < `and` < equality < relational <
    /// additive < multiplicative < exponent < unary < postfix. Separate methods rather than a
    /// table, because the operators are few and a hand-written level gives a better message.
    pub(crate) fn expr(&mut self) -> Result<Option<Expr>, String> {
        if !self.at_expr_start() {
            return Ok(None);
        }
        self.enter()?;
        let e = self.expr_null_coalescing();
        self.leave();
        e.map(Some)
    }

    /// `a ?? b` — `NullCoalescingExpression`, the loosest binary operator (KerML 1.0 §8.2.5.8.1).
    /// `collection->reduce '+' ?? zero` in the function library stopped at `??` without it, and
    /// `?`, `?` and `zero` became members of the function.
    fn expr_null_coalescing(&mut self) -> Result<Expr, String> {
        let mut lhs = self.expr_implies()?;
        while self.text() == "??" {
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_implies()?;
            lhs = Expr::Binary {
                op: "??".to_string(),
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn at_expr_start(&self) -> bool {
        self.at_expr_start_at(0)
    }

    /// Whether the token at `offset` can start an expression.
    fn at_expr_start_at(&self, offset: usize) -> bool {
        let t = self.at(offset);
        // A reserved word names a feature when it is a path head or a whole argument:
        // `frame.localClock`, and `middle->excluding(do)` (`StatePerformances.kerml:41`), where `do`
        // is the state's `do` action.
        matches!(
            t.kind,
            TokenKind::Number | TokenKind::String | TokenKind::Ident | TokenKind::QuotedName
        ) || (matches!(t.kind, TokenKind::Keyword)
            && matches!(self.at(offset + 1).text.as_str(), "." | "::")
            || matches!(t.kind, TokenKind::Keyword)
                && matches!(self.at(offset + 1).text.as_str(), "," | ")")
                && (self.pos + offset)
                    .checked_sub(1)
                    .and_then(|prev| self.tokens.get(prev))
                    .is_some_and(|prev| matches!(prev.text.as_str(), "(" | ",")))
            || matches!(
                t.text.as_str(),
                "(" | "["
                    | "-"
                    | "+"
                    | "not"
                    | "~"
                    | "all"
                    | "true"
                    | "false"
                    | "null"
                    | "if"
                    | "@"
                    | "#"
                    | "new"
            )
    }

    /// An expression in condition position, stopping before the `?` that starts the branches.
    ///
    /// The ordinary ladder never consumes a bare `?`, so this is the same parse; the method
    /// exists to name the intent and to keep `if` from being treated as a ternary start.
    fn cond_expr(&mut self) -> Result<Option<Expr>, String> {
        self.expr()
    }

    /// Continue parsing an operator run whose left operand is already built.
    ///
    /// Needed where a construct starts with something that is not a primary (`@Tag` in a filter),
    /// so the ordinary precedence ladder cannot begin.
    fn expr_from(&mut self, lhs: Expr) -> Result<Expr, String> {
        let mut lhs = lhs;
        loop {
            let op = match self.text() {
                "and" | "or" | "xor" | "|" | "&" | "==" | "!=" | "<" | "<=" | ">" | ">="
                | "implies" | "+" | "-" | "*" | "/" => self.text().to_string(),
                _ => break,
            };
            let start = lhs.span();
            self.bump();
            let rhs = self.expr()?.ok_or_else(|| {
                format!(
                    "`{op}` needs a right operand, found {}",
                    self.describe_current()
                )
            })?;
            lhs = Expr::Binary {
                op,
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
            // A trailing member access on the whole comparison's right side is handled by the
            // ordinary postfix loop inside `expr`.
            if self.text() == "." && self.at_name_at(1) {
                let start = lhs.span();
                self.bump();
                let member = self.cur().name_text();
                self.bump();
                lhs = Expr::Member {
                    base: Box::new(lhs),
                    member,
                    span: start.to(self.span_prev()),
                };
            }
        }
        Ok(lhs)
    }

    fn expr_implies(&mut self) -> Result<Expr, String> {
        let mut lhs = self.expr_or()?;
        while self.text() == "implies" {
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_or()?;
            lhs = Expr::Binary {
                op: "implies".to_string(),
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn expr_or(&mut self) -> Result<Expr, String> {
        let mut lhs = self.expr_and()?;
        loop {
            let op = match self.text() {
                "or" => "or",
                "xor" => "xor",
                "|" => "|",
                _ => break,
            };
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_and()?;
            lhs = Expr::Binary {
                op: op.to_string(),
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn expr_and(&mut self) -> Result<Expr, String> {
        let mut lhs = self.expr_equality()?;
        loop {
            let op = match self.text() {
                "and" => "and",
                "&" => "&",
                _ => break,
            };
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_equality()?;
            lhs = Expr::Binary {
                op: op.to_string(),
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn expr_equality(&mut self) -> Result<Expr, String> {
        let mut lhs = self.expr_relational()?;
        loop {
            let op = match self.text() {
                "==" | "!=" | "===" | "!==" => self.text().to_string(),
                _ => break,
            };
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_relational()?;
            lhs = Expr::Binary {
                op,
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn expr_relational(&mut self) -> Result<Expr, String> {
        let mut lhs = self.expr_additive()?;
        loop {
            // `istype` / `hastype` / `as` take a type name, not an expression. So do the two
            // metaclassification operators (`MetaclassificationExpression`, KerML 1.0 §8.2.5.8.1):
            // `f meta SysML::Usage` casts `f`'s metadata and `x @@ T` tests it. Without them the
            // expression stopped at `f`, and `meta SysML::Usage` became two phantom members of every
            // semantic-metadata definition in the library.
            if matches!(self.text(), "istype" | "hastype" | "as" | "meta" | "@@") {
                let (kind, keyword) = match self.text() {
                    "istype" => (TypeTestKind::Istype, "istype"),
                    "hastype" => (TypeTestKind::Hastype, "hastype"),
                    "meta" => (TypeTestKind::Meta, "meta"),
                    "@@" => (TypeTestKind::MetaTest, "@@"),
                    _ => (TypeTestKind::As, "as"),
                };
                let start = lhs.span();
                self.bump();
                let Some(type_ref) = self.type_ref() else {
                    return Err(format!(
                        "`{keyword}` needs a type name, found {}",
                        self.describe_current()
                    ));
                };
                lhs = Expr::TypeTest {
                    kind,
                    span: start.to(type_ref.span),
                    operand: Box::new(lhs),
                    type_name: type_ref,
                };
                continue;
            }
            let op = match self.text() {
                // `1..n` is a range, written with the same `..` the import suffix uses.
                "<" | "<=" | ">" | ">=" | ".." => self.text().to_string(),
                _ => break,
            };
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_additive()?;
            lhs = Expr::Binary {
                op,
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn expr_additive(&mut self) -> Result<Expr, String> {
        let mut lhs = self.expr_multiplicative()?;
        loop {
            let op = match self.text() {
                "+" | "-" => self.text().to_string(),
                _ => break,
            };
            // A `-` or `+` followed by nothing that can start an expression is not an operator.
            // This keeps `a - b` working while `first a;` does not read `a;` as an operand.
            if !self.at_expr_start_at(1) {
                break;
            }
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_multiplicative()?;
            lhs = Expr::Binary {
                op,
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn expr_multiplicative(&mut self) -> Result<Expr, String> {
        let mut lhs = self.expr_power()?;
        loop {
            let op = match self.text() {
                "*" | "/" | "%" => self.text().to_string(),
                _ => break,
            };
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_power()?;
            lhs = Expr::Binary {
                op,
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn expr_power(&mut self) -> Result<Expr, String> {
        let lhs = self.expr_unary()?;
        if self.text() == "**" || self.text() == "^" {
            let op = self.text().to_string();
            let start = lhs.span();
            self.bump();
            let rhs = self.expr_power()?;
            return Ok(Expr::Binary {
                op,
                span: start.to(rhs.span()),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            });
        }
        Ok(lhs)
    }

    fn expr_unary(&mut self) -> Result<Expr, String> {
        if matches!(self.text(), "-" | "+" | "not" | "~" | "all") {
            let op = self.text().to_string();
            let start = self.span_here();
            self.bump();
            self.enter()?;
            let operand = self.expr_unary();
            self.leave();
            let operand = operand?;
            return Ok(Expr::Unary {
                op,
                span: start.to(operand.span()),
                operand: Box::new(operand),
            });
        }
        self.expr_postfix()
    }

    /// A primary expression followed by any number of call, member, unit and chain suffixes.
    fn expr_postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.expr_primary()?;
        loop {
            if self.text() == "(" {
                let start = e.span();
                self.bump();
                let args = self.call_args()?;
                e = Expr::Call {
                    callee: Box::new(e),
                    args: args.0,
                    arg_names: args.1,
                    span: start.to(self.span_prev()),
                };
                continue;
            }
            // `base->op(args)`
            if self.text() == "->" {
                let start = e.span();
                self.bump();
                let mut op = if self.at_name() {
                    self.bump().name_text()
                } else {
                    String::new()
                };
                // `x->ControlFunctions::collect { … }` names the library function by its qualified
                // path. Reading one segment left `::collect` behind, and the expression ended
                // there: the model then failed with "expected a name, found `::`" at the tail of a
                // clause that had already been read as something else.
                while !op.is_empty() && self.text() == "::" && self.at_name_at(1) {
                    self.bump();
                    op.push_str("::");
                    op.push_str(&self.bump().name_text());
                }
                // `->collect { in x; ... }` spells the lambda as a braced body rather than a
                // parenthesized argument list. Both are arguments to the same operator.
                let mut params: Vec<String> = Vec::new();
                let args = if self.text() == "(" {
                    self.bump();
                    // A chained operation takes values, not named redefinitions, so the names are
                    // dropped here: `x->select(y)` has no feature of `x` to redefine.
                    self.call_args()?.0
                } else if self.text() == "{" {
                    self.bump();
                    let mut items = Vec::new();
                    while !self.is_eof() && self.text() != "}" {
                        // A lambda parameter declaration: `in x : Real`.
                        if matches!(self.text(), "in" | "out" | "inout") {
                            self.bump();
                            // `in ref w` — the modifier sits between the direction and the name.
                            while matches!(self.text(), "ref" | "attribute" | "feature") {
                                self.bump();
                            }
                            if self.at_name() {
                                params.push(self.cur().name_text());
                            }
                            let _ = self.name();
                            let _ = self.relations(false);
                            // `in ref a { … }` carries a body of its own — an
                            // `ExpressionBodyMember` is `'{' FunctionBodyPart '}'`
                            // (`SysML.xtext:1580`) — and those braces nest inside the lambda's.
                            // Reading only the parameter ended the lambda at the *inner* `}`, and
                            // the rest of the body (`tradeStudyObjective(…)`) fell to the enclosing
                            // declaration, which then ended one `}` short of the file
                            // (`Domain Libraries/Analysis/TradeStudies.sysml:162`, where the drift
                            // surfaced as an unmatched `}` at `:171`).
                            let _ = self.body();
                            continue;
                        }
                        match self.expr()? {
                            Some(x) => {
                                items.push(x);
                            }
                            None => {
                                self.bump();
                            }
                        }
                        self.eat(";");
                    }
                    self.eat("}");
                    items
                } else if self.at_name() {
                    // `collection->reduce '+'`, `->reduce min` and `->reduce RealFunctions::'+'`
                    // pass the function by name: after `->op` the grammar allows a body, an
                    // argument list or a `FunctionReferenceExpression` and nothing else (KerML 1.0
                    // §8.2.5.8.1), so a name here is always the function reference.
                    let reference = self.type_ref().ok_or("expected a function reference")?;
                    vec![Expr::Ref(reference)]
                } else {
                    Vec::new()
                };
                e = Expr::ChainOp {
                    base: Box::new(e),
                    op,
                    args,
                    params,
                    span: start.to(self.span_prev()),
                };
                continue;
            }
            // `base.member` that is not part of a plain path. The member name is the token
            // *after* the dot, so the check must look ahead: `at_name()` describes the current
            // token, which here is the dot itself.
            if self.text() == "." && self.at_name_at(1) {
                let start = e.span();
                self.bump();
                let mut member = self.cur().name_text();
                self.bump();
                // `a.b::c` — a `::` step written after a `.` step. The reference rejects the
                // shape ("Must be a valid feature"), but it has to parse before anything can say
                // so: it used to end the expression here and leave `::c` to the enclosing body,
                // which reported a syntax error and dropped the rest of the statement
                // (`FeatureChain_invalid.kerml.xt:59`). Folding it into the member keeps the path
                // as one text, which then does not resolve.
                while self.text() == "::" && self.at_name_at(1) {
                    self.bump();
                    member.push_str("::");
                    member.push_str(&self.cur().name_text());
                    self.bump();
                }
                e = Expr::Member {
                    base: Box::new(e),
                    member,
                    span: start.to(self.span_prev()),
                };
                continue;
            }
            // `seq#(index)` — index into a sequence. `#` is a postfix operator here, not a
            // metadata sigil, and it may be followed by a parenthesized subscript.
            if self.text() == "#" {
                let start = e.span();
                self.bump();
                let mut args = Vec::new();
                if self.eat("(") {
                    args = self.call_args()?.0;
                }
                e = Expr::ChainOp {
                    base: Box::new(e),
                    op: "#".to_string(),
                    args,
                    params: Vec::new(),
                    span: start.to(self.span_prev()),
                };
                continue;
            }
            // `value [unit]` — a unit bracket.
            if self.text() == "[" {
                let start = e.span();
                self.bump();
                let mut text = String::new();
                let ustart = self.span_here();
                let mut depth = 0usize;
                while !self.is_eof() {
                    if self.text() == "[" {
                        depth += 1;
                    } else if self.text() == "]" {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    }
                    text.push_str(self.text());
                    self.bump();
                }
                self.eat("]");
                e = Expr::Unit {
                    value: Box::new(e),
                    unit: TypeRef {
                        text,
                        span: ustart.to(self.span_here()),
                    },
                    span: start.to(self.span_prev()),
                };
                continue;
            }
            break;
        }
        Ok(e)
    }

    /// `( a, b, c )` — the argument list after an opening paren.
    /// The arguments of a call or a constructor, with the name each one was written under.
    ///
    /// A named argument (`f(mass = 1)`) states which feature of the callee it redefines, and the
    /// specification's invocation rules are stated over exactly that
    /// (`inp.ownedRedefinition.redefinedFeature->intersection(parameters)`). The name used to be
    /// dropped because it "is not a reference to a model element" — true, and beside the point:
    /// it is the only thing that says what the argument redefines.
    fn call_args(&mut self) -> Result<(Vec<Expr>, Vec<Option<String>>), String> {
        let mut args = Vec::new();
        let mut names = Vec::new();
        while !self.is_eof() && self.text() != ")" {
            let mut name = None;
            if self.at_name() && self.at(1).text == "=" {
                name = Some(self.cur().name_text());
                self.bump();
                self.bump();
            }
            match self.expr()? {
                Some(a) => {
                    args.push(a);
                    names.push(name);
                }
                None => break,
            }
            if !self.eat(",") {
                break;
            }
        }
        self.eat(")");
        Ok((args, names))
    }

    fn expr_primary(&mut self) -> Result<Expr, String> {
        let start = self.span_here();
        // `new T(...)` creates an instance. It appears wherever a value does — most visibly as
        // a named argument (`PassIf(vehicle = new testVehicle(mass = x))`) — and without this
        // the constructor's name and its arguments were left in the token stream and read as
        // members of the enclosing body.
        if self.text() == "new" {
            self.bump();
            let Some(type_ref) = self.type_ref() else {
                return Err(format!(
                    "`new` needs a type, found {}",
                    self.describe_current()
                ));
            };
            // The arguments were parsed and thrown away: `new Wheel(1)` and `new Wheel` became
            // the same opaque node, so nothing downstream could tell a constructed instance from
            // a type reference.
            let (args, arg_names) = if self.text() == "(" {
                self.bump();
                self.call_args()?
            } else {
                (Vec::new(), Vec::new())
            };
            return Ok(Expr::Constructor {
                type_ref,
                args,
                arg_names,
                span: start.to(self.span_prev()),
            });
        }
        if self.text() == "if" {
            self.bump();
            self.enter()?;
            let test = self.cond_expr();
            self.leave();
            let test = test?.ok_or("`if` needs a condition")?;
            // Both spellings: `if c ? a else b` and `if c then a else b`.
            if !self.eat("?") {
                self.eat("then");
            }
            let then_expr = self.expr()?.ok_or_else(|| {
                format!(
                    "`if` needs a value for its true branch, found {}",
                    self.describe_current()
                )
            })?;
            // An `if` without `else` yields the then-branch when the condition holds and
            // nothing otherwise. The language allows it; the model just has a partial value.
            let else_expr = if self.eat("else") {
                self.expr()?.ok_or_else(|| {
                    format!(
                        "`if` needs a value for its false branch, found {}",
                        self.describe_current()
                    )
                })?
            } else {
                Expr::Null(start)
            };
            return Ok(Expr::Conditional {
                test: Box::new(test),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
                span: start.to(self.span_prev()),
            });
        }
        // `(as Safety).isMandatory` casts without naming an operand: the cast applies to the
        // element the clause is written on. The general `(` branch below reads a list of
        // expressions, so the `as` was left over, attached to an empty sequence, and the member
        // access after it turned into a stray name in the body — `(as A).y` lowered as `() as A`
        // plus `y`. Every pilot snippet that asks whether a filter's condition is model-level
        // evaluable is written this way.
        if self.text() == "(" && matches!(self.at(1).text.as_str(), "as" | "istype" | "hastype") {
            self.bump();
            let keyword = self.bump().text.clone();
            let kind = match keyword.as_str() {
                "istype" => TypeTestKind::Istype,
                "hastype" => TypeTestKind::Hastype,
                _ => TypeTestKind::As,
            };
            let Some(type_name) = self.type_ref() else {
                return Err(format!(
                    "`{keyword}` needs a type, found {}",
                    self.describe_current()
                ));
            };
            self.eat(")");
            // No operand was written. Recording that as an empty opaque node keeps the clause's
            // own shape (`Expression`, with the type as its reference) without inventing a name
            // for something the model did not write.
            let operand = Expr::Opaque {
                text: String::new(),
                children: Vec::new(),
                span: start,
            };
            return Ok(Expr::TypeTest {
                kind,
                operand: Box::new(operand),
                type_name,
                span: start.to(self.span_prev()),
            });
        }
        if self.text() == "(" {
            self.bump();
            self.enter()?;
            let mut items = Vec::new();
            while !self.is_eof() && self.text() != ")" {
                match self.expr()? {
                    Some(e) => items.push(e),
                    None => break,
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.leave();
            self.eat(")");
            // One item is the common case and keeps its parentheses in the span; more than one is
            // a sequence. Taking the single item out here rather than inside the match is what
            // makes the `pop` below infallible without an unwrap.
            if items.len() == 1 {
                let Some(mut inner) = items.pop() else {
                    unreachable!("the length was just checked");
                };
                // A parenthesised single expression keeps its parentheses in the span. The inner
                // node's range excludes them, so returning it unchanged left the range stopping
                // before the closing paren, and a value spliced into that range left a stray
                // paren after the new number.
                let close = self.span_prev();
                inner.set_span(start.to(close));
                return Ok(inner);
            }
            return Ok(Expr::Sequence {
                items,
                span: start.to(self.span_prev()),
            });
        }
        // A bracketed literal `[a, b]` is a collection value.
        if self.text() == "[" {
            self.bump();
            let mut items = Vec::new();
            while !self.is_eof() && self.text() != "]" {
                match self.expr()? {
                    Some(e) => items.push(e),
                    None => break,
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.eat("]");
            return Ok(Expr::Sequence {
                items,
                span: start.to(self.span_prev()),
            });
        }
        match self.kind() {
            TokenKind::Number => {
                let tok = self.cur().clone();
                let t = self.bump();
                let is_real = t.text.contains('.') || t.text.contains('e') || t.text.contains('E');
                let lit = if is_real {
                    Expr::Real(t.text, Span::new(&tok))
                } else {
                    Expr::Int(t.text, Span::new(&tok))
                };
                // A bracket directly after a number is a unit, as in `2500[SI::kg]`. The postfix
                // loop already handles `x [unit]`, and reaching it from a literal is the same
                // shape, so the literal is returned and the loop in `expr_postfix` picks it up.
                Ok(lit)
            }
            TokenKind::String => {
                let tok = self.cur().clone();
                let t = self.bump();
                Ok(Expr::Str(
                    t.text.trim_matches('"').to_string(),
                    Span::new(&tok),
                ))
            }
            _ => {
                // `@Safety` / `#Safety` name a metadata tag; in a filter it is an operand. A `#`
                // followed by `(` is an index operator and is handled as a postfix.
                if self.text() == "#" && self.at(1).text == "(" {
                    return Err("`#` must follow a value to index".to_string());
                }
                if self.text() == "@" || self.text() == "#" {
                    let sigil = self.bump().text;
                    let Some(name) = self.type_ref() else {
                        return Err(format!("`{sigil}` needs a metadata name"));
                    };
                    return Ok(Expr::Opaque {
                        text: format!("{sigil}{}", name.text),
                        children: Vec::new(),
                        span: start.to(name.span),
                    });
                }
                match self.text() {
                    "true" => {
                        let sp = self.span_here();
                        self.bump();
                        return Ok(Expr::Bool(true, sp));
                    }
                    "false" => {
                        let sp = self.span_here();
                        self.bump();
                        return Ok(Expr::Bool(false, sp));
                    }
                    "null" => {
                        let sp = self.span_here();
                        self.bump();
                        return Ok(Expr::Null(sp));
                    }
                    _ => {}
                }
                if self.at_name() {
                    let mut text = self.cur().name_text();
                    let s = self.span_here();
                    self.bump();
                    while self.text() == "::" {
                        text.push_str("::");
                        self.bump();
                        if self.text() == "*" {
                            text.push('*');
                            self.bump();
                            break;
                        }
                        if !self.at_name() {
                            break;
                        }
                        text.push_str(&self.cur().name_text());
                        self.bump();
                    }
                    return Ok(Expr::Ref(TypeRef {
                        text,
                        span: s.to(self.span_prev()),
                    }));
                }
                Err(format!(
                    "expected an expression, found {}",
                    self.describe_current()
                ))
            }
        }
    }
}

/// Render an expression to a canonical single-line form.
///
/// This is the text a report quotes back and the diagram draws, so it has to round-trip the
/// authored meaning: `a + b * c` keeps its grouping because the tree already encodes it.
/// A stored (unquoted) name as the notation writes it: a segment that is not a plain identifier is
/// quoted again, so `'+'` renders as `'+'` rather than `+`. The same rule as
/// `sysml_model::quote_name`, which this crate cannot depend on.
fn quote_name(text: &str) -> String {
    fn plain(segment: &str) -> bool {
        let mut chars = segment.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
    // Some reference texts carry the end's own notation around the name: a bound
    // (`[2] d`), a conjugation (`~p`) or a reference subsetting (`cd ::> b.c`). Those parts are
    // written as they are and only the names inside are quoted.
    if let Some(close) = text.strip_prefix('[').and_then(|rest| rest.find(']')) {
        let (bound, rest) = text.split_at(close + 2);
        let name = rest.trim_start();
        let gap = &rest[..rest.len() - name.len()];
        return format!("{bound}{gap}{}", quote_name(name));
    }
    if let Some(rest) = text.strip_prefix('~') {
        return format!("~{}", quote_name(rest));
    }
    if let Some((declared, target)) = text.split_once(" ::> ") {
        return format!("{} ::> {}", quote_name(declared), quote_name(target));
    }
    let mut out = String::with_capacity(text.len());
    let mut segment = String::new();
    let mut rest = text;
    let flush = |segment: &mut String, out: &mut String| {
        if !segment.is_empty() {
            if plain(segment) || segment == "*" || segment == "**" {
                out.push_str(segment);
            } else {
                out.push('\'');
                out.push_str(&segment.replace('\'', "\\'"));
                out.push('\'');
            }
            segment.clear();
        }
    };
    while let Some(c) = rest.chars().next() {
        if rest.starts_with("::") {
            flush(&mut segment, &mut out);
            out.push_str("::");
            rest = &rest[2..];
        } else if c == '.' {
            flush(&mut segment, &mut out);
            out.push('.');
            rest = &rest[1..];
        } else {
            segment.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    flush(&mut segment, &mut out);
    out
}

pub fn render_expr(e: &Expr) -> String {
    fn go(e: &Expr, out: &mut String) {
        match e {
            Expr::Int(t, _) | Expr::Real(t, _) => out.push_str(t),
            Expr::Str(s, _) => {
                out.push('"');
                out.push_str(s);
                out.push('"');
            }
            Expr::Bool(b, _) => out.push_str(if *b { "true" } else { "false" }),
            Expr::Null(_) => out.push_str("null"),
            Expr::Ref(r) => out.push_str(&quote_name(&r.text)),
            Expr::Binary { op, lhs, rhs, .. } => {
                go(lhs, out);
                out.push(' ');
                out.push_str(op);
                out.push(' ');
                go(rhs, out);
            }
            Expr::Unary { op, operand, .. } => {
                out.push_str(op);
                if matches!(op.as_str(), "not" | "all") {
                    out.push(' ');
                }
                go(operand, out);
            }
            Expr::Call { callee, args, .. } => {
                go(callee, out);
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    go(a, out);
                }
                out.push(')');
            }
            Expr::Conditional {
                test,
                then_expr,
                else_expr,
                ..
            } => {
                out.push_str("if ");
                go(test, out);
                out.push_str(" ? ");
                go(then_expr, out);
                out.push_str(" else ");
                go(else_expr, out);
            }
            Expr::Unit { value, unit, .. } => {
                go(value, out);
                out.push_str(" [");
                out.push_str(&unit.text);
                out.push(']');
            }
            Expr::Member { base, member, .. } => {
                go(base, out);
                out.push('.');
                out.push_str(member);
            }
            Expr::Sequence { items, .. } => {
                out.push('(');
                for (i, a) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    go(a, out);
                }
                out.push(')');
            }
            Expr::TypeTest {
                kind,
                operand,
                type_name,
                ..
            } => {
                go(operand, out);
                out.push(' ');
                out.push_str(match kind {
                    TypeTestKind::Istype => "istype",
                    TypeTestKind::Hastype => "hastype",
                    TypeTestKind::As => "as",
                    TypeTestKind::Meta => "meta",
                    TypeTestKind::MetaTest => "@@",
                });
                out.push(' ');
                out.push_str(&type_name.text);
            }
            Expr::ChainOp { base, op, args, .. } => {
                go(base, out);
                out.push_str("->");
                out.push_str(op);
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    go(a, out);
                }
                out.push(')');
            }
            Expr::Constructor { type_ref, args, .. } => {
                out.push_str("new ");
                out.push_str(&type_ref.text);
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    go(a, out);
                }
                out.push(')');
            }
            Expr::Opaque { text, .. } => out.push_str(text),
        }
    }
    let mut s = String::new();
    go(e, &mut s);
    s
}

// -- statements and relationships --------------------------------------------

impl Parser {
    /// `import A::B::*;`
    fn import(&mut self, visibility: Option<Visibility>) -> Result<Relationship, String> {
        let start = self.span_here();
        self.expect("import")?;
        self.eat("all");
        // The target path, with its suffix kept separate.
        let mut target = String::new();
        let mut shape = ImportShape::Membership { recursive: false };
        let mut recursive = false;
        loop {
            if !self.at_name() {
                break;
            }
            // The path keeps its separators: `Q::A` must not become `QA`, or the resolver sees
            // one segment where the model wrote two.
            if !target.is_empty() {
                target.push_str("::");
            }
            target.push_str(&self.cur().name_text());
            self.bump();
            // `import Q [filter]` — a filter written directly after the target, with no `::`.
            if self.text() == "[" {
                let mut depth = 0usize;
                while !self.is_eof() {
                    if self.text() == "[" {
                        depth += 1;
                    } else if self.text() == "]" {
                        depth -= 1;
                        if depth == 0 {
                            self.bump();
                            break;
                        }
                    }
                    self.bump();
                }
                shape = ImportShape::Filter;
                break;
            }
            if self.text() != "::" {
                break;
            }
            // `::*`, `::**`, `::[filter]`, or another segment. The lexer folds `**` into one
            // token (it is also the exponent operator), so the recursive form arrives as `**`
            // and not as two `*` — testing only for `*` sent `import A::**` down the segment
            // loop, where it bound `A` itself and nothing inside it.
            if self.at(1).text == "*" || self.at(1).text == "**" {
                self.bump();
                let star = self.bump().text;
                // `**` is the recursive form on its own: `import Q::**` binds what `Q`
                // contains at every depth, exactly as `Q::*::**` does.
                recursive = star == "**";
                shape = ImportShape::Namespace { recursive };
                // `::**` — the lexer folds the two stars into one `**` token (the exponent
                // operator), so the recursive suffix is one token here, not two.
                if self.text() == "::" {
                    // Both spellings are the recursive form: the lexer folds `**` into one
                    // token, and a plain `*` after `::` means the same depth-all import.
                    if self.at(1).text == "**" || self.at(1).text == "*" {
                        self.bump();
                        self.bump();
                        recursive = true;
                    }
                }
                // `import vehicle::**[@Safety];` filters a namespace import
                // (`FilterPackage`, `13b-Safety and Security Features Element Group-2.sysml:35`).
                // Left unread, the filter became phantom members. The condition is consumed the
                // same way the `Q [filter]` form's is; the import keeps its namespace shape.
                while self.text() == "[" {
                    let mut depth = 0usize;
                    while !self.is_eof() {
                        match self.text() {
                            "[" => depth += 1,
                            "]" => {
                                depth -= 1;
                                if depth == 0 {
                                    self.bump();
                                    break;
                                }
                            }
                            _ => {}
                        }
                        self.bump();
                    }
                }
                break;
            }
            if self.at(1).text == "[" {
                self.bump();
                let mut depth = 0usize;
                while !self.is_eof() {
                    if self.text() == "[" {
                        depth += 1;
                    } else if self.text() == "]" {
                        depth -= 1;
                        if depth == 0 {
                            self.bump();
                            break;
                        }
                    }
                    self.bump();
                }
                shape = ImportShape::Filter;
                break;
            }
            self.bump();
        }
        // A trailing `::**` (or `::*::**`) makes a membership import recursive too.
        if !recursive && self.text() == "::" {
            if self.at(1).text == "**" || self.at(1).text == "*" {
                self.bump();
                self.bump();
                recursive = true;
            }
        }
        if recursive {
            shape = match shape {
                ImportShape::Namespace { .. } => ImportShape::Namespace { recursive: true },
                ImportShape::Membership { .. } => ImportShape::Membership { recursive: true },
                other => other,
            };
        }
        // An import may carry a relationship body: `private import Definitions::* { doc ... }`.
        // Skipping it left the body's tokens to be read as the next member's, which cascaded
        // into every name that member then referenced.
        self.body();
        self.eat(";");
        Ok(Relationship::Import {
            visibility,
            target,
            shape,
            span: start.to(self.span_here()),
        })
    }

    /// `alias Hub for Wheel;`
    /// `featuring of M by X;` — a `TypeFeaturing` stated on its own (`KerML.xtext:651-655`). The
    /// optional identification names it: `featuring f of M by X;`.
    fn featuring(&mut self) -> Result<Relationship, String> {
        let start = self.span_here();
        self.expect("featuring")?;
        let name = if self.text() == "of" {
            None
        } else {
            Some(self.name())
        };
        self.expect("of")?;
        let featured = self.type_ref().ok_or("`featuring … of` needs a feature")?;
        self.expect("by")?;
        let featuring = self.type_ref().ok_or("`featuring … by` needs a feature")?;
        self.body();
        Ok(Relationship::Featuring {
            name,
            featured,
            featuring,
            span: start.to(self.span_here()),
        })
    }

    fn alias(&mut self) -> Result<Relationship, String> {
        let start = self.span_here();
        self.expect("alias")?;
        // `AliasMember` is `'alias' ( '<' memberShortName = Name '>' )? ( memberName = Name )?
        // 'for' …` (`KerML.xtext:161-167`): the name is optional, and `alias for
        // Base::exactlyOne;` inside `class M [0..1]` is the spelling that says so
        // (`MultiplicityAlias_valid.kerml.xt:42`). Reading a name unconditionally consumed `for`
        // as the name and then failed on the target, which cost the whole body after it — the
        // model parsed as one syntax error and nothing else.
        let name = if self.text() == "for" {
            Name::default()
        } else {
            self.name()
        };
        self.expect("for")?;
        let target = self
            .type_ref()
            .ok_or("`alias` needs a target after `for`")?;
        // An alias may carry a relationship body: `alias Torque for ISQ::TorqueValue { ... }`.
        // Skipping it left the closing brace to be read as the package's, which truncated every
        // member after it.
        self.body();
        self.eat(";");
        Ok(Relationship::Alias {
            name,
            target,
            span: start.to(self.span_here()),
        })
    }

    /// `expose A::*;` inside a view.
    fn expose(&mut self) -> Result<Relationship, String> {
        let start = self.span_here();
        self.expect("expose")?;
        let mut target = String::new();
        let mut recursive = false;
        while self.at_name() {
            let segment = self.cur().name_text();
            self.bump();
            if self.text() == "::" {
                self.bump();
                // The lexer folds `**` into one token, so the recursive form arrives as a
                // single `**`; both `::*::**` and `::**` end the path here. The segment before
                // the suffix is still part of the target.
                if self.text() == "*" || self.text() == "**" {
                    if !target.is_empty() && !target.ends_with("::") {
                        target.push_str("::");
                    }
                    target.push_str(&segment);
                    let star = self.bump().text;
                    recursive = star == "**";
                    if self.text() == "::" && (self.at(1).text == "**" || self.at(1).text == "*") {
                        self.bump();
                        recursive = self.bump().text == "**";
                    }
                    break;
                }
                target.push_str(&segment);
                target.push_str("::");
                continue;
            }
            if !target.is_empty() && !target.ends_with("::") {
                target.push_str("::");
            }
            target.push_str(&segment);
            break;
        }
        // `expose X::*[@Filter]` — the filter selects among what the exposure brings in. It is
        // part of the statement, not the next member.
        if self.text() == "[" {
            self.bump();
            let mut depth = 1usize;
            while !self.is_eof() && depth > 0 {
                match self.text() {
                    "[" => depth += 1,
                    "]" => depth -= 1,
                    _ => {}
                }
                self.bump();
            }
        }
        self.eat(";");
        Ok(Relationship::Expose {
            target: target.trim_end_matches("::").to_string(),
            recursive,
            span: start.to(self.span_here()),
        })
    }

    /// `dependency a from b;`
    fn dependency(&mut self) -> Result<Relationship, String> {
        let start = self.span_here();
        self.expect("dependency")?;
        // `dependency from A to B;` is anonymous: `from` is the clause keyword, not a name, and
        // reading it as one left the client list unread (`12a-Dependency.sysml:8`).
        let name = if self.text() == "from" {
            Name::default()
        } else {
            self.name()
        };
        let mut clients = Vec::new();
        let mut suppliers = Vec::new();
        if self.eat("from") {
            while let Some(t) = self.type_ref() {
                clients.push(t);
                if !self.eat(",") {
                    break;
                }
            }
        }
        if self.eat("to") {
            while let Some(t) = self.type_ref() {
                suppliers.push(t);
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.body();
        self.eat(";");
        Ok(Relationship::Dependency {
            name,
            clients,
            suppliers,
            span: start.to(self.span_here()),
        })
    }

    /// Whether a `subtype X specializes Y` statement starts here.
    ///
    /// `subtype` reads as a name like any other, so the word alone does not decide: the statement
    /// continues with a qualified name and one of the two specialization operators
    /// (`KerML.xtext:392-395`).
    fn at_specialization(&self) -> bool {
        // `subclassifier` is the `Subclassification` spelling of the same statement
        // (`KerML.xtext:485-490`); `Occurrences.kerml:729` writes it.
        if !matches!(self.text(), "subtype" | "subclassifier") {
            return false;
        }
        let mut i = 1;
        loop {
            if matches!(self.at(i).text.as_str(), ":>" | "specializes") {
                return true;
            }
            if self.at_name_at(i) || matches!(self.at(i).text.as_str(), "::" | ".") {
                i += 1;
            } else {
                return false;
            }
        }
    }

    /// `subtype C specializes B;` (`KerML.xtext:388-398`).
    ///
    /// The production opens with an optional `specialization <name>?`, which this parser does not
    /// read: a named specialization is not written anywhere in the corpus, and the name would be
    /// dropped at lowering anyway.
    fn specialization(&mut self) -> Result<Relationship, String> {
        let start = self.span_here();
        if !self.eat("subclassifier") {
            self.expect("subtype")?;
        }
        let specific = self.type_ref().ok_or_else(|| {
            format!(
                "`subtype` needs a specific type, found {}",
                self.describe_current()
            )
        })?;
        if !self.eat("specializes") {
            self.expect(":>")?;
        }
        let general = self.type_ref().ok_or_else(|| {
            format!(
                "`subtype ... specializes` needs a general type, found {}",
                self.describe_current()
            )
        })?;
        self.body();
        self.eat(";");
        Ok(Relationship::Specialization {
            specific,
            general,
            span: start.to(self.span_here()),
        })
    }

    /// `satisfy R by x;` / `satisfy R;`
    fn satisfy(&mut self) -> Result<Relationship, String> {
        let start = self.span_here();
        self.expect("satisfy")?;
        self.eat("not");
        // `satisfy requirement viewpointConformance by that { … }` writes the `requirement` keyword
        // before the satisfied usage's name (`SatisfyRequirementUsage`, `SysML.xtext`). Taken as
        // the requirement's name, the keyword left `viewpointConformance`, `by` and `that` as
        // phantom members (`Views.sysml:37`).
        if self.text() == "requirement" && self.at_name_at(1) {
            self.bump();
        }
        // The requirement can be a `(name : Type)` parenthesized form.
        let requirement = if self.text() == "(" {
            self.bump();
            let t = self.type_ref();
            self.eat(")");
            t.ok_or("`satisfy (...)` needs a requirement")?
        } else {
            self.type_ref()
                .ok_or("`satisfy` needs a requirement to point at")?
        };
        let by = if self.eat("by") {
            self.type_ref()
        } else {
            None
        };
        self.body();
        self.eat(";");
        Ok(Relationship::Satisfy {
            requirement,
            by,
            span: start.to(self.span_here()),
        })
    }

    /// Statements that live in bodies and are not declarations.
    ///
    /// Returns `None` when the current token does not start one of them, leaving the position
    /// untouched so the caller can try a declaration instead.
    fn statement(&mut self, prefixes: &Prefixes, start: Span) -> Result<Option<Statement>, String> {
        // A `connect` statement: optional `connection name : T` then `connect a to b`.
        if self.text() == "connect" {
            return self.connect_statement(None, start).map(Some);
        }
        // `inverse A::f of B::g;` and `inverting X inverse A::f of B::g;` — a feature inverting
        // stated on its own (`KerML.xtext:632-641`). Both words and both paths are consumed so
        // neither becomes a member named `inverse`.
        if self.text() == "inverse" || self.text() == "inverting" {
            if self.text() == "inverting" {
                self.bump();
                let _ = self.name();
            }
            self.expect("inverse")?;
            let inverted = self.type_ref();
            self.expect("of")?;
            let inverting = self.type_ref();
            self.body();
            // A statement, not `None`: returning `None` after consuming the tokens told the caller
            // nothing had been read, so it went on to parse the *next* statement as the rest of
            // this member — `inverting Invert inverse …` after `inverse B::g of A::f;` became two
            // phantom attributes (`FeatureInvertingTest_valid.kerml.xt:35`).
            return Ok(Some(Statement::Inverting {
                inverted,
                inverting,
                span: start.to(self.span_here()),
            }));
        }
        if self.text() == "bind" || self.text() == "binding" {
            return self.bind_statement(start).map(Some);
        }
        if self.text() == "assign" {
            return self.assign_statement(start).map(Some);
        }
        if self.text() == "succession" {
            return self.succession(start).map(Some);
        }
        // `transition name if guard then Target;` — a state transition. The guard is a condition
        // in exactly the sense a requirement's predicate is, and it belongs on the edge.
        if self.text() == "transition" {
            self.bump();
            // `TransitionUsage` is `'transition' (UsageDeclaration? 'first')?
            // ownedRelationship += TransitionSourceMember …` (`SysML.xtext:1854-1864`): the source
            // is *mandatory* and the declaration is optional, and a declaration only counts when
            // `first` follows it. So the first word is the transition's name only when a
            // declaration clause follows — `transition t first a then b` names it `t` and starts
            // at `a`, while `transition off then on` and `transition init accept A then S2_1`
            // start at `off` / `init` and declare no name at all.
            //
            // Reading that word as the name unconditionally left the source empty, and the
            // lowering then filled `from` with the enclosing state — which *is* a state, so
            // `validateTransitionUsageTriggerActions` had nothing to reject and
            // `transition init accept A then S2_1` (`init` being an entry action) went
            // unreported (`TransitionUsage_invalid.sysml.xt:55`). `first` must not be read as a
            // name either: `transition first S2_1 … then S2_2` named the transition `first` and
            // lost its source, which is what the `first` branch below is for.
            let name = if self.at_name()
                && !matches!(self.text(), "first" | "accept" | "if" | "do" | "then")
                && matches!(
                    self.at(1).text.as_str(),
                    // `in`/`out`/`inout` start the declaration's parameters, which sit between the
                    // name and the `first`: `transition t in p first a then b` declares `t` and
                    // takes `p` as an input. Matching only `first` here made `t` the *source* and
                    // the transition lost its name (`rules_state_transition.rs:696`, measured).
                    "first"
                        | "in"
                        | "out"
                        | "inout"
                        | ":"
                        | ":>"
                        | ":>>"
                        | "subsetted"
                        | "subsets"
                        | "redefines"
                        | "["
                ) {
                self.name()
            } else {
                Name::default()
            };
            // The full spelling is `transition name first a accept trigger then b;`, with the
            // trigger and any guard optional and in either order. Reading only `if ... then ...`
            // left `first` and `accept` in the stream, where they consumed the enclosing brace.
            if !name.is_empty() {
                let _ = self.relations(false);
            }
            let mut source = None;
            // Without `first` and without a declaration the source sits right here. Leaving it in
            // the stream made the lowering substitute the enclosing state for it (see above).
            // `then`/`;`/`{` can stand where the source would be in a malformed statement, and
            // reading one as a name would give the transition a source the model never wrote.
            if !matches!(
                self.text(),
                "first" | "then" | ";" | "}" | "in" | "out" | "inout"
            ) {
                source = self.succession_end()?;
            }
            let mut trigger = None;
            let mut trigger_kind = None;
            let mut guard = None;
            let mut effect = None;
            loop {
                if self.eat("first") {
                    let _ = self.multiplicity();
                    source = self.succession_end()?;
                    continue;
                }
                if self.eat("accept") {
                    // `accept <payload>`, `accept at <time>`, `accept when <expr>` and
                    // `accept after <duration>` are all triggers. The keyword decides which rule
                    // the argument has to satisfy — `at` a time instant, `after` a duration, `when`
                    // a Boolean — so it is recorded rather than dropped. `when` and `after` take a
                    // whole expression (`when 'sense temperature'.temp > vehicle.Tmax`); `at` and a
                    // bare payload name a feature, which `succession_end` reads.
                    let keyword = if self.eat("at") {
                        "at"
                    } else if self.eat("when") {
                        "when"
                    } else if self.eat("after") {
                        "after"
                    } else {
                        "accept"
                    };
                    trigger_kind = Some(keyword.to_string());
                    trigger = if matches!(keyword, "when" | "after") {
                        self.expr()?
                    } else {
                        self.accept_payload()?
                    };
                    continue;
                }
                if self.eat("if") {
                    guard = self.expr()?;
                    continue;
                }
                // `do send X to y` is the transition's effect. It is a statement in its own
                // right, and leaving it in the stream split `send ... to ...` into members of
                // the enclosing state.
                if self.eat("do") {
                    // The effect is recorded now: it used to be parsed and dropped, so a
                    // transition's `do` clause left no trace and a rule about what an effect may
                    // be had nothing to read.
                    if matches!(self.text(), "send" | "accept") {
                        let span = self.span_here();
                        let _ = self.message(start, false)?;
                        effect = Some(Expr::Opaque {
                            text: "message".to_string(),
                            children: Vec::new(),
                            span,
                        });
                        continue;
                    }
                    // Any other effect is an action reference or assignment.
                    if self.eat("assign") {
                        let span = self.span_here();
                        let target = self.expr()?;
                        self.eat(":=");
                        let value = self.expr()?;
                        let mut children: Vec<Expr> = Vec::new();
                        children.extend(target);
                        children.extend(value);
                        effect = Some(Expr::Opaque {
                            text: "assign".to_string(),
                            children,
                            span,
                        });
                        continue;
                    }
                    effect = self.expr()?;
                    continue;
                }
                break;
            }
            // `transition t first a then b if g do send x` — the guard and the effect may follow
            // the target as well as precede it. Reading only the clauses before `then` left `if`
            // and `do` in the stream, where they became members of the enclosing state.
            let mut target = None;
            loop {
                if self.eat("then") {
                    let _ = self.multiplicity();
                    target = self.succession_end()?;
                    continue;
                }
                if self.eat("if") {
                    guard = self.expr()?;
                    continue;
                }
                if self.eat("do") {
                    if matches!(self.text(), "send" | "accept") {
                        let span = self.span_here();
                        let _ = self.message(start, false)?;
                        effect = Some(Expr::Opaque {
                            text: "message".to_string(),
                            children: Vec::new(),
                            span,
                        });
                        continue;
                    }
                    effect = self.expr()?;
                    continue;
                }
                // `accept` may follow the target as well as precede it:
                // `transition off then on accept when x` is the same statement as
                // `transition off accept when x then on`. Reading only the clauses before `then`
                // left `accept when x` in the stream, where it became a member of the enclosing
                // state and the transition lost its trigger entirely — no `trigger_kind`, no
                // argument, and nothing for the trigger rules to judge.
                if self.eat("accept") {
                    let keyword = if self.eat("at") {
                        "at"
                    } else if self.eat("when") {
                        "when"
                    } else if self.eat("after") {
                        "after"
                    } else {
                        "accept"
                    };
                    trigger_kind = Some(keyword.to_string());
                    trigger = if matches!(keyword, "when" | "after") {
                        self.expr()?
                    } else {
                        self.accept_payload()?
                    };
                    continue;
                }
                break;
            }
            self.body();
            self.eat(";");
            return Ok(Some(Statement::Succession {
                name,
                first: source,
                then: target,
                trigger,
                trigger_kind,
                trigger_type: self.pending_trigger_type.take(),
                trigger_via: self.pending_trigger_via.take(),
                guard,
                effect,
                implicit_source: false,
                span: start.to(self.span_here()),
            }));
        }
        if self.text() == "first" {
            self.bump();
            // `first [0..1] a then b` — the end is a feature chain with an optional bound, which
            // `expr()` reads as a collection literal.
            let target = self.succession_end()?;
            // The guard, the trigger and the effect may all sit between the source and `then`:
            // `TransitionUsage` is `… TransitionSourceMember EmptyParameterMember
            // (EmptyParameterMember TriggerActionMember)? GuardExpressionMember?
            // EffectBehaviorMember? 'then' …` (`SysML.xtext:1854-1864`), so
            // `first S2_1 if "test" then S2_2` is exactly the guard's position. Reading `then`
            // straight after the source left `if "test" then S2_2` to the next member: the guard
            // reached no transition, so `validateTransitionFeatureMembershipGuardExpression` had
            // nothing to judge and the miss at `TransitionUsage_invalid.sysml.xt:61` survived —
            // the transition was recorded as a bare `first` with no target at all.
            let mut leading_guard = None;
            let mut leading_trigger = None;
            let mut leading_trigger_kind = None;
            let mut leading_effect = None;
            loop {
                if self.eat("if") {
                    leading_guard = self.expr()?;
                    continue;
                }
                if self.eat("accept") {
                    let keyword = if self.eat("at") {
                        "at"
                    } else if self.eat("when") {
                        "when"
                    } else if self.eat("after") {
                        "after"
                    } else {
                        "accept"
                    };
                    leading_trigger_kind = Some(keyword.to_string());
                    leading_trigger = if matches!(keyword, "when" | "after") {
                        self.expr()?
                    } else {
                        self.accept_payload()?
                    };
                    continue;
                }
                if self.eat("do") {
                    leading_effect = self.expr()?;
                    continue;
                }
                break;
            }
            // `first a then b { ... }` is a succession, not a lone `first a`. Reading only the
            // first operand left `then b` to be parsed as the next member, and every member after
            // it escaped the enclosing action body — the official 3a model lost its `merge`,
            // `first continue then engineStarted` and the rest of `provide power` this way.
            if self.text() == "then" {
                self.bump();
                let rest = self.succession_end()?;
                // `first a then b if g do x` — the guard and the effect may follow the target, just
                // as they may after a named transition's target (`:2487-2515`). Leaving them in the
                // stream made `do x` a state subaction of the enclosing machine: the effect the
                // model wrote reached no element, and a rule about what an effect may be had
                // nothing to read.
                let mut guard = leading_guard;
                let mut effect = leading_effect;
                loop {
                    if self.eat("if") {
                        guard = self.expr()?;
                        continue;
                    }
                    if self.eat("do") {
                        if matches!(self.text(), "send" | "accept") {
                            let span = self.span_here();
                            let _ = self.message(start, false)?;
                            effect = Some(Expr::Opaque {
                                text: "message".to_string(),
                                children: Vec::new(),
                                span,
                            });
                            continue;
                        }
                        effect = self.expr()?;
                        continue;
                    }
                    break;
                }
                self.body();
                self.eat(";");
                return Ok(Some(Statement::Succession {
                    name: Name::default(),
                    first: target,
                    then: rest,
                    trigger: leading_trigger,
                    trigger_kind: leading_trigger_kind,
                    trigger_type: self.pending_trigger_type.take(),
                    trigger_via: self.pending_trigger_via.take(),
                    guard,
                    effect,
                    implicit_source: false,
                    span: start.to(self.span_here()),
                }));
            }
            self.eat(";");
            return Ok(Some(Statement::First {
                target,
                span: start.to(self.span_here()),
            }));
        }
        if self.text() == "then" {
            self.bump();
            // `then` may introduce a *declaration* (`then private action whileLoop while ...`),
            // not only a reference to one. When a declaration keyword follows, this statement is
            // the declaration and its own body; when a plain name follows, it is a succession.
            let looks_like_decl = matches!(
                self.text(),
                "private"
                    | "public"
                    | "protected"
                    | "action"
                    | "state"
                    | "assign"
                    | "perform"
                    | "accept"
                    | "send"
                    | "merge"
                    | "decision"
                    | "join"
                    | "fork"
                    | "if"
                    | "while"
                    | "loop"
                    | "for"
                    | "terminate"
                    | "return"
                    | "include"
                    | "exhibit"
            ) || (self.text() == "then");
            if looks_like_decl {
                let visibility = self.visibility();
                let prefixes = self.prefixes();
                if let Some(stmt) = self.statement(&prefixes, start)? {
                    let _ = visibility;
                    return Ok(Some(stmt));
                }
                let head = self.head(prefixes, None)?;
                let body = self.body();
                return Ok(Some(Statement::Expression {
                    expr: Expr::Ref(TypeRef {
                        text: head.name.display(),
                        span: head.span,
                    }),
                    span: head
                        .span
                        .to(self.span_here())
                        .to(body_span(&body, head.span)),
                }));
            }
            let target = self.expr()?;
            // `then b if g do x` — the guard and the effect may follow the target of a bare
            // succession, as they may after a transition's (`:2487-2515`). Leaving `do x` in the
            // stream made it a state subaction of the enclosing machine and the effect the model
            // wrote reached no element.
            let mut guard = None;
            let mut effect = None;
            loop {
                if self.eat("if") {
                    guard = self.expr()?;
                    continue;
                }
                if self.eat("do") {
                    if matches!(self.text(), "send" | "accept") {
                        let span = self.span_here();
                        let _ = self.message(start, false)?;
                        effect = Some(Expr::Opaque {
                            text: "message".to_string(),
                            children: Vec::new(),
                            span,
                        });
                        continue;
                    }
                    effect = self.expr()?;
                    continue;
                }
                break;
            }
            self.eat(";");
            return Ok(Some(Statement::Succession {
                name: Name::default(),
                first: None,
                then: target,
                trigger: None,
                trigger_kind: None,
                trigger_type: None,
                trigger_via: None,
                guard,
                effect,
                // `then b;` writes no source: it is the feature before it in the body.
                implicit_source: true,
                span: start.to(self.span_here()),
            }));
        }
        if matches!(self.text(), "if" | "while" | "loop" | "for")
            && self.at(1).text != "?"
            && !(self.text() == "if" && self.if_is_an_expression())
        {
            return self.control_flow(start).map(Some);
        }
        if matches!(self.text(), "merge" | "decision" | "join" | "fork") {
            return self.control_node(start).map(Some);
        }
        if self.text() == "terminate" {
            self.bump();
            let target = self.expr()?;
            self.eat(";");
            return Ok(Some(Statement::Terminate {
                target,
                span: start.to(self.span_here()),
            }));
        }
        if self.text() == "until" {
            self.bump();
            let condition = self.expr()?.ok_or("`until` needs a condition")?;
            self.eat(";");
            return Ok(Some(Statement::Until {
                condition,
                span: start.to(self.span_here()),
            }));
        }
        if self.text() == "return" {
            self.bump();
            // Two spellings. `return expr;` states the result; `return : T = expr;` and
            // `return attribute [0..*] = expr;` declare the return parameter, which is how the
            // KerML function library writes every operator. The second form was what made five
            // library files unreadable.
            if self.text() == ":"
                || self.text() == ":>"
                || self.text() == ":>>"
                || self.text() == "="
                || self.text() == "["
                || self.at_name()
            {
                let decl_start = start;
                // `return attribute [0..*] = ...` names the kind keyword after `return`, and the
                // usage element may carry its own prefix, a direction included: `return in r : Real`
                // (`ReturnParameterMember : 'return' UsageElement`, `SysML.xtext:1961-1963`;
                // `ReturnFeatureMember : 'return' FeatureElement`, `KerML.xtext:943-946`). Read as
                // the name, `in` became the parameter and `r` a phantom attribute; the direction
                // itself is what `validateParameterMembershipParameterDirection` judges.
                let mut authored_direction = None;
                while matches!(
                    self.text(),
                    "in" | "out"
                        | "inout"
                        | "ref"
                        | "attribute"
                        | "feature"
                        | "item"
                        | "part"
                        | "occurrence"
                        | "port"
                ) && (self.at_name_at(1)
                    || matches!(self.at(1).text.as_str(), ":" | "[" | "="))
                {
                    match self.bump().text.as_str() {
                        "in" => authored_direction = Some(Direction::In),
                        "out" => authored_direction = Some(Direction::Out),
                        "inout" => authored_direction = Some(Direction::InOut),
                        _ => {}
                    }
                }
                let authored = self.name();
                let authored_empty = authored.is_empty();
                let mut head = Head {
                    name: if authored_empty {
                        Name {
                            text: "return".to_string(),
                            short: None,
                            span: decl_start,
                        }
                    } else {
                        authored
                    },
                    prefixes: Prefixes {
                        // A return parameter is `out` unless the model wrote otherwise.
                        direction: Some(authored_direction.unwrap_or(Direction::Out)),
                        ..Prefixes::default()
                    },
                    span: decl_start,
                    ..Head::default()
                };
                // The whole `FeatureSpecializationPart`: specializations on either side of one
                // multiplicity part, `ordered` and `nonunique` included. Reading one bound and one
                // run of relations stopped at `return : Anything[0..*] ordered nonunique;` — the
                // function library's most common line — and the rest became phantom members.
                self.flow_specializations(&mut head)?;
                // `return :>> result` redefines the inherited slot and names it there: the name
                // slot before the clause is empty, and without this the target is re-read as a
                // member of the enclosing body and the slot it was meant to name is unreachable.
                if authored_empty {
                    if let Some(target) = head
                        .relations
                        .iter()
                        .find(|r| r.op == RelationOp::Redefines || r.op == RelationOp::Specializes)
                        .and_then(|r| r.targets.first())
                    {
                        head.name = Name {
                            text: target.text.clone(),
                            short: None,
                            span: decl_start,
                        };
                    }
                }
                // `= ` may be followed by nothing: a KerML return parameter can declare the shape
                // of its result without giving one. Otherwise the value is a full `ValuePart`,
                // `default` included (`return : Real default sum0(…)`, `RealFunctions.kerml:50`).
                if self.text() == "=" && matches!(self.at(1).text.as_str(), ";" | "{") {
                    self.bump();
                    head.value_text = Some("=".to_string());
                } else {
                    self.value_part(&mut head)?;
                }
                self.body();
                self.eat(";");
                head.span = decl_start.to(self.span_here());
                // A `return` declaration is a parameter of the behaviour, not a bare expression.
                // It carries the declared result's type and, when written, its value; both are
                // kept on the `Head` so the lowering layer sees the whole declaration.
                return Ok(Some(Statement::ReturnDeclaration {
                    head,
                    span: decl_start.to(self.span_here()),
                }));
            }
            let expr = self.expr()?;
            self.eat(";");
            return Ok(Some(Statement::Return {
                expr,
                span: start.to(self.span_here()),
            }));
        }
        if matches!(self.text(), "accept" | "send") {
            return self.message(start, true).map(Some);
        }
        // `entry action a; do 'sense temperature' { ... } exit ...` — a state's own behaviours.
        // The `do` spelling takes an action reference and may carry a body.
        //
        // `do send x to y` is not a subaction: the `do` is a clause of the enclosing
        // accept/send/transition and what follows is the action it performs. Reading `send` as the
        // subaction's name made the message's own payload members of the state, and a state with
        // two `do`s then reported a duplicate that the model never wrote.
        if matches!(self.text(), "entry" | "do" | "exit")
            && (self.at_name_at(1) || self.at(1).text == "{")
            && !(self.text() == "do" && matches!(self.at(1).text.as_str(), "send" | "accept"))
        {
            let keyword = self.bump().text;
            // `entry action boot;` declares a behaviour; `entry boot;` names one that exists
            // elsewhere. The two are different claims — one introduces a name, the other refers
            // to it — and a rule that checks references must not judge the first. The `action`
            // word is the only thing that tells them apart, so it is recorded rather than eaten.
            let declares = self.eat("action");
            let name = self.name();
            let relations = self.relations(false);
            let type_ref = relations.first().and_then(|r| r.targets.first()).cloned();
            let body = self.body();
            self.eat(";");
            return Ok(Some(Statement::StateAction {
                keyword,
                declares,
                name,
                type_ref,
                body,
                span: start.to(self.span_here()),
            }));
        }
        if matches!(self.text(), "perform" | "exhibit") {
            return self.behavior(start).map(Some);
        }
        if matches!(self.text(), "require" | "assume") {
            return self.requirement_constraint(start).map(Some);
        }
        if self.text() == "assert" {
            return self.assert_constraint(start).map(Some);
        }
        if self.text() == "objective" {
            return self.objective(start).map(Some);
        }
        if self.text() == "subject" {
            self.bump();
            let mut head = Head {
                name: self.name(),
                ..Head::default()
            };
            let mut relations = self.relations(false);
            head.multiplicity = self.multiplicity();
            relations.extend(self.relations(false));
            let type_ref = relations.first().and_then(|r| r.targets.first()).cloned();
            self.value_part(&mut head)?;
            // `subject = MassAnalysisCase::result;` binds the subject to a value rather than
            // typing it; a plain reference value remains its target for existing consumers.
            let type_ref = type_ref.or_else(|| match &head.value {
                Some(Expr::Ref(reference)) => Some(reference.clone()),
                _ => None,
            });
            self.body();
            self.eat(";");
            return Ok(Some(Statement::Subject {
                name: head.name,
                type_ref,
                value: head.value,
                value_text: head.value_text,
                value_span: head.value_span,
                is_initial: head.is_initial,
                is_default: head.is_default,
                span: start.to(self.span_here()),
            }));
        }
        // A `frame` is a requirement's frame, not a feature named `frame`: only when no
        // direction prefix was written. `in frame : SpatialFrame default defaultFrame` is a
        // parameter whose *name* is `frame`.
        if self.text() == "frame" && prefixes.direction.is_none() {
            self.bump();
            let name = self.name();
            let relations = self.relations(false);
            let type_ref = relations.first().and_then(|r| r.targets.first()).cloned();
            self.body();
            self.eat(";");
            return Ok(Some(Statement::Frame {
                name,
                type_ref,
                span: start.to(self.span_here()),
            }));
        }
        if self.text() == "allocate" {
            self.bump();
            let from = self.allocation_end("source")?;
            self.eat("to");
            let to = self.allocation_end("target")?;
            // `allocate a to b { ... }` carries a body; `allocate a to b;` does not.
            self.body();
            self.eat(";");
            return Ok(Some(Statement::Allocate {
                from,
                to,
                span: start.to(self.span_here()),
            }));
        }
        if self.text() == "include" {
            self.bump();
            // `include use case X` and `include X` are the same statement. `include use case i : T`
            // names the usage the include makes (`IncludeUseCaseUsage`, `SysML.xtext:2300-2306`):
            // the name comes first and the typing names the included case.
            self.eat("use");
            self.eat("case");
            let name = if self.at_name() && self.at(1).text == ":" {
                let name = self.name();
                self.expect(":")?;
                name
            } else {
                Name::default()
            };
            let target = self
                .type_ref()
                .ok_or("`include` needs a use case to include")?;
            // A `then include X { ... }` carries a body: the included case's own members.
            let body = self.body();
            self.eat(";");
            return Ok(Some(Statement::Include {
                name,
                target,
                body,
                span: start.to(self.span_here()),
            }));
        }
        if matches!(self.text(), "inv" | "invariant") {
            self.bump();
            let name = self.name();
            let body = self.body();
            let exprs = collect_expressions(&body);
            return Ok(Some(Statement::Invariant {
                name,
                exprs,
                span: start.to(self.span_here()),
            }));
        }
        Ok(None)
    }

    /// `connect a to b;` with the optional named-connector prefix already consumed.
    /// One end of an `allocate` statement.
    ///
    /// `allocate logical ::> torqueGenerator to physical ::> powerTrain` names an end that
    /// already exists and states the reference subsetting that makes it this allocation's end
    /// (`12b-Allocation-1.sysml:52`). The `::>` is part of the end, so reading the end as a bare
    /// expression stopped at it and the following `to` was reported missing.
    fn allocation_end(&mut self, which: &str) -> Result<Expr, String> {
        let span = self.span_here();
        let mut end = self
            .expr()?
            .ok_or_else(|| format!("`allocate` needs a {which}"))?;
        while self.text() == "::>" {
            self.bump();
            let Some(reference) = self.expr()? else {
                break;
            };
            end = Expr::Binary {
                op: "::>".to_string(),
                lhs: Box::new(end),
                rhs: Box::new(reference),
                span,
            };
        }
        Ok(end)
    }

    fn connect_statement(&mut self, name: Option<Name>, start: Span) -> Result<Statement, String> {
        self.expect("connect")?;
        let (from, to, extra) = if self.text() == "(" {
            let mut ends = self.nary_connector_ends()?.into_iter();
            (
                ends.next().unwrap_or_default(),
                ends.next().unwrap_or_default(),
                ends.collect::<Vec<String>>(),
            )
        } else {
            let from = self.connector_end()?;
            self.expect("to")?;
            (from, self.connector_end()?, Vec::new())
        };
        self.body();
        self.eat(";");
        Ok(Statement::Connect {
            name: name.unwrap_or_default(),
            type_ref: None,
            from,
            to,
            extra,
            span: start.to(self.span_here()),
        })
    }

    /// The ends of an n-ary connector, `(a, b, c)`, in the order written.
    ///
    /// `NaryConnectorPart` (`SysML.xtext:1079-1082`) requires a comma after the first end, so one
    /// end in parentheses is a syntax error rather than a connector with one end.
    fn nary_connector_ends(&mut self) -> Result<Vec<String>, String> {
        self.expect("(")?;
        let mut ends = vec![self.connector_end()?];
        self.expect(",")?;
        ends.push(self.connector_end()?);
        while self.eat(",") {
            ends.push(self.connector_end()?);
        }
        self.expect(")")?;
        Ok(ends)
    }

    /// One end written after a declaration head (`connection c : C connect a to b`), as authored
    /// text.
    ///
    /// The `::>` clause belongs to the end: `allocate logical ::> torqueGenerator to …` names a
    /// reference subsetting, and stopping at the path left the clause to be read as a member of the
    /// body while the end recorded `logical` — the end's own name — as the thing it refers to.
    fn connector_head_end(&mut self) -> Option<String> {
        // The same `ConnectorEnd` grammar as the statement form: an optional cross multiplicity,
        // an optional `name references` prefix, then the reference. Reading only a path here left
        // `connect [1] a to [1] b` with no ends and its text as phantom members — sixty-odd
        // connectors in the standard library alone, invisible because nothing was reported.
        if self.text() != "[" && !self.at_name() {
            return None;
        }
        self.connector_end().ok()
    }

    /// One end of a connect statement, as authored text.
    ///
    /// An end can carry an optional `name references` prefix and a multiplicity; the text kept is
    /// the target path, because that is what the resolver walks.
    fn connector_end(&mut self) -> Result<String, String> {
        // here left nothing to read. Callers that want the bare path strip it.
        let mut multiplicity = String::new();
        if self.text() == "[" {
            multiplicity.push('[');
            self.bump();
            while !self.is_eof() && self.text() != "]" {
                multiplicity.push_str(self.text());
                self.bump();
            }
            self.eat("]");
            multiplicity.push(']');
        }
        // `[multiplicity] a.b` or `name references a.b`. The name is the end's own
        // (`ConnectorEnd : … ( declaredName = Name ReferencesKeyword )? …`), so it is kept: the
        // end is written as `name ::> target`, the spelling the `::>` form below produces and the
        // projection reads. Skipping it left the end anonymous.
        if self.at_name() && self.at(1).text == "references" {
            let declared = self.cur().name_text();
            self.bump();
            self.bump();
            let target = self.type_ref().ok_or_else(|| {
                format!(
                    "expected what the end references, found {}",
                    self.describe_current()
                )
            })?;
            let text = format!("{declared} ::> {}", target.text);
            return Ok(if multiplicity.is_empty() {
                text
            } else {
                format!("{multiplicity} {text}")
            });
        }
        let name = self.type_ref().ok_or_else(|| {
            format!(
                "expected a connection end, found {}",
                self.describe_current()
            )
        })?;
        // `[1] cd ::> b.c.d` — an end may carry its own reference subsetting, which names what
        // the end refers to. Left unread, the `::>` and its path became a separate member of the
        // enclosing body and the end's text stopped at `cd`, so the resolver reported a
        // connection whose end names nothing (`valid_Connector0.sysml.xt:48`).
        let mut text = name.text;
        if self.text() == "::>" {
            self.bump();
            if let Some(reference) = self.type_ref() {
                text.push_str(" ::> ");
                text.push_str(&reference.text);
            }
        }
        Ok(if multiplicity.is_empty() {
            text
        } else {
            format!("{multiplicity} {text}")
        })
    }

    /// Whether a KerML `connector` names its two ends and nothing else before them.
    ///
    /// True when a `to` comes before the declaration ends and no `from`, typing or
    /// specialization does: those belong to the declared form, which `head` reads.
    fn at_bare_connector_ends(&self) -> bool {
        let mut i = 0usize;
        let mut depth = 0usize;
        loop {
            let t = self.at(i);
            match t.text.as_str() {
                "[" => depth += 1,
                "]" if depth > 0 => depth -= 1,
                _ if depth > 0 => {}
                "to" => return true,
                "from" | ":" | ":>" | ":>>" | "typed" | "subsets" | "redefines" | ";" | "{"
                | "}" => return false,
                _ if matches!(t.kind, TokenKind::Eof) => return false,
                _ => {}
            }
            i += 1;
        }
    }

    /// Whether a `flow` at the current position is the connector form (`flow a to b`) rather
    /// than a declaration (`flow f : T`, `flow f [0..*] ...`).
    ///
    /// Scans the path, its optional `: Type` and its optional `[multiplicity]`: a declaration
    /// reaches a `:>`, `:>>`, `{` or `;`, while the connector reaches `to` or `from`.
    fn is_flow_connector(&self) -> bool {
        let mut i = 0usize;
        let mut depth = 0usize;
        loop {
            match self.at(i).text.as_str() {
                "[" => depth += 1,
                "]" if depth > 0 => depth -= 1,
                ";" | "{" | "}" => return false,
                "of" | "from" | "to" | "=" | ":=" | "default" if depth == 0 => return true,
                _ if matches!(self.at(i).kind, TokenKind::Eof) => return false,
                _ => {}
            }
            i += 1;
        }
    }

    // An absent long name must not consume a clause following a short name.
    fn flow_name(&mut self) -> Name {
        let start = self.span_here();
        let mut name = Name {
            span: start,
            ..Name::default()
        };
        if matches!(self.kind(), TokenKind::ShortName) {
            name.short = Some(self.cur().name_text());
            self.bump();
        }
        if self.at_name()
            && !self.is_member_boundary()
            && !matches!(self.text(), "of" | "from" | "to" | "ordered" | "nonunique")
        {
            name.text = self.cur().name_text();
            name.span = start.to(self.span_here());
            self.bump();
        }
        name
    }

    // Unlike type_ref(), this leaves a bound for the owning feature and excludes clause keywords.
    fn flow_type_ref(&mut self) -> Option<TypeRef> {
        if !self.at_name()
            || self.is_member_boundary()
            || matches!(self.text(), "of" | "from" | "to" | "ordered" | "nonunique")
        {
            return None;
        }
        let start = self.span_here();
        let mut text = String::new();
        let end = loop {
            text.push_str(&self.cur().name_text());
            let end = self.span_here();
            self.bump();
            if matches!(self.text(), "::" | ".") && self.at_name_at(1) {
                text.push_str(&self.bump().text);
            } else {
                break end;
            }
        };
        Some(TypeRef {
            text,
            span: start.to(end),
        })
    }

    fn flow_bound(&mut self) -> Result<Option<Multiplicity>, String> {
        if self.text() != "[" {
            return Ok(None);
        }
        let mut i = 0usize;
        let mut depth = 0usize;
        loop {
            match self.at(i).text.as_str() {
                "[" => depth += 1,
                "]" => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(self.multiplicity());
                    }
                }
                ";" | "{" | "}" => return Err("expected a closing multiplicity bracket".into()),
                _ if matches!(self.at(i).kind, TokenKind::Eof) => {
                    return Err("expected a closing multiplicity bracket".into());
                }
                _ => {}
            }
            i += 1;
        }
    }

    // FeatureSpecialization* surrounding at most one MultiplicityPart. Targets use the same
    // Relation representation as ordinary declarations, so the lowerer need not reinterpret them.
    fn flow_specializations(&mut self, head: &mut Head) -> Result<(), String> {
        let mut has_multiplicity_part = false;
        loop {
            if matches!(self.text(), "[" | "ordered" | "nonunique") {
                if has_multiplicity_part {
                    return Err("a feature may have only one multiplicity part".into());
                }
                has_multiplicity_part = true;
                head.multiplicity = self.flow_bound()?;
                if self.eat("ordered") {
                    head.prefixes.is_ordered = true;
                    head.prefixes.is_nonunique = self.eat("nonunique");
                } else if self.eat("nonunique") {
                    head.prefixes.is_nonunique = true;
                    head.prefixes.is_ordered = self.eat("ordered");
                }
                continue;
            }
            let start = self.span_here();
            let (op, two_words, multiple) = match self.text() {
                ":" => (RelationOp::Typing, false, true),
                "defined" | "typed" if self.at(1).text == "by" => (RelationOp::Typing, true, true),
                ":>" | "subsets" => (RelationOp::Subsets, false, true),
                ":>>" | "redefines" => (RelationOp::Redefines, false, true),
                "::>" | "references" => (RelationOp::References, false, false),
                "=>" | "crosses" => (RelationOp::Crosses, false, false),
                _ => break,
            };
            self.bump();
            if two_words {
                self.expect("by")?;
            }
            let mut targets = vec![self
                .flow_type_ref()
                .ok_or("expected a specialization target")?];
            while multiple && self.eat(",") {
                targets.push(
                    self.flow_type_ref()
                        .ok_or("expected a specialization target after `,`")?,
                );
            }
            // `targets` was seeded with one entry above and only grows, so it is never empty.
            let end = targets
                .last()
                .map(|t| t.span)
                .unwrap_or_else(|| self.span_here());
            let span = start.to(end);
            head.relations.push(Relation { op, targets, span });
        }
        Ok(())
    }

    fn flow_payload(&mut self) -> Result<Head, String> {
        let checkpoint = self.pos;
        let start = self.span_here();
        let mut head = Head {
            name: self.flow_name(),
            span: start,
            ..Head::default()
        };
        self.flow_specializations(&mut head)?;
        let has_value = self.value_part(&mut head)?;
        if !head.relations.is_empty() || has_value {
            if head.relations.is_empty()
                && (head.multiplicity.is_some()
                    || head.prefixes.is_ordered
                    || head.prefixes.is_nonunique)
            {
                return Err("a payload multiplicity requires a specialization".into());
            }
            head.span = start.to(self.span_here());
            return Ok(head);
        }
        // Payload's last two alternatives are anonymous typing, with an optional bound on
        // either side of the type. A bare type must not become the payload's own name.
        self.pos = checkpoint;
        let multiplicity = self.flow_bound()?;
        let type_ref = self
            .flow_type_ref()
            .ok_or("expected a payload type, specialization or value")?;
        let trailing_multiplicity = self.flow_bound()?;
        if multiplicity.is_some() && trailing_multiplicity.is_some() {
            return Err("a payload may have only one multiplicity".into());
        }
        let multiplicity = trailing_multiplicity.or(multiplicity);
        let relation = Relation {
            op: RelationOp::Typing,
            span: type_ref.span,
            targets: vec![type_ref],
        };
        Ok(Head {
            multiplicity,
            relations: vec![relation],
            span: start.to(self.span_here()),
            ..Head::default()
        })
    }

    fn flow_statement(&mut self, start: Span, prefixes: Prefixes) -> Result<Statement, String> {
        let mut head = Head {
            prefixes,
            span: start,
            ..Head::default()
        };
        // The bare `a to b` and `a.b to c.d` alternatives declare no name.
        let bare_end = self.at_name() && matches!(self.at(1).text.as_str(), "to" | "." | "::");
        if !bare_end {
            head.name = self.flow_name();
            self.flow_specializations(&mut head)?;
            self.value_part(&mut head)?;
        }
        let payload = if self.eat("of") {
            Some(self.flow_payload()?)
        } else {
            None
        };
        // The entire ends group is optional. A closing `}` still belongs to the parent.
        if !matches!(self.text(), ";" | "{") {
            if !bare_end {
                self.expect("from")?;
            }
            head.from = Some(self.connector_end()?);
            self.expect("to")?;
            head.to = Some(self.connector_end()?);
        }
        head.span = start.to(self.span_here());
        let body = self.body();
        self.eat(";");
        Ok(Statement::Flow {
            head,
            payload,
            body,
            span: start.to(self.span_here()),
        })
    }

    fn bind_statement(&mut self, start: Span) -> Result<Statement, String> {
        // `binding a = b;` and `binding [1] bind a = b;` are the same statement: the `binding`
        // keyword introduces it, the `bind` keyword is optional, and multiplicities may appear on
        // the binding and on each end.
        self.eat("binding");
        // The binding's own name, type and multiplicity appear before `bind`/`of`. When neither
        // follows, the token after `binding` is the left-hand end and must not be taken as a name.
        if matches!(self.text(), "bind" | "[")
            || self.text() == ":"
            || (self.at_name() && matches!(self.at(1).text.as_str(), "of" | ":" | "bind" | "["))
        {
            let _ = self.name();
            let _ = self.relations(false);
            let _ = self.multiplicity();
        }
        self.eat("bind");
        // `binding loopBack of [0..1] x = [1] y;` — the `of` clause states the connector kind.
        if self.eat("of") {
            let _ = self.relations(false);
        }
        let _ = self.multiplicity();
        let lhs = self
            .feature_path()?
            .or(self.expr()?)
            .ok_or("`bind` needs a left-hand side")?;
        self.expect("=")?;
        let _ = self.multiplicity();
        let rhs = self
            .feature_path()?
            .or(self.expr()?)
            .ok_or("`bind` needs a right-hand side")?;
        // `bind a = b = c` states a third end. A binding is binary, and reading only two left the
        // rest in the stream as a stray member. Collected so the rule can report it rather than
        // the parser guessing.
        let mut extra = Vec::new();
        while self.text() == "=" {
            self.bump();
            let _ = self.multiplicity();
            if let Some(third) = self.feature_path()?.or(self.expr()?) {
                extra.push(third);
            } else {
                break;
            }
        }
        // `bind a = b { ... }` is legal: the body documents the binding.
        self.body();
        self.eat(";");
        Ok(Statement::Bind {
            lhs,
            rhs,
            extra,
            span: start.to(self.span_here()),
        })
    }

    fn assign_statement(&mut self, start: Span) -> Result<Statement, String> {
        self.expect("assign")?;
        self.eat("then");
        let lhs = self
            .feature_path()?
            .or(self.expr()?)
            .ok_or("`assign` needs a target")?;
        // Both spellings appear: `:=` and `=`.
        if !self.eat(":=") {
            self.eat("=");
        }
        let rhs = self
            .assign_value()?
            .ok_or_else(|| format!("`assign` needs a value, found {}", self.describe_current()))?;
        self.eat(";");
        Ok(Statement::Assign {
            lhs,
            rhs,
            span: start.to(self.span_here()),
        })
    }

    /// The right-hand side of an `assign`: an expression, `new T(...)`, or `null`.
    ///
    /// `new` introduces an instance rather than referencing one, so it is not an expression in
    /// the ordinary operators — but a model writes it exactly where a value goes, and refusing it
    /// made every action that creates a link unreadable.
    fn assign_value(&mut self) -> Result<Option<Expr>, String> {
        if self.text() == "new" {
            let start = self.span_here();
            self.bump();
            let Some(type_ref) = self.type_ref() else {
                return Err(format!(
                    "`new` needs a type, found {}",
                    self.describe_current()
                ));
            };
            // The constructor's arguments are not values this layer evaluates, but they are
            // part of the statement and are parsed so the parens balance.
            if self.text() == "(" {
                self.bump();
                let _ = self.call_args()?;
            }
            return Ok(Some(Expr::Opaque {
                text: format!("new {}", type_ref.text),
                children: Vec::new(),
                span: start.to(self.span_prev()),
            }));
        }
        self.expr()
    }

    /// `succession first a then b;`, with an optional guard and name.
    fn succession(&mut self, start: Span) -> Result<Statement, String> {
        self.expect("succession")?;
        // `succession flow a to b` is a `SuccessionFlowUsage`, whose keyword is the pair
        // (`SuccessionFlowKeyword : SuccessionKeyword FlowKeyword`, `SysML.xtext:1274-1279`), and
        // it shares `FlowDeclaration` with `flow`. Read as a succession, `flow` became the *name*
        // of a one-ended succession and `a to b` spilled into the enclosing body as members called
        // `a`, `b` and `q` — which is where `valid_ActionUsage.sysml.xt:63`'s bogus duplicate
        // members came from.
        if self.text() == "flow" {
            self.bump();
            return self.flow_statement(start, Prefixes::default());
        }
        self.eat("all");
        // The succession's own multiplicity can come before its name (`succession [1] a then b`)
        // or after it (`succession s [1] first a then b`); both spellings appear in the library.
        let _ = self.multiplicity();
        // A succession may be *named* (`succession s1 : AB first a then b`) but the name is
        // optional and the endpoint that follows is usually a bare path. Only consume a name
        // when what follows can be a declaration clause rather than an endpoint.
        // A name is consumed only when the token after it starts a declaration clause. With a
        // bare `succession a then b`, `a` is the first endpoint, not the succession's name, and
        // consuming it here left the endpoint chain pointing at the wrong token.
        let name = if self.at_name()
            && matches!(
                self.at(1).text.as_str(),
                ":" | ":>" | ":>>" | "::>" | "[" | "=" | "subsets" | "specializes" | "redefines"
            ) {
            self.name()
        } else {
            Name::default()
        };
        // Only a *named* succession carries a typing clause; a bare `succession a then b` must
        // not try to read a relation, which would swallow the first endpoint as its target.
        if !name.is_empty() {
            let _ = self.relations(false);
        }
        let _ = self.multiplicity();
        self.eat("first");
        let _ = self.multiplicity();
        let first = self.succession_end()?;
        let guard = if self.text() == "if" {
            self.bump();
            self.expr()?
        } else {
            None
        };
        let then = if self.eat("then") {
            let _ = self.multiplicity();
            self.succession_end()?
        } else {
            None
        };
        self.eat(";");
        Ok(Statement::Succession {
            name,
            first,
            then,
            trigger: None,
            trigger_kind: None,
            trigger_type: None,
            trigger_via: None,
            guard,
            effect: None,
            implicit_source: false,
            span: start.to(self.span_here()),
        })
    }

    /// A dotted feature path, where any segment may be a reserved word.
    ///
    /// `vehicle.engine.mass` and `do.startShot` are the same shape. The ordinary expression
    /// ladder reads a leading keyword as an operator and stops, so a path whose first segment is
    /// reserved has to be assembled here.
    fn feature_path(&mut self) -> Result<Option<Expr>, String> {
        if !self.at_name() {
            return Ok(None);
        }
        let start = self.span_here();
        let mut text = String::new();
        let mut end = start;
        loop {
            if !self.at_name() {
                break;
            }
            text.push_str(&self.cur().name_text());
            end = self.span_here();
            self.bump();
            if (self.text() == "." || self.text() == "::") && self.at_name_at(1) {
                text.push_str(self.text());
                self.bump();
                continue;
            }
            break;
        }
        Ok(Some(Expr::Ref(TypeRef {
            text,
            span: start.to(end),
        })))
    }

    /// One endpoint of a succession.
    ///
    /// Endpoints are feature paths, and a path segment may be a reserved word: `do.startShot`
    /// names the feature `do` and then a feature of it. Parsing these through the ordinary
    /// expression ladder read `do` as a keyword and then failed on `.startShot`, so the path has
    /// to be assembled directly.
    fn succession_end(&mut self) -> Result<Option<Expr>, String> {
        // A succession end may carry its own multiplicity (`first [0..1] a then [1] b`). Reading
        // the bracket as a collection literal turned the end into a sequence, and a rule bounding
        // it had nothing to read.
        let mut multiplicity = String::new();
        if self.text() == "[" {
            multiplicity.push('[');
            self.bump();
            while !self.is_eof() && self.text() != "]" {
                multiplicity.push_str(self.text());
                self.bump();
            }
            self.eat("]");
            multiplicity.push(']');
        }
        if !self.at_name() {
            return self.expr();
        }
        let start = self.span_here();
        let mut text = multiplicity;
        if !text.is_empty() {
            text.push(' ');
        }
        let mut end = start;
        loop {
            if !self.at_name() {
                break;
            }
            text.push_str(&self.cur().name_text());
            end = self.span_here();
            self.bump();
            if (self.text() == "." || self.text() == "::") && self.at_name_at(1) {
                text.push_str(self.text());
                self.bump();
                continue;
            }
            break;
        }
        Ok(Some(Expr::Ref(TypeRef {
            text,
            span: start.to(end),
        })))
    }

    /// Whether the token at `offset` can be a name.
    fn at_name_at(&self, offset: usize) -> bool {
        matches!(
            self.at(offset).kind,
            TokenKind::Ident | TokenKind::Keyword | TokenKind::QuotedName
        )
    }

    /// `if c then ... else ...`, `while`, `loop`, `for`.
    fn control_flow(&mut self, start: Span) -> Result<Statement, String> {
        let keyword = self.bump().text;
        if keyword == "for" {
            // `ForLoopNode` is `'for' ownedRelationship += ForVariableDeclarationMember 'in'
            // ownedRelationship += NodeParameterMember ownedRelationship +=
            // ActionBodyParameterMember` (`SysML.xtext:1624-1630`): the variable is written, and
            // `in` cannot stand where a name belongs. Accepting `for in 1..3` lowered it to a
            // control flow with no loop variable, which is what made the two `for` rules look
            // inapplicable.
            if self.text() == "in" {
                return Err("`for` needs a variable before `in`".to_string());
            }
            let variable = self.name();
            if variable.is_empty() {
                return Err("`for` needs a variable before `in`".to_string());
            }
            if !self.eat("in") {
                return Err(format!(
                    "`for {}` needs `in` and a collection, found {}",
                    variable.display(),
                    self.cur().text
                ));
            }
            let source = self.expr()?.ok_or("`for` needs a collection to iterate")?;
            if !self.at_body() {
                return Err("`for` needs a body".to_string());
            }
            let body = self.body();
            return Ok(Statement::For {
                variable,
                source,
                body,
                span: start.to(self.span_here()),
            });
        }
        // `IfNode` writes `'if' ExpressionParameterMember ActionBodyParameterMember`
        // (`SysML.xtext:1596-1601`) and `WhileLoopNode` writes either `'while'
        // ExpressionParameterMember` or `'loop' EmptyParameterMember`, then the body
        // (`:1615-1622`). So `if` and `while` always state a condition; only `loop` may omit it,
        // and it writes an empty member in its place.
        let condition = if keyword == "if" || keyword == "while" {
            let Some(condition) = self.expr()? else {
                return Err(format!(
                    "`{keyword}` needs a condition, found {}",
                    self.cur().text
                ));
            };
            Some(condition)
        } else {
            None
        };
        let mut branches = Vec::new();
        // `then` branch: a bare member or a braced body.
        if self.eat("then") {
            let b = self.branch_body();
            branches.push(("then".to_string(), b));
        } else if self.at_body() {
            let b = self.body();
            branches.push(("body".to_string(), b));
        }
        if self.eat("else") {
            let b = self.branch_body();
            branches.push(("else".to_string(), b));
        }
        // The body membership is not optional in any of the three productions: `if c;` with
        // nothing to do and `while c;` with no body are both outside the grammar.
        if branches.is_empty() {
            return Err(format!("`{keyword}` needs a body"));
        }
        self.eat(";");
        Ok(Statement::ControlFlow {
            keyword,
            condition,
            branches,
            span: start.to(self.span_here()),
        })
    }

    /// Whether the next token opens a body.
    fn at_body(&self) -> bool {
        self.cur().text == "{"
    }

    /// Whether the `if` at the cursor begins an *expression* rather than a statement.
    ///
    /// The two spellings differ in one token: `if c ? a else b` is a value and
    /// `if c then a else b` is a statement (`SysML.xtext:1596-1601` gives the statement form a
    /// body membership, and the conditional expression is a different production). The `?` can sit
    /// far from the keyword — `if i <= 1? indexes#(1) else …` — so this scans forward to the end of
    /// the statement and looks for one at bracket depth zero.
    fn if_is_an_expression(&self) -> bool {
        let mut depth = 0i32;
        let mut i = 1usize;
        loop {
            let text = self.at(i).text.as_str();
            if text.is_empty() {
                return false;
            }
            match text {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                "?" if depth == 0 => return true,
                "then" if depth == 0 => return false,
                ";" if depth == 0 => return false,
                _ => {}
            }
            i += 1;
        }
    }

    /// One branch of a conditional: a braced body, or a single member.
    fn branch_body(&mut self) -> Vec<BodyMember> {
        if self.text() == "{" {
            return self.body();
        }
        // A single member without braces.
        let before = self.pos;
        match self.parse_member(false) {
            Ok(Some(m)) => vec![m],
            _ => {
                self.pos = before;
                Vec::new()
            }
        }
    }

    /// `merge m { ... }` and the other control nodes.
    fn control_node(&mut self, start: Span) -> Result<Statement, String> {
        let keyword = self.bump().text;
        let name = self.name();
        let body = self.body();
        self.eat(";");
        Ok(Statement::ControlNode {
            keyword,
            name,
            body,
            span: start.to(self.span_here()),
        })
    }

    /// A transition's accept payload: `AcceptParameterPart` is a payload parameter, optionally
    /// typed, then an optional `via` receiver (`SysML.xtext`, `AcceptParameterPart`).
    ///
    /// `transition t first start accept p : Anything via receiver then done` stopped after `p`,
    /// and `: Anything`, `via`, `receiver` and `then done` became phantom members of the state
    /// (`Actions.sysml:222`). The payload's type and receiver are consumed; the trigger records
    /// the payload, as it did before.
    fn accept_payload(&mut self) -> Result<Option<Expr>, String> {
        let payload = if self.text() == ":" {
            None
        } else {
            self.succession_end()?
        };
        // Both are kept for the transition that owns the trigger, which takes them when it is
        // built: the payload's type says what the trigger accepts, and `via` names the port.
        self.pending_trigger_type = None;
        self.pending_trigger_via = None;
        if self.eat(":") {
            self.pending_trigger_type = Some(self.type_ref().ok_or_else(|| {
                format!(
                    "expected the accepted payload's type, found {}",
                    self.describe_current()
                )
            })?);
        }
        if self.eat("via") {
            self.pending_trigger_via = self.succession_end()?;
        }
        Ok(payload)
    }

    /// `accept x : T;` / `send x : T;`
    /// `accept x;` / `send x to y;`.
    ///
    /// `owns_then` says whether a following `then <target>` belongs to this message. It does when
    /// the message *is* the statement (`accept 'Vehicle Start Signal' … then starting;`), and it
    /// does not when the message is a transition's `do` effect: there the `then` is the
    /// transition's own target, and consuming it here left every `… do send x to y then z`
    /// transition without one.
    fn message(&mut self, start: Span, owns_then: bool) -> Result<Statement, String> {
        let keyword = self.bump().text;
        // `send new 'Start Signal'() to vehicle1_c1.vehicleController` — the payload is a
        // constructor, and the target follows `to`. Both have to be consumed here or their
        // tokens become members of the enclosing body.
        let mut name = Name::default();
        let mut type_ref = None;
        // The feature the message is addressed to, from `to` or `via`. Kept because a message
        // without a far end cannot be placed on a lifeline and no rule can ask whether the
        // receiver exists. `at` (a time) and `from` (a source) are still consumed and dropped.
        let mut addressed: Option<Expr> = None;
        // The two ends apart. A send's `via` names its sender and its `to` its receiver
        // (`SenderReceiverPart`, `SysML.xtext:1516-1521`); an accept's `via` names its receiver
        // (`AcceptParameterPart`, `:1450-1453`). `addressed` keeps whichever was written last,
        // which is the end a lifeline needs.
        let mut via: Option<Expr> = None;
        let mut to: Option<Expr> = None;
        let mut has_payload = true;
        let is_send = keyword == "send";
        // Whether `to` / `via` / `from` / `at` clauses follow. An accept trigger written
        // `when <expr>`, `after <expr>` or `at <expr>` has none of its own.
        let mut reads_ends = true;
        if self.eat("new") {
            type_ref = self.type_ref();
            if self.text() == "(" {
                self.bump();
                let _ = self.call_args()?;
            }
        } else if matches!(self.text(), "when" | "after") {
            // `accept when <expr>` / `accept after <expr>`: the keyword names a trigger kind, and
            // the name slot would otherwise take it for the payload and leave the expression to be
            // re-parsed as a member of the enclosing body (`do` then became a state subaction that
            // the model never wrote).
            reads_ends = false;
            self.bump();
            let _ = self.expr()?;
        } else if matches!(self.text(), "at") && !(self.at_name_at(1) && self.at(2).text == ":") {
            reads_ends = false;
            self.bump();
            let _ = self.succession_end()?;
        } else if is_send && (matches!(self.text(), "via" | "to" | ";") || self.at_body()) {
            // `send via p;`, `send to q;` and `send;` write no payload. When `via` or `to` follows,
            // the grammar's `EmptyParameterMember` stands in for it (`SysML.xtext:1499-1506`).
            // Reading `via` or `to` as the payload's name left the end after it as a stray member.
            has_payload = false;
        } else if is_send {
            // A send's payload is an expression: `NodeParameter` binds an `OwnedExpression`
            // (`SysML.xtext:1523-1533`). A bare name that ends at a clause keyword or at the end
            // of the statement is read as before. Anything else (`send S()`, `send q.signal`,
            // `send 5`) is read again as an expression; it used to stop at the first token after
            // the name and leave the rest of the statement as stray members. An invocation of a
            // type names the type, as `new T()` does.
            let before = self.pos;
            if self.at_name() {
                name = self.name();
                let relations = self.relations(false);
                type_ref = relations.first().and_then(|r| r.targets.first()).cloned();
            }
            let ends_here = self.at_body()
                || matches!(
                    self.text(),
                    "via" | "to" | ";" | "if" | "do" | "then" | "from" | "at"
                );
            if !ends_here {
                self.pos = before;
                name = Name::default();
                type_ref = None;
                if let Some(Expr::Call { callee, .. }) = self.expr()? {
                    if let Expr::Ref(callee) = *callee {
                        type_ref = Some(callee);
                    }
                }
            }
        } else {
            name = self.name();
            let relations = self.relations(false);
            type_ref = relations.first().and_then(|r| r.targets.first()).cloned();
        }
        // `send payload via port to target`, `send new T() to x` and `accept payload at time`:
        // each clause names something else. The two ends are kept; `from` and `at` are consumed
        // and dropped.
        if reads_ends {
            loop {
                let clause = if self.eat("to") {
                    "to"
                } else if self.eat("via") {
                    "via"
                } else if self.eat("from") || self.eat("at") {
                    "other"
                } else {
                    break;
                };
                let end = self.succession_end()?;
                match clause {
                    "to" => {
                        to = end.clone();
                        addressed = end;
                    }
                    "via" => {
                        via = end.clone();
                        addressed = end;
                    }
                    _ => {}
                }
            }
        }
        let mut took_then = false;
        // The accept/send action carries up to three more clauses, in any order:
        // `if <guard>`, `do <behavior>` and `then <target>`.
        // `accept 'Vehicle Start Signal' if vehicle1_c1.'brake pedal depressed' do send ... to
        // ... then starting;` splits into all three, and leaving them unconsumed made `if` a
        // control-flow statement, `do` a behaviour, and the guard's head an attribute of the
        // enclosing state — which is how a phantom `vehicle1_c1` attribute appeared at state
        // level and shadowed the real part in every later lookup.
        loop {
            if self.eat("if") {
                let _ = self.expr()?;
                continue;
            }
            if self.eat("do") {
                // The clause names the action the accept/send performs. It must not go through
                // `statement()`: an action reference there becomes a `Statement::StateAction`
                // whose keyword is `do`, which the lowering then reads as a state subaction the
                // model never wrote — the state ends up with a second `do`.
                if matches!(self.text(), "send" | "accept") {
                    let inner = start.to(self.span_here());
                    let _ = self.message(inner, false)?;
                } else if self.eat("assign") {
                    let _ = self.assign_statement(start)?;
                } else {
                    let _ = self.succession_end()?;
                }
                continue;
            }
            if owns_then && self.eat("then") {
                took_then = true;
                let _ = self.succession_end()?;
                continue;
            }
            break;
        }
        // A message nested as a transition's `do` effect ends where the transition goes on:
        // `do send new S() to c then starting { … }` — the `then` and the body are the
        // transition's (`5-State-based Behavior-1.sysml:85-86`).
        let mut body = Vec::new();
        if owns_then {
            let members = self.body();
            // A send's own body holds members of the send: `send x via p { in extra; }` declares a
            // further input parameter. After `then` the body is the transition's, and an accept's
            // body is not kept.
            if is_send && !took_then {
                body = members;
            }
            self.eat(";");
        }
        Ok(Statement::Message {
            keyword,
            name,
            type_ref,
            addressed,
            has_payload,
            via,
            to,
            body,
            span: start.to(self.span_here()),
        })
    }

    /// `perform x : T;` / `exhibit s : T;`
    fn behavior(&mut self, start: Span) -> Result<Statement, String> {
        let keyword = self.bump().text;
        // `perform action x : T` / `exhibit state s : T` name the kind after the keyword, which is
        // the alternative that declares a name. Without it what follows references a feature:
        // `perform b;` denotes `b`, and `perform b.a;` denotes `b`'s member `a`.
        let declares = matches!(self.text(), "action" | "state");
        if declares {
            self.bump();
        }
        let name = self.name();
        // `perform vehicleMassTest.collectData { ... }` refines a feature of an existing
        // performance instead of declaring a new one. The dotted remainder is kept apart so the
        // lowering layer can place the body under that feature rather than under the performance.
        let mut path: Vec<String> = Vec::new();
        while self.text() == "." && self.at_name_at(1) {
            self.bump();
            path.push(self.cur().name_text());
            self.bump();
        }
        let relations = self.relations(false);
        // Only a `:` clause types the performance. `perform A.B :>> C` redefines `C`; reading the
        // redefinition target as the type made the performance name itself, and every rule asking
        // "what does this perform" got the wrong answer.
        let type_ref = relations
            .iter()
            .find(|r| r.op == RelationOp::Typing)
            .and_then(|r| r.targets.first())
            .cloned();
        // `perform action :>> doXorY = doX;` (`7a1-Variant Configuration - General Concept-a.sysml:66`)
        // — a `UsageDeclaration` may carry a `ValuePart`. Left unread, `= doX` started a phantom
        // anonymous member and the variant's choice reached no element.
        let mut value_head = Head::default();
        self.value_part(&mut value_head)?;
        // A performance may carry its own body (`perform 'provide power' { bind ... }`). Its
        // members belong to the performance, not to whatever contains it.
        let body = self.body();
        self.eat(";");
        Ok(Statement::Behavior {
            keyword,
            declares,
            name,
            path,
            type_ref,
            value: value_head.value,
            value_span: value_head.value_span,
            body,
            span: start.to(self.span_here()),
        })
    }

    /// `event a;` / `event a.areal;` — the reference form of an event occurrence.
    ///
    /// `event` has already been consumed. What follows is an `OwnedReferenceSubsetting`, which is
    /// a name or a feature chain and nothing else, then an optional `FeatureSpecializationPart`
    /// (`:> x`, `: T`, `[n]`) and `UsageCompletion`. No name is declared: the element is named
    /// after the feature it references, the way KerML derives a reference feature's name from its
    /// first chaining feature.
    fn event_reference(&mut self, start: Span) -> Result<Statement, String> {
        // [`Parser::feature_path`] rather than [`Parser::type_ref`]: the latter swallows a
        // trailing `[n]` into the text, which is right for a type (`ScalarValue[1]` is how the
        // units layer reads it) and wrong for a reference that has to resolve.
        let Some(Expr::Ref(reference)) = self.feature_path()? else {
            return Err(format!(
                "`event` needs the occurrence it denotes, found {}",
                self.describe_current()
            ));
        };
        // `event 'glucose level undetected'[*];` — the bound is the event's own, and it sits
        // before or after the specialization clause (`FeatureSpecializationPart`:
        // `FeatureSpecialization+ MultiplicityBounds? | MultiplicityBounds FeatureSpecialization*`).
        let multiplicity = self.multiplicity();
        let relations = self.relations(false);
        let multiplicity = multiplicity.or_else(|| self.multiplicity());
        let type_ref = relations
            .iter()
            .find(|r| r.op == RelationOp::Typing)
            .and_then(|r| r.targets.first())
            .cloned();
        let body = self.body();
        self.eat(";");
        Ok(Statement::Event {
            reference,
            multiplicity,
            type_ref,
            body,
            span: start.to(self.span_here()),
        })
    }

    /// `require constraint { ... }` / `assume constraint c { ... }` / `require name;`
    fn requirement_constraint(&mut self, start: Span) -> Result<Statement, String> {
        let keyword = self.bump().text;
        let is_assume = keyword == "assume";
        // `assume #g constraint b;` annotates the constraint, and the tag sits between the keyword
        // and `constraint`. The expression reader cannot start at `#`, so it gave up and stored the
        // clause as `Expr::Opaque`, which the evaluator then reported as "expression form is not
        // evaluated" — a diagnostic about our own limitation where the model was ordinary.
        // Measured: the same clause without the tag, or with the tag before `assume`, decides fine;
        // only the position between the two keywords broke it.
        //
        // The tag's meaning is metadata on the membership, which this reader does not model, so it
        // is skipped rather than attached. Reading it as a prefix here would be a larger change and
        // nothing in the suite depends on it.
        while self.text() == "#" || self.text() == "@" {
            self.bump();
            if self.at_name() {
                self.bump();
            }
        }
        // The `constraint` keyword is optional: `require name;` references an existing constraint.
        let has_keyword = self.eat("constraint");
        let name = if has_keyword || self.at_name() {
            self.name()
        } else {
            Name::default()
        };
        // A `require constraint name : Type { ... }` may name the constraint's type. It is kept:
        // the element is a `ConstraintUsage` typed by a `ConstraintDefinition`, and a rule that
        // asks what a constraint is typed by had nothing to read while the target was dropped.
        let type_ref = if has_keyword {
            self.relations(false)
                .into_iter()
                .find(|r| r.op == RelationOp::Typing)
                .and_then(|r| r.targets.into_iter().next())
        } else {
            None
        };
        if !has_keyword && !name.is_empty() && self.text() == ";" {
            // `require R;` — a reference to an existing constraint, not an inline condition.
            let target = TypeRef {
                text: name.display(),
                span: name.span,
            };
            self.bump();
            return Ok(Statement::Assert {
                is_negated: false,
                name: Name::default(),
                type_ref: None,
                type_refs: Vec::new(),
                exprs: Vec::new(),
                target: Some(target),
                span: start.to(self.span_here()),
            });
        }
        let exprs = self.constraint_expressions();
        Ok(Statement::Constraint {
            is_assume,
            name,
            type_ref,
            exprs,
            span: start.to(self.span_here()),
        })
    }

    /// The expression members of a constraint body, or a single trailing expression.
    fn constraint_expressions(&mut self) -> Vec<Expr> {
        if self.text() == "{" {
            let body = self.body();
            return collect_expressions(&body);
        }
        if self.is_eof() || self.text() == ";" {
            return Vec::new();
        }
        // `assume constraint #g b;` annotates the expression, and `expr()` reads a leading `#` as a
        // metadata *operand* (`parser.rs:2356`), so the whole clause became `Expr::Opaque` and the
        // evaluator reported "expression form is not evaluated". The tag is metadata on the
        // membership, not part of the condition, so it is skipped here for the same reason as the
        // one between `assume` and `constraint`.
        while self.text() == "#" || self.text() == "@" {
            self.bump();
            if self.at_name() {
                self.bump();
            }
        }
        self.expr().ok().flatten().into_iter().collect()
    }

    /// `assert constraint { ... }` / `assert not constraint { ... }` / `assert path;`
    fn assert_constraint(&mut self, start: Span) -> Result<Statement, String> {
        self.expect("assert")?;
        let is_negated = self.eat("not");
        let has_keyword = self.eat("constraint");
        let name = if has_keyword || self.at_name() {
            self.name()
        } else {
            Name::default()
        };
        let mut type_ref = None;
        let mut type_refs: Vec<TypeRef> = Vec::new();
        if has_keyword && self.text() == ":" {
            let rels = self.relations(false);
            type_ref = rels.first().and_then(|r| r.targets.first()).cloned();
            // Every type the clause names, not only the first: `assert constraint two_types :
            // AConstraint, ABlock;` states two, and the rule that rejects it counts them
            // (`ConstraintUsage_Invalid.sysml.xt:39`).
            type_refs = rels
                .iter()
                .flat_map(|r| r.targets.iter().cloned())
                .collect();
        }
        // `assert path;` — a reference to an existing constraint. Only when no type was written:
        // `assert constraint named : A;` also ends at `;` after its clause, and treating it as a
        // reference moved its name into `target` and cleared it, so the declaration lost the name
        // the model gave it (`ConstraintUsage_Invalid.sysml.xt:42` asserts on that spelling).
        if self.text() == ";" && type_ref.is_none() {
            self.bump();
            let target = if name.is_empty() {
                None
            } else {
                Some(TypeRef {
                    text: name.display(),
                    span: name.span,
                })
            };
            return Ok(Statement::Assert {
                is_negated,
                name: Name::default(),
                type_ref,
                type_refs,
                exprs: Vec::new(),
                target,
                span: start.to(self.span_here()),
            });
        }
        let exprs = self.constraint_expressions();
        Ok(Statement::Assert {
            is_negated,
            name,
            type_ref,
            type_refs,
            exprs,
            target: None,
            span: start.to(self.span_here()),
        })
    }

    /// `objective { ... }` / `objective name : T { ... }`
    fn objective(&mut self, start: Span) -> Result<Statement, String> {
        self.expect("objective")?;
        let name = self.name();
        let rels = self.relations(false);
        let type_ref = rels.first().and_then(|r| r.targets.first()).cloned();
        let body = self.body();
        self.eat(";");
        Ok(Statement::Objective {
            name,
            type_ref,
            body,
            span: start.to(self.span_here()),
        })
    }
}

/// The span covering a body's members, or `fallback` when empty.
fn body_span(body: &[BodyMember], fallback: Span) -> Span {
    match (body.first(), body.last()) {
        (Some(f), Some(l)) => f.span().to(l.span()),
        _ => fallback,
    }
}

/// Every expression member of a body, in authored order.
///
/// A constraint body states its condition as a bare expression; a calculation body states its
/// result the same way. Both arrive here.
fn collect_expressions(body: &[BodyMember]) -> Vec<Expr> {
    body.iter()
        .filter_map(|m| match m {
            BodyMember::Statement(Statement::Expression { expr, .. }) => Some(expr.clone()),
            _ => None,
        })
        .collect()
}
