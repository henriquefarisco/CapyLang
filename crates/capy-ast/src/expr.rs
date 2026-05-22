//! Expression-level AST nodes.
//!
//! S2.0 keeps literal values **opaque text** (the exact byte slice from the
//! source). Numeric and string evaluation is deferred to the bytecode emitter
//! (S5) so the parser stays a pure structural pass and so error recovery can
//! preserve malformed literals as text without forcing a numeric parse.

#![forbid(unsafe_code)]

use capy_lexer::Span;

/// A single identifier occurrence (also used for path segments and field
/// names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A CapyLang expression.
///
/// Every variant carries its own [`Span`]. Spans of composite nodes cover
/// from the first byte of the leftmost subtree to the last byte of the
/// rightmost subtree (or the closing delimiter, when present).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// Integer literal, raw text (e.g. `"42"`, `"0xff"`, `"0b1010"`).
    Int { text: String, span: Span },
    /// Float literal, raw text (e.g. `"3.14"`, `"1e9"`).
    Float { text: String, span: Span },
    /// String literal, raw text including the surrounding quotes.
    Str { text: String, span: Span },
    /// Boolean literal.
    Bool { value: bool, span: Span },
    /// `None` keyword used as a literal.
    NoneLit { span: Span },
    /// Identifier reference.
    Ident(Ident),
    /// Path expression (`a::b::c`). Always at least two segments; a single
    /// identifier becomes [`Expr::Ident`] instead.
    Path { segments: Vec<Ident>, span: Span },
    /// `( inner )`. The `span` covers the parentheses.
    Paren { inner: Box<Expr>, span: Span },
    /// `callee(args...)`.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    /// `target[index]`.
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// `target.name`.
    Field {
        target: Box<Expr>,
        name: Ident,
        span: Span,
    },
    /// Unary prefix expression.
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
    /// Binary infix expression.
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// Block expression `{ stmts; tail? }`. Rust-style: the optional `tail`
    /// is an expression without a trailing `;` and becomes the value of the
    /// block. When the block ends with a `;` (or is empty / closes
    /// immediately after a statement) the block has no tail and its value
    /// is the unit type.
    Block {
        stmts: Vec<Stmt>,
        tail: Option<Box<Expr>>,
        span: Span,
    },
    /// `if <cond> <then-block> [else (if-expr | block)]`.
    ///
    /// `then_branch` is always an [`Expr::Block`]; `else_branch`, when
    /// present, is either another [`Expr::If`] (`else if ...`) or an
    /// [`Expr::Block`] (`else { ... }`).
    If {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
        span: Span,
    },
    /// `while <cond> <body-block>`.
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
        span: Span,
    },
    /// `loop <body-block>`.
    Loop { body: Box<Expr>, span: Span },
    /// `return [<value>]`. The trailing `;` (when used as a statement) is
    /// captured by [`Stmt::Expr`].
    Return {
        value: Option<Box<Expr>>,
        span: Span,
    },
    /// `break [<value>]`.
    Break {
        value: Option<Box<Expr>>,
        span: Span,
    },
    /// `continue`.
    Continue { span: Span },
    /// `match <scrutinee> { <arm,>* }` (S2.2b).
    ///
    /// `arms` may be empty (an exhaustiveness check runs at lower
    /// layers); each arm pairs a [`Pattern`] with an optional `if`
    /// guard and a body expression. `span` covers from the `match`
    /// keyword to the closing `}`.
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// Placeholder produced by parser error recovery. The `span` points at
    /// the offending input. Always paired with at least one parse
    /// diagnostic.
    Error { span: Span },
}

/// A single arm of a [`Expr::Match`]: `<pattern> [if <guard>] => <body>`.
///
/// `body` is an arbitrary expression; the trailing `,` between arms is
/// optional when `body` is block-like, matching Rust convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

