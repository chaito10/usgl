use crate::ast::*;

/// Canonical USGL formatting (`us fmt`): 4-space indentation, one statement
/// per line, matching the style shown throughout the RFC.
pub fn format_program(p: &Program) -> String {
    let mut out = String::new();
    for s in &p.stmts {
        stmt(s, 0, &mut out);
    }
    for c in &p.eof_comments {
        out.push_str(c);
        out.push('\n');
    }
    out
}

fn indent(ind: usize, out: &mut String) {
    for _ in 0..ind {
        out.push_str("    ");
    }
}

fn comments(stmt: &Stmt, ind: usize, out: &mut String) {
    for c in &stmt.comments {
        indent(ind, out);
        out.push_str(c.trim());
        out.push('\n');
    }
}

fn stmt(s: &Stmt, ind: usize, out: &mut String) {
    comments(s, ind, out);
    match &s.kind {
        StmtKind::Let { name, mutable, ty, value } => {
            indent(ind, out);
            if *mutable {
                out.push_str("var ");
            } else {
                out.push_str("let ");
            }
            out.push_str(name);
            if let Some(t) = ty {
                out.push_str(": ");
                out.push_str(&type_str(t));
            }
            out.push_str(" = ");
            out.push_str(&expr(value, 0));
            out.push('\n');
        }
        StmtKind::Expr(e) => {
            indent(ind, out);
            out.push_str(&expr(e, 0));
            out.push('\n');
        }
        StmtKind::Fn { name, params, ret, body, is_async } => {
            indent(ind, out);
            if *is_async {
                out.push_str("async ");
            }
            out.push_str("fn ");
            out.push_str(name);
            out.push('(');
            params_str(params, out);
            out.push(')');
            if let Some(t) = ret {
                out.push_str(" -> ");
                out.push_str(&type_str(t));
            }
            out.push_str(" {");
            if body.is_empty() {
                out.push_str(" }\n");
            } else {
                out.push('\n');
                for b in body {
                    stmt(b, ind + 1, out);
                }
                indent(ind, out);
                out.push_str("}\n");
            }
        }
        StmtKind::Return(v) => {
            indent(ind, out);
            if let Some(e) = v {
                out.push_str(&format!("return {}\n", expr(e, 0)));
            } else {
                out.push_str("return\n");
            }
        }
        StmtKind::If { cond, then, alt } => {
            indent(ind, out);
            out.push_str(&format!("if {} {{", expr(cond, 0)));
            if then.is_empty() {
                out.push_str(" }\n");
            } else {
                out.push('\n');
                for b in then {
                    stmt(b, ind + 1, out);
                }
                indent(ind, out);
                out.push_str("}\n");
            }
            if let Some(else_stmt) = alt.last() {
                if let StmtKind::If { .. } = &else_stmt.kind {
                    // else if
                    let mut tail = else_stmt.clone();
                    let tail_cmts = std::mem::take(&mut tail.comments);
                    indent(ind, out);
                    out.push_str("else ");
                    let mut tmp = String::new();
                    stmt(&tail, ind, &mut tmp);
                    // strip the leading comments/indent that stmt() added
                    let trimmed = tmp.trim_start();
                    out.push_str(trimmed);
                    for _ in 0..tail_cmts.len() {
                        out.pop();
                        out.pop();
                    }
                } else {
                    indent(ind, out);
                    out.push_str("else {");
                    if alt.is_empty() {
                        out.push_str(" }\n");
                    } else {
                        out.push('\n');
                        for b in alt {
                            stmt(b, ind + 1, out);
                        }
                        indent(ind, out);
                        out.push_str("}\n");
                    }
                }
            }
        }
        StmtKind::While { cond, body } => {
            block_stmt(&format!("while {}", expr(cond, 0)), body, ind, out);
        }
        StmtKind::For { name, iter, body } => {
            block_stmt(&format!("for {} in {}", name, expr(iter, 0)), body, ind, out);
        }
        StmtKind::Loop { body } => {
            block_stmt("loop", body, ind, out);
        }
        StmtKind::Match { expr: me, arms } => {
            indent(ind, out);
            out.push_str(&format!("match {} {{", expr(me, 0)));
            if arms.is_empty() {
                out.push_str(" }\n");
            } else {
                out.push('\n');
                for a in arms {
                    indent(ind + 1, out);
                    out.push_str(&pattern_str(&a.pat));
                    out.push_str(" => ");
                    if a.body.len() == 1 && matches!(a.body[0].kind, StmtKind::Expr(_)) {
                        if let StmtKind::Expr(e) = &a.body[0].kind {
                            out.push_str(&expr(e, 0));
                        }
                        out.push('\n');
                    } else {
                        out.push_str("{\n");
                        for b in &a.body {
                            stmt(b, ind + 2, out);
                        }
                        indent(ind + 1, out);
                        out.push_str("}\n");
                    }
                }
                indent(ind, out);
                out.push_str("}\n");
            }
        }
        StmtKind::Struct { name, fields } => {
            indent(ind, out);
            out.push_str(&format!("struct {} {{", name));
            if fields.is_empty() {
                out.push_str(" }\n");
            } else {
                out.push('\n');
                for (fname, ty) in fields {
                    indent(ind + 1, out);
                    out.push_str(&format!("{}: {}", fname, type_str(ty)));
                    out.push('\n');
                }
                indent(ind, out);
                out.push_str("}\n");
            }
        }
        StmtKind::Enum { name, variants } => {
            indent(ind, out);
            out.push_str(&format!("enum {} {{", name));
            if variants.is_empty() {
                out.push_str(" }\n");
            } else {
                out.push('\n');
                for (vname, ty) in variants {
                    indent(ind + 1, out);
                    out.push_str(vname);
                    if let Some(t) = ty {
                        out.push_str(&format!("({})", type_str(t)));
                    }
                    out.push('\n');
                }
                indent(ind, out);
                out.push_str("}\n");
            }
        }
        StmtKind::Import(parts) => {
            indent(ind, out);
            out.push_str(&format!("import {}\n", parts.join(".")));
        }
        StmtKind::Test { name, body } => {
            indent(ind, out);
            out.push_str(&format!("test {:?} {{", name));
            if body.is_empty() {
                out.push_str(" }\n");
            } else {
                out.push('\n');
                for b in body {
                    stmt(b, ind + 1, out);
                }
                indent(ind, out);
                out.push_str("}\n");
            }
        }
        StmtKind::Break => {
            indent(ind, out);
            out.push_str("break\n");
        }
        StmtKind::Continue => {
            indent(ind, out);
            out.push_str("continue\n");
        }
        StmtKind::Block { body, unsafe_block } => {
            let head = if *unsafe_block { "unsafe {" } else { "{" };
            block_stmt(head, body, ind, out);
        }
    }
}

