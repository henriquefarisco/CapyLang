//! Canonical textual dump for [`Expr`] trees.
//!
//! Mirrors the lexer dump format documented in `docs/lexer.md`: one line per
//! node, with `[start..end] <Kind>` followed by an optional payload. Child
//! nodes are indented with two spaces per level.
//!
//! Stability: the dump format is part of the parser's golden-test contract.
//! Changes between minor versions must remain additive (new optional
//! trailers); kind names and field order are frozen for S2.

#![forbid(unsafe_code)]

use std::fmt::Write as _;

use crate::expr::{
    ConstItem, EnumItem, Expr, FnItem, Ident, ImportItem, Item, MatchArm, Pattern, Source, Stmt,
    StructField, StructItem, StructLitField, StructPatternField, Type, TypeAlias, Variant,
    VariantBody,
};

const INDENT: &str = "  ";

/// Renders `expr` into the canonical AST dump format.
#[must_use]
pub fn dump_expr(expr: &Expr) -> String {
    let mut out = String::new();
    write_expr(&mut out, expr, 0);
    out
}

/// Renders a top-level [`Source`] tree into the canonical dump format.
///
/// Format:
///
/// ```text
/// [<start>..<end>] Source
///   <stmt 1>
///   <stmt 2>
///   ...
/// ```
///
/// See [`dump_expr`] for the per-expression line format.
#[must_use]
pub fn dump_source(source: &Source) -> String {
    let mut out = String::new();
    write_source(&mut out, source, 0);
    out
}

fn write_source(out: &mut String, source: &Source, depth: usize) {
    write_indent(out, depth);
    writeln!(out, "[{}..{}] Source", source.span.start, source.span.end)
        .expect("writing into String is infallible");
    for stmt in &source.stmts {
        write_stmt(out, stmt, depth + 1);
    }
}

fn write_stmt(out: &mut String, stmt: &Stmt, depth: usize) {
    match stmt {
        Stmt::Let {
            name,
            ty,
            init,
            span,
        } => {
            write_indent(out, depth);
            writeln!(out, "[{}..{}] Let {:?}", span.start, span.end, name.name)
                .expect("infallible");
            if let Some(t) = ty {
                write_indent(out, depth + 1);
                out.push_str("Type\n");
                write_type(out, t, depth + 2);
            }
            if let Some(i) = init {
                write_indent(out, depth + 1);
                out.push_str("Init\n");
                write_expr(out, i, depth + 2);
            }
        }
        Stmt::Expr {
            expr,
            has_semi,
            span,
        } => {
            write_indent(out, depth);
            let trailer = if *has_semi { ";" } else { "" };
            writeln!(out, "[{}..{}] ExprStmt{}", span.start, span.end, trailer)
                .expect("infallible");
            write_expr(out, expr, depth + 1);
        }
        Stmt::Item(item) => write_item(out, item, depth),
    }
}

fn write_item(out: &mut String, item: &Item, depth: usize) {
    match item {
        Item::Fn(f) => write_fn_item(out, f, depth),
        Item::Const(c) => write_const_item(out, c, depth),
        Item::Struct(s) => write_struct_item(out, s, depth),
        Item::TypeAlias(t) => write_type_alias_item(out, t, depth),
        Item::Enum(e) => write_enum_item(out, e, depth),
        Item::Import(i) => write_import_item(out, i, depth),
    }
}

fn write_fn_item(out: &mut String, f: &FnItem, depth: usize) {
    write_indent(out, depth);
    writeln!(
        out,
        "[{}..{}] Item Fn {:?}",
        f.span.start, f.span.end, f.name.name
    )
    .expect("infallible");
    write_indent(out, depth + 1);
    out.push_str("Params\n");
    for p in &f.params {
        write_indent(out, depth + 2);
        writeln!(
            out,
            "[{}..{}] Param {:?}",
            p.span.start, p.span.end, p.name.name
        )
        .expect("infallible");
        write_type(out, &p.ty, depth + 3);
    }
    if let Some(ret) = &f.ret_ty {
        write_indent(out, depth + 1);
        out.push_str("RetType\n");
        write_type(out, ret, depth + 2);
    }
    write_indent(out, depth + 1);
    out.push_str("Body\n");
    write_expr(out, &f.body, depth + 2);
}

fn write_const_item(out: &mut String, c: &ConstItem, depth: usize) {
    write_indent(out, depth);
    writeln!(
        out,
        "[{}..{}] Item Const {:?}",
        c.span.start, c.span.end, c.name.name
    )
    .expect("infallible");
    write_indent(out, depth + 1);
    out.push_str("Type\n");
    write_type(out, &c.ty, depth + 2);
    write_indent(out, depth + 1);
    out.push_str("Init\n");
    write_expr(out, &c.init, depth + 2);
}

