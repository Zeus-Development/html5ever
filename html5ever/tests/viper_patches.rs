// VIPER-PATCH(svg-script-pause): the fork tests of Viper's patches, as
// PATCHES.md lists them.
// VIPER-PATCH(select-fragment-input): its tests are the last two below.

use html5ever::interface::{ElementFlags, NodeOrText, QuirksMode};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{BufferQueue, Tokenizer};
use html5ever::tree_builder::{TreeBuilder, TreeSink};
use html5ever::{ns, Attribute, ExpandedName, LocalName, QualName, TokenizerResult};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// A sink that keeps element names and logs element creation, pops and
/// parse errors, so a test can see what the tree builder did before each
/// pause.
#[derive(Default)]
struct Sink {
    next_id: Cell<usize>,
    names: RefCell<HashMap<usize, &'static QualName>>,
    log: RefCell<Vec<String>>,
}

impl Sink {
    fn new_id(&self) -> usize {
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        id
    }

    fn describe(&self, node: usize) -> String {
        let names = self.names.borrow();
        let name = names.get(&node).expect("not an element");
        format!(
            "{} {}",
            name.ns.rsplit('/').next().unwrap_or(""),
            name.local
        )
    }
}

impl TreeSink for Sink {
    type Handle = usize;
    type Output = Self;
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> Self {
        self
    }

    fn parse_error(&self, msg: Cow<'static, str>) {
        self.log.borrow_mut().push(format!("error {msg}"));
    }

    fn get_document(&self) -> usize {
        0
    }

    fn elem_name(&self, target: &usize) -> ExpandedName<'_> {
        self.names
            .borrow()
            .get(target)
            .expect("not an element")
            .expanded()
    }

    fn create_element(&self, name: QualName, _: Vec<Attribute>, _: ElementFlags) -> usize {
        let id = self.new_id();
        self.names
            .borrow_mut()
            .insert(id, Box::leak(Box::new(name)));
        self.log
            .borrow_mut()
            .push(format!("create {}", self.describe(id)));
        id
    }

    fn create_comment(&self, _: StrTendril) -> usize {
        self.new_id()
    }

    fn create_pi(&self, _: StrTendril, _: StrTendril) -> usize {
        self.new_id()
    }

    fn append(&self, _: &usize, _: NodeOrText<usize>) {}

    fn append_based_on_parent_node(&self, _: &usize, _: &usize, _: NodeOrText<usize>) {}

    fn append_doctype_to_document(&self, _: StrTendril, _: StrTendril, _: StrTendril) {}

    fn get_template_contents(&self, target: &usize) -> usize {
        *target
    }

    fn same_node(&self, x: &usize, y: &usize) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _: QuirksMode) {}

    fn append_before_sibling(&self, _: &usize, _: NodeOrText<usize>) {}

    fn add_attrs_if_missing(&self, _: &usize, _: Vec<Attribute>) {}

    fn remove_from_parent(&self, _: &usize) {}

    fn reparent_children(&self, _: &usize, _: &usize) {}

    fn pop(&self, node: &usize) {
        self.log
            .borrow_mut()
            .push(format!("pop {}", self.describe(*node)));
    }
}

/// Parses `html` in one chunk and returns the log, with `pause <element>`
/// where the tokenizer returned a script pause, resuming after each pause
/// as a parser driver does once the script has run.
fn parse(html: &str) -> Vec<String> {
    let builder = TreeBuilder::new(Sink::default(), Default::default());
    let tokenizer = Tokenizer::new(builder, Default::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));
    while let TokenizerResult::Script(node) = tokenizer.feed(&input) {
        let sink = &tokenizer.sink.sink;
        let entry = format!("pause {}", sink.describe(node));
        sink.log.borrow_mut().push(entry);
    }
    tokenizer.end();
    let log = tokenizer.sink.sink.log.borrow().clone();
    log.into_iter()
        .filter(|entry| {
            entry.starts_with("pause") || entry.contains("svg") || entry.contains("math")
        })
        .collect()
}

/// HTML Standard, "the rules for parsing tokens in foreign content": an
/// end tag whose tag name is "script", if the current node is an SVG
/// script element, pops it and pauses before the next input character, so
/// the elements after it are not created yet.
#[test]
fn an_svg_script_end_tag_pauses_the_parser() {
    let log = parse("<!DOCTYPE html><svg><script>a()</script><rect/></svg><script>b()</script>");
    assert_eq!(
        log,
        [
            "create svg svg",
            "create svg script",
            "pop svg script",
            "pause svg script",
            "create svg rect",
            "pause xhtml script",
        ]
    );
}

/// The same section, a start tag with its self-closing flag set: a
/// "script" whose new current node is in the SVG namespace is acknowledged
/// (no "Unacknowledged self-closing tag" error) and handled as the "script"
/// end tag.
#[test]
fn a_self_closing_svg_script_pauses_the_parser() {
    let log = parse("<!DOCTYPE html><svg><script href='a.js'/><rect/></svg>");
    assert_eq!(
        log,
        [
            "create svg svg",
            "create svg script",
            "pop svg script",
            "pause svg script",
            "create svg rect",
        ]
    );
    let builder = TreeBuilder::new(Sink::default(), Default::default());
    let tokenizer = Tokenizer::new(builder, Default::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice("<svg><script/></svg>"));
    while let TokenizerResult::Script(_) = tokenizer.feed(&input) {}
    tokenizer.end();
    let errors = tokenizer.sink.sink.log.borrow().clone();
    assert!(!errors.iter().any(|entry| entry.contains("self-closing")));
}

/// Only an SVG script pauses: a MathML "script" end tag, a self-closing
/// MathML "script", and a "script" end tag whose current node is another
/// element inside an SVG script (handled as any other end tag) do not.
#[test]
fn other_script_end_tags_in_foreign_content_do_not_pause() {
    let log = parse("<!DOCTYPE html><math><script>a()</script><script/></math>");
    assert!(
        !log.iter().any(|entry| entry.starts_with("pause")),
        "{log:?}"
    );
    let log = parse("<!DOCTYPE html><svg><script><g></script><rect/></svg>");
    assert_eq!(
        log,
        [
            "create svg svg",
            "create svg script",
            "create svg g",
            "create svg rect",
        ]
    );
}

/// Parses `html` as a fragment whose context element is the HTML element
/// `context`, and returns the elements the tree builder created for it and
/// the parse errors it reported.
fn parse_fragment(context: &str, html: &str) -> (Vec<String>, usize) {
    let sink = Sink::default();
    let name = QualName::new(None, ns!(html), LocalName::from(context));
    let context = sink.create_element(name, Vec::new(), ElementFlags::default());
    sink.log.borrow_mut().clear();
    let builder = TreeBuilder::new_for_fragment(sink, context, None, Default::default());
    let tokenizer = Tokenizer::new(builder, Default::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from_slice(html));
    while let TokenizerResult::Script(_) = tokenizer.feed(&input) {}
    tokenizer.end();
    let log = tokenizer.sink.sink.log.borrow().clone();
    let errors = log.iter().filter(|entry| entry.starts_with("error")).count();
    let created = log
        .into_iter()
        .filter(|entry| entry.starts_with("create"))
        .collect();
    (created, errors)
}

/// HTML Standard, the "in body" insertion mode, a start tag whose tag name
/// is "input": if the parser's fragment context element is a select element,
/// parse error, ignore the token (html5lib's tests_innerHTML_1.dat case 76).
#[test]
fn an_input_in_a_select_fragment_is_ignored() {
    let (created, errors) = parse_fragment("select", "<input><option>");
    assert_eq!(created, ["create xhtml option"]);
    assert!(errors >= 1, "the ignored input is a parse error");
    let (created, _) = parse_fragment("select", "<input type=hidden/>");
    assert!(created.is_empty(), "{created:?}");
}

/// The rule's other steps are unchanged: in another context an input is
/// inserted, and a select in scope is popped before it.
#[test]
fn an_input_in_other_fragments_is_inserted() {
    let (created, _) = parse_fragment("div", "<input><option>");
    assert_eq!(created, ["create xhtml input", "create xhtml option"]);
    let (created, _) = parse_fragment("div", "<select><input>");
    assert_eq!(created, ["create xhtml select", "create xhtml input"]);
}
