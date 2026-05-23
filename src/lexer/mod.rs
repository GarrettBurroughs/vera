use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip r"[ \t\n\f]+")] // Skip whitespace
#[logos(skip r"//[^\n]*")]   // Skip line comments
pub enum Token {
    #[token("pub")] Pub,
    #[token("func")] Func,
    #[token("return")] Return,
    #[token("i32")] I32,
    
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),
    
    #[regex(r"[0-9]+", |lex| lex.slice().parse().ok())]
    Int(i64),
    
    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token(":")] Colon,
    #[token(";")] Semi,
}