fn write_struct_item(out: &mut String, s: &StructItem, depth: usize) {
    write_indent(out, depth);
    writeln!(
        out,
        "[{}..{}] Item Struct {:?}",
        s.span.start, s.span.end, s.name.name
    )
    .expect("infallible");
    for f in &s.fields {
        write_struct_field(out, f, depth + 1);
    }
}

fn write_struct_field(out: &mut String, f: &StructField, depth: usize) {
    write_indent(out, depth);
    writeln!(
        out,
        "[{}..{}] Field {:?}",
        f.span.start, f.span.end, f.name.name
    )
    .expect("infallible");
    write_type(out, &f.ty, depth + 1);
}

fn write_type_alias_item(out: &mut String, t: &TypeAlias, depth: usize) {
    write_indent(out, depth);
    writeln!(
        out,
        "[{}..{}] Item TypeAlias {:?}",
        t.span.start, t.span.end, t.name.name
    )
    .expect("infallible");
    write_indent(out, depth + 1);
    out.push_str("Type\n");
    write_type(out, &t.ty, depth + 2);
}

fn write_enum_item(out: &mut String, e: &EnumItem, depth: usize) {
    write_indent(out, depth);
    writeln!(
        out,
        "[{}..{}] Item Enum {:?}",
        e.span.start, e.span.end, e.name.name
    )
    .expect("infallible");
    for v in &e.variants {
        write_variant(out, v, depth + 1);
    }
}

fn write_variant(out: &mut String, v: &Variant, depth: usize) {
    write_indent(out, depth);
    match &v.body {
        VariantBody::Unit => {
            writeln!(
                out,
                "[{}..{}] Variant {:?}",
                v.span.start, v.span.end, v.name.name
            )
            .expect("infallible");
        }
        VariantBody::Tuple(types) => {
            writeln!(
                out,
                "[{}..{}] Variant {:?} Tuple",
                v.span.start, v.span.end, v.name.name
            )
            .expect("infallible");
            for t in types {
                write_type(out, t, depth + 1);
            }
        }
        VariantBody::Struct(fields) => {
            writeln!(
                out,
                "[{}..{}] Variant {:?} Struct",
                v.span.start, v.span.end, v.name.name
            )
            .expect("infallible");
            for f in fields {
                write_struct_field(out, f, depth + 1);
            }
        }
    }
}

fn write_import_item(out: &mut String, i: &ImportItem, depth: usize) {
    write_indent(out, depth);
    let path: Vec<&str> = i.path.iter().map(|s| s.name.as_str()).collect();
    let path_text = path.join("::");
    match &i.alias {
        Some(alias) => {
            writeln!(
                out,
                "[{}..{}] Item Import {:?} as {:?}",
                i.span.start, i.span.end, path_text, alias.name
            )
            .expect("infallible");
        }
        None => {
            writeln!(
                out,
                "[{}..{}] Item Import {:?}",
                i.span.start, i.span.end, path_text
            )
            .expect("infallible");
        }
    }
}

fn write_type(out: &mut String, ty: &Type, depth: usize) {
    write_indent(out, depth);
    match ty {
        Type::Path { segments, span } => {
            let names: Vec<&str> = segments.iter().map(|s| s.name.as_str()).collect();
            let joined = names.join("::");
            writeln!(out, "[{}..{}] TypePath {:?}", span.start, span.end, joined)
                .expect("infallible");
        }
        Type::Error { span } => {
            writeln!(out, "[{}..{}] TypeError", span.start, span.end).expect("infallible");
        }
    }
}

fn write_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str(INDENT);
    }
}

fn write_span(out: &mut String, expr: &Expr) {
    let span = expr.span();
    write!(out, "[{}..{}]", span.start, span.end).expect("writing into String is infallible");
}