/// A CapyLang pattern (S2.2b).
///
/// Patterns appear on the left of `=>` in [`MatchArm`]. The grammar is
/// kept small and recovery-friendly: malformed input becomes
/// [`Pattern::Error`] and is paired with at least one parse diagnostic.
///
/// The shape mirrors Rust's pattern grammar without sub-patterns inside
/// tuple-struct / struct payloads beyond identifier and wildcard
/// references; richer nesting (literal sub-patterns, nested or-patterns,
/// reference patterns) lands in a later S2.x sub-slice once the emitter
/// is ready to lower matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// `_` — matches anything; does not bind.
    Wildcard { span: Span },
    /// `..` — rest pattern, only legal inside tuple-struct or struct
    /// patterns. Reported as a typed diagnostic when found elsewhere.
    Rest { span: Span },
    /// Literal pattern: `42`, `3.14`, `"hi"`, `true`, `None`. The inner
    /// [`Expr`] is restricted to the literal variants
    /// (`Int` / `Float` / `Str` / `Bool` / `NoneLit`) plus a leading
    /// `-` over a numeric literal; other shapes become
    /// [`Pattern::Error`].
    Literal { value: Expr, span: Span },
    /// Single-identifier pattern, either a binding (`x`) or a
    /// zero-arg constructor (`None`-like). Disambiguation happens at
    /// resolution time; the AST captures the source shape verbatim.
    Ident(Ident),
    /// Multi-segment path pattern, e.g. `Color::Red`. Always at least
    /// two segments; a single segment becomes [`Pattern::Ident`].
    Path { segments: Vec<Ident>, span: Span },
    /// Tuple-struct / tuple-variant pattern: `Some(x)`, `Pair(a, b)`,
    /// `Wrapper(_)`. Sub-patterns are restricted to the same
    /// [`Pattern`] grammar; nested tuple-struct payloads are allowed.
    TupleStruct {
        path: Vec<Ident>,
        elems: Vec<Pattern>,
        span: Span,
    },
    /// Struct / struct-variant pattern: `Point { x, y }`,
    /// `Point { x: 1, .. }`. Field shorthand `x` is recorded with the
    /// same identifier on both sides of the colon.
    Struct {
        path: Vec<Ident>,
        fields: Vec<StructPatternField>,
        has_rest: bool,
        span: Span,
    },
    /// Or-pattern: `1 | 2 | 3`. Always at least two alternatives.
    Or { alts: Vec<Pattern>, span: Span },
    /// Range pattern: `1..10` (exclusive) or `1..=10` (inclusive).
    /// Endpoints are restricted to literal expressions; richer ranges
    /// land alongside the const-eval slice.
    Range {
        lo: Box<Pattern>,
        hi: Box<Pattern>,
        inclusive: bool,
        span: Span,
    },
    /// Recovery placeholder; always paired with at least one parse
    /// diagnostic.
    Error { span: Span },
}

impl Pattern {
    /// Returns the byte [`Span`] of this pattern.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Wildcard { span }
            | Self::Rest { span }
            | Self::Literal { span, .. }
            | Self::Path { span, .. }
            | Self::TupleStruct { span, .. }
            | Self::Struct { span, .. }
            | Self::Or { span, .. }
            | Self::Range { span, .. }
            | Self::Error { span } => *span,
            Self::Ident(id) => id.span,
        }
    }
}

/// `<field_name> [: <sub-pattern>]` inside a [`Pattern::Struct`].
///
/// When `pattern` is `None` the source used the field-shorthand form
/// `Point { x }`, equivalent to `Point { x: x }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructPatternField {
    pub name: Ident,
    pub pattern: Option<Pattern>,
    pub span: Span,
}

