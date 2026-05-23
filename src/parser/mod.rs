use crate::lexer::Token;
use logos::Logos;

#[derive(Debug)]
pub struct FuncDecl {
    pub name: String,
    pub ret_type: String,
    pub body_ret_val: i64,
}

pub fn parse(input: &str) -> Option<FuncDecl> {
    let mut lexer = Token::lexer(input);
    
    let mut name = String::new();
    let mut ret_type = String::new();
    let mut body_ret_val = 0;
    
    while let Some(res) = lexer.next() {
        if let Ok(Token::Ident(ref id)) = res {
            if id == "main" { name = id.clone(); }
        }
        if let Ok(Token::I32) = res {
            ret_type = "i32".to_string();
        }
        if let Ok(Token::Int(val)) = res {
            body_ret_val = val;
        }
    }
    
    if name == "main" && ret_type == "i32" {
        Some(FuncDecl { name, ret_type, body_ret_val })
    } else {
        None
    }
}