fn write_expr(out: &mut String, expr: &Expr, depth: usize) {
    write_indent(out, depth);
    write_span(out, expr);
    match expr {
        Expr::Int { text, .. } => {
            write!(out, " Int {text:?}").expect("infallible");
            out.push('\n');
        }
        Expr::Float { text, .. } => {
            write!(out, " Float {text:?}").expect("infallible");
            out.push('\n');
        }
        Expr::Str { text, .. } => {
            write!(out, " Str {text:?}").expect("infallible");
            out.push('\n');
        }
        Expr::Bool { value, .. } => {
            write!(out, " Bool {value}").expect("infallible");
            out.push('\n');
        }
        Expr::NoneLit { .. } => {
            out.push_str(" NoneLit\n");
        }
        Expr::Ident(Ident { name, .. }) => {
            write!(out, " Ident {name:?}").expect("infallible");
            out.push('\n');
        }
        Expr::Path { segments, .. } => {
            let joined: Vec<&str> = segments.iter().map(|s| s.name.as_str()).collect();
            let joined = joined.join("::");
            write!(out, " Path {joined:?}").expect("infallible");
            out.push('\n');
        }
        Expr::Paren { inner, .. } => {
            out.push_str(" Paren\n");
            write_expr(out, inner, depth + 1);
        }
        Expr::Call { callee, args, .. } => {
            out.push_str(" Call\n");
            write_expr(out, callee, depth + 1);
            for arg in args {
                write_expr(out, arg, depth + 1);
            }
        }
        Expr::Index { target, index, .. } => {
            out.push_str(" Index\n");
            write_expr(out, target, depth + 1);
            write_expr(out, index, depth + 1);
        }
        Expr::Field { target, name, .. } => {
            write!(out, " Field {:?}", name.name).expect("infallible");
            out.push('\n');
            write_expr(out, target, depth + 1);
        }
        Expr::Unary { op, operand, .. } => {
            write!(out, " Unary {}", op.as_str()).expect("infallible");
            out.push('\n');
            write_expr(out, operand, depth + 1);
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            write!(out, " Binary {}", op.as_str()).expect("infallible");
            out.push('\n');
            write_expr(out, lhs, depth + 1);
            write_expr(out, rhs, depth + 1);
        }
        Expr::Block { stmts, tail, .. } => {
            out.push_str(" Block\n");
            for stmt in stmts {
                write_stmt(out, stmt, depth + 1);
            }
            if let Some(tail) = tail {
                write_indent(out, depth + 1);
                out.push_str("Tail\n");
                write_expr(out, tail, depth + 2);
            }
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            out.push_str(" If\n");
            write_indent(out, depth + 1);
            out.push_str("Cond\n");
            write_expr(out, cond, depth + 2);
            write_indent(out, depth + 1);
            out.push_str("Then\n");
            write_expr(out, then_branch, depth + 2);
            if let Some(else_b) = else_branch {
                write_indent(out, depth + 1);
                out.push_str("Else\n");
                write_expr(out, else_b, depth + 2);
            }
        }
        Expr::While { cond, body, .. } => {
            out.push_str(" While\n");
            write_indent(out, depth + 1);
            out.push_str("Cond\n");
            write_expr(out, cond, depth + 2);
            write_indent(out, depth + 1);
            out.push_str("Body\n");
            write_expr(out, body, depth + 2);
        }
        Expr::Loop { body, .. } => {
            out.push_str(" Loop\n");
            write_indent(out, depth + 1);
            out.push_str("Body\n");
            write_expr(out, body, depth + 2);
        }
        Expr::For {
            var,
            start,
            end,
            inclusive,
            body,
            ..
        } => {
            let op = if *inclusive { "..=" } else { ".." };
            write!(out, " For {:?} {}", var.name, op).expect("infallible");
            out.push('\n');
            write_indent(out, depth + 1);
            out.push_str("Start\n");
            write_expr(out, start, depth + 2);
            write_indent(out, depth + 1);
            out.push_str("End\n");
            write_expr(out, end, depth + 2);
            write_indent(out, depth + 1);
            out.push_str("Body\n");
            write_expr(out, body, depth + 2);
        }
        Expr::Return { value, .. } => {
            out.push_str(" Return\n");
            if let Some(v) = value {
                write_expr(out, v, depth + 1);
            }
        }
        Expr::Break { value, .. } => {
            out.push_str(" Break\n");
            if let Some(v) = value {
                write_expr(out, v, depth + 1);
            }
        }
        Expr::Continue { .. } => {
            out.push_str(" Continue\n");
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            out.push_str(" Match\n");
            write_indent(out, depth + 1);
            out.push_str("Scrutinee\n");
            write_expr(out, scrutinee, depth + 2);
            for arm in arms {
                write_match_arm(out, arm, depth + 1);
            }
        }
        Expr::Array { elems, .. } => {
            out.push_str(" Array\n");
            for e in elems {
                write_expr(out, e, depth + 1);
            }
        }
        Expr::Assign { target, value, .. } => {
            out.push_str(" Assign\n");
            write_expr(out, target, depth + 1);
            write_expr(out, value, depth + 1);
        }
        Expr::StructLit { path, fields, .. } => {
            let joined: Vec<&str> = path.iter().map(|s| s.name.as_str()).collect();
            let joined = joined.join("::");
            write!(out, " StructLit {joined:?}").expect("infallible");
            out.push('\n');
            for f in fields {
                write_struct_lit_field(out, f, depth + 1);
            }
        }
        Expr::Error { .. } => {
            out.push_str(" Error\n");
        }
    }
}

fn write_struct_lit_field(out: &mut String, f: &StructLitField, depth: usize) {
    write_indent(out, depth);
    writeln!(
        out,
        "[{}..{}] LitField {:?}",
        f.span.start, f.span.end, f.name.name
    )
    .expect("infallible");
    write_expr(out, &f.value, depth + 1);
}

