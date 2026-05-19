//! Token kinds recognised by the CapyLang lexer.
//!
//! The token set is intentionally explicit and stable: it forms part of the
//! contract between the lexer and downstream slices (parser, formatter,
//! syntax highlighter, IDE tooling). Additions are additive between minor
//! versions; renames or removals require a major bump.

use logos::{Lexer, Logos};

/// Every lexical category produced by the CapyLang lexer.
///
/// Variants are grouped into trivia, literals, keywords, identifiers,
/// punctuation, operators and synthetic markers. The grouping is also
/// reflected in the canonical token dump used by golden tests.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // === Trivia =============================================================
    #[regex(r"[ \t]+")]
    Whitespace,
    #[regex(r"\r?\n")]
    Newline,
    #[regex(r"//[^\r\n]*")]
    LineComment,
    #[token("/*", lex_block_comment)]
    BlockComment,

    // === Literals ===========================================================
    // Float must out-prioritise Int so `3.14` is not tokenised as `3` `.` `14`.
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9][0-9_]*)?", priority = 4)]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9][0-9_]*", priority = 4)]
    Float,

    #[regex(r"0|[1-9][0-9_]*", priority = 3)]
    #[regex(r"0x[0-9a-fA-F][0-9a-fA-F_]*", priority = 3)]
    #[regex(r"0b[01][01_]*", priority = 3)]
    #[regex(r"0o[0-7][0-7_]*", priority = 3)]
    Int,

    #[token("\"", lex_string)]
    Str,

    // === Keywords ===========================================================
    // Declared as `#[token]` so they out-prioritise the `Ident` regex even
    // though `logos` uses longest-match first.
    #[token("fn")]
    Fn,
    #[token("let")]
    Let,
    #[token("const")]
    Const,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("loop")]
    Loop,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("return")]
    Return,
    #[token("match")]
    Match,
    #[token("try")]
    Try,
    #[token("catch")]
    Catch,
    #[token("throw")]
    Throw,
    #[token("async")]
    Async,
    #[token("await")]
    Await,
    #[token("import")]
    Import,
    #[token("from")]
    From,
    #[token("as")]
    As,
    #[token("enum")]
    Enum,
    #[token("trait")]
    Trait,
    #[token("impl")]
    Impl,
    #[token("struct")]
    Struct,
    #[token("type")]
    Type,
    #[token("arena")]
    Arena,
    #[token("pub")]
    Pub,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("None")]
    NoneKw,
    #[token("Some")]
    SomeKw,

    // === Identifiers ========================================================
    #[regex(r"[\p{XID_Start}_][\p{XID_Continue}]*", priority = 2)]
    Ident,

    // === Punctuation ========================================================
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token("::")]
    ColonColon,
    #[token(":")]
    Colon,
    #[token("...")]
    DotDotDot,
    #[token("..")]
    DotDot,
    #[token(".")]
    Dot,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("?")]
    Question,
    #[token("@")]
    At,

    // === Operators ==========================================================
    #[token("==")]
    EqEq,
    #[token("=")]
    Eq,
    #[token("!=")]
    BangEq,
    #[token("!")]
    Bang,
    #[token("<=")]
    LtEq,
    #[token("<<")]
    LtLt,
    #[token("<")]
    Lt,
    #[token(">=")]
    GtEq,
    #[token(">>")]
    GtGt,
    #[token(">")]
    Gt,
    #[token("&&")]
    AmpAmp,
    #[token("&")]
    Amp,
    #[token("||")]
    PipePipe,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("+=")]
    PlusEq,
    #[token("+")]
    Plus,
    #[token("-=")]
    MinusEq,
    #[token("-")]
    Minus,
    #[token("*=")]
    StarEq,
    #[token("*")]
    Star,
    #[token("/=")]
    SlashEq,
    #[token("/")]
    Slash,
    #[token("%=")]
    PercentEq,
    #[token("%")]
    Percent,

    // === Synthetic ==========================================================
    /// Marker emitted once at the end of the source. Span is `[len, len)`.
    Eof,
    /// Emitted when input cannot be classified into any other kind. A
    /// matching [`crate::Diagnostic`] is stored alongside, describing the
    /// concrete failure (unterminated literal, unknown character, ...).
    Error,
}

impl TokenKind {
    /// True when the canonical dump should include the lexeme text.
    ///
    /// Used by the golden test infrastructure to keep snapshots compact while
    /// still preserving enough information to debug lexer regressions.
    #[must_use]
    pub fn carries_text(self) -> bool {
        matches!(self, Self::Ident | Self::Int | Self::Float | Self::Str)
    }

    /// True when the variant carries no semantic meaning for downstream
    /// stages (parser, type checker).
    ///
    /// Trivia tokens still appear in the stream so a formatter or syntax
    /// highlighter can reconstruct the source byte-for-byte; downstream
    /// pipelines may filter them out with this predicate.
    ///
    /// The set is stable across minor versions: removing a kind from it
    /// is a breaking change.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Whitespace | Self::Newline | Self::LineComment | Self::BlockComment
        )
    }
}

/// Callback used by `logos` for the `Str` token.
///
/// Scans the remainder of the source for the matching closing `"`, honouring
/// backslash escapes (`\"`, `\\`, `\n`, ...). On EOF or raw newline inside the
/// literal, returns `Err(())` so the surrounding lexer can emit a recoverable
/// [`crate::Diagnostic::UnterminatedString`].
fn lex_string(lex: &mut Lexer<TokenKind>) -> Result<(), ()> {
    let remainder = lex.remainder();
    let mut chars = remainder.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                if chars.next().is_none() {
                    lex.bump(remainder.len());
                    return Err(());
                }
            }
            '"' => {
                lex.bump(i + c.len_utf8());
                return Ok(());
            }
            '\n' => {
                lex.bump(i);
                return Err(());
            }
            _ => {}
        }
    }
    lex.bump(remainder.len());
    Err(())
}

/// Callback used by `logos` for the `BlockComment` token.
///
/// Supports arbitrarily-nested `/* ... */` comments. On EOF before the final
/// closing delimiter, returns `Err(())` so the surrounding lexer can emit a
/// recoverable [`crate::Diagnostic::UnterminatedBlockComment`].
fn lex_block_comment(lex: &mut Lexer<TokenKind>) -> Result<(), ()> {
    let remainder = lex.remainder();
    let mut chars = remainder.char_indices().peekable();
    let mut depth: usize = 1;
    while let Some((_, c)) = chars.next() {
        match c {
            '/' => {
                if let Some(&(_, '*')) = chars.peek() {
                    chars.next();
                    depth += 1;
                }
            }
            '*' => {
                if let Some(&(j, '/')) = chars.peek() {
                    chars.next();
                    depth -= 1;
                    if depth == 0 {
                        lex.bump(j + 1);
                        return Ok(());
                    }
                }
            }
            _ => {}
        }
    }
    lex.bump(remainder.len());
    Err(())
}
