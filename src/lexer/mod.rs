use logos::Logos;

/// The fundamental lexical tokens of the Vera language.
/// We do NOT skip whitespace or comments because the parser needs them 
/// to construct a lossless Concrete Syntax Tree (CST) for the LSP.
#[derive(Logos, Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Token {
    // Keywords
    #[token("import")] KwImport,
    #[token("as")] KwAs,
    #[token("pub")] KwPub,
    #[token("struct")] KwStruct,
    #[token("enum")] KwEnum,
    #[token("variant")] KwVariant,
    #[token("func")] KwFunc,
    #[token("pure")] KwPure,
    #[token("type")] KwType,
    #[token("trait")] KwTrait,
    #[token("impl")] KwImpl,
    #[token("for")] KwFor,
    #[token("const")] KwConst,
    #[token("var")] KwVar,
    #[token("if")] KwIf,
    #[token("else")] KwElse,
    #[token("while")] KwWhile,
    #[token("in")] KwIn,
    #[token("return")] KwReturn,
    #[token("break")] KwBreak,
    #[token("continue")] KwContinue,
    #[token("ghost")] KwGhost,
    #[token("match")] KwMatch,
    #[token("case")] KwCase,
    #[token("unsafe")] KwUnsafe,

    // Types
    #[token("bool")] TyBool,
    #[token("i8")] TyI8,
    #[token("i16")] TyI16,
    #[token("i32")] TyI32,
    #[token("i64")] TyI64,
    #[token("u8")] TyU8,
    #[token("u16")] TyU16,
    #[token("u32")] TyU32,
    #[token("u64")] TyU64,
    #[token("w8")] TyW8,
    #[token("w16")] TyW16,
    #[token("w32")] TyW32,
    #[token("w64")] TyW64,
    #[token("f32")] TyF32,
    #[token("f64")] TyF64,
    #[token("char")] TyChar,
    #[token("void")] TyVoid,
    #[token("string")] TyString,
    #[token("array")] TyArray,
    #[token("slice")] TySlice,
    #[token("ref")] TyRef,
    #[token("ptr")] TyPtr,
    #[token("mut")] KwMut,

    // Verification Keywords
    #[token("spec")] KwSpec,
    #[token("requires")] KwRequires,
    #[token("ensures")] KwEnsures,
    #[token("assigns")] KwAssigns,
    #[token("invariant")] KwInvariant,
    #[token("decreases")] KwDecreases,

    // Operators and Punctuation
    #[token("=")] Eq,
    #[token("==")] EqEq,
    #[token("!=")] BangEq,
    #[token("<")] Less,
    #[token(">")] Greater,
    #[token("<=")] LessEq,
    #[token(">=")] GreaterEq,
    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Star,
    #[token("/")] Slash,
    #[token("%")] Percent,
    #[token("!")] Bang,
    #[token("&")] Amp,
    #[token("&&")] AmpAmp,
    #[token("||")] PipePipe,
    #[token("==>")] Implies,
    #[token("<==>")] Iff,
    #[token("..")] DotDot,
    #[token("=>")] FatArrow,
    #[token("?")] Question,
    #[token(".")] Dot,
    #[token(",")] Comma,
    #[token(":")] Colon,
    #[token(";")] Semi,
    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("[")] LBracket,
    #[token("]")] RBracket,
    #[token("|")] Pipe,
    #[token("@")] At,

    // Literals and Identifiers
    #[token("true")] BoolTrue,
    #[token("false")] BoolFalse,
    
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,

    #[regex(r"[0-9]+")]
    IntLit,

    #[regex(r"[0-9]+\.[0-9]+")]
    FloatLit,

    #[regex(r#""([^"\\]|\\.)*""#)]
    StringLit,

    // Trivia (Whitespace and Comments are NOT skipped for lossless CST)
    #[regex(r"[ \t\n\f]+")]
    Whitespace,

    #[regex(r"//[^\n]*")]
    Comment,

    #[regex(r"/\*[^*]*\*+(?:[^/*][^*]*\*+)*/")]
    BlockComment,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that basic keywords and identifiers are lexed correctly, 
    /// including whitespace which is critical for lossless CST construction.
    #[test]
    fn test_lex_basic_function() {
        let input = "func main() {}";
        let mut lexer = Token::lexer(input);
        
        assert_eq!(lexer.next(), Some(Ok(Token::KwFunc)));
        assert_eq!(lexer.next(), Some(Ok(Token::Whitespace)));
        assert_eq!(lexer.next(), Some(Ok(Token::Ident)));
        assert_eq!(lexer.next(), Some(Ok(Token::LParen)));
        assert_eq!(lexer.next(), Some(Ok(Token::RParen)));
        assert_eq!(lexer.next(), Some(Ok(Token::Whitespace)));
        assert_eq!(lexer.next(), Some(Ok(Token::LBrace)));
        assert_eq!(lexer.next(), Some(Ok(Token::RBrace)));
        assert_eq!(lexer.next(), None);
    }
    
    /// Tests that comments are properly recognized as tokens.
    #[test]
    fn test_lex_comments() {
        let input = "// line comment\n/* block */";
        let mut lexer = Token::lexer(input);
        
        assert_eq!(lexer.next(), Some(Ok(Token::Comment)));
        assert_eq!(lexer.next(), Some(Ok(Token::Whitespace))); // The \n
        assert_eq!(lexer.next(), Some(Ok(Token::BlockComment)));
    }
    
    /// Tests verification keywords, ensuring they don't get misidentified.
    #[test]
    fn test_lex_verification() {
        let input = "spec { requires x > 0; }";
        let mut lexer = Token::lexer(input);
        
        assert_eq!(lexer.next(), Some(Ok(Token::KwSpec)));
        assert_eq!(lexer.next(), Some(Ok(Token::Whitespace)));
        assert_eq!(lexer.next(), Some(Ok(Token::LBrace)));
        assert_eq!(lexer.next(), Some(Ok(Token::Whitespace)));
        assert_eq!(lexer.next(), Some(Ok(Token::KwRequires)));
    }
}
