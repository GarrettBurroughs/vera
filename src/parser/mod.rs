pub mod syntax;
pub mod sink;
pub mod ast;

use crate::lexer::Token;
use syntax::{SyntaxKind, SyntaxNode};
use sink::{Event, Sink};
use logos::Logos;

/// The Vera Parser.
/// It consumes a string of source code and constructs a Lossless Concrete Syntax Tree (CST)
/// using rowan via an event-driven sink architecture.
pub struct Parser<'a> {
    tokens: Vec<(SyntaxKind, &'a str)>,
    cursor: usize,
    events: Vec<Event>,
}

impl<'a> Parser<'a> {
    /// Initializes the parser by lexing the entire input upfront.
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Token::lexer(input);
        let mut tokens = Vec::new();
        while let Some(res) = lexer.next() {
            let kind = match res {
                Ok(t) => SyntaxKind::from(t),
                Err(_) => SyntaxKind::ErrorToken,
            };
            tokens.push((kind, lexer.slice()));
        }
        
        Self {
            tokens,
            cursor: 0,
            events: Vec::new(),
        }
    }
    
    /// Parses the tokens into a rowan CST.
    pub fn parse(mut self) -> (SyntaxNode, Vec<String>) {
        self.start_node(SyntaxKind::SOURCE_FILE);
        
        while self.cursor < self.tokens.len() {
            // Very simplified parser for Phase 2 slice
            if self.at(SyntaxKind::KwFunc) {
                self.parse_func();
            } else {
                self.error("Expected function declaration");
                self.advance(); // recover
            }
        }
        
        self.finish_node();
        
        let sink = Sink::new(&self.tokens, self.events);
        let (green, errors) = sink.finish();
        (SyntaxNode::new_root(green), errors)
    }
    
    // -- Parser Primitives --
    
    fn at(&mut self, kind: SyntaxKind) -> bool {
        self.eat_trivia();
        if self.cursor < self.tokens.len() {
            self.tokens[self.cursor].0 == kind
        } else {
            false
        }
    }
    
    fn advance(&mut self) {
        self.eat_trivia();
        if self.cursor < self.tokens.len() {
            self.events.push(Event::AddToken);
            self.cursor += 1;
        }
    }
    
    fn expect(&mut self, kind: SyntaxKind) {
        if self.at(kind) {
            self.advance();
        } else {
            self.error(format!("Expected {:?}", kind));
        }
    }
    
    fn start_node(&mut self, kind: SyntaxKind) {
        self.events.push(Event::StartNode(kind));
    }
    
    fn finish_node(&mut self) {
        self.events.push(Event::FinishNode);
    }
    
    fn error<S: Into<String>>(&mut self, msg: S) {
        self.events.push(Event::Error(msg.into()));
    }
    
    fn eat_trivia(&mut self) {
        while self.cursor < self.tokens.len() {
            let kind = self.tokens[self.cursor].0;
            if matches!(kind, SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                self.cursor += 1;
            } else {
                break;
            }
        }
    }
    
    // -- Parsing Rules --
    
    fn parse_func(&mut self) {
        self.start_node(SyntaxKind::FUNC_DECL);
        self.expect(SyntaxKind::KwFunc);
        self.expect(SyntaxKind::Ident);
        self.expect(SyntaxKind::LParen);
        self.expect(SyntaxKind::RParen);
        
        if self.at(SyntaxKind::Colon) {
            self.advance();
            self.parse_type();
        }
        
        self.parse_block();
        self.finish_node();
    }
    
    fn parse_type(&mut self) {
        self.start_node(SyntaxKind::TYPE_REF);
        if self.at(SyntaxKind::TyI32) {
            self.advance();
        } else {
            self.error("Expected type");
            self.advance();
        }
        self.finish_node();
    }
    
    fn parse_block(&mut self) {
        self.start_node(SyntaxKind::BLOCK_EXPR);
        self.expect(SyntaxKind::LBrace);
        
        while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
            if self.at(SyntaxKind::KwReturn) {
                self.parse_return_stmt();
            } else {
                self.error("Expected statement");
                self.advance();
            }
        }
        
        self.expect(SyntaxKind::RBrace);
        self.finish_node();
    }
    
    fn parse_return_stmt(&mut self) {
        self.start_node(SyntaxKind::RETURN_STMT);
        self.expect(SyntaxKind::KwReturn);
        if self.at(SyntaxKind::IntLit) {
            self.advance();
        } else {
            self.error("Expected expression");
        }
        self.expect(SyntaxKind::Semi);
        self.finish_node();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_func() {
        let input = "func main(): i32 { return 42; }";
        let parser = Parser::new(input);
        let (cst, errors) = parser.parse();
        assert!(errors.is_empty());
        assert_eq!(cst.kind(), SyntaxKind::SOURCE_FILE);
    }
    
    #[test]
    fn test_parse_error_recovery() {
        // Missing closing brace
        let input = "func main(): i32 { return 42; ";
        let parser = Parser::new(input);
        let (_, errors) = parser.parse();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_parse_pub_func() {
        let input = "pub func main(): i32 { return 42; }";
        let parser = Parser::new(input);
        let (_, _) = parser.parse();
    }
}