impl Expr {
    /// Returns the byte [`Span`] of this expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Int { span, .. }
            | Self::Float { span, .. }
            | Self::Str { span, .. }
            | Self::Bool { span, .. }
            | Self::NoneLit { span }
            | Self::Path { span, .. }
            | Self::Paren { span, .. }
            | Self::Call { span, .. }
            | Self::Index { span, .. }
            | Self::Field { span, .. }
            | Self::Unary { span, .. }
            | Self::Binary { span, .. }
            | Self::Block { span, .. }
            | Self::If { span, .. }
            | Self::While { span, .. }
            | Self::Loop { span, .. }
            | Self::Return { span, .. }
            | Self::Break { span, .. }
            | Self::Continue { span }
            | Self::Match { span, .. }
            | Self::Error { span } => *span,
            Self::Ident(id) => id.span,
        }
    }

    /// True when this expression is a "block-like" construct that may stand
    /// alone as a statement without a trailing `;`.
    ///
    /// S2.1 introduced [`Expr::Block`]. S2.2 extends the set to control
    /// flow that takes a block body (`if`, `while`, `loop`). `return`,
    /// `break` and `continue` are intentionally **not** block-like: they
    /// always require a trailing `;` when used as a statement.
    #[must_use]
    pub fn is_block_like(&self) -> bool {
        matches!(
            self,
            Self::Block { .. }
                | Self::If { .. }
                | Self::While { .. }
                | Self::Loop { .. }
                | Self::Match { .. }
        )
    }
}

/// A CapyLang statement.
///
/// Statements only appear inside a [`Expr::Block`] body or at the top level
/// of a [`Source`] tree. Top-level statements in a [`Source`] are
/// semantically equivalent to the body of an implicit module block but with
/// no tail expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `let <name> [: <ty>] [= <init>] ;`
    ///
    /// `ty` (optional) uses the dedicated [`Type`] grammar introduced in
    /// S2.3b; before S2.3b the slot temporarily reused [`Expr`] as a
    /// placeholder.
    Let {
        name: Ident,
        ty: Option<Type>,
        init: Option<Expr>,
        span: Span,
    },
    /// An expression used as a statement.
    ///
    /// `has_semi` is `true` when the source carried a trailing `;`. A
    /// statement without a trailing `;` is only well-formed when its
    /// expression is [`Expr::is_block_like`].
    Expr {
        expr: Expr,
        has_semi: bool,
        span: Span,
    },
    /// A top-level (or block-level) item declaration (`fn`, `const`,
    /// `struct`). Items have their own internal termination (block body or
    /// trailing `;` for `const`) so there is no `has_semi` flag.
    Item(Item),
}

impl Stmt {
    /// Returns the byte [`Span`] of this statement.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Let { span, .. } | Self::Expr { span, .. } => *span,
            Self::Item(item) => item.span(),
        }
    }
}

/// A CapyLang item declaration (S2.3).
///
/// S2.3a delivers `fn`, `const`, `struct`. S2.3b adds `type`, `enum`,
/// `import`. `trait` and `impl` are added in subsequent S2.3 sub-slices.
/// Items may appear at the top level of a [`Source`] tree and inside any
/// block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// `fn <name> ( <params> ) [-> <ret_ty>] <block>`
    Fn(FnItem),
    /// `const <name> : <ty> = <init> ;`
    Const(ConstItem),
    /// `struct <name> { <fields,> }`
    Struct(StructItem),
    /// `type <name> = <ty> ;`
    TypeAlias(TypeAlias),
    /// `enum <name> { <variants,> }`
    Enum(EnumItem),
    /// `import <path::...> [as <alias>] ;`
    Import(ImportItem),
}

impl Item {
    /// Returns the byte [`Span`] of this item.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Fn(f) => f.span,
            Self::Const(c) => c.span,
            Self::Struct(s) => s.span,
            Self::TypeAlias(t) => t.span,
            Self::Enum(e) => e.span,
            Self::Import(i) => i.span,
        }
    }
}

/// `fn <name> ( <params,> ) [-> <ret_ty>] <body>`.
///
/// `body` is always an [`Expr::Block`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnItem {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret_ty: Option<Type>,
    pub body: Box<Expr>,
    pub span: Span,
}

/// `<name> : <ty>` — a single function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// `const <name> : <ty> = <init> ;`. Both `ty` and `init` are required by
/// the S2.3a grammar (unlike `let`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstItem {
    pub name: Ident,
    pub ty: Type,
    pub init: Box<Expr>,
    pub span: Span,
}