/// Render a chain `if ... { ... } else if ... { ... } else { ... }` from a
/// statement-level `StmtKind::If` (used by `else` continuations in expressions).
fn render_if_stmt(s: &Stmt, out: &mut String, ind: usize) {
    if let StmtKind::If { cond, then, alt } = &s.kind {
        out.push_str(&format!("if {} {{", expr(cond, 0)));
        if !then.is_empty() {
            out.push('\n');
            let mut body = String::new();
            for b in then {
                stmt(b, ind + 1, &mut body);
            }
            out.push_str(body.trim_end());
            out.push('\n');
        }
        out.push('}');
        if !alt.is_empty() {
            out.push_str(" else");
            if alt.len() == 1 && matches!(alt[0].kind, StmtKind::If { cond: _, then: _, alt: _ }) {
                out.push(' ');
                render_if_stmt(&alt[0], out, ind);
            } else {
                out.push_str(" {");
                if !alt.is_empty() {
                    out.push('\n');
                    let mut body = String::new();
                    for b in alt {
                        stmt(b, ind + 1, &mut body);
                    }
                    out.push_str(body.trim_end());
                    out.push('\n');
                }
                out.push('}');
            }
        }
    }
}

fn block_stmt(head: &str, body: &[Stmt], ind: usize, out: &mut String) {
    indent(ind, out);
    out.push_str(head);
    if body.is_empty() {
        out.push_str(" }\n");
    } else {
        out.push_str(" {\n");
        for b in body {
            stmt(b, ind + 1, out);
        }
        indent(ind, out);
        out.push_str("}\n");
    }
}

