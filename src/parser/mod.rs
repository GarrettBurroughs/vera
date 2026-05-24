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
    
pub struct Marker(usize);
pub struct CompletedMarker(usize);

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
        
        self.eat_trivia();
        while self.cursor < self.tokens.len() {
            let mut peek = self.cursor;
            while peek < self.tokens.len() && matches!(self.tokens[peek].0, SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                peek += 1;
            }
            let mut is_pub = false;
            if peek < self.tokens.len() && self.tokens[peek].0 == SyntaxKind::KwPub {
                is_pub = true;
                peek += 1;
                while peek < self.tokens.len() && matches!(self.tokens[peek].0, SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                    peek += 1;
                }
            }
            
            let kind = if peek < self.tokens.len() { self.tokens[peek].0 } else { SyntaxKind::ErrorToken };
            
            if kind == SyntaxKind::KwFunc {
                self.parse_func();
            } else if kind == SyntaxKind::KwStruct {
                self.parse_struct_decl();
            } else {
                self.error("Expected function or struct declaration");
                self.advance();
            }
            self.eat_trivia();
        }
        
        self.finish_node();
        
        let sink = Sink::new(&self.tokens, self.events);
        let (green, errors) = sink.finish();
        (SyntaxNode::new_root(green), errors)
    }
    
    // -- Parser Primitives --
    
    fn start(&mut self) -> Marker {
        Marker(self.events.len())
    }
    
    fn complete(&mut self, marker: Marker, kind: SyntaxKind) -> CompletedMarker {
        self.events.insert(marker.0, Event::StartNode(kind));
        self.events.push(Event::FinishNode);
        CompletedMarker(marker.0)
    }
    
    fn precede(&mut self, completed: CompletedMarker) -> Marker {
        Marker(completed.0)
    }
    
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
        
        if self.at(SyntaxKind::KwPub) {
            self.advance();
        }
        
        self.expect(SyntaxKind::KwFunc);
        self.expect(SyntaxKind::Ident);
        self.expect(SyntaxKind::LParen);
        
        if !self.at(SyntaxKind::RParen) {
            self.start_node(SyntaxKind::PARAM_LIST);
            loop {
                self.start_node(SyntaxKind::PARAM);
                self.expect(SyntaxKind::Ident);
                self.expect(SyntaxKind::Colon);
                self.parse_type();
                self.finish_node();
                
                if self.at(SyntaxKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.finish_node();
        }
        
        self.expect(SyntaxKind::RParen);
        
        if self.at(SyntaxKind::Colon) {
            self.advance();
            self.parse_type();
        }
        
        if self.at(SyntaxKind::KwSpec) {
            self.parse_spec_block();
        }
        
        self.parse_block();
        self.finish_node();
    }
    
    fn parse_type(&mut self) {
        self.start_node(SyntaxKind::TYPE_REF);
        if self.at(SyntaxKind::TyI32) || self.at(SyntaxKind::TyBool) || self.at(SyntaxKind::Ident) {
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
            self.parse_stmt();
        }
        
        self.expect(SyntaxKind::RBrace);
        self.finish_node();
    }
    
    fn parse_spec_block(&mut self) {
        self.start_node(SyntaxKind::SPEC_BLOCK);
        self.expect(SyntaxKind::KwSpec);
        self.expect(SyntaxKind::LBrace);
        
        while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
            if self.at(SyntaxKind::KwRequires) {
                self.start_node(SyntaxKind::REQUIRES_CLAUSE);
                self.advance();
                self.parse_expr();
                self.expect(SyntaxKind::Semi);
                self.finish_node();
            } else if self.at(SyntaxKind::KwEnsures) {
                self.start_node(SyntaxKind::ENSURES_CLAUSE);
                self.advance();
                self.parse_expr();
                self.expect(SyntaxKind::Semi);
                self.finish_node();
            } else {
                self.error("Expected requires or ensures");
                self.advance();
            }
        }
        
        self.expect(SyntaxKind::RBrace);
        self.finish_node();
    }

    fn parse_stmt(&mut self) {
        if self.at(SyntaxKind::KwReturn) {
            self.parse_return_stmt();
        } else if self.at(SyntaxKind::KwConst) || self.at(SyntaxKind::KwVar) {
            self.parse_let_stmt();
        } else if self.at(SyntaxKind::KwIf) {
            self.parse_if_expr();
        } else if self.at(SyntaxKind::KwAssert) {
            self.start_node(SyntaxKind::ASSERT_STMT);
            self.advance();
            self.parse_expr();
            self.expect(SyntaxKind::Semi);
            self.finish_node();
        } else if self.at(SyntaxKind::KwAssume) {
            self.start_node(SyntaxKind::ASSUME_STMT);
            self.advance();
            self.parse_expr();
            self.expect(SyntaxKind::Semi);
            self.finish_node();
        } else {
            self.parse_expr_stmt();
        }
    }
    
    fn parse_let_stmt(&mut self) {
        self.start_node(SyntaxKind::LET_STMT);
        if self.at(SyntaxKind::KwConst) {
            self.expect(SyntaxKind::KwConst);
        } else {
            self.expect(SyntaxKind::KwVar);
        }
        self.expect(SyntaxKind::Ident);
        
        if self.at(SyntaxKind::Colon) {
            self.advance();
            self.parse_type();
        }
        
        self.expect(SyntaxKind::Eq);
        self.parse_expr();
        self.expect(SyntaxKind::Semi);
        self.finish_node();
    }
    
    fn parse_expr_stmt(&mut self) {
        self.start_node(SyntaxKind::EXPR_STMT);
        self.parse_expr();
        if self.at(SyntaxKind::Semi) {
            self.advance();
        } else {
            self.error("Expected semicolon");
        }
        self.finish_node();
    }
    
    fn parse_return_stmt(&mut self) {
        self.start_node(SyntaxKind::RETURN_STMT);
        self.expect(SyntaxKind::KwReturn);
        
        if !self.at(SyntaxKind::Semi) {
            self.parse_expr();
        }
        
        self.expect(SyntaxKind::Semi);
        self.finish_node();
    }
    
    fn parse_if_expr(&mut self) {
        self.start_node(SyntaxKind::IF_EXPR);
        self.expect(SyntaxKind::KwIf);
        
        self.start_node(SyntaxKind::CONDITION);
        self.parse_expr();
        self.finish_node();
        
        if self.at(SyntaxKind::LBrace) {
            self.parse_block();
        } else {
            self.error("Expected block");
        }
        
        if self.at(SyntaxKind::KwElse) {
            self.advance();
            if self.at(SyntaxKind::KwIf) {
                self.parse_if_expr();
            } else if self.at(SyntaxKind::LBrace) {
                self.parse_block();
            } else {
                self.error("Expected block or if");
            }
        }
        
        self.finish_node();
    }
    
    fn parse_expr(&mut self) {
        self.parse_assignment_expr();
    }
    
    fn parse_assignment_expr(&mut self) {
        let m = self.start();
        self.parse_logic_expr();
        if self.at(SyntaxKind::Eq) {
            self.advance();
            self.parse_assignment_expr();
            self.complete(m, SyntaxKind::BIN_EXPR);
        }
    }
    
    fn parse_logic_expr(&mut self) {
        let mut m = self.start();
        self.parse_equality_expr();
        while self.at(SyntaxKind::AmpAmp) || self.at(SyntaxKind::PipePipe) || self.at(SyntaxKind::Implies) || self.at(SyntaxKind::Iff) {
            self.advance();
            self.parse_equality_expr();
            let comp = self.complete(m, SyntaxKind::BIN_EXPR);
            m = self.precede(comp);
        }
    }
    
    fn parse_equality_expr(&mut self) {
        let mut m = self.start();
        self.parse_rel_expr();
        while self.at(SyntaxKind::EqEq) || self.at(SyntaxKind::BangEq) {
            self.advance();
            self.parse_rel_expr();
            let comp = self.complete(m, SyntaxKind::BIN_EXPR);
            m = self.precede(comp);
        }
    }
    
    fn parse_rel_expr(&mut self) {
        let mut m = self.start();
        self.parse_add_expr();
        while self.at(SyntaxKind::Less) || self.at(SyntaxKind::Greater) || self.at(SyntaxKind::LessEq) || self.at(SyntaxKind::GreaterEq) {
            self.advance();
            self.parse_add_expr();
            let comp = self.complete(m, SyntaxKind::BIN_EXPR);
            m = self.precede(comp);
        }
    }
    
    fn parse_add_expr(&mut self) {
        let mut m = self.start();
        self.parse_mul_expr();
        while self.at(SyntaxKind::Plus) || self.at(SyntaxKind::Minus) {
            self.advance();
            self.parse_mul_expr();
            let comp = self.complete(m, SyntaxKind::BIN_EXPR);
            m = self.precede(comp);
        }
    }
    
    fn parse_mul_expr(&mut self) {
        let mut m = self.start();
        self.parse_unary_expr();
        while self.at(SyntaxKind::Star) || self.at(SyntaxKind::Slash) || self.at(SyntaxKind::Percent) {
            self.advance();
            self.parse_unary_expr();
            let comp = self.complete(m, SyntaxKind::BIN_EXPR);
            m = self.precede(comp);
        }
    }
    
    fn parse_unary_expr(&mut self) {
        if self.at(SyntaxKind::Bang) || self.at(SyntaxKind::Minus) || self.at(SyntaxKind::Star) || self.at(SyntaxKind::Amp) {
            self.start_node(SyntaxKind::PREFIX_EXPR);
            self.advance();
            self.parse_postfix_expr();
            self.finish_node();
        } else {
            self.parse_postfix_expr();
        }
    }
    
    fn parse_postfix_expr(&mut self) {
        let mut m = self.start();
        self.parse_primary_expr();
        loop {
            if self.at(SyntaxKind::LParen) {
                self.start_node(SyntaxKind::ARG_LIST);
                self.advance(); // consume LParen
                if !self.at(SyntaxKind::RParen) {
                    loop {
                        self.parse_expr();
                        if self.at(SyntaxKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(SyntaxKind::RParen);
                self.finish_node(); // finish ARG_LIST
                let comp = self.complete(m, SyntaxKind::CALL_EXPR);
                m = self.precede(comp);
            } else if self.at(SyntaxKind::Dot) {
                self.advance(); // consume dot
                self.start_node(SyntaxKind::NAME_REF);
                self.expect(SyntaxKind::Ident);
                self.finish_node(); // NAME_REF
                let comp = self.complete(m, SyntaxKind::FIELD_EXPR);
                m = self.precede(comp);
            } else {
                break;
            }
        }
    }
    
    fn parse_struct_decl(&mut self) {
        self.start_node(SyntaxKind::STRUCT_DECL);
        
        if self.at(SyntaxKind::KwPub) {
            self.advance();
        }
        
        self.expect(SyntaxKind::KwStruct);
        self.expect(SyntaxKind::Ident);
        self.expect(SyntaxKind::LBrace);
        
        self.start_node(SyntaxKind::FIELD_DECL_LIST);
        while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
            self.start_node(SyntaxKind::FIELD_DECL);
            
            if self.at(SyntaxKind::KwPub) {
                self.advance();
            }
            self.expect(SyntaxKind::Ident);
            self.expect(SyntaxKind::Colon);
            self.parse_type();
            self.finish_node();
            
            if self.at(SyntaxKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.finish_node(); // FIELD_DECL_LIST
        self.expect(SyntaxKind::RBrace);
        
        self.finish_node(); // STRUCT_DECL
    }

    fn parse_primary_expr(&mut self) {
        if self.at(SyntaxKind::Ident) {
            // Peek to see if it's a struct expr: Ident {
            let mut peek = self.cursor + 1;
            while peek < self.tokens.len() && matches!(self.tokens[peek].0, SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                peek += 1;
            }
            
            if peek < self.tokens.len() && self.tokens[peek].0 == SyntaxKind::LBrace {
                self.start_node(SyntaxKind::STRUCT_EXPR);
                self.start_node(SyntaxKind::NAME_REF);
                self.advance(); // Ident
                self.finish_node(); // NAME_REF
                
                self.expect(SyntaxKind::LBrace);
                self.start_node(SyntaxKind::STRUCT_EXPR_FIELD_LIST);
                while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
                    self.start_node(SyntaxKind::STRUCT_EXPR_FIELD);
                    self.expect(SyntaxKind::Ident);
                    self.expect(SyntaxKind::Colon);
                    self.parse_expr();
                    self.finish_node(); // STRUCT_EXPR_FIELD
                    
                    if self.at(SyntaxKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.finish_node(); // STRUCT_EXPR_FIELD_LIST
                self.expect(SyntaxKind::RBrace);
                self.finish_node(); // STRUCT_EXPR
            } else {
                self.start_node(SyntaxKind::NAME_REF);
                self.advance();
                self.finish_node();
            }
        } else if self.at(SyntaxKind::IntLit) || self.at(SyntaxKind::FloatLit) || self.at(SyntaxKind::StringLit) || self.at(SyntaxKind::BoolTrue) || self.at(SyntaxKind::BoolFalse) {
            self.start_node(SyntaxKind::LITERAL);
            self.advance();
            self.finish_node();
        } else if self.at(SyntaxKind::LParen) {
            self.advance();
            self.parse_expr();
            self.expect(SyntaxKind::RParen);
        } else if self.at(SyntaxKind::KwIf) {
            self.parse_if_expr();
        } else {
            self.error("Expected expression");
            self.advance(); // Prevent infinite loop
        }
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
        let (_, errors) = parser.parse();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_parse_spec_and_assert() {
        let input = r#"
            func verify_math(): i32
            spec {
                requires 1 > 0;
                ensures true;
            }
            {
                assert 1 > 0;
                assume 1 == 1;
                return 1;
            }
        "#;
        let parser = Parser::new(input);
        let (_, errors) = parser.parse();
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    }
}
