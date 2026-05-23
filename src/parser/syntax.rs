use num_derive::{FromPrimitive, ToPrimitive};
use crate::lexer::Token;

/// The syntax kinds used by rowan to construct the CST.
/// It encompasses all lexical tokens and all structural nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, FromPrimitive, ToPrimitive)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // We map every Token over exactly
    KwImport, KwAs, KwPub, KwStruct, KwEnum, KwVariant, KwFunc, KwPure,
    KwType, KwTrait, KwImpl, KwFor, KwConst, KwVar, KwIf, KwElse, KwWhile,
    KwIn, KwReturn, KwBreak, KwContinue, KwGhost, KwMatch, KwCase, KwUnsafe,
    
    TyBool, TyI8, TyI16, TyI32, TyI64, TyU8, TyU16, TyU32, TyU64,
    TyW8, TyW16, TyW32, TyW64, TyF32, TyF64, TyChar, TyVoid, TyString,
    TyArray, TySlice, TyRef, TyPtr, KwMut,
    
    KwSpec, KwRequires, KwEnsures, KwAssigns, KwInvariant, KwDecreases,
    
    Eq, EqEq, BangEq, Less, Greater, LessEq, GreaterEq, Plus, Minus,
    Star, Slash, Percent, Bang, Amp, AmpAmp, PipePipe, Implies, Iff,
    DotDot, FatArrow, Question, Dot, Comma, Colon, Semi,
    LParen, RParen, LBrace, RBrace, LBracket, RBracket, Pipe, At,
    
    BoolTrue, BoolFalse, Ident, IntLit, FloatLit, StringLit,
    Whitespace, Comment, BlockComment, ErrorToken,
    
    // Nodes (Branches)
    SOURCE_FILE,
    FUNC_DECL,
    PARAM_LIST,
    PARAM,
    TYPE_REF,
    BLOCK_EXPR,
    RETURN_STMT,
    ERROR_NODE,
}

