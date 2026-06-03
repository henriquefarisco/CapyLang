//! Recursive-descent expression parser.
//!
//! See `docs/grammar.ebnf` (`expression` and below) for the grammar this
//! module implements. Trivia tokens (whitespace, newlines, comments) are
//! filtered out by [`Parser::new`]; the canonical AST therefore has no
//! trivia. Lexer diagnostics are forwarded as [`ParseErrorKind::Lex`].
//!
//! Error recovery: on an unexpected token, the parser emits a
//! [`ParseDiagnostic`] and produces an [`Expr::Error`] placeholder. On a
//! missing closing delimiter, the parser advances past the offending token
//! to make progress; this is documented in `docs/compatibility.md` (error
//! model row "Parse error (S2)").

#![forbid(unsafe_code)]

use capy_ast::{
    BinOp, ConstItem, EnumItem, Expr, FnItem, Ident, ImportItem, Item, MatchArm, Param, Pattern,
    Source, Span, Stmt, StructField, StructItem, StructLitField, StructPatternField, Type,
    TypeAlias, UnOp, Variant, VariantBody,
};
use capy_lexer::{tokenize, LexResult, Token, TokenKind};

use crate::diagnostic::{ParseDiagnostic, ParseErrorKind};

/// Output of [`parse_expr`].
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub expr: Expr,
    pub diagnostics: Vec<ParseDiagnostic>,
}

/// Output of [`parse_source`].
#[derive(Debug, Clone)]
pub struct ParseSourceResult {
    pub source: Source,
    pub diagnostics: Vec<ParseDiagnostic>,
}

/// Parses `source` as a single CapyLang expression.
///
/// Always returns: lex errors are surfaced as [`ParseErrorKind::Lex`] and
/// recoverable parse errors as [`ParseErrorKind::UnexpectedToken`] /
/// [`ParseErrorKind::UnexpectedEof`]. Any trailing input after the
/// expression is reported as a single `UnexpectedToken` diagnostic.
#[must_use]
pub fn parse_expr(source: &str) -> ParseResult {
    let lex = tokenize(source);
    let mut parser = Parser::new(source, &lex);
    let expr = parser.parse_expression(MIN_PREC);
    parser.expect_eof();
    ParseResult {
        expr,
        diagnostics: parser.diagnostics,
    }
}

/// Parses `source` as a top-level [`Source`] (a sequence of statements).
///
/// Same fail-closed contract as [`parse_expr`]: lex errors surface as
/// [`ParseErrorKind::Lex`], every malformed range becomes a typed
/// diagnostic. Block-like expression statements may omit the trailing `;`;
/// every other expression statement must carry one.
#[must_use]
pub fn parse_source(source: &str) -> ParseSourceResult {
    let lex = tokenize(source);
    let mut parser = Parser::new(source, &lex);
    let stmts = parser.parse_top_level_stmts();
    let total = source.len();
    let result = Source {
        stmts,
        span: Span::new(0, total),
    };
    ParseSourceResult {
        source: result,
        diagnostics: parser.diagnostics,
    }
}

/// Minimum binary-operator precedence accepted at the top level.
const MIN_PREC: u8 = 1;