fn params_str(params: &[Param], out: &mut String) {
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&p.name);
        if let Some(t) = &p.ty {
            out.push_str(": ");
            out.push_str(&type_str(t));
        }
    }
}

fn type_str(t: &Type) -> String {
    let mut s = String::new();
    if t.is_ref {
        s.push('&');
        if t.is_mut {
            s.push_str("mut ");
        }
    }
    if t.is_ptr {
        s.push('*');
    }
    if t.base == "[]" {
        s.push('[');
        s.push_str(&type_str(&t.generics[0]));
        s.push(']');
    } else {
        s.push_str(&t.base);
        if !t.generics.is_empty() {
            s.push('[');
            let parts: Vec<String> = t.generics.iter().map(type_str).collect();
            s.push_str(&parts.join(", "));
            s.push(']');
        }
    }
    s
}

fn pattern_str(p: &Pattern) -> String {
    match p {
        Pattern::Wild => "_".into(),
        Pattern::Lit(l) => match l {
            LitVal::Int(n) => n.to_string(),
            LitVal::Float(f) => f.to_string(),
            LitVal::Str(s) => format!("{:?}", s),
            LitVal::Char(c) => format!("{:?}", c),
            LitVal::Bool(b) => b.to_string(),
        },
        Pattern::Bind(n) => n.clone(),
        Pattern::Variant { ty, name, inner } => {
            let mut s = String::new();
            if let Some(t) = ty {
                s.push_str(t);
                s.push('.');
            }
            s.push_str(name);
            if let Some(i) = inner {
                s.push('(');
                s.push_str(&pattern_str(i));
                s.push(')');
            }
            s
        }
    }
}

// ---------- expressions ----------

fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Assign { .. } => 0,
        Expr::Range(..) => 2,
        Expr::Binary { op, .. } => match op {
            BinOp::Or => 3,
            BinOp::And => 4,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 5,
            BinOp::Add | BinOp::Sub => 6,
            BinOp::Mul | BinOp::Div | BinOp::Mod => 7,
        },
        Expr::Unary { .. } | Expr::Await(_) => 8,
        _ => 9,
    }
}

