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
    forbid_struct_expr: bool,
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
            forbid_struct_expr: false,
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
            } else if kind == SyntaxKind::KwEnum {
                self.parse_enum_decl();
            } else if kind == SyntaxKind::KwVariant {
                self.parse_variant_decl();
            } else if kind == SyntaxKind::KwTrait {
                self.parse_trait_decl();
            } else if kind == SyntaxKind::KwImpl {
                self.parse_impl_decl();
            } else if kind == SyntaxKind::KwType {
                self.parse_type_alias();
            } else {
                self.error("Expected function, struct, enum, trait, impl, or variant declaration");
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
    
    fn nth_at(&self, mut n: usize, kind: SyntaxKind) -> bool {
        let mut cur = self.cursor;
        while cur < self.tokens.len() {
            let k = self.tokens[cur].0;
            if matches!(k, SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                cur += 1;
                continue;
            }
            if n == 0 {
                return k == kind;
            }
            n -= 1;
            cur += 1;
        }
        false
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
        
        if self.at(SyntaxKind::Less) {
            self.parse_generic_params();
        }
        
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
        
        if self.at(SyntaxKind::LBrace) {
            self.parse_block();
        } else if self.at(SyntaxKind::Semi) {
            self.advance();
        } else {
            self.error("Expected '{' or ';'");
            self.advance();
        }
        self.finish_node();
    }
    
    fn parse_type(&mut self) {
        self.start_node(SyntaxKind::TYPE_REF);
        
        if self.at(SyntaxKind::TyArray) {
            self.start_node(SyntaxKind::ARRAY_TYPE);
            self.advance();
            self.expect(SyntaxKind::LBracket);
            self.parse_type();
            self.expect(SyntaxKind::Comma);
            self.parse_expr(); // array size
            self.expect(SyntaxKind::RBracket);
            self.finish_node();
        } else if self.at(SyntaxKind::TySlice) {
            self.start_node(SyntaxKind::SLICE_TYPE);
            self.advance();
            self.expect(SyntaxKind::LBracket);
            self.parse_type();
            self.expect(SyntaxKind::RBracket);
            self.finish_node();
        } else if self.at(SyntaxKind::TyResult) {
            self.start_node(SyntaxKind::RESULT_TYPE);
            self.advance();
            self.expect(SyntaxKind::LBracket);
            self.parse_type(); // Ok type
            self.expect(SyntaxKind::Comma);
            self.parse_type(); // Err type
            self.expect(SyntaxKind::RBracket);
            self.finish_node();
        } else if self.at(SyntaxKind::TyRef) {
            self.start_node(SyntaxKind::REF_TYPE);
            self.advance();
            self.parse_type();
            self.finish_node();
        } else if self.at(SyntaxKind::TyPtr) {
            self.start_node(SyntaxKind::POINTER_TYPE);
            self.advance();
            self.parse_type();
            self.finish_node();
        } else if self.at(SyntaxKind::KwMut) && self.nth_at(1, SyntaxKind::TyRef) {
            self.start_node(SyntaxKind::REF_TYPE);
            self.advance(); // mut
            self.advance(); // ref
            self.parse_type();
            self.finish_node();
        } else if self.at(SyntaxKind::KwMut) && self.nth_at(1, SyntaxKind::TyPtr) {
            self.start_node(SyntaxKind::POINTER_TYPE);
            self.advance(); // mut
            self.advance(); // ptr
            self.parse_type();
            self.finish_node();
        } else if self.at(SyntaxKind::KwMut) && self.nth_at(1, SyntaxKind::TySlice) {
            self.start_node(SyntaxKind::SLICE_TYPE);
            self.advance(); // mut
            self.advance(); // slice
            self.expect(SyntaxKind::LBracket);
            self.parse_type();
            self.expect(SyntaxKind::RBracket);
            self.finish_node();
        } else if self.at(SyntaxKind::KwFunc) {
            self.start_node(SyntaxKind::FUNC_TYPE);
            self.advance(); // func
            self.expect(SyntaxKind::LParen);
            while !self.at(SyntaxKind::RParen) && self.cursor < self.tokens.len() {
                self.parse_type();
                if self.at(SyntaxKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(SyntaxKind::RParen);
            if self.at(SyntaxKind::Arrow) {
                self.advance();
                self.parse_type(); // Return type
            }
            self.finish_node();
        } else if self.at(SyntaxKind::TyI32) || self.at(SyntaxKind::TyBool) || self.at(SyntaxKind::Ident)
            || self.at(SyntaxKind::TyU64) || self.at(SyntaxKind::TyF32) || self.at(SyntaxKind::TyF64)
            || self.at(SyntaxKind::TyU32) || self.at(SyntaxKind::TyI64) || self.at(SyntaxKind::TyI16)
            || self.at(SyntaxKind::TyU16) || self.at(SyntaxKind::TyI8) || self.at(SyntaxKind::TyU8)
            || self.at(SyntaxKind::TyString) || self.at(SyntaxKind::TyVoid) || self.at(SyntaxKind::TyChar) {
            
            let is_ident = self.at(SyntaxKind::Ident);
            self.advance();
            
            if is_ident && self.at(SyntaxKind::Less) {
                self.start_node(SyntaxKind::GENERIC_ARGS);
                self.advance(); // <
                while !self.at(SyntaxKind::Greater) && self.cursor < self.tokens.len() {
                    self.parse_type();
                    if self.at(SyntaxKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(SyntaxKind::Greater);
                self.finish_node(); // GENERIC_ARGS
            }
        } else {
            self.error("Expected type");
            self.advance();
        }
        
        if self.at(SyntaxKind::KwWhere) {
            self.start_node(SyntaxKind::REFINEMENT_TYPE);
            self.advance(); // where
            if self.at(SyntaxKind::LParen) {
                self.advance(); // (
                let old = self.forbid_struct_expr;
                self.forbid_struct_expr = true;
                self.parse_expr();
                self.forbid_struct_expr = old;
                self.expect(SyntaxKind::RParen);
            } else {
                self.error("Expected '(' after where");
            }
            self.finish_node();
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
            } else if self.at(SyntaxKind::KwInvariant) {
                self.start_node(SyntaxKind::INVARIANT_CLAUSE);
                self.advance();
                self.parse_expr();
                self.expect(SyntaxKind::Semi);
                self.finish_node();
            } else if self.at(SyntaxKind::KwDecreases) {
                self.start_node(SyntaxKind::DECREASES_CLAUSE);
                self.advance();
                self.parse_expr();
                self.expect(SyntaxKind::Semi);
                self.finish_node();
            } else if self.at(SyntaxKind::KwAssigns) {
                self.start_node(SyntaxKind::ASSIGNS_CLAUSE);
                self.advance();
                self.parse_expr();
                self.expect(SyntaxKind::Semi);
                self.finish_node();
            } else {
                self.error("Expected requires, ensures, invariant, decreases, or assigns");
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
        } else if self.at(SyntaxKind::KwWhile) {
            self.parse_while_stmt();
        } else if self.at(SyntaxKind::KwFor) {
            self.parse_for_stmt();
        } else if self.at(SyntaxKind::KwBreak) {
            self.parse_break_stmt();
        } else if self.at(SyntaxKind::KwContinue) {
            self.parse_continue_stmt();
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

    fn parse_while_stmt(&mut self) {
        self.start_node(SyntaxKind::WHILE_STMT);
        self.expect(SyntaxKind::KwWhile);
        let old = self.forbid_struct_expr;
        self.forbid_struct_expr = true;
        self.parse_expr();
        self.forbid_struct_expr = old;
        if self.at(SyntaxKind::KwSpec) {
            self.parse_spec_block();
        }
        if self.at(SyntaxKind::LBrace) {
            self.parse_block();
        } else {
            self.error("Expected loop body block");
        }
        self.finish_node();
    }

    fn parse_for_stmt(&mut self) {
        self.start_node(SyntaxKind::FOR_STMT);
        self.expect(SyntaxKind::KwFor);
        self.expect(SyntaxKind::Ident); // the iteration variable
        self.expect(SyntaxKind::KwIn);
        
        let old = self.forbid_struct_expr;
        self.forbid_struct_expr = true;
        self.parse_expr(); // the iterable
        self.forbid_struct_expr = old;
        
        if self.at(SyntaxKind::LBrace) {
            self.parse_block();
        } else {
            self.error("Expected loop body block");
        }
        self.finish_node();
    }

    fn parse_break_stmt(&mut self) {
        self.start_node(SyntaxKind::BREAK_STMT);
        self.expect(SyntaxKind::KwBreak);
        self.expect(SyntaxKind::Semi);
        self.finish_node();
    }

    fn parse_continue_stmt(&mut self) {
        self.start_node(SyntaxKind::CONTINUE_STMT);
        self.expect(SyntaxKind::KwContinue);
        self.expect(SyntaxKind::Semi);
        self.finish_node();
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
        let old = self.forbid_struct_expr;
        self.forbid_struct_expr = true;
        self.parse_expr();
        self.forbid_struct_expr = old;
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
        if self.at(SyntaxKind::Star) {
            self.start_node(SyntaxKind::DEREF_EXPR);
            self.advance();
            self.parse_unary_expr();
            self.finish_node();
        } else if self.at(SyntaxKind::Amp) {
            self.start_node(SyntaxKind::REF_EXPR);
            self.advance();
            if self.at(SyntaxKind::KwMut) {
                self.advance();
            }
            self.parse_unary_expr();
            self.finish_node();
        } else if self.at(SyntaxKind::Bang) || self.at(SyntaxKind::Minus) {
            self.start_node(SyntaxKind::PREFIX_EXPR);
            self.advance();
            self.parse_unary_expr();
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
            } else if self.at(SyntaxKind::ColonColon) {
                let mut peek = self.cursor + 1;
                while peek < self.tokens.len() && matches!(self.tokens[peek].0, SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                    peek += 1;
                }
                if peek < self.tokens.len() && self.tokens[peek].0 == SyntaxKind::Less {
                    self.advance(); // consume ::
                    self.start_node(SyntaxKind::GENERIC_ARGS);
                    self.advance(); // consume <
                    while !self.at(SyntaxKind::Greater) && self.cursor < self.tokens.len() {
                        self.parse_type();
                        if self.at(SyntaxKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    self.expect(SyntaxKind::Greater);
                    self.finish_node(); // finish GENERIC_ARGS
                    
                    let comp = self.complete(m, SyntaxKind::GENERIC_INST_EXPR);
                    m = self.precede(comp);
                } else {
                    break;
                }
            } else if self.at(SyntaxKind::Dot) {
                self.advance(); // consume dot
                self.start_node(SyntaxKind::NAME_REF);
                self.expect(SyntaxKind::Ident);
                self.finish_node(); // NAME_REF
                let comp = self.complete(m, SyntaxKind::FIELD_EXPR);
                m = self.precede(comp);
            } else if self.at(SyntaxKind::LBracket) {
                self.advance(); // consume [
                self.parse_expr();
                if self.at(SyntaxKind::DotDot) {
                    self.advance(); // consume ..
                    self.parse_expr();
                    self.expect(SyntaxKind::RBracket);
                    let comp = self.complete(m, SyntaxKind::SLICE_EXPR);
                    m = self.precede(comp);
                } else {
                    self.expect(SyntaxKind::RBracket);
                    let comp = self.complete(m, SyntaxKind::INDEX_EXPR);
                    m = self.precede(comp);
                }
            } else if self.at(SyntaxKind::Question) {
                self.advance(); // consume ?
                let comp = self.complete(m, SyntaxKind::TRY_EXPR);
                m = self.precede(comp);
            } else {
                break;
            }
        }
    }
    
    fn parse_generic_params(&mut self) {
        self.start_node(SyntaxKind::GENERIC_PARAMS);
        self.expect(SyntaxKind::Less);
        while !self.at(SyntaxKind::Greater) && self.cursor < self.tokens.len() {
            self.start_node(SyntaxKind::TYPE_REF);
            self.expect(SyntaxKind::Ident);
            self.finish_node();
            if self.at(SyntaxKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(SyntaxKind::Greater);
        self.finish_node();
    }
    
    fn parse_trait_decl(&mut self) {
        self.start_node(SyntaxKind::TRAIT_DECL);
        
        if self.at(SyntaxKind::KwPub) {
            self.advance();
        }
        
        self.expect(SyntaxKind::KwTrait);
        self.expect(SyntaxKind::Ident);
        
        if self.at(SyntaxKind::Less) {
            self.parse_generic_params();
        }
        
        self.expect(SyntaxKind::LBrace);
        while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
            // Parses method signatures. For now, we reuse `parse_func`,
            // but `parse_func` expects a body block. 
            // In traits, methods might lack bodies, but we'll accept them or reuse parse_func.
            // Let's just call parse_func.
            self.parse_func();
        }
        self.expect(SyntaxKind::RBrace);
        self.finish_node();
    }
    
    fn parse_impl_decl(&mut self) {
        self.start_node(SyntaxKind::IMPL_DECL);
        self.expect(SyntaxKind::KwImpl);
        
        if self.at(SyntaxKind::Less) {
            self.parse_generic_params();
        }
        
        self.parse_type(); // The trait or the target type
        
        if self.at(SyntaxKind::KwFor) {
            self.advance();
            self.parse_type(); // Target type
        }
        
        self.expect(SyntaxKind::LBrace);
        while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
            self.parse_func();
        }
        self.expect(SyntaxKind::RBrace);
        self.finish_node();
    }

    fn parse_type_alias(&mut self) {
        self.start_node(SyntaxKind::TYPE_ALIAS);
        
        if self.at(SyntaxKind::KwPub) {
            self.advance();
        }
        
        self.expect(SyntaxKind::KwType);
        self.expect(SyntaxKind::Ident);
        self.expect(SyntaxKind::Eq);
        self.parse_type();
        self.expect(SyntaxKind::Semi);
        
        self.finish_node();
    }
    
    fn parse_struct_decl(&mut self) {
        self.start_node(SyntaxKind::STRUCT_DECL);
        
        if self.at(SyntaxKind::KwPub) {
            self.advance();
        }
        
        self.expect(SyntaxKind::KwStruct);
        self.expect(SyntaxKind::Ident);
        
        if self.at(SyntaxKind::Less) {
            self.parse_generic_params();
        }
        
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
            
            if !self.forbid_struct_expr && peek < self.tokens.len() && self.tokens[peek].0 == SyntaxKind::LBrace {
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
        } else if self.at(SyntaxKind::KwMatch) {
            self.parse_match_expr();
        } else if self.at(SyntaxKind::LBracket) {
            self.start_node(SyntaxKind::ARRAY_EXPR);
            self.advance(); // consume [
            while !self.at(SyntaxKind::RBracket) && self.cursor < self.tokens.len() {
                self.parse_expr();
                if self.at(SyntaxKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(SyntaxKind::RBracket);
            self.finish_node();
        } else if self.at(SyntaxKind::KwUnsafe) {
            self.start_node(SyntaxKind::UNSAFE_BLOCK);
            self.advance(); // consume unsafe
            if self.at(SyntaxKind::LBrace) {
                self.parse_block();
            } else {
                self.error("Expected '{' after 'unsafe'");
            }
            self.finish_node();
        } else if self.at(SyntaxKind::Pipe) || self.at(SyntaxKind::PipePipe) {
            self.start_node(SyntaxKind::CLOSURE_EXPR);
            if self.at(SyntaxKind::PipePipe) {
                self.advance();
            } else {
                self.advance(); // |
                while !self.at(SyntaxKind::Pipe) && self.cursor < self.tokens.len() {
                    self.start_node(SyntaxKind::PARAM);
                    self.expect(SyntaxKind::Ident);
                    if self.at(SyntaxKind::Colon) {
                        self.advance();
                        self.parse_type();
                    }
                    self.finish_node(); // PARAM
                    
                    if self.at(SyntaxKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(SyntaxKind::Pipe);
            }
            self.parse_expr(); // body
            self.finish_node();
        } else {
            self.error("Expected expression");
            self.advance(); // Prevent infinite loop
        }
    }

    fn parse_match_expr(&mut self) {
        self.start_node(SyntaxKind::MATCH_EXPR);
        self.expect(SyntaxKind::KwMatch);
        let old = self.forbid_struct_expr;
        self.forbid_struct_expr = true;
        self.parse_expr();
        self.forbid_struct_expr = old;
        self.expect(SyntaxKind::LBrace);
        
        while self.at(SyntaxKind::KwCase) {
            self.start_node(SyntaxKind::MATCH_ARM);
            self.advance(); // consume case
            
            self.parse_pattern();
            self.expect(SyntaxKind::FatArrow);
            
            if self.at(SyntaxKind::LBrace) {
                self.parse_block();
            } else {
                self.parse_expr();
            }
            self.finish_node();
            
            if self.at(SyntaxKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        
        self.expect(SyntaxKind::RBrace);
        self.finish_node();
    }

    fn parse_pattern(&mut self) {
        self.start_node(SyntaxKind::PATTERN);
        if self.at(SyntaxKind::Ident) {
            self.advance();
            if self.at(SyntaxKind::LParen) {
                self.advance();
                loop {
                    self.parse_pattern();
                    if self.at(SyntaxKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(SyntaxKind::RParen);
            }
        } else if self.at(SyntaxKind::IntLit) || self.at(SyntaxKind::BoolTrue) || self.at(SyntaxKind::BoolFalse) {
            self.advance();
        } else {
            self.error("Expected pattern");
            self.advance();
        }
        self.finish_node();
    }

    fn parse_enum_decl(&mut self) {
        self.start_node(SyntaxKind::ENUM_DECL);
        
        if self.at(SyntaxKind::KwPub) {
            self.advance();
        }
        
        self.expect(SyntaxKind::KwEnum);
        self.expect(SyntaxKind::Ident);
        
        if self.at(SyntaxKind::Less) {
            self.parse_generic_params();
        }
        
        self.expect(SyntaxKind::LBrace);
        
        while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
            self.start_node(SyntaxKind::ENUM_VARIANT);
            self.expect(SyntaxKind::Ident);
            self.finish_node();
            
            if self.at(SyntaxKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        
        self.expect(SyntaxKind::RBrace);
        self.finish_node();
    }

    fn parse_variant_decl(&mut self) {
        self.start_node(SyntaxKind::VARIANT_DECL);
        
        if self.at(SyntaxKind::KwPub) {
            self.advance();
        }
        
        self.expect(SyntaxKind::KwVariant);
        self.expect(SyntaxKind::Ident);
        
        if self.at(SyntaxKind::Less) {
            self.parse_generic_params();
        }
        
        if self.at(SyntaxKind::LBracket) {
            self.advance();
            loop {
                self.expect(SyntaxKind::Ident);
                if self.at(SyntaxKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(SyntaxKind::RBracket);
        }
        
        self.expect(SyntaxKind::LBrace);
        
        while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
            self.start_node(SyntaxKind::VARIANT_CASE);
            self.expect(SyntaxKind::Ident);
            
            if self.at(SyntaxKind::LParen) {
                self.advance();
                loop {
                    self.parse_type();
                    if self.at(SyntaxKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(SyntaxKind::RParen);
            } else if self.at(SyntaxKind::LBrace) {
                self.advance();
                while !self.at(SyntaxKind::RBrace) && self.cursor < self.tokens.len() {
                    self.expect(SyntaxKind::Ident);
                    self.expect(SyntaxKind::Colon);
                    self.parse_type();
                    if self.at(SyntaxKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(SyntaxKind::RBrace);
            }
            
            self.finish_node();
            
            if self.at(SyntaxKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        
        self.expect(SyntaxKind::RBrace);
        self.finish_node();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{AstNode, SourceFile};

    fn parse(input: &str) -> (SyntaxNode, Vec<String>) {
        Parser::new(input).parse()
    }

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

    /// Tests that `const` and `var` let-statements are parsed without errors.
    #[test]
    fn test_parse_let_statements() {
        let (_, errors) = parse("func f(): i32 { const x: i32 = 1; var y: i32 = 2; return x; }");
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that type-annotated and type-inferred let-statements are both accepted.
    #[test]
    fn test_parse_let_without_type_annotation() {
        // The parser accepts `const x = 1;` (type inferred by lowering later).
        let (_, errors) = parse("func f(): i32 { const x = 1; return x; }");
        // Parser does not enforce type annotations — lowering does.
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that binary expressions obey correct precedence:
    /// `1 + 2 * 3` should parse as `1 + (2 * 3)`, not `(1 + 2) * 3`.
    #[test]
    fn test_parse_expr_precedence() {
        let input = "func f(): i32 { return 1 + 2 * 3; }";
        let (cst, errors) = parse(input);
        assert!(errors.is_empty(), "errors: {:?}", errors);

        // Inspect the shape of the returned expression: the outermost binary
        // op should be `+`, whose RHS should be a `BIN_EXPR` (the `*`).
        let src = SourceFile::cast(cst).unwrap();
        let func = src.functions().next().unwrap();
        let body = func.body().unwrap();
        let ret = body.statements().next().unwrap();
        if let crate::parser::ast::Stmt::ReturnStmt(ret_stmt) = ret {
            if let Some(crate::parser::ast::Expr::BinExpr(bin)) = ret_stmt.expr() {
                // The operator should be `+` (Add), not `*`
                let op = bin.op().expect("BinExpr must have operator");
                assert_eq!(op.kind(), SyntaxKind::Plus, "outer op must be +");
                // The RHS should itself be a BIN_EXPR (the * node)
                let rhs = bin.rhs().expect("BinExpr must have rhs");
                assert!(
                    matches!(rhs, crate::parser::ast::Expr::BinExpr(_)),
                    "RHS of + must be a BinExpr (* node), got {:?}", rhs
                );
            } else {
                panic!("Expected BinExpr return");
            }
        } else {
            panic!("Expected ReturnStmt");
        }
    }

    /// Tests that a struct declaration is parsed without errors.
    #[test]
    fn test_parse_struct_decl() {
        let input = "struct Point { x: i32, y: i32 }";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that struct instantiation expressions are parsed without errors.
    #[test]
    fn test_parse_struct_expr() {
        let input = "func f(): i32 { const p = Point { x: 1, y: 2 }; return 0; }";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that if/else and else-if chains are parsed without errors.
    #[test]
    fn test_parse_if_else_chain() {
        let input = r#"
            func f(x: i32): i32 {
                if x > 10 {
                    return 1;
                } else if x > 5 {
                    return 2;
                } else {
                    return 3;
                }
            }
        "#;
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that a void return (no expression) is parsed correctly.
    #[test]
    fn test_parse_return_no_value() {
        let input = "func f() { return; }";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that function calls with arguments are parsed correctly.
    #[test]
    fn test_parse_function_call() {
        let input = "func f(): i32 { return add(1, 2); }";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that field access expressions (`a.b`) are parsed correctly.
    #[test]
    fn test_parse_field_access() {
        let input = "func f(): i32 { return p.x; }";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that a function with no parameters and no return type is valid.
    #[test]
    fn test_parse_void_func() {
        let input = "func noop() { }";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "errors: {:?}", errors);
    }

    /// Tests that missing semicolons are caught as parse errors.
    #[test]
    fn test_parse_missing_semicolon() {
        let input = "func f(): i32 { return 42 }"; // missing semicolon after 42
        let (_, errors) = parse(input);
        assert!(!errors.is_empty(), "Expected a parse error for missing semicolon");
    }

    #[test]
    fn test_parse_variant() {
        let input = "variant Option { None, Some(i32) }";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "Expected no parse errors, got {:?}", errors);
    }

    #[test]
    fn test_parse_match_expr() {
        let input = "func main(): i32 { const x: i32 = match opt { case None => 0, case Some(x) => x }; return x; }";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "Expected no parse errors, got {:?}", errors);
    }

    #[test]
    fn test_parse_arrays_and_slices() {
        let input = "
            func process_data(arr: array[i32, 10], s: slice[u8], m: mut slice[f32]) {
                const my_arr: array[i32, 3] = [1, 2, 3];
                const x = my_arr[1];
                const y = s[0..5];
            }
        ";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "Expected no parse errors, got {:?}", errors);
    }

    #[test]
    fn test_parse_result_type() {
        let input = "func foo(): result[i32, i32] {}";
        let (_, errors) = parse(input);
        assert!(errors.is_empty(), "Expected no parse errors, got {:?}", errors);
    }
}