struct Parser<'a> {
    source: &'a str,
    /// Non-trivia tokens. Always ends with a synthetic `Eof` token.
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Vec<ParseDiagnostic>,
    /// When set, a path immediately followed by `{` is **not** parsed as
    /// a struct-literal expression (S6.3c): the `{` belongs to a control
    /// flow body instead. Set only while parsing the head of `if` /
    /// `while` / `match` and the bounds of `for` (via
    /// [`Parser::parse_head_expr`]); reset to `false` inside every
    /// delimiter — parens, call args, array elements, index, block
    /// bodies, struct-literal field values and `match` arms (via
    /// [`Parser::parse_delimited_expr`] / [`Parser::parse_block_expr`]).
    no_struct_literal: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, lex: &LexResult) -> Self {
        let tokens: Vec<Token> = lex
            .tokens
            .iter()
            .copied()
            .filter(|t| !t.kind.is_trivia())
            .collect();
        let mut diagnostics = Vec::new();
        for d in &lex.diagnostics {
            diagnostics.push(ParseDiagnostic::new(ParseErrorKind::Lex(d.kind), d.span));
        }
        // `tokenize` guarantees at least the trailing `Eof`; after filtering
        // trivia we are still left with `Eof` so `peek()` is always safe.
        debug_assert!(matches!(
            tokens.last().map(|t| t.kind),
            Some(TokenKind::Eof)
        ));
        Self {
            source,
            tokens,
            pos: 0,
            diagnostics,
            no_struct_literal: false,
        }
    }

    /// Parses a full expression with struct-literal syntax **suppressed**
    /// at the top level. Used for the head of `if` / `while` / `match`
    /// and the bounds of `for`, where `Path { ... }` is a path followed
    /// by a block rather than a struct literal. The suppression is reset
    /// inside any delimiter (see [`Self::parse_delimited_expr`]).
    fn parse_head_expr(&mut self) -> Expr {
        let saved = self.no_struct_literal;
        self.no_struct_literal = true;
        let e = self.parse_expression(MIN_PREC);
        self.no_struct_literal = saved;
        e
    }

    /// Parses a full expression with struct-literal syntax **allowed**.
    /// Used inside every delimiter (parens, call args, array elements,
    /// index, struct-literal field values and `match` arms) so a head
    /// context does not leak its suppression into nested expressions.
    fn parse_delimited_expr(&mut self) -> Expr {
        let saved = self.no_struct_literal;
        self.no_struct_literal = false;
        let e = self.parse_expression(MIN_PREC);
        self.no_struct_literal = saved;
        e
    }

    fn peek(&self) -> Token {
        self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek();
        if tok.kind != TokenKind::Eof {
            self.pos += 1;
        }
        tok
    }

    fn lexeme(&self, span: Span) -> &str {
        self.source.get(span.start..span.end).unwrap_or("")
    }

    fn error(&mut self, kind: ParseErrorKind, span: Span) {
        self.diagnostics.push(ParseDiagnostic::new(kind, span));
    }

    /// Expects a specific token kind. On mismatch, emits a diagnostic and
    /// advances past the offending token (except at EOF) so the surrounding
    /// loop can make progress.
    fn expect(&mut self, kind: TokenKind, expected: &'static str) -> Token {
        let tok = self.peek();
        if tok.kind == kind {
            self.advance();
            return tok;
        }
        if tok.kind == TokenKind::Eof {
            self.error(ParseErrorKind::UnexpectedEof { expected }, tok.span);
        } else {
            self.error(
                ParseErrorKind::UnexpectedToken {
                    found: tok.kind,
                    expected,
                },
                tok.span,
            );
            self.advance();
        }
        tok
    }

    fn expect_ident(&mut self, expected: &'static str) -> Ident {
        let tok = self.peek();
        if matches!(tok.kind, TokenKind::Ident | TokenKind::SomeKw) {
            self.advance();
            return Ident {
                name: self.lexeme(tok.span).to_string(),
                span: tok.span,
            };
        }
        let kind = if tok.kind == TokenKind::Eof {
            ParseErrorKind::UnexpectedEof { expected }
        } else {
            ParseErrorKind::UnexpectedToken {
                found: tok.kind,
                expected,
            }
        };
        self.error(kind, tok.span);
        // Synthesise a zero-width identifier anchored at the offending span
        // so the AST can still describe the structure. Do **not** advance
        // here: the caller (path or field parser) is responsible for the
        // surrounding context and may want to keep the offending token for
        // a higher-level recovery decision.
        Ident {
            name: String::new(),
            span: Span::new(tok.span.start, tok.span.start),
        }
    }

    fn expect_eof(&mut self) {
        let tok = self.peek();
        if tok.kind != TokenKind::Eof {
            self.error(
                ParseErrorKind::UnexpectedToken {
                    found: tok.kind,
                    expected: "end of input",
                },
                tok.span,
            );
        }
    }

    /// Precedence-climbing binary parser. Left-associative for every operator
    /// in `BinOp` (S2.0 has no right-associative binary operators).
    fn parse_expression(&mut self, min_prec: u8) -> Expr {
        let mut lhs = self.parse_unary();
        loop {
            let tok = self.peek();
            let op = match token_to_binop(tok.kind) {
                Some(op) if op.precedence() >= min_prec => op,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_expression(op.precedence() + 1);
            let span = Span::new(lhs.span().start, rhs.span().end);
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        // Assignment binds looser than every binary operator and is
        // right-associative. It is only recognised at the top expression
        // level (`MIN_PREC`); binary operands recurse with a higher
        // `min_prec` and therefore never absorb a trailing `=`. Compound
        // assignments (`+=`, ...) desugar into `target = target <op> rhs`.
        let assign = if min_prec == MIN_PREC {
            token_to_assign_op(self.peek().kind)
        } else {
            None
        };
        if let Some(compound) = assign {
            self.advance();
            let rhs = self.parse_expression(MIN_PREC);
            let span = Span::new(lhs.span().start, rhs.span().end);
            let value = match compound {
                None => rhs,
                Some(op) => Expr::Binary {
                    op,
                    lhs: Box::new(lhs.clone()),
                    rhs: Box::new(rhs),
                    span,
                },
            };
            lhs = Expr::Assign {
                target: Box::new(lhs),
                value: Box::new(value),
                span,
            };
        }
        lhs
    }

    fn parse_unary(&mut self) -> Expr {
        let tok = self.peek();
        let op = match tok.kind {
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Bang => Some(UnOp::Not),
            TokenKind::Tilde => Some(UnOp::BitNot),
            _ => None,
        };
        if let Some(op) = op {
            let start = tok.span.start;
            self.advance();
            let operand = self.parse_unary();
            let span = Span::new(start, operand.span().end);
            return Expr::Unary {
                op,
                operand: Box::new(operand),
                span,
            };
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        loop {
            let tok = self.peek();
            match tok.kind {
                TokenKind::LParen => {
                    self.advance();
                    let args = self.parse_call_args();
                    let close = self.expect(TokenKind::RParen, "`)`");
                    let span = Span::new(expr.span().start, close.span.end);
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                        span,
                    };
                }
                TokenKind::LBracket => {
                    self.advance();
                    let index = self.parse_delimited_expr();
                    let close = self.expect(TokenKind::RBracket, "`]`");
                    let span = Span::new(expr.span().start, close.span.end);
                    expr = Expr::Index {
                        target: Box::new(expr),
                        index: Box::new(index),
                        span,
                    };
                }
                TokenKind::Dot => {
                    self.advance();
                    let name = self.expect_ident("field name");
                    let span = Span::new(expr.span().start, name.span.end);
                    expr = Expr::Field {
                        target: Box::new(expr),
                        name,
                        span,
                    };
                }
                _ => break,
            }
        }
        expr
    }

    fn parse_call_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        loop {
            let tok = self.peek();
            if matches!(tok.kind, TokenKind::RParen | TokenKind::Eof) {
                break;
            }
            args.push(self.parse_delimited_expr());
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }
        args
    }

    fn parse_primary(&mut self) -> Expr {
        let tok = self.peek();
        match tok.kind {
            TokenKind::Int => {
                self.advance();
                Expr::Int {
                    text: self.lexeme(tok.span).to_string(),
                    span: tok.span,
                }
            }
            TokenKind::Float => {
                self.advance();
                Expr::Float {
                    text: self.lexeme(tok.span).to_string(),
                    span: tok.span,
                }
            }
            TokenKind::Str => {
                self.advance();
                Expr::Str {
                    text: self.lexeme(tok.span).to_string(),
                    span: tok.span,
                }
            }
            TokenKind::True => {
                self.advance();
                Expr::Bool {
                    value: true,
                    span: tok.span,
                }
            }
            TokenKind::False => {
                self.advance();
                Expr::Bool {
                    value: false,
                    span: tok.span,
                }
            }
            TokenKind::NoneKw => {
                self.advance();
                Expr::NoneLit { span: tok.span }
            }
            TokenKind::Ident => self.parse_ident_or_path(),
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_delimited_expr();
                let close = self.expect(TokenKind::RParen, "`)`");
                let span = Span::new(tok.span.start, close.span.end);
                Expr::Paren {
                    inner: Box::new(inner),
                    span,
                }
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LBrace => self.parse_block_expr(),
            TokenKind::If => self.parse_if_expr(),
            TokenKind::While => self.parse_while_expr(),
            TokenKind::For => self.parse_for_expr(),
            TokenKind::Loop => self.parse_loop_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Return => self.parse_return_expr(),
            TokenKind::Break => self.parse_break_expr(),
            TokenKind::Continue => self.parse_continue_expr(),
            TokenKind::Error => {
                // The lexer already attached a Diagnostic for this region,
                // forwarded as ParseErrorKind::Lex by `Parser::new`. Emit a
                // structural placeholder and consume the token so the outer
                // loop can keep going.
                self.advance();
                Expr::Error { span: tok.span }
            }
            TokenKind::Eof => {
                self.error(
                    ParseErrorKind::UnexpectedEof {
                        expected: "expression",
                    },
                    tok.span,
                );
                Expr::Error { span: tok.span }
            }
            kind => {
                self.error(
                    ParseErrorKind::UnexpectedToken {
                        found: kind,
                        expected: "expression",
                    },
                    tok.span,
                );
                self.advance();
                Expr::Error { span: tok.span }
            }
        }
    }

    /// Parses a top-level sequence of statements until [`TokenKind::Eof`].
    ///
    /// Mirrors the body of a [`Expr::Block`] but with no enclosing braces
    /// and no tail expression slot: every statement must terminate with `;`
    /// or be block-like.
    fn parse_top_level_stmts(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        while self.peek().kind != TokenKind::Eof {
            let before = self.pos;
            let kind = self.peek().kind;
            if token_starts_item(kind) {
                let item = self.parse_item();
                stmts.push(Stmt::Item(item));
            } else if kind == TokenKind::Let {
                stmts.push(self.parse_let_stmt());
            } else {
                stmts.push(self.parse_expr_stmt());
            }
            // Defensive: if a sub-parser failed to advance even after error
            // recovery (which would otherwise spin the outer loop), force a
            // single-token bump so the loop terminates.
            if self.pos == before {
                if self.peek().kind == TokenKind::Eof {
                    break;
                }
                self.advance();
            }
        }
        stmts
    }

    fn parse_let_stmt(&mut self) -> Stmt {
        let let_kw = self.expect(TokenKind::Let, "`let`");
        let name = self.expect_ident("identifier after `let`");
        let ty = if self.peek().kind == TokenKind::Colon {
            self.advance();
            Some(self.parse_type())
        } else {
            None
        };
        let init = if self.peek().kind == TokenKind::Eq {
            self.advance();
            Some(self.parse_expression(MIN_PREC))
        } else {
            None
        };
        let semi = self.expect(TokenKind::Semicolon, "`;`");
        Stmt::Let {
            name,
            ty,
            init,
            span: Span::new(let_kw.span.start, semi.span.end),
        }
    }

    /// Parses one expression statement at top level: `<expr> ;` or, when
    /// the expression is block-like, `<expr>` (no `;`).
    fn parse_expr_stmt(&mut self) -> Stmt {
        let start = self.peek().span.start;
        let expr = self.parse_expression(MIN_PREC);
        if self.peek().kind == TokenKind::Semicolon {
            let semi = self.advance();
            return Stmt::Expr {
                expr,
                has_semi: true,
                span: Span::new(start, semi.span.end),
            };
        }
        if expr.is_block_like() {
            let span = expr.span();
            return Stmt::Expr {
                expr,
                has_semi: false,
                span,
            };
        }
        // Missing `;`. Emit diagnostic; keep the expression as a stmt and
        // let the outer loop make progress.
        let cur = self.peek();
        let kind = if cur.kind == TokenKind::Eof {
            ParseErrorKind::UnexpectedEof { expected: "`;`" }
        } else {
            ParseErrorKind::UnexpectedToken {
                found: cur.kind,
                expected: "`;`",
            }
        };
        self.error(kind, cur.span);
        let span = expr.span();
        Stmt::Expr {
            expr,
            has_semi: false,
            span,
        }
    }

    /// Parses `{ stmts; tail? }`. The opening `{` is at [`Self::peek`].
    fn parse_block_expr(&mut self) -> Expr {
        let open = self.expect(TokenKind::LBrace, "`{`");
        // A block body is a fresh expression context: struct literals are
        // allowed even when the block itself sits in a no-struct-literal
        // head (e.g. the body of `if cond { Point { x: 1 } }`).
        let saved_no_struct = self.no_struct_literal;
        self.no_struct_literal = false;
        let mut stmts: Vec<Stmt> = Vec::new();
        let mut tail: Option<Box<Expr>> = None;
        loop {
            let cur = self.peek();
            if matches!(cur.kind, TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let before = self.pos;
            if token_starts_item(cur.kind) {
                let item = self.parse_item();
                stmts.push(Stmt::Item(item));
            } else if cur.kind == TokenKind::Let {
                stmts.push(self.parse_let_stmt());
            } else {
                let start = cur.span.start;
                let expr = self.parse_expression(MIN_PREC);
                let after = self.peek();
                if after.kind == TokenKind::Semicolon {
                    let semi = self.advance();
                    stmts.push(Stmt::Expr {
                        expr,
                        has_semi: true,
                        span: Span::new(start, semi.span.end),
                    });
                } else if matches!(after.kind, TokenKind::RBrace | TokenKind::Eof) {
                    // Tail expression: no `;`, immediately followed by the
                    // block closer. End of block body.
                    tail = Some(Box::new(expr));
                    break;
                } else if expr.is_block_like() {
                    let span = expr.span();
                    stmts.push(Stmt::Expr {
                        expr,
                        has_semi: false,
                        span,
                    });
                } else {
                    // Missing `;` between non-block-like statements; recover
                    // by treating the expression as a stmt and reporting.
                    self.error(
                        ParseErrorKind::UnexpectedToken {
                            found: after.kind,
                            expected: "`;`",
                        },
                        after.span,
                    );
                    let span = expr.span();
                    stmts.push(Stmt::Expr {
                        expr,
                        has_semi: false,
                        span,
                    });
                }
            }
            if self.pos == before {
                // Defensive: never spin the loop without progress.
                if self.peek().kind == TokenKind::Eof {
                    break;
                }
                self.advance();
            }
        }
        let close = self.expect(TokenKind::RBrace, "`}`");
        self.no_struct_literal = saved_no_struct;
        Expr::Block {
            stmts,
            tail,
            span: Span::new(open.span.start, close.span.end),
        }
    }

    /// `[ [ expr ("," expr)* [","] ] ]` — array literal (S6.2).
    fn parse_array_literal(&mut self) -> Expr {
        let open = self.expect(TokenKind::LBracket, "`[`");
        let mut elems = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBracket | TokenKind::Eof) {
            elems.push(self.parse_delimited_expr());
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }
        let close = self.expect(TokenKind::RBracket, "`]`");
        Expr::Array {
            elems,
            span: Span::new(open.span.start, close.span.end),
        }
    }

    /// `if <cond> <block> [else (if-expr | block)]`
    fn parse_if_expr(&mut self) -> Expr {
        let if_kw = self.expect(TokenKind::If, "`if`");
        let cond = self.parse_head_expr();
        let then_branch = self.parse_block_expr();
        let mut end = then_branch.span().end;
        let else_branch = if self.peek().kind == TokenKind::Else {
            self.advance();
            let next = if self.peek().kind == TokenKind::If {
                self.parse_if_expr()
            } else {
                self.parse_block_expr()
            };
            end = next.span().end;
            Some(Box::new(next))
        } else {
            None
        };
        Expr::If {
            cond: Box::new(cond),
            then_branch: Box::new(then_branch),
            else_branch,
            span: Span::new(if_kw.span.start, end),
        }
    }

    /// `while <cond> <block>`
    fn parse_while_expr(&mut self) -> Expr {
        let while_kw = self.expect(TokenKind::While, "`while`");
        let cond = self.parse_head_expr();
        let body = self.parse_block_expr();
        let span = Span::new(while_kw.span.start, body.span().end);
        Expr::While {
            cond: Box::new(cond),
            body: Box::new(body),
            span,
        }
    }

    /// `loop <block>`
    fn parse_loop_expr(&mut self) -> Expr {
        let loop_kw = self.expect(TokenKind::Loop, "`loop`");
        let body = self.parse_block_expr();
        let span = Span::new(loop_kw.span.start, body.span().end);
        Expr::Loop {
            body: Box::new(body),
            span,
        }
    }

    /// `for <ident> in <start> ( ".." | "..=" ) <end> <block>`.
    ///
    /// v0 supports an integer range iterator only. `..=` is recognised by
    /// the same `..`-then-adjacent-`=` rule used by range patterns (no
    /// dedicated `..=` lexer token).
    fn parse_for_expr(&mut self) -> Expr {
        let for_kw = self.expect(TokenKind::For, "`for`");
        let var = self.expect_ident("loop variable after `for`");
        self.expect(TokenKind::In, "`in`");
        let start = self.parse_head_expr();
        let dotdot = self.expect(TokenKind::DotDot, "`..` in `for` range");
        let inclusive =
            self.peek().kind == TokenKind::Eq && self.peek().span.start == dotdot.span.end;
        if inclusive {
            self.advance();
        }
        let end = self.parse_head_expr();
        let body = self.parse_block_expr();
        let span = Span::new(for_kw.span.start, body.span().end);
        Expr::For {
            var,
            start: Box::new(start),
            end: Box::new(end),
            inclusive,
            body: Box::new(body),
            span,
        }
    }

    /// `match <scrutinee> { <arm,>* }` (S2.2b).
    ///
    /// `match` is a primary expression; the scrutinee is parsed with the
    /// usual expression grammar. Arms are separated by `,`; a trailing
    /// `,` after the last arm is allowed and `,` is optional after an
    /// arm whose body is block-like. The arm grammar is
    /// `pattern (if <guard>)? => <body>`.
    fn parse_match_expr(&mut self) -> Expr {
        let kw = self.expect(TokenKind::Match, "`match`");
        let scrutinee = self.parse_head_expr();
        self.expect(TokenKind::LBrace, "`{`");
        let mut arms: Vec<MatchArm> = Vec::new();
        loop {
            let cur = self.peek();
            if matches!(cur.kind, TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let before = self.pos;
            let arm = self.parse_match_arm();
            let body_block_like = arm.body.is_block_like();
            arms.push(arm);
            let after = self.peek();
            if after.kind == TokenKind::Comma {
                self.advance();
            } else if matches!(after.kind, TokenKind::RBrace | TokenKind::Eof) {
                break;
            } else if !body_block_like {
                self.error(
                    ParseErrorKind::UnexpectedToken {
                        found: after.kind,
                        expected: "`,` or `}` after match arm",
                    },
                    after.span,
                );
            }
            // Defensive: never spin without progress.
            if self.pos == before {
                if self.peek().kind == TokenKind::Eof {
                    break;
                }
                self.advance();
            }
        }
        let close = self.expect(TokenKind::RBrace, "`}`");
        Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: Span::new(kw.span.start, close.span.end),
        }
    }

    fn parse_match_arm(&mut self) -> MatchArm {
        let start = self.peek().span.start;
        let pattern = self.parse_pattern();
        let guard = if self.peek().kind == TokenKind::If {
            self.advance();
            Some(self.parse_delimited_expr())
        } else {
            None
        };
        self.expect(TokenKind::FatArrow, "`=>`");
        let body = self.parse_delimited_expr();
        let end = body.span().end;
        MatchArm {
            pattern,
            guard,
            body,
            span: Span::new(start, end),
        }
    }

    /// Parse a full pattern (or-pattern at the outermost level).
    ///
    /// An or-pattern always has at least two alternatives; a single
    /// alternative collapses to the underlying primary pattern.
    fn parse_pattern(&mut self) -> Pattern {
        let first = self.parse_primary_pattern();
        if self.peek().kind != TokenKind::Pipe {
            return first;
        }
        let mut alts = vec![first];
        while self.peek().kind == TokenKind::Pipe {
            self.advance();
            alts.push(self.parse_primary_pattern());
        }
        let start = alts.first().expect("non-empty or-pattern").span().start;
        let end = alts.last().expect("non-empty or-pattern").span().end;
        Pattern::Or {
            alts,
            span: Span::new(start, end),
        }
    }

    fn parse_primary_pattern(&mut self) -> Pattern {
        let tok = self.peek();
        let base = match tok.kind {
            TokenKind::DotDot => {
                self.advance();
                Pattern::Rest { span: tok.span }
            }
            TokenKind::Int
            | TokenKind::Float
            | TokenKind::Str
            | TokenKind::True
            | TokenKind::False
            | TokenKind::NoneKw
            | TokenKind::Minus => self.parse_literal_pattern(),
            TokenKind::Ident | TokenKind::SomeKw => self.parse_ident_pattern(),
            TokenKind::Error => {
                self.advance();
                Pattern::Error { span: tok.span }
            }
            TokenKind::Eof => {
                self.error(
                    ParseErrorKind::UnexpectedEof {
                        expected: "pattern",
                    },
                    tok.span,
                );
                Pattern::Error { span: tok.span }
            }
            kind => {
                self.error(
                    ParseErrorKind::UnexpectedToken {
                        found: kind,
                        expected: "pattern",
                    },
                    tok.span,
                );
                self.advance();
                Pattern::Error { span: tok.span }
            }
        };
        // Range pattern: literal followed by `..` or `..=` and another
        // literal. Only literals can be range endpoints in S2.2b; richer
        // endpoints (path constants, expressions) follow in a later slice.
        if matches!(base, Pattern::Literal { .. }) && self.peek().kind == TokenKind::DotDot {
            return self.parse_range_pattern_after_lo(base);
        }
        base
    }

    fn parse_literal_pattern(&mut self) -> Pattern {
        let tok = self.peek();
        if tok.kind == TokenKind::Minus {
            let minus_span = tok.span;
            self.advance();
            let inner_tok = self.peek();
            let inner = match inner_tok.kind {
                TokenKind::Int => {
                    self.advance();
                    Expr::Int {
                        text: self.lexeme(inner_tok.span).to_string(),
                        span: inner_tok.span,
                    }
                }
                TokenKind::Float => {
                    self.advance();
                    Expr::Float {
                        text: self.lexeme(inner_tok.span).to_string(),
                        span: inner_tok.span,
                    }
                }
                other => {
                    self.error(
                        ParseErrorKind::UnexpectedToken {
                            found: other,
                            expected: "numeric literal after `-` in pattern",
                        },
                        inner_tok.span,
                    );
                    return Pattern::Error {
                        span: Span::new(minus_span.start, inner_tok.span.start),
                    };
                }
            };
            let span = Span::new(minus_span.start, inner.span().end);
            let value = Expr::Unary {
                op: UnOp::Neg,
                operand: Box::new(inner),
                span,
            };
            return Pattern::Literal { value, span };
        }
        let value = match tok.kind {
            TokenKind::Int => {
                self.advance();
                Expr::Int {
                    text: self.lexeme(tok.span).to_string(),
                    span: tok.span,
                }
            }
            TokenKind::Float => {
                self.advance();
                Expr::Float {
                    text: self.lexeme(tok.span).to_string(),
                    span: tok.span,
                }
            }
            TokenKind::Str => {
                self.advance();
                Expr::Str {
                    text: self.lexeme(tok.span).to_string(),
                    span: tok.span,
                }
            }
            TokenKind::True => {
                self.advance();
                Expr::Bool {
                    value: true,
                    span: tok.span,
                }
            }
            TokenKind::False => {
                self.advance();
                Expr::Bool {
                    value: false,
                    span: tok.span,
                }
            }
            TokenKind::NoneKw => {
                self.advance();
                Expr::NoneLit { span: tok.span }
            }
            _ => unreachable!("parse_literal_pattern dispatched on non-literal token"),
        };
        let span = value.span();
        Pattern::Literal { value, span }
    }

    fn parse_ident_pattern(&mut self) -> Pattern {
        let first = self.expect_ident("pattern");
        // The lexer treats `_` as part of the `Ident` regex; the wildcard
        // pattern is the bare identifier `_`.
        if first.name == "_" {
            return Pattern::Wildcard { span: first.span };
        }
        let mut segments = vec![first];
        while self.peek().kind == TokenKind::ColonColon {
            self.advance();
            segments.push(self.expect_ident("path segment"));
        }
        let head_start = segments.first().expect("non-empty path").span.start;
        let head_end = segments.last().expect("non-empty path").span.end;
        match self.peek().kind {
            TokenKind::LParen => {
                self.advance();
                let elems = self.parse_pattern_list(TokenKind::RParen);
                let close = self.expect(TokenKind::RParen, "`)`");
                Pattern::TupleStruct {
                    path: segments,
                    elems,
                    span: Span::new(head_start, close.span.end),
                }
            }
            TokenKind::LBrace => {
                self.advance();
                let (fields, has_rest) = self.parse_struct_pattern_fields();
                let close = self.expect(TokenKind::RBrace, "`}`");
                Pattern::Struct {
                    path: segments,
                    fields,
                    has_rest,
                    span: Span::new(head_start, close.span.end),
                }
            }
            _ => {
                if segments.len() == 1 {
                    Pattern::Ident(segments.pop().expect("len == 1"))
                } else {
                    Pattern::Path {
                        segments,
                        span: Span::new(head_start, head_end),
                    }
                }
            }
        }
    }

    fn parse_pattern_list(&mut self, terminator: TokenKind) -> Vec<Pattern> {
        let mut elems: Vec<Pattern> = Vec::new();
        loop {
            let tok = self.peek();
            if tok.kind == terminator || tok.kind == TokenKind::Eof {
                break;
            }
            let before = self.pos;
            elems.push(self.parse_pattern());
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
            if self.pos == before {
                if self.peek().kind == TokenKind::Eof {
                    break;
                }
                self.advance();
            }
        }
        elems
    }

    fn parse_struct_pattern_fields(&mut self) -> (Vec<StructPatternField>, bool) {
        let mut fields: Vec<StructPatternField> = Vec::new();
        let mut has_rest = false;
        loop {
            let tok = self.peek();
            if matches!(tok.kind, TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            if tok.kind == TokenKind::DotDot {
                self.advance();
                has_rest = true;
                if self.peek().kind == TokenKind::Comma {
                    self.advance();
                }
                // Per Rust convention `..` must be the last element. Any
                // remaining field tokens before `}` are reported as
                // `UnexpectedToken` by the field-name parser on the next
                // iteration.
                continue;
            }
            let before = self.pos;
            let name = self.expect_ident("field name in struct pattern");
            let (pattern, end) = if self.peek().kind == TokenKind::Colon {
                self.advance();
                let p = self.parse_pattern();
                let end = p.span().end;
                (Some(p), end)
            } else {
                (None, name.span.end)
            };
            let start = name.span.start;
            fields.push(StructPatternField {
                name,
                pattern,
                span: Span::new(start, end),
            });
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
            if self.pos == before {
                if self.peek().kind == TokenKind::Eof {
                    break;
                }
                self.advance();
            }
        }
        (fields, has_rest)
    }

    /// Parse the `..` / `..=` and upper endpoint of a range pattern,
    /// given `lo` already parsed. `..=` is recognised by spatial
    /// adjacency (the `..` token's `span.end` matching the `=` token's
    /// `span.start`) rather than as a dedicated lexer token.
    fn parse_range_pattern_after_lo(&mut self, lo: Pattern) -> Pattern {
        let dotdot_tok = self.advance();
        let inclusive =
            self.peek().kind == TokenKind::Eq && self.peek().span.start == dotdot_tok.span.end;
        if inclusive {
            self.advance();
        }
        let hi = self.parse_primary_pattern();
        let span = Span::new(lo.span().start, hi.span().end);
        Pattern::Range {
            lo: Box::new(lo),
            hi: Box::new(hi),
            inclusive,
            span,
        }
    }

    /// `return [<value>]`. The optional value is parsed greedily; absence is
    /// inferred when the next non-trivia token cannot start an expression.
    fn parse_return_expr(&mut self) -> Expr {
        let kw = self.expect(TokenKind::Return, "`return`");
        let (value, end) = if token_starts_expression(self.peek().kind) {
            let e = self.parse_expression(MIN_PREC);
            let end = e.span().end;
            (Some(Box::new(e)), end)
        } else {
            (None, kw.span.end)
        };
        Expr::Return {
            value,
            span: Span::new(kw.span.start, end),
        }
    }

    /// `break [<value>]`. Same value-detection rule as [`Self::parse_return_expr`].
    fn parse_break_expr(&mut self) -> Expr {
        let kw = self.expect(TokenKind::Break, "`break`");
        let (value, end) = if token_starts_expression(self.peek().kind) {
            let e = self.parse_expression(MIN_PREC);
            let end = e.span().end;
            (Some(Box::new(e)), end)
        } else {
            (None, kw.span.end)
        };
        Expr::Break {
            value,
            span: Span::new(kw.span.start, end),
        }
    }

    /// `continue`. The keyword carries no payload in S2.2.
    fn parse_continue_expr(&mut self) -> Expr {
        let kw = self.expect(TokenKind::Continue, "`continue`");
        Expr::Continue { span: kw.span }
    }

    /// Dispatcher for item declarations. Caller must guarantee that
    /// [`token_starts_item`] holds for the current token.
    fn parse_item(&mut self) -> Item {
        match self.peek().kind {
            TokenKind::Fn => Item::Fn(self.parse_fn_item()),
            TokenKind::Const => Item::Const(self.parse_const_item()),
            TokenKind::Struct => Item::Struct(self.parse_struct_item()),
            TokenKind::Type => Item::TypeAlias(self.parse_type_alias_item()),
            TokenKind::Enum => Item::Enum(self.parse_enum_item()),
            TokenKind::Import => Item::Import(self.parse_import_item()),
            _ => unreachable!("parse_item called on non-item token"),
        }
    }

    /// `fn <name> ( <params,> ) [-> <ret_ty>] <block>`
    fn parse_fn_item(&mut self) -> FnItem {
        let kw = self.expect(TokenKind::Fn, "`fn`");
        let name = self.expect_ident("function name");
        self.expect(TokenKind::LParen, "`(`");
        let params = self.parse_param_list();
        self.expect(TokenKind::RParen, "`)`");
        let ret_ty = if self.peek().kind == TokenKind::Arrow {
            self.advance();
            Some(self.parse_type())
        } else {
            None
        };
        let body_expr = self.parse_block_expr();
        let end = body_expr.span().end;
        FnItem {
            name,
            params,
            ret_ty,
            body: Box::new(body_expr),
            span: Span::new(kw.span.start, end),
        }
    }

    /// `<name> : <ty>` separated by `,`; trailing `,` allowed; empty list OK.
    fn parse_param_list(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        loop {
            let tok = self.peek();
            if matches!(tok.kind, TokenKind::RParen | TokenKind::Eof) {
                break;
            }
            let start = tok.span.start;
            let name = self.expect_ident("parameter name");
            self.expect(TokenKind::Colon, "`:`");
            let ty = self.parse_type();
            let end = ty.span().end;
            params.push(Param {
                name,
                ty,
                span: Span::new(start, end),
            });
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }
        params
    }

    /// `const <name> : <ty> = <init> ;`
    fn parse_const_item(&mut self) -> ConstItem {
        let kw = self.expect(TokenKind::Const, "`const`");
        let name = self.expect_ident("constant name");
        self.expect(TokenKind::Colon, "`:`");
        let ty = self.parse_type();
        self.expect(TokenKind::Eq, "`=`");
        let init = self.parse_expression(MIN_PREC);
        let semi = self.expect(TokenKind::Semicolon, "`;`");
        ConstItem {
            name,
            ty,
            init: Box::new(init),
            span: Span::new(kw.span.start, semi.span.end),
        }
    }

    /// `struct <name> { <field,> }`
    fn parse_struct_item(&mut self) -> StructItem {
        let kw = self.expect(TokenKind::Struct, "`struct`");
        let name = self.expect_ident("struct name");
        self.expect(TokenKind::LBrace, "`{`");
        let fields = self.parse_struct_field_list();
        let close = self.expect(TokenKind::RBrace, "`}`");
        StructItem {
            name,
            fields,
            span: Span::new(kw.span.start, close.span.end),
        }
    }

    /// `<name> : <ty>` separated by `,`; trailing `,` allowed; empty list OK.
    /// Shared by `struct` and struct-like enum variants.
    fn parse_struct_field_list(&mut self) -> Vec<StructField> {
        let mut fields: Vec<StructField> = Vec::new();
        loop {
            let tok = self.peek();
            if matches!(tok.kind, TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let start = tok.span.start;
            let field_name = self.expect_ident("field name");
            self.expect(TokenKind::Colon, "`:`");
            let ty = self.parse_type();
            let end = ty.span().end;
            fields.push(StructField {
                name: field_name,
                ty,
                span: Span::new(start, end),
            });
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }
        fields
    }

    /// `type <name> = <ty> ;`
    fn parse_type_alias_item(&mut self) -> TypeAlias {
        let kw = self.expect(TokenKind::Type, "`type`");
        let name = self.expect_ident("type alias name");
        self.expect(TokenKind::Eq, "`=`");
        let ty = self.parse_type();
        let semi = self.expect(TokenKind::Semicolon, "`;`");
        TypeAlias {
            name,
            ty,
            span: Span::new(kw.span.start, semi.span.end),
        }
    }

    /// `enum <name> { <variant,> }`
    fn parse_enum_item(&mut self) -> EnumItem {
        let kw = self.expect(TokenKind::Enum, "`enum`");
        let name = self.expect_ident("enum name");
        self.expect(TokenKind::LBrace, "`{`");
        let mut variants: Vec<Variant> = Vec::new();
        loop {
            let tok = self.peek();
            if matches!(tok.kind, TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            variants.push(self.parse_variant());
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }
        let close = self.expect(TokenKind::RBrace, "`}`");
        EnumItem {
            name,
            variants,
            span: Span::new(kw.span.start, close.span.end),
        }
    }

    /// `<name>` | `<name>(<type,>)` | `<name> { <field,> }`
    fn parse_variant(&mut self) -> Variant {
        let start = self.peek().span.start;
        let name = self.expect_ident("variant name");
        let mut end = name.span.end;
        let body = match self.peek().kind {
            TokenKind::LParen => {
                self.advance();
                let mut types: Vec<Type> = Vec::new();
                loop {
                    let t = self.peek();
                    if matches!(t.kind, TokenKind::RParen | TokenKind::Eof) {
                        break;
                    }
                    types.push(self.parse_type());
                    if self.peek().kind == TokenKind::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let close = self.expect(TokenKind::RParen, "`)`");
                end = close.span.end;
                VariantBody::Tuple(types)
            }
            TokenKind::LBrace => {
                self.advance();
                let fields = self.parse_struct_field_list();
                let close = self.expect(TokenKind::RBrace, "`}`");
                end = close.span.end;
                VariantBody::Struct(fields)
            }
            _ => VariantBody::Unit,
        };
        Variant {
            name,
            body,
            span: Span::new(start, end),
        }
    }

    /// `import <path::...> [as <alias>] ;`
    fn parse_import_item(&mut self) -> ImportItem {
        let kw = self.expect(TokenKind::Import, "`import`");
        let mut path = vec![self.expect_ident("import path segment")];
        while self.peek().kind == TokenKind::ColonColon {
            self.advance();
            path.push(self.expect_ident("import path segment"));
        }
        let alias = if self.peek().kind == TokenKind::As {
            self.advance();
            Some(self.expect_ident("import alias"))
        } else {
            None
        };
        let semi = self.expect(TokenKind::Semicolon, "`;`");
        ImportItem {
            path,
            alias,
            span: Span::new(kw.span.start, semi.span.end),
        }
    }

    /// Dedicated type-grammar entry (S2.3b).
    ///
    /// S2.3b only delivers path types (`i32`, `mod::Foo`). Tuple, function,
    /// array, reference, generic and trait-object forms are added in later
    /// S2.3 sub-slices. On an unexpected token the parser emits a typed
    /// diagnostic and returns [`Type::Error`] without advancing (recovery
    /// tokens like `;`, `}`, `)` , `,`) or advancing exactly one token so
    /// the surrounding loop can still make progress.
    fn parse_type(&mut self) -> Type {
        let tok = self.peek();
        if tok.kind == TokenKind::Ident {
            let first = self.expect_ident("type name");
            let mut segments = vec![first];
            while self.peek().kind == TokenKind::ColonColon {
                self.advance();
                segments.push(self.expect_ident("type path segment"));
            }
            let start = segments.first().expect("non-empty path").span.start;
            let end = segments.last().expect("non-empty path").span.end;
            return Type::Path {
                segments,
                span: Span::new(start, end),
            };
        }
        let kind = if tok.kind == TokenKind::Eof {
            ParseErrorKind::UnexpectedEof { expected: "type" }
        } else {
            ParseErrorKind::UnexpectedToken {
                found: tok.kind,
                expected: "type",
            }
        };
        self.error(kind, tok.span);
        // Recovery: do not consume tokens that look like the end of the
        // enclosing context. Otherwise advance one token so the outer loop
        // makes progress.
        let is_recovery = matches!(
            tok.kind,
            TokenKind::Semicolon
                | TokenKind::Comma
                | TokenKind::RParen
                | TokenKind::RBrace
                | TokenKind::RBracket
                | TokenKind::Eq
                | TokenKind::Eof
        );
        if !is_recovery {
            self.advance();
        }
        Type::Error { span: tok.span }
    }

    fn parse_ident_or_path(&mut self) -> Expr {
        let first = self.expect_ident("identifier");
        let mut segments = vec![first];
        while self.peek().kind == TokenKind::ColonColon {
            self.advance();
            segments.push(self.expect_ident("path segment"));
        }
        // Struct-literal expression `Path { field, ... }` (S6.3c). Only
        // recognised outside a no-struct-literal head context, so an
        // `if cond { .. }` head treats the `{` as a block instead.
        if !self.no_struct_literal && self.peek().kind == TokenKind::LBrace {
            return self.parse_struct_literal_body(segments);
        }
        if segments.len() == 1 {
            return Expr::Ident(segments.pop().expect("one segment"));
        }
        let start = segments.first().expect("non-empty path").span.start;
        let end = segments.last().expect("non-empty path").span.end;
        Expr::Path {
            segments,
            span: Span::new(start, end),
        }
    }

    /// Parses the `{ name [: value], ... }` body of a struct literal,
    /// given the already-parsed leading `path` (S6.3c). Field shorthand
    /// `Point { x }` records `value = Expr::Ident(x)`. Field initialiser
    /// values are parsed in a delimited (struct-literals-allowed) context.
    fn parse_struct_literal_body(&mut self, path: Vec<Ident>) -> Expr {
        let start = path.first().expect("non-empty struct path").span.start;
        self.expect(TokenKind::LBrace, "`{`");
        let mut fields: Vec<StructLitField> = Vec::new();
        loop {
            let tok = self.peek();
            if matches!(tok.kind, TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let name = self.expect_ident("field name in struct literal");
            let name_span = name.span;
            let value = if self.peek().kind == TokenKind::Colon {
                self.advance();
                self.parse_delimited_expr()
            } else {
                // Shorthand `Point { x }` ≡ `Point { x: x }`.
                Expr::Ident(name.clone())
            };
            let end = value.span().end;
            fields.push(StructLitField {
                name,
                value,
                span: Span::new(name_span.start, end),
            });
            if self.peek().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }
        let close = self.expect(TokenKind::RBrace, "`}`");
        Expr::StructLit {
            path,
            fields,
            span: Span::new(start, close.span.end),
        }
    }
}

/// True when `kind` starts an item declaration (S2.3a / S2.3b).
///
/// Used by [`Parser::parse_top_level_stmts`] and
/// [`Parser::parse_block_expr`] to dispatch on item keywords before
/// falling back to statement parsing.
fn token_starts_item(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Fn
            | TokenKind::Const
            | TokenKind::Struct
            | TokenKind::Type
            | TokenKind::Enum
            | TokenKind::Import
    )
}

/// True when `kind` can start a primary expression.
///
/// Used by [`Parser::parse_return_expr`] and [`Parser::parse_break_expr`]
/// to decide whether the value-carrying form should be parsed.
fn token_starts_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Int
            | TokenKind::Float
            | TokenKind::Str
            | TokenKind::True
            | TokenKind::False
            | TokenKind::NoneKw
            | TokenKind::Ident
            | TokenKind::LParen
            | TokenKind::LBrace
            | TokenKind::Minus
            | TokenKind::Bang
            | TokenKind::Tilde
            | TokenKind::If
            | TokenKind::While
            | TokenKind::Loop
            | TokenKind::Match
            | TokenKind::Return
            | TokenKind::Break
            | TokenKind::Continue
    )
}

fn token_to_binop(t: TokenKind) -> Option<BinOp> {
    Some(match t {
        TokenKind::PipePipe => BinOp::Or,
        TokenKind::AmpAmp => BinOp::And,
        TokenKind::Pipe => BinOp::BitOr,
        TokenKind::Caret => BinOp::BitXor,
        TokenKind::Amp => BinOp::BitAnd,
        TokenKind::EqEq => BinOp::Eq,
        TokenKind::BangEq => BinOp::Ne,
        TokenKind::Lt => BinOp::Lt,
        TokenKind::LtEq => BinOp::Le,
        TokenKind::Gt => BinOp::Gt,
        TokenKind::GtEq => BinOp::Ge,
        TokenKind::LtLt => BinOp::Shl,
        TokenKind::GtGt => BinOp::Shr,
        TokenKind::Plus => BinOp::Add,
        TokenKind::Minus => BinOp::Sub,
        TokenKind::Star => BinOp::Mul,
        TokenKind::Slash => BinOp::Div,
        TokenKind::Percent => BinOp::Mod,
        _ => return None,
    })
}

/// Maps an assignment-operator token to its desugaring.
///
/// Returns `Some(None)` for a plain `=`, `Some(Some(op))` for a compound
/// assignment `<op>=` (which the parser desugars into
/// `target = target <op> value`), and `None` when `t` is not an assignment
/// operator. Only the arithmetic compounds the lexer emits are recognised
/// (`+=`, `-=`, `*=`, `/=`, `%=`).
fn token_to_assign_op(t: TokenKind) -> Option<Option<BinOp>> {
    Some(match t {
        TokenKind::Eq => None,
        TokenKind::PlusEq => Some(BinOp::Add),
        TokenKind::MinusEq => Some(BinOp::Sub),
        TokenKind::StarEq => Some(BinOp::Mul),
        TokenKind::SlashEq => Some(BinOp::Div),
        TokenKind::PercentEq => Some(BinOp::Mod),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_expr;
    use capy_ast::{dump_expr, BinOp, Expr};

    #[test]
    fn empty_input_is_recoverable_error() {
        let r = parse_expr("");
        assert!(matches!(r.expr, Expr::Error { .. }));
        assert_eq!(r.diagnostics.len(), 1);
    }

    #[test]
    fn int_literal() {
        let r = parse_expr("42");
        assert!(r.diagnostics.is_empty());
        assert!(matches!(r.expr, Expr::Int { .. }));
    }

    #[test]
    fn precedence_mul_over_add() {
        let r = parse_expr("1 + 2 * 3");
        assert!(r.diagnostics.is_empty());
        if let Expr::Binary { op, rhs, .. } = &r.expr {
            assert_eq!(*op, BinOp::Add);
            assert!(matches!(rhs.as_ref(), Expr::Binary { op: BinOp::Mul, .. }));
        } else {
            panic!("expected Binary, got {:?}", r.expr);
        }
    }

    #[test]
    fn left_associative_subtraction() {
        let r = parse_expr("1 - 2 - 3");
        assert!(r.diagnostics.is_empty());
        // ((1 - 2) - 3)
        if let Expr::Binary { lhs, .. } = &r.expr {
            assert!(matches!(lhs.as_ref(), Expr::Binary { op: BinOp::Sub, .. }));
        } else {
            panic!("expected Binary, got {:?}", r.expr);
        }
    }

    #[test]
    fn path_two_segments() {
        let r = parse_expr("a::b");
        assert!(r.diagnostics.is_empty());
        assert!(matches!(r.expr, Expr::Path { .. }));
    }

    #[test]
    fn single_ident_is_not_a_path() {
        let r = parse_expr("foo");
        assert!(matches!(r.expr, Expr::Ident(_)));
    }

    #[test]
    fn call_with_args() {
        let r = parse_expr("f(1, 2, 3)");
        assert!(r.diagnostics.is_empty());
        if let Expr::Call { args, .. } = &r.expr {
            assert_eq!(args.len(), 3);
        } else {
            panic!("expected Call");
        }
    }

    #[test]
    fn call_with_trailing_comma() {
        let r = parse_expr("f(1,)");
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn index_then_field() {
        let r = parse_expr("a[0].name");
        assert!(r.diagnostics.is_empty());
        assert!(matches!(r.expr, Expr::Field { .. }));
    }

    #[test]
    fn missing_rparen_is_recoverable() {
        let r = parse_expr("(1 + 2");
        assert!(!r.diagnostics.is_empty());
        // Parser still produced a Paren node spanning the available input.
        assert!(matches!(r.expr, Expr::Paren { .. }));
    }

    #[test]
    fn lex_error_surfaces_as_parse_diagnostic() {
        let r = parse_expr("\"oops");
        assert!(r
            .diagnostics
            .iter()
            .any(|d| matches!(d.kind, crate::diagnostic::ParseErrorKind::Lex(_))));
    }

    #[test]
    fn trailing_input_is_reported_once() {
        let r = parse_expr("1 2");
        assert_eq!(r.diagnostics.len(), 1);
    }

    #[test]
    fn unary_neg_binds_tighter_than_add() {
        let r = parse_expr("-1 + 2");
        assert!(r.diagnostics.is_empty());
        if let Expr::Binary { lhs, op, .. } = &r.expr {
            assert_eq!(*op, BinOp::Add);
            assert!(matches!(lhs.as_ref(), Expr::Unary { .. }));
        } else {
            panic!("expected Binary at root");
        }
    }

    #[test]
    fn dump_format_is_stable() {
        let r = parse_expr("1+2");
        assert_eq!(
            dump_expr(&r.expr),
            "[0..3] Binary Add\n  [0..1] Int \"1\"\n  [2..3] Int \"2\"\n"
        );
    }

    #[test]
    fn empty_block_as_expression() {
        let r = parse_expr("{}");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Block { stmts, tail, .. } = &r.expr {
            assert!(stmts.is_empty());
            assert!(tail.is_none());
        } else {
            panic!("expected Block, got {:?}", r.expr);
        }
    }

    #[test]
    fn block_with_tail() {
        let r = parse_expr("{ 1 + 2 }");
        assert!(r.diagnostics.is_empty());
        if let Expr::Block { stmts, tail, .. } = &r.expr {
            assert!(stmts.is_empty());
            assert!(matches!(tail.as_deref(), Some(Expr::Binary { .. })));
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn parse_source_let_stmt() {
        let r = super::parse_source("let x = 42;\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.source.stmts.len(), 1);
        assert!(matches!(r.source.stmts[0], capy_ast::Stmt::Let { .. }));
    }

    #[test]
    fn parse_source_missing_semi_is_recoverable() {
        let r = super::parse_source("let x = 1\n");
        // Exactly one diagnostic for the missing `;`, no panic.
        assert_eq!(r.diagnostics.len(), 1);
        assert!(matches!(
            r.diagnostics[0].kind,
            crate::diagnostic::ParseErrorKind::UnexpectedEof { .. }
        ));
    }

    #[test]
    fn parse_source_two_statements() {
        let r = super::parse_source("let x = 1;\nlet y = x;\n");
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.source.stmts.len(), 2);
    }

    #[test]
    fn if_else_parses() {
        let r = parse_expr("if x { 1 } else { 2 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert!(matches!(r.expr, Expr::If { .. }));
    }

    #[test]
    fn if_without_else() {
        let r = parse_expr("if x { 1 }");
        assert!(r.diagnostics.is_empty());
        if let Expr::If {
            else_branch: None, ..
        } = r.expr
        {
            // ok
        } else {
            panic!("expected If without else, got {:?}", r.expr);
        }
    }

    #[test]
    fn else_if_chain_is_recursive() {
        let r = parse_expr("if a { 1 } else if b { 2 } else { 3 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::If { else_branch, .. } = &r.expr {
            // The else branch should itself be an If (else-if chain).
            assert!(matches!(else_branch.as_deref(), Some(Expr::If { .. })));
        } else {
            panic!("expected outer If");
        }
    }

    #[test]
    fn while_loop_parses() {
        let r = parse_expr("while x { 1 }");
        assert!(r.diagnostics.is_empty());
        assert!(matches!(r.expr, Expr::While { .. }));
    }

    #[test]
    fn loop_parses() {
        let r = parse_expr("loop { 1 }");
        assert!(r.diagnostics.is_empty());
        assert!(matches!(r.expr, Expr::Loop { .. }));
    }

    #[test]
    fn return_with_value() {
        let r = parse_expr("return 42");
        assert!(r.diagnostics.is_empty());
        if let Expr::Return { value: Some(_), .. } = r.expr {
            // ok
        } else {
            panic!("expected Return with value");
        }
    }

    #[test]
    fn return_without_value() {
        // Wrapped in a block so the bare `return` is followed by `}` (which
        // does not start an expression).
        let r = parse_expr("{ return }");
        assert!(r.diagnostics.is_empty());
        if let Expr::Block { tail: Some(t), .. } = &r.expr {
            assert!(matches!(t.as_ref(), Expr::Return { value: None, .. }));
        } else {
            panic!("expected Block with Return tail");
        }
    }

    #[test]
    fn continue_is_block_like_false() {
        // `continue` must require `;` as a statement.
        let r = super::parse_source("continue\n");
        assert_eq!(r.diagnostics.len(), 1);
    }

    #[test]
    fn if_stmt_no_semi_in_source() {
        // `if` is block-like; no `;` required at top level.
        let r = super::parse_source("if x { 1 }\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.source.stmts.len(), 1);
    }

    #[test]
    fn fn_item_simple() {
        let r = super::parse_source("fn add(x: i32, y: i32) -> i32 { x + y }\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.source.stmts.len(), 1);
        if let capy_ast::Stmt::Item(capy_ast::Item::Fn(f)) = &r.source.stmts[0] {
            assert_eq!(f.name.name, "add");
            assert_eq!(f.params.len(), 2);
            assert!(f.ret_ty.is_some());
        } else {
            panic!("expected Stmt::Item(Fn)");
        }
    }

    #[test]
    fn fn_item_no_params_no_ret() {
        let r = super::parse_source("fn nop() {}\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::Fn(f)) = &r.source.stmts[0] {
            assert!(f.params.is_empty());
            assert!(f.ret_ty.is_none());
        } else {
            panic!("expected Stmt::Item(Fn)");
        }
    }

    #[test]
    fn const_item_simple() {
        let r = super::parse_source("const PI: f64 = 314;\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::Const(c)) = &r.source.stmts[0] {
            assert_eq!(c.name.name, "PI");
        } else {
            panic!("expected Stmt::Item(Const)");
        }
    }

    #[test]
    fn struct_item_simple() {
        let r = super::parse_source("struct Point { x: i32, y: i32 }\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::Struct(s)) = &r.source.stmts[0] {
            assert_eq!(s.name.name, "Point");
            assert_eq!(s.fields.len(), 2);
        } else {
            panic!("expected Stmt::Item(Struct)");
        }
    }

    #[test]
    fn struct_item_empty() {
        let r = super::parse_source("struct Unit {}\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::Struct(s)) = &r.source.stmts[0] {
            assert!(s.fields.is_empty());
        } else {
            panic!("expected Stmt::Item(Struct)");
        }
    }

    #[test]
    fn items_and_stmts_interleave() {
        let r = super::parse_source("const X: i32 = 1;\nlet y = X;\nfn f() {}\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.source.stmts.len(), 3);
    }

    #[test]
    fn type_alias_item() {
        let r = super::parse_source("type Int = i32;\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::TypeAlias(t)) = &r.source.stmts[0] {
            assert_eq!(t.name.name, "Int");
            assert!(matches!(t.ty, capy_ast::Type::Path { .. }));
        } else {
            panic!("expected Stmt::Item(TypeAlias)");
        }
    }

    #[test]
    fn enum_item_unit_variants() {
        let r = super::parse_source("enum Color { Red, Green, Blue }\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::Enum(e)) = &r.source.stmts[0] {
            assert_eq!(e.name.name, "Color");
            assert_eq!(e.variants.len(), 3);
            assert!(e
                .variants
                .iter()
                .all(|v| matches!(v.body, capy_ast::VariantBody::Unit)));
        } else {
            panic!("expected Stmt::Item(Enum)");
        }
    }

    #[test]
    fn enum_item_payload_variants() {
        // Tuple- and struct-like variants.
        let r = super::parse_source("enum Shape { Circle(f64), Square { side: f64 }, Empty }\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::Enum(e)) = &r.source.stmts[0] {
            assert_eq!(e.variants.len(), 3);
            assert!(matches!(
                e.variants[0].body,
                capy_ast::VariantBody::Tuple(_)
            ));
            assert!(matches!(
                e.variants[1].body,
                capy_ast::VariantBody::Struct(_)
            ));
            assert!(matches!(e.variants[2].body, capy_ast::VariantBody::Unit));
        } else {
            panic!("expected Stmt::Item(Enum)");
        }
    }

    #[test]
    fn import_item_without_alias() {
        let r = super::parse_source("import std::time::Instant;\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::Import(i)) = &r.source.stmts[0] {
            assert_eq!(i.path.len(), 3);
            assert!(i.alias.is_none());
        } else {
            panic!("expected Stmt::Item(Import)");
        }
    }

    #[test]
    fn import_item_with_alias() {
        let r = super::parse_source("import std::time as t;\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Item(capy_ast::Item::Import(i)) = &r.source.stmts[0] {
            assert_eq!(i.path.len(), 2);
            assert_eq!(i.alias.as_ref().map(|a| a.name.as_str()), Some("t"));
        } else {
            panic!("expected Stmt::Item(Import)");
        }
    }

    #[test]
    fn type_grammar_rejects_non_ident() {
        // `let x: 42 = 0;` — `42` is not a type. parse_type must emit a
        // typed diagnostic and the rest of the let still parses.
        let r = super::parse_source("let x: 42 = 0;\n");
        assert!(!r.diagnostics.is_empty());
        assert!(r.diagnostics.iter().any(|d| matches!(
            d.kind,
            crate::diagnostic::ParseErrorKind::UnexpectedToken {
                expected: "type",
                ..
            }
        )));
    }

    // --- S2.2b: match + patterns --------------------------------------

    #[test]
    fn match_with_int_literal_arms() {
        let r = parse_expr("match x { 1 => 10, 2 => 20, _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        let arms = match &r.expr {
            Expr::Match { arms, .. } => arms,
            other => panic!("expected Match, got {other:?}"),
        };
        assert_eq!(arms.len(), 3);
        assert!(matches!(arms[0].pattern, capy_ast::Pattern::Literal { .. }));
        assert!(matches!(
            arms[2].pattern,
            capy_ast::Pattern::Wildcard { .. }
        ));
    }

    #[test]
    fn match_with_guard() {
        let r = parse_expr("match x { n if n > 0 => 1, _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            assert!(arms[0].guard.is_some());
            assert!(arms[1].guard.is_none());
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_with_or_pattern() {
        let r = parse_expr("match x { 1 | 2 | 3 => 1, _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            if let capy_ast::Pattern::Or { alts, .. } = &arms[0].pattern {
                assert_eq!(alts.len(), 3);
            } else {
                panic!("expected Or pattern");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_with_tuple_struct_pattern() {
        let r = parse_expr("match x { Some(y) => y, None => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            assert!(matches!(
                arms[0].pattern,
                capy_ast::Pattern::TupleStruct { .. }
            ));
            // `None` is the `NoneLit` keyword token, so the second arm's
            // pattern is a literal pattern rather than a tuple-struct.
            assert!(matches!(arms[1].pattern, capy_ast::Pattern::Literal { .. }));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_with_path_pattern() {
        let r = parse_expr("match c { Color::Red => 1, Color::Green => 2, _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            assert!(matches!(arms[0].pattern, capy_ast::Pattern::Path { .. }));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_with_struct_pattern_and_rest() {
        let r = parse_expr("match p { Point { x, .. } => x, _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            if let capy_ast::Pattern::Struct {
                fields, has_rest, ..
            } = &arms[0].pattern
            {
                assert_eq!(fields.len(), 1);
                assert!(*has_rest);
                assert!(fields[0].pattern.is_none(), "shorthand field");
            } else {
                panic!("expected Struct pattern");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_with_exclusive_range_pattern() {
        let r = parse_expr("match n { 0..10 => 1, _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            if let capy_ast::Pattern::Range { inclusive, .. } = &arms[0].pattern {
                assert!(!inclusive);
            } else {
                panic!("expected Range pattern");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_with_inclusive_range_pattern() {
        let r = parse_expr("match n { 0..=10 => 1, _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            if let capy_ast::Pattern::Range { inclusive, .. } = &arms[0].pattern {
                assert!(*inclusive);
            } else {
                panic!("expected Range pattern");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_with_negative_literal_pattern() {
        let r = parse_expr("match n { -1 => 1, _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            if let capy_ast::Pattern::Literal { value, .. } = &arms[0].pattern {
                assert!(matches!(
                    value,
                    Expr::Unary {
                        op: capy_ast::UnOp::Neg,
                        ..
                    }
                ));
            } else {
                panic!("expected Literal pattern");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_block_arm_body_skips_comma() {
        let r = parse_expr("match x { 1 => { 10 } _ => { 0 } }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { arms, .. } = &r.expr {
            assert_eq!(arms.len(), 2);
            assert!(arms[0].body.is_block_like());
            assert!(arms[1].body.is_block_like());
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn match_missing_fat_arrow_is_diagnostic() {
        let r = parse_expr("match x { 1 10, _ => 0 }");
        assert!(!r.diagnostics.is_empty());
        assert!(r.diagnostics.iter().any(|d| matches!(
            d.kind,
            crate::diagnostic::ParseErrorKind::UnexpectedToken {
                expected: "`=>`",
                ..
            }
        )));
    }

    #[test]
    fn match_is_block_like_in_stmt_position() {
        // A `match` used at statement position can omit the trailing `;`
        // because it is block-like.
        let r = super::parse_source("match x { _ => 0 }\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert_eq!(r.source.stmts.len(), 1);
        if let capy_ast::Stmt::Expr { has_semi, expr, .. } = &r.source.stmts[0] {
            assert!(!*has_semi);
            assert!(matches!(expr, Expr::Match { .. }));
        } else {
            panic!("expected ExprStmt");
        }
    }

    #[test]
    fn match_as_expression_in_let() {
        let r = super::parse_source("let s = match x { 1 => 10, _ => 0 };\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let capy_ast::Stmt::Let { init, .. } = &r.source.stmts[0] {
            assert!(matches!(init.as_ref().unwrap(), Expr::Match { .. }));
        } else {
            panic!("expected Let");
        }
    }
}

#[cfg(test)]
mod assign_tests {
    use super::{parse_expr, parse_source};
    use capy_ast::{BinOp, Expr};

    #[test]
    fn simple_assignment_parses() {
        let r = parse_expr("x = 1");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Assign { target, value, .. } = &r.expr {
            assert!(matches!(target.as_ref(), Expr::Ident(_)));
            assert!(matches!(value.as_ref(), Expr::Int { .. }));
        } else {
            panic!("expected Assign, got {:?}", r.expr);
        }
    }

    #[test]
    fn assignment_binds_looser_than_arithmetic() {
        // `x = 1 + 2` parses as `x = (1 + 2)`.
        let r = parse_expr("x = 1 + 2");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Assign { value, .. } = &r.expr {
            assert!(matches!(
                value.as_ref(),
                Expr::Binary { op: BinOp::Add, .. }
            ));
        } else {
            panic!("expected Assign, got {:?}", r.expr);
        }
    }

    #[test]
    fn assignment_is_right_associative() {
        // `a = b = c` parses as `a = (b = c)`.
        let r = parse_expr("a = b = c");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Assign { value, .. } = &r.expr {
            assert!(
                matches!(value.as_ref(), Expr::Assign { .. }),
                "rhs should nest"
            );
        } else {
            panic!("expected Assign, got {:?}", r.expr);
        }
    }

    #[test]
    fn compound_assignment_desugars() {
        // `x += 1` desugars to `x = (x + 1)`: the inner binary's left
        // operand is the assignment target.
        let r = parse_expr("x += 1");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Assign { value, .. } = &r.expr {
            if let Expr::Binary { op, lhs, .. } = value.as_ref() {
                assert_eq!(*op, BinOp::Add);
                assert!(matches!(lhs.as_ref(), Expr::Ident(_)));
            } else {
                panic!("expected desugared Binary, got {:?}", value);
            }
        } else {
            panic!("expected Assign, got {:?}", r.expr);
        }
    }

    #[test]
    fn assignment_statement_parses_cleanly() {
        let r = parse_source("fn main() { let x = 0; x = 1; }\n");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn for_loop_parses_exclusive_and_inclusive() {
        let r = parse_expr("for i in 0..3 { i }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::For { inclusive, .. } = &r.expr {
            assert!(!*inclusive);
        } else {
            panic!("expected For, got {:?}", r.expr);
        }
        let r2 = parse_expr("for i in 0..=3 { i }");
        assert!(r2.diagnostics.is_empty(), "{:?}", r2.diagnostics);
        if let Expr::For { inclusive, .. } = &r2.expr {
            assert!(*inclusive);
        } else {
            panic!("expected For, got {:?}", r2.expr);
        }
    }
}

#[cfg(test)]
mod struct_lit_tests {
    use super::{parse_expr, parse_source};
    use capy_ast::Expr;

    #[test]
    fn struct_literal_parses_in_value_position() {
        let r = parse_expr("Point { x: 1, y: 2 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::StructLit { path, fields, .. } = &r.expr {
            assert_eq!(path.len(), 1);
            assert_eq!(path[0].name, "Point");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name.name, "x");
            assert_eq!(fields[1].name.name, "y");
        } else {
            panic!("expected StructLit, got {:?}", r.expr);
        }
    }

    #[test]
    fn struct_literal_shorthand_records_ident_value() {
        let r = parse_expr("P { x }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::StructLit { fields, .. } = &r.expr {
            assert_eq!(fields.len(), 1);
            assert!(matches!(&fields[0].value, Expr::Ident(id) if id.name == "x"));
        } else {
            panic!("expected StructLit, got {:?}", r.expr);
        }
    }

    #[test]
    fn qualified_struct_literal_keeps_path() {
        let r = parse_expr("m::Point { x: 1 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::StructLit { path, .. } = &r.expr {
            assert_eq!(path.len(), 2);
            assert_eq!(path[1].name, "Point");
        } else {
            panic!("expected StructLit, got {:?}", r.expr);
        }
    }

    #[test]
    fn if_head_does_not_parse_struct_literal() {
        // `cond` must be the condition (an identifier); the `{` opens the
        // then-block, not a struct literal. Regression guard for the
        // `no_struct_literal` context (S6.3c).
        let r = parse_expr("if cond { 1 } else { 2 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::If { cond, .. } = &r.expr {
            assert!(matches!(cond.as_ref(), Expr::Ident(id) if id.name == "cond"));
        } else {
            panic!("expected If, got {:?}", r.expr);
        }
    }

    #[test]
    fn match_scrutinee_is_not_a_struct_literal() {
        let r = parse_expr("match v { _ => 0 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        if let Expr::Match { scrutinee, .. } = &r.expr {
            assert!(matches!(scrutinee.as_ref(), Expr::Ident(id) if id.name == "v"));
        } else {
            panic!("expected Match, got {:?}", r.expr);
        }
    }

    #[test]
    fn while_head_does_not_parse_struct_literal() {
        let r = parse_expr("while running { 1 }");
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        assert!(matches!(&r.expr, Expr::While { .. }));
    }

    #[test]
    fn struct_literal_is_allowed_inside_a_block_in_a_head() {
        // The `{` after `cond` opens the block; inside it, a struct literal
        // is allowed again (the suppression is reset by `parse_block_expr`).
        let r = parse_source(
            "struct P { x: Int }\nfn main() { if cond { let p = P { x: 1 }; p } else { 0 } }\n",
        );
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn struct_literal_is_allowed_inside_call_args_in_a_head() {
        // Inside `f(...)` the suppression is reset, so `P { x: 1 }` parses
        // as a struct literal even though the call sits in an `if` head.
        let r = parse_source(
            "struct P { x: Int }\nfn f(p: Int) { p }\nfn main() { if f(P { x: 1 }) { 1 } else { 0 } }\n",
        );
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }
}