/// `struct <name> { <fields,> }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructItem {
    pub name: Ident,
    pub fields: Vec<StructField>,
    pub span: Span,
}

/// `<name> : <ty>` — a single struct field. Distinct from [`Expr::Field`]
/// (the postfix `.name` access expression).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructField {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// `type <name> = <ty> ;` — a type alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// `enum <name> { <variants,> }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumItem {
    pub name: Ident,
    pub variants: Vec<Variant>,
    pub span: Span,
}

/// A single enum variant. Body shape selects between unit, tuple and
/// struct-like payloads (Rust convention).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub name: Ident,
    pub body: VariantBody,
    pub span: Span,
}

/// Payload shape of an enum [`Variant`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantBody {
    /// `Name`
    Unit,
    /// `Name(<types,>)`
    Tuple(Vec<Type>),
    /// `Name { <fields,> }`
    Struct(Vec<StructField>),
}

/// `import <path::...> [as <alias>] ;`.
///
/// `path` is always non-empty; the alias, when present, renames the
/// imported entity in the current namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportItem {
    pub path: Vec<Ident>,
    pub alias: Option<Ident>,
    pub span: Span,
}

/// A CapyLang type expression.
///
/// S2.3b delivers path types only (`i32`, `mod::Foo`). Tuple, function,
/// array, reference and generic type forms land in later S2.3 sub-slices.
/// Recovery via [`Type::Error`] mirrors [`Expr::Error`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// `ident (:: ident)*`
    Path { segments: Vec<Ident>, span: Span },
    /// Recovery placeholder; always paired with a parse diagnostic.
    Error { span: Span },
}

impl Type {
    /// Returns the byte [`Span`] of this type.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Path { span, .. } | Self::Error { span } => *span,
        }
    }
}

/// Top-level CapyLang source unit (a sequence of statements).
///
/// `parse_source` returns a [`Source`] whose `span` always covers the entire
/// input byte range `[0, source.len())`, even when the byte range contained
/// only trivia or recoverable errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

/// Unary prefix operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    /// `-`
    Neg,
    /// `!`
    Not,
    /// `~`
    BitNot,
}

impl UnOp {
    /// Canonical mnemonic used in the AST dump.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Neg => "Neg",
            Self::Not => "Not",
            Self::BitNot => "BitNot",
        }
    }
}

/// Binary infix operator.
///
/// The enum doubles as the canonical mnemonic in the AST dump via
/// [`BinOp::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    /// `||`
    Or,
    /// `&&`
    And,
    /// `|`
    BitOr,
    /// `^`
    BitXor,
    /// `&`
    BitAnd,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `<<`
    Shl,
    /// `>>`
    Shr,
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
}

impl BinOp {
    /// Canonical mnemonic used in the AST dump.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Or => "Or",
            Self::And => "And",
            Self::BitOr => "BitOr",
            Self::BitXor => "BitXor",
            Self::BitAnd => "BitAnd",
            Self::Eq => "Eq",
            Self::Ne => "Ne",
            Self::Lt => "Lt",
            Self::Le => "Le",
            Self::Gt => "Gt",
            Self::Ge => "Ge",
            Self::Shl => "Shl",
            Self::Shr => "Shr",
            Self::Add => "Add",
            Self::Sub => "Sub",
            Self::Mul => "Mul",
            Self::Div => "Div",
            Self::Mod => "Mod",
        }
    }

    /// Numeric precedence (higher binds tighter). Stable across minor
    /// versions; documented in `docs/grammar.ebnf`.
    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Or => 1,
            Self::And => 2,
            Self::BitOr => 3,
            Self::BitXor => 4,
            Self::BitAnd => 5,
            Self::Eq | Self::Ne => 6,
            Self::Lt | Self::Le | Self::Gt | Self::Ge => 7,
            Self::Shl | Self::Shr => 8,
            Self::Add | Self::Sub => 9,
            Self::Mul | Self::Div | Self::Mod => 10,
        }
    }
}