fn expr(e: &Expr, min: u8) -> String {
    let my = prec(e);
    let s = match e {
        Expr::Int(n) => n.to_string(),
        Expr::Float(f) => crate::value::format_float(*f),
        Expr::Str(s) => format!("{:?}", s),
        Expr::Char(c) => format!("{:?}", c),
        Expr::Bool(b) => b.to_string(),
        Expr::Ident(n) => n.clone(),
        Expr::Unary { op, e } => format!("{}{}", op, expr(e, 8)),
        Expr::Await(inner) => format!("await {}", expr(inner, 8)),
        Expr::Binary { op, l, r } => {
            format!(
                "{} {} {}",
                expr(l, my),
                op.symbol(),
                expr(r, if left_assoc(*op) { my + 1 } else { my })
            )
        }
        Expr::Assign { target, value } => {
            format!("{} = {}", expr(target, 9), expr(value, 0))
        }
        Expr::Call { callee, args, .. } => {
            let mut s = callee_str(callee);
            s.push('(');
            let parts: Vec<String> = args
                .iter()
                .map(|a| {
                    if let Some(n) = &a.name {
                        format!("{} = {}", n, expr(&a.value, 0))
                    } else {
                        expr(&a.value, 0)
                    }
                })
                .collect();
            s.push_str(&parts.join(", "));
            s.push(')');
            s
        }
        Expr::Member { base, name } => {
            let mut s = callee_str(base);
            s.push('.');
            s.push_str(name);
            s
        }
        Expr::Index { base, index } => {
            format!("{}[{}]", callee_str(base), expr(index, 0))
        }
        Expr::ErrProp(inner) => format!("{}?", callee_str(inner)),
        Expr::Array(items) => {
            let parts: Vec<String> = items.iter().map(|x| expr(x, 0)).collect();
            format!("[{}]", parts.join(", "))
        }
        Expr::Map(fields) => {
            let parts: Vec<String> =
                fields.iter().map(|(k, v)| format!("{}: {}", k, expr(v, 0))).collect();
            format!("{{ {} }}", parts.join(", "))
        }
        Expr::StructLit { ty, fields } => {
            let parts: Vec<String> =
                fields.iter().map(|(k, v)| format!("{}: {}", k, expr(v, 0))).collect();
            format!("{} {{ {} }}", ty, parts.join(", "))
        }
        Expr::Ctor { ty, variant, payload } => {
            let mut s = String::new();
            if let Some(t) = ty {
                s.push_str(t);
                s.push('.');
            }
            s.push_str(variant);
            if let Some(p) = payload {
                s.push('(');
                s.push_str(&expr(p, 0));
                s.push(')');
            }
            s
        }
        Expr::Range(l, r) => format!("{}..{}", expr(l, 1), expr(r, 2)),
        Expr::Fn { params, arrow, body } => {
            let mut s = String::from("fn(");
            params_str(params, &mut s);
            s.push(')');
            if let Some(a) = arrow {
                s.push_str(" => ");
                s.push_str(&expr(a, 0));
            } else {
                s.push_str(" {");
                if body.is_empty() {
                    s.push_str(" }");
                } else {
                    let mut tmp = String::new();
                    for b in body {
                        stmt(b, 1, &mut tmp);
                    }
                    // insert indentation after opening brace
                    s.push('\n');
                    let body_str = tmp.trim_end();
                    s.push_str(body_str);
                    s.push_str("\n}");
                }
            }
            s
        }
        Expr::If { cond, then, alt } => {
            let mut s = format!("if {} {{", expr(cond, 0));
            if !then.is_empty() {
                s.push('\n');
                let mut body = String::new();
                for b in then {
                    stmt(b, 1, &mut body);
                }
                s.push_str(body.trim_end());
                s.push('\n');
            }
            s.push('}');
            if !alt.is_empty() {
                s.push_str(" else");
                if alt.len() == 1 && matches!(alt[0].kind, StmtKind::If { cond: _, then: _, alt: _ }) {
                    s.push(' ');
                    render_if_stmt(&alt[0], &mut s, 1);
                } else {
                    s.push_str(" {");
                    if !alt.is_empty() {
                        s.push('\n');
                        let mut body = String::new();
                        for b in alt {
                            stmt(b, 1, &mut body);
                        }
                        s.push_str(body.trim_end());
                        s.push('\n');
                    }
                    s.push('}');
                }
            }
            s
        }
    };
    if my < min {
        format!("({})", s)
    } else {
        s
    }
}

fn left_assoc(op: BinOp) -> bool {
    !matches!(op, BinOp::And | BinOp::Or)
}

fn callee_str(e: &Expr) -> String {
    match e {
        Expr::Ident(n) => n.clone(),
        Expr::Member { base, name } => format!("{}.{}", callee_str(base), name),
        Expr::Call { callee, args, .. } => {
            let mut s = callee_str(callee);
            s.push('(');
            let parts: Vec<String> = args
                .iter()
                .map(|a| {
                    if let Some(n) = &a.name {
                        format!("{} = {}", n, expr(&a.value, 0))
                    } else {
                        expr(&a.value, 0)
                    }
                })
                .collect();
            s.push_str(&parts.join(", "));
            s.push(')');
            s
        }
        _ => expr(e, 9),
    }
}