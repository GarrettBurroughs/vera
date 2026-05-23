use crate::parser::syntax::{SyntaxKind, VeraLanguage};
use rowan::{GreenNode, GreenNodeBuilder, Language};

/// The `Sink` consumes an event stream from the parser and builds a
/// lossless Concrete Syntax Tree (CST) using `rowan`.
/// 
/// We use an event-sink architecture to separate the recursive descent parsing logic
/// from the complex state management of rowan's GreenNodeBuilder. This makes lookahead
/// and error recovery significantly easier.
pub struct Sink<'a> {
    builder: GreenNodeBuilder<'static>,
    tokens: &'a [(SyntaxKind, &'a str)],
    cursor: usize,
    events: Vec<Event>,
}

#[derive(Debug, Clone)]
pub enum Event {
    StartNode(SyntaxKind),
    AddToken,
    FinishNode,
    Error(String),
}

impl<'a> Sink<'a> {
    pub fn new(tokens: &'a [(SyntaxKind, &'a str)], events: Vec<Event>) -> Self {
        Self {
            builder: GreenNodeBuilder::new(),
            tokens,
            cursor: 0,
            events,
        }
    }

    /// Processes all events and builds the final GreenNode.
    pub fn finish(mut self) -> (GreenNode, Vec<String>) {
        let mut errors = Vec::new();
        let events = std::mem::take(&mut self.events);
        let mut depth = 0;

        for event in events {
            match event {
                Event::StartNode(kind) => {
                    self.builder.start_node(VeraLanguage::kind_to_raw(kind));
                    self.eat_trivia();
                    depth += 1;
                }
                Event::AddToken => {
                    self.eat_trivia();
                    let (kind, text) = self.tokens[self.cursor];
                    self.builder.token(VeraLanguage::kind_to_raw(kind), text);
                    self.cursor += 1;
                }
                Event::FinishNode => {
                    depth -= 1;
                    if depth == 0 {
                        self.eat_trivia(); // Eat EOF trivia into the root node
                    }
                    self.builder.finish_node();
                }
                Event::Error(err) => {
                    errors.push(err);
                }
            }
        }

        (self.builder.finish(), errors)
    }

    /// Attaches whitespace and comments to the current node as trivia.
    fn eat_trivia(&mut self) {
        while self.cursor < self.tokens.len() {
            let (kind, text) = self.tokens[self.cursor];
            if matches!(kind, SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                self.builder.token(VeraLanguage::kind_to_raw(kind), text);
                self.cursor += 1;
            } else {
                break;
            }
        }
    }
}