impl From<Token> for SyntaxKind {
    fn from(token: Token) -> Self {
        match token {
            Token::KwImport => SyntaxKind::KwImport,
            Token::KwAs => SyntaxKind::KwAs,
            Token::KwPub => SyntaxKind::KwPub,
            Token::KwStruct => SyntaxKind::KwStruct,
            Token::KwEnum => SyntaxKind::KwEnum,
            Token::KwVariant => SyntaxKind::KwVariant,
            Token::KwFunc => SyntaxKind::KwFunc,
            Token::KwPure => SyntaxKind::KwPure,
            Token::KwType => SyntaxKind::KwType,
            Token::KwTrait => SyntaxKind::KwTrait,
            Token::KwImpl => SyntaxKind::KwImpl,
            Token::KwFor => SyntaxKind::KwFor,
            Token::KwConst => SyntaxKind::KwConst,
            Token::KwVar => SyntaxKind::KwVar,
            Token::KwIf => SyntaxKind::KwIf,
            Token::KwElse => SyntaxKind::KwElse,
            Token::KwWhile => SyntaxKind::KwWhile,
            Token::KwIn => SyntaxKind::KwIn,
            Token::KwReturn => SyntaxKind::KwReturn,
            Token::KwBreak => SyntaxKind::KwBreak,
            Token::KwContinue => SyntaxKind::KwContinue,
            Token::KwGhost => SyntaxKind::KwGhost,
            Token::KwMatch => SyntaxKind::KwMatch,
            Token::KwCase => SyntaxKind::KwCase,
            Token::KwUnsafe => SyntaxKind::KwUnsafe,
            
            Token::TyBool => SyntaxKind::TyBool,
            Token::TyI8 => SyntaxKind::TyI8,
            Token::TyI16 => SyntaxKind::TyI16,
            Token::TyI32 => SyntaxKind::TyI32,
            Token::TyI64 => SyntaxKind::TyI64,
            Token::TyU8 => SyntaxKind::TyU8,
            Token::TyU16 => SyntaxKind::TyU16,
            Token::TyU32 => SyntaxKind::TyU32,
            Token::TyU64 => SyntaxKind::TyU64,
            Token::TyW8 => SyntaxKind::TyW8,
            Token::TyW16 => SyntaxKind::TyW16,
            Token::TyW32 => SyntaxKind::TyW32,
            Token::TyW64 => SyntaxKind::TyW64,
            Token::TyF32 => SyntaxKind::TyF32,
            Token::TyF64 => SyntaxKind::TyF64,
            Token::TyChar => SyntaxKind::TyChar,
            Token::TyVoid => SyntaxKind::TyVoid,
            Token::TyString => SyntaxKind::TyString,
            Token::TyArray => SyntaxKind::TyArray,
            Token::TySlice => SyntaxKind::TySlice,
            Token::TyRef => SyntaxKind::TyRef,
            Token::TyPtr => SyntaxKind::TyPtr,
            Token::KwMut => SyntaxKind::KwMut,
            
            Token::KwSpec => SyntaxKind::KwSpec,
            Token::KwRequires => SyntaxKind::KwRequires,
            Token::KwEnsures => SyntaxKind::KwEnsures,
            Token::KwAssigns => SyntaxKind::KwAssigns,
            Token::KwInvariant => SyntaxKind::KwInvariant,
            Token::KwDecreases => SyntaxKind::KwDecreases,
            
            Token::Eq => SyntaxKind::Eq,
            Token::EqEq => SyntaxKind::EqEq,
            Token::BangEq => SyntaxKind::BangEq,
            Token::Less => SyntaxKind::Less,
            Token::Greater => SyntaxKind::Greater,
            Token::LessEq => SyntaxKind::LessEq,
            Token::GreaterEq => SyntaxKind::GreaterEq,
            Token::Plus => SyntaxKind::Plus,
            Token::Minus => SyntaxKind::Minus,
            Token::Star => SyntaxKind::Star,
            Token::Slash => SyntaxKind::Slash,
            Token::Percent => SyntaxKind::Percent,
            Token::Bang => SyntaxKind::Bang,
            Token::Amp => SyntaxKind::Amp,
            Token::AmpAmp => SyntaxKind::AmpAmp,
            Token::PipePipe => SyntaxKind::PipePipe,
            Token::Implies => SyntaxKind::Implies,
            Token::Iff => SyntaxKind::Iff,
            Token::DotDot => SyntaxKind::DotDot,
            Token::FatArrow => SyntaxKind::FatArrow,
            Token::Question => SyntaxKind::Question,
            Token::Dot => SyntaxKind::Dot,
            Token::Comma => SyntaxKind::Comma,
            Token::Colon => SyntaxKind::Colon,
            Token::Semi => SyntaxKind::Semi,
            Token::LParen => SyntaxKind::LParen,
            Token::RParen => SyntaxKind::RParen,
            Token::LBrace => SyntaxKind::LBrace,
            Token::RBrace => SyntaxKind::RBrace,
            Token::LBracket => SyntaxKind::LBracket,
            Token::RBracket => SyntaxKind::RBracket,
            Token::Pipe => SyntaxKind::Pipe,
            Token::At => SyntaxKind::At,
            
            Token::BoolTrue => SyntaxKind::BoolTrue,
            Token::BoolFalse => SyntaxKind::BoolFalse,
            Token::Ident => SyntaxKind::Ident,
            Token::IntLit => SyntaxKind::IntLit,
            Token::FloatLit => SyntaxKind::FloatLit,
            Token::StringLit => SyntaxKind::StringLit,
            
            Token::Whitespace => SyntaxKind::Whitespace,
            Token::Comment => SyntaxKind::Comment,
            Token::BlockComment => SyntaxKind::BlockComment,
        }
    }
}

/// The rowan Language definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VeraLanguage {}

impl rowan::Language for VeraLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        num_traits::FromPrimitive::from_u16(raw.0).unwrap()
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(num_traits::ToPrimitive::to_u16(&kind).unwrap())
    }
}

pub type SyntaxNode = rowan::SyntaxNode<VeraLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<VeraLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<VeraLanguage>;
