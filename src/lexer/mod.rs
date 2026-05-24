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
    #[token("result")] TyResult,
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
    #[token("assert")] KwAssert,
    #[token("assume")] KwAssume,

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

    #[test]
    fn test_lex_keywords() {
        let input = "import pub func if while return match unsafe array slice";
        let mut lexer = Token::lexer(input);
        assert_eq!(lexer.next(), Some(Ok(Token::KwImport)));
        lexer.next(); // skip ws
        assert_eq!(lexer.next(), Some(Ok(Token::KwPub)));
        lexer.next(); // skip ws
        assert_eq!(lexer.next(), Some(Ok(Token::KwFunc)));
        lexer.next(); // skip ws
        assert_eq!(lexer.next(), Some(Ok(Token::KwIf)));
        lexer.next(); // skip ws
        assert_eq!(lexer.next(), Some(Ok(Token::KwWhile)));
        lexer.next(); // skip ws
        assert_eq!(lexer.next(), Some(Ok(Token::KwReturn)));
        lexer.next(); // skip ws
        assert_eq!(lexer.next(), Some(Ok(Token::KwMatch)));
        lexer.next(); // skip ws
        assert_eq!(lexer.next(), Some(Ok(Token::KwUnsafe)));
    }

    #[test]
    fn test_lex_types() {
        let input = "bool i32 f64 string";
        let mut lexer = Token::lexer(input);
        assert_eq!(lexer.next(), Some(Ok(Token::TyBool)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::TyI32)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::TyF64)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::TyString)));
    }

    #[test]
    fn test_lex_literals() {
        let input = "123 3.14 \"hello\" true";
        let mut lexer = Token::lexer(input);
        assert_eq!(lexer.next(), Some(Ok(Token::IntLit)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::FloatLit)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::StringLit)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::BoolTrue)));
    }

    #[test]
    fn test_lex_operators() {
        let input = "== <= ==> && ||";
        let mut lexer = Token::lexer(input);
        assert_eq!(lexer.next(), Some(Ok(Token::EqEq)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::LessEq)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::Implies)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::AmpAmp)));
        lexer.next();
        assert_eq!(lexer.next(), Some(Ok(Token::PipePipe)));
    }

    #[test]
    fn test_lex_trivia() {
        let input = "  // comment\n/* block */";
        let mut lexer = Token::lexer(input);
        assert_eq!(lexer.next(), Some(Ok(Token::Whitespace)));
        assert_eq!(lexer.next(), Some(Ok(Token::Comment)));
        assert_eq!(lexer.next(), Some(Ok(Token::Whitespace)));
        assert_eq!(lexer.next(), Some(Ok(Token::BlockComment)));
    }

    /// Tests all remaining verification-specific keywords.
    #[test]
    fn test_lex_all_verification_keywords() {
        let input = "requires ensures assigns invariant decreases assert assume";
        let tokens: Vec<_> = Token::lexer(input)
            .filter(|t| t.as_ref().map_or(true, |t| *t != Token::Whitespace))
            .collect();
        assert_eq!(tokens[0], Ok(Token::KwRequires));
        assert_eq!(tokens[1], Ok(Token::KwEnsures));
        assert_eq!(tokens[2], Ok(Token::KwAssigns));
        assert_eq!(tokens[3], Ok(Token::KwInvariant));
        assert_eq!(tokens[4], Ok(Token::KwDecreases));
        assert_eq!(tokens[5], Ok(Token::KwAssert));
        assert_eq!(tokens[6], Ok(Token::KwAssume));
    }

    /// Tests comparison and equality operators, including multi-char tokens.
    /// Verifies that `==` is not ambiguous with two `=` tokens and `<=` with `<` then `=`.
    #[test]
    fn test_lex_comparison_operators() {
        let input = "== != < > <= >=";
        let tokens: Vec<_> = Token::lexer(input)
            .filter(|t| t.as_ref().map_or(true, |t| *t != Token::Whitespace))
            .collect();
        assert_eq!(tokens[0], Ok(Token::EqEq));
        assert_eq!(tokens[1], Ok(Token::BangEq));
        assert_eq!(tokens[2], Ok(Token::Less));
        assert_eq!(tokens[3], Ok(Token::Greater));
        assert_eq!(tokens[4], Ok(Token::LessEq));
        assert_eq!(tokens[5], Ok(Token::GreaterEq));
    }

    /// Tests that an unknown character produces an error token rather than panicking.
    #[test]
    fn test_lex_error_token() {
        // `$` is not part of the Vera language
        let input = "$";
        let tokens: Vec<_> = Token::lexer(input).collect();
        assert_eq!(tokens.len(), 1);
        assert!(tokens[0].is_err(), "Expected an error token for unknown char '$'");
    }

    /// Tests that empty input produces no tokens at all.
    #[test]
    fn test_lex_empty_input() {
        let input = "";
        let tokens: Vec<_> = Token::lexer(input).collect();
        assert!(tokens.is_empty());
    }

    /// Tests that identifiers starting with underscores are correctly recognized.
    #[test]
    fn test_lex_underscore_ident() {
        let input = "_foo _bar_baz";
        let tokens: Vec<_> = Token::lexer(input)
            .filter(|t| t.as_ref().map_or(true, |t| *t != Token::Whitespace))
            .collect();
        assert_eq!(tokens[0], Ok(Token::Ident));
        assert_eq!(tokens[1], Ok(Token::Ident));
    }

    /// Tests that arithmetic operators are all individually correct.
    #[test]
    fn test_lex_arithmetic_operators() {
        let input = "+ - * / %";
        let tokens: Vec<_> = Token::lexer(input)
            .filter(|t| t.as_ref().map_or(true, |t| *t != Token::Whitespace))
            .collect();
        assert_eq!(tokens[0], Ok(Token::Plus));
        assert_eq!(tokens[1], Ok(Token::Minus));
        assert_eq!(tokens[2], Ok(Token::Star));
        assert_eq!(tokens[3], Ok(Token::Slash));
        assert_eq!(tokens[4], Ok(Token::Percent));
    }

    /// Tests that `false` boolean literal is correctly identified (not `Ident`).
    #[test]
    fn test_lex_false_literal() {
        let input = "false";
        let mut lexer = Token::lexer(input);
        assert_eq!(lexer.next(), Some(Ok(Token::BoolFalse)));
        assert_eq!(lexer.next(), None);
    }

    /// Tests a realistic function signature tokens sequence.
    #[test]
    fn test_lex_function_with_params() {
        let input = "func add(a: i32, b: i32): i32 {}";
        let tokens: Vec<_> = Token::lexer(input)
            .filter(|t| t.as_ref().map_or(true, |t| *t != Token::Whitespace))
            .collect();
        assert_eq!(tokens[0], Ok(Token::KwFunc));
        assert_eq!(tokens[1], Ok(Token::Ident)); // "add"
        assert_eq!(tokens[2], Ok(Token::LParen));
        assert_eq!(tokens[3], Ok(Token::Ident)); // "a"
        assert_eq!(tokens[4], Ok(Token::Colon));
        assert_eq!(tokens[5], Ok(Token::TyI32));
        assert_eq!(tokens[6], Ok(Token::Comma));
        assert_eq!(tokens[7], Ok(Token::Ident)); // "b"
        assert_eq!(tokens[8], Ok(Token::Colon));
        assert_eq!(tokens[9], Ok(Token::TyI32));
        assert_eq!(tokens[10], Ok(Token::RParen));
        assert_eq!(tokens[11], Ok(Token::Colon));
        assert_eq!(tokens[12], Ok(Token::TyI32));
        assert_eq!(tokens[13], Ok(Token::LBrace));
        assert_eq!(tokens[14], Ok(Token::RBrace));
    }
}