fn write_match_arm(out: &mut String, arm: &MatchArm, depth: usize) {
    write_indent(out, depth);
    writeln!(out, "[{}..{}] Arm", arm.span.start, arm.span.end).expect("infallible");
    write_pattern(out, &arm.pattern, depth + 1);
    if let Some(g) = &arm.guard {
        write_indent(out, depth + 1);
        out.push_str("Guard\n");
        write_expr(out, g, depth + 2);
    }
    write_indent(out, depth + 1);
    out.push_str("Body\n");
    write_expr(out, &arm.body, depth + 2);
}

fn write_pattern(out: &mut String, p: &Pattern, depth: usize) {
    write_indent(out, depth);
    let span = p.span();
    write!(out, "[{}..{}]", span.start, span.end).expect("infallible");
    match p {
        Pattern::Wildcard { .. } => {
            out.push_str(" PatWildcard\n");
        }
        Pattern::Rest { .. } => {
            out.push_str(" PatRest\n");
        }
        Pattern::Literal { value, .. } => {
            out.push_str(" PatLiteral\n");
            write_expr(out, value, depth + 1);
        }
        Pattern::Ident(Ident { name, .. }) => {
            write!(out, " PatIdent {name:?}").expect("infallible");
            out.push('\n');
        }
        Pattern::Path { segments, .. } => {
            let joined: Vec<&str> = segments.iter().map(|s| s.name.as_str()).collect();
            let joined = joined.join("::");
            write!(out, " PatPath {joined:?}").expect("infallible");
            out.push('\n');
        }
        Pattern::TupleStruct { path, elems, .. } => {
            let joined: Vec<&str> = path.iter().map(|s| s.name.as_str()).collect();
            let joined = joined.join("::");
            write!(out, " PatTupleStruct {joined:?}").expect("infallible");
            out.push('\n');
            for e in elems {
                write_pattern(out, e, depth + 1);
            }
        }
        Pattern::Struct {
            path,
            fields,
            has_rest,
            ..
        } => {
            let joined: Vec<&str> = path.iter().map(|s| s.name.as_str()).collect();
            let joined = joined.join("::");
            write!(out, " PatStruct {joined:?}").expect("infallible");
            if *has_rest {
                out.push_str(" ..");
            }
            out.push('\n');
            for f in fields {
                write_struct_pattern_field(out, f, depth + 1);
            }
        }
        Pattern::Or { alts, .. } => {
            out.push_str(" PatOr\n");
            for a in alts {
                write_pattern(out, a, depth + 1);
            }
        }
        Pattern::Range {
            lo, hi, inclusive, ..
        } => {
            if *inclusive {
                out.push_str(" PatRange ..=\n");
            } else {
                out.push_str(" PatRange ..\n");
            }
            write_pattern(out, lo, depth + 1);
            write_pattern(out, hi, depth + 1);
        }
        Pattern::Error { .. } => {
            out.push_str(" PatError\n");
        }
    }
}

fn write_struct_pattern_field(out: &mut String, f: &StructPatternField, depth: usize) {
    write_indent(out, depth);
    writeln!(
        out,
        "[{}..{}] PatField {:?}",
        f.span.start, f.span.end, f.name.name
    )
    .expect("infallible");
    if let Some(sub) = &f.pattern {
        write_pattern(out, sub, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::dump_expr;
    use crate::expr::{BinOp, Expr, Ident, UnOp};
    use capy_lexer::Span;

    fn s(a: usize, b: usize) -> Span {
        Span::new(a, b)
    }

    #[test]
    fn dump_int_literal() {
        let e = Expr::Int {
            text: "42".into(),
            span: s(0, 2),
        };
        assert_eq!(dump_expr(&e), "[0..2] Int \"42\"\n");
    }

    #[test]
    fn dump_binary_tree() {
        let lhs = Expr::Int {
            text: "1".into(),
            span: s(0, 1),
        };
        let rhs = Expr::Int {
            text: "2".into(),
            span: s(4, 5),
        };
        let e = Expr::Binary {
            op: BinOp::Add,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: s(0, 5),
        };
        assert_eq!(
            dump_expr(&e),
            "[0..5] Binary Add\n  [0..1] Int \"1\"\n  [4..5] Int \"2\"\n"
        );
    }

    #[test]
    fn dump_unary_negate() {
        let e = Expr::Unary {
            op: UnOp::Neg,
            operand: Box::new(Expr::Ident(Ident {
                name: "x".into(),
                span: s(1, 2),
            })),
            span: s(0, 2),
        };
        assert_eq!(dump_expr(&e), "[0..2] Unary Neg\n  [1..2] Ident \"x\"\n");
    }
}
