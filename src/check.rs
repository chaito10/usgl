//! Phase 2 static type checker.
//!
//! Walks the AST and infers/validates types against the annotations the
//! parser already retains (params, struct fields, enum payloads, `-> Ret`).
//! The interpreter remains dynamic; the checker is deliberately permissive
//! (`Any` flows through json/map/callback boundaries) and only rejects
//! clearly-wrong programs: unknown names, bad argument types, assignment
//! to immutable bindings, unknown struct/variant fields, `?` on
//! non-`Result` values, type mismatches against annotations.

use std::collections::HashMap;

use crate::ast::{self, *};
use crate::error::UsglError;

/// Static type of a checked expression.
#[derive(Debug, Clone, PartialEq)]
pub enum T {
    Int,
    Float,
    Str,
    Bool,
    Char,
    Nil,
    /// Dynamic escape hatch (json, maps, unannotated callbacks, imports).
    Any,
    Array(Box<T>),
    Map,
    ProcResult,
    Func(Box<FuncSig>),
    Struct(String),
    Enum(String),
    Opt(Box<T>),
    Res(Box<T>),
}

impl T {
    fn describe(&self) -> String {
        match self {
            T::Int => "int".into(),
            T::Float => "float".into(),
            T::Str => "string".into(),
            T::Bool => "bool".into(),
            T::Char => "char".into(),
            T::Nil => "nil".into(),
            T::Any => "any".into(),
            T::Array(_) => "array".into(),
            T::Map => "map".into(),
            T::ProcResult => "process result".into(),
            T::Func(_) => "function".into(),
            T::Struct(n) => format!("struct `{n}`"),
            T::Enum(n) => format!("enum `{n}`"),
            T::Opt(_) => "Option".into(),
            T::Res(_) => "Result".into(),
        }
    }
}

/// An inferred callable signature.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncSig {
    /// One entry per positional parameter (`Any` = accepts anything).
    pub params: Vec<T>,
    pub ret: T,
    /// Builtins like `println` accept extra positional arguments.
    pub variadic: bool,
}

impl FuncSig {
    fn fixed(params: Vec<T>, ret: T) -> Self {
        FuncSig {
            params,
            ret,
            variadic: false,
        }
    }
}

/// A scoped binding: implied type + mutability.
type Cell = (T, bool);

fn is_module(name: &str) -> bool {
    matches!(
        name,
        "fs" | "json" | "math" | "strings" | "time" | "os" | "process" | "Buffer" | "Bytes"
    )
}

/// Builtin global + module function signatures.
fn builtin_sig(name: &str) -> Option<FuncSig> {
    let one = |p: T| FuncSig::fixed(vec![p], T::Str);
    match name {
        "println" | "print" => Some(FuncSig {
            params: vec![T::Any],
            ret: T::Nil,
            variadic: true,
        }),
        "assert" => Some(FuncSig::fixed(vec![T::Any], T::Nil)),
        "assert_eq" => Some(FuncSig::fixed(vec![T::Any, T::Any], T::Nil)),
        "shell" => Some(FuncSig::fixed(vec![T::Str], T::Str)),
        "type_of" => Some(FuncSig::fixed(vec![T::Any], T::Str)),
        "str" => Some(FuncSig::fixed(vec![T::Any], T::Str)),
        "len" => Some(FuncSig::fixed(vec![T::Any], T::Int)),
        "map" | "filter" => Some(FuncSig::fixed(
            vec![
                T::Array(Box::new(T::Any)),
                T::Func(Box::new(FuncSig::fixed(vec![T::Any], T::Any))),
            ],
            T::Array(Box::new(T::Any)),
        )),

        "fs.read" => Some(FuncSig::fixed(vec![T::Str], T::Res(Box::new(T::Str)))),
        "fs.write" | "fs.append" => Some(FuncSig::fixed(
            vec![T::Str, T::Str],
            T::Res(Box::new(T::Nil)),
        )),
        "fs.exists" => Some(FuncSig::fixed(vec![T::Str], T::Bool)),
        "fs.list" | "fs.directories" => Some(FuncSig::fixed(
            vec![T::Str],
            T::Res(Box::new(T::Array(Box::new(T::Str)))),
        )),
        "fs.glob" => Some(FuncSig::fixed(vec![T::Str], T::Array(Box::new(T::Str)))),
        "fs.open" => Some(FuncSig::fixed(vec![T::Str], T::Any)),
        "fs.remove" | "fs.mkdir" => Some(FuncSig::fixed(vec![T::Str], T::Res(Box::new(T::Nil)))),

        "json.parse" => Some(FuncSig::fixed(vec![T::Any], T::Res(Box::new(T::Any)))),
        "json.stringify" => Some(FuncSig::fixed(vec![T::Any], T::Str)),

        "math.sqrt" | "math.cbrt" | "math.abs" | "math.floor" | "math.ceil" | "math.round"
        | "math.trunc" | "math.sin" | "math.cos" | "math.tan" | "math.asin" | "math.acos"
        | "math.atan" | "math.log" | "math.log2" | "math.log10" | "math.exp" => {
            Some(FuncSig::fixed(vec![T::Any], T::Float))
        }
        "math.pow" => Some(FuncSig::fixed(vec![T::Any, T::Any], T::Float)),
        "math.min" | "math.max" => Some(FuncSig::fixed(vec![T::Any, T::Any], T::Any)),

        "strings.upper" | "strings.lower" | "strings.trim" | "strings.trim_start"
        | "strings.trim_end" | "strings.reverse" => Some(one(T::Str)),
        "strings.repeat" => Some(FuncSig::fixed(vec![T::Str, T::Int], T::Str)),
        "strings.replace" => Some(FuncSig::fixed(vec![T::Str, T::Str, T::Str], T::Str)),
        "strings.starts_with" | "strings.ends_with" | "strings.contains" => {
            Some(FuncSig::fixed(vec![T::Str, T::Str], T::Bool))
        }
        "strings.char_at" => Some(FuncSig::fixed(vec![T::Str, T::Int], T::Str)),
        "strings.length" => Some(FuncSig::fixed(vec![T::Str], T::Int)),
        "strings.split" => Some(FuncSig::fixed(
            vec![T::Str, T::Str],
            T::Array(Box::new(T::Str)),
        )),
        "strings.join" => Some(FuncSig::fixed(
            vec![T::Array(Box::new(T::Any)), T::Str],
            T::Str,
        )),
        "strings.chars" => Some(FuncSig::fixed(vec![T::Str], T::Array(Box::new(T::Char)))),

        "time.sleep" => Some(FuncSig::fixed(vec![T::Int], T::Nil)),
        "time.millis" => Some(FuncSig::fixed(vec![], T::Int)),
        "time.now" => Some(FuncSig::fixed(vec![], T::Str)),
        "os.name" => Some(FuncSig::fixed(vec![], T::Str)),
        "os.env" => Some(FuncSig::fixed(vec![T::Str], T::Any)),
        "os.args" => Some(FuncSig::fixed(vec![], T::Array(Box::new(T::Str)))),
        "os.cwd" => Some(FuncSig::fixed(vec![], T::Str)),
        "os.exit" => Some(FuncSig::fixed(vec![T::Int], T::Nil)),
        "process.run" => Some(FuncSig::fixed(
            vec![T::Str, T::Array(Box::new(T::Any))],
            T::ProcResult,
        )),
        "process.shell" => Some(FuncSig::fixed(vec![T::Str], T::Res(Box::new(T::Str)))),
        "Buffer.new" | "Bytes.new" => Some(FuncSig::fixed(vec![T::Int], T::Any)),
        _ => None,
    }
}

/// Returns true when `got` may be passed where `expect` is required.
fn assignable(expect: &T, got: &T) -> bool {
    use T::*;
    match (expect, got) {
        (a, b) if a == b => true,
        (Any, _) | (_, Any) => true,
        (Float, Int) => true,
        (Array(ae), Array(ag)) => assignable(ae, ag),
        (Opt(e), Opt(g)) => assignable(e, g),
        (Res(e), Res(g)) => assignable(e, g),
        (Func(_), Func(_)) => true, // caller is responsible for the body itself
        _ => false,
    }
}

fn resolve_type(t: &Type) -> T {
    use T::*;
    match t.base.as_str() {
        "int" | "i64" | "i32" | "u64" | "usize" => Int,
        "float" | "f64" | "f32" | "double" => Float,
        "string" | "str" => Str,
        "bool" => Bool,
        "char" => Char,
        "nil" | "void" => Nil,
        "any" | "Any" | "dynamic" | "Dyn" => Any,
        "map" => Map,
        "Option" | "Optional" => Opt(Box::new(
            t.generics.first().map(resolve_type).unwrap_or(Any),
        )),
        "Result" | "Res" => Res(Box::new(
            t.generics.first().map(resolve_type).unwrap_or(Any),
        )),
        "Array" | "List" => Array(Box::new(
            t.generics.first().map(resolve_type).unwrap_or(Any),
        )),
        other => {
            if let Some(s) = resolve_known(other) {
                s
            } else {
                Any
            }
        }
    }
}

// Flatten unknown names to `Any`; concrete user-defined types are
// resolved from declarations instead of here.
pub(crate) fn resolve_known(_name: &str) -> Option<T> {
    None
}

pub struct Checker {
    structs: HashMap<String, Vec<(String, T)>>,
    enums: HashMap<String, Vec<(String, Option<T>)>>,
    fns: HashMap<String, FuncSig>,
    scopes: Vec<HashMap<String, Cell>>,
    /// Return type expected at the current function depth (None = unannotated).
    ret_stack: Vec<Option<T>>,
    loop_depth: usize,
    errors: Vec<String>,
}

impl Checker {
    pub fn new() -> Self {
        Checker {
            structs: HashMap::new(),
            enums: HashMap::new(),
            fns: HashMap::new(),
            scopes: Vec::new(),
            ret_stack: Vec::new(),
            loop_depth: 0,
            errors: Vec::new(),
        }
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Top-level typecheck entry: pre-registers declarations so mutual
    /// recursion and forward references work, then checks every statement.
    pub fn check(&mut self, program: &Program) -> Vec<String> {
        for stmt in &program.stmts {
            match &stmt.kind {
                StmtKind::Struct { name, .. } => {
                    self.structs.entry(name.clone()).or_default();
                }
                StmtKind::Enum { name, .. } => {
                    self.enums.entry(name.clone()).or_default();
                }
                _ => {}
            }
        }
        for stmt in &program.stmts {
            match &stmt.kind {
                StmtKind::Struct { name, fields } => {
                    if let Some(f) = self.structs.get_mut(name) {
                        for (n, t) in fields {
                            f.push((n.clone(), resolve_type(t)));
                        }
                    }
                }
                StmtKind::Enum { name, variants } => {
                    if let Some(v) = self.enums.get_mut(name) {
                        for (n, t) in variants {
                            v.push((n.clone(), t.as_ref().map(resolve_type)));
                        }
                    }
                }
                StmtKind::Fn {
                    name, params, ret, ..
                } => {
                    let sig = FuncSig::fixed(
                        params
                            .iter()
                            .map(|p| p.ty.as_ref().map(resolve_type).unwrap_or(T::Any))
                            .collect(),
                        ret.as_ref().map(resolve_type).unwrap_or(T::Any),
                    );
                    self.fns.insert(name.clone(), sig);
                }
                _ => {}
            }
        }
        for stmt in &program.stmts {
            self.check_stmt(stmt);
        }
        std::mem::take(&mut self.errors)
    }

    // ---------------- scopes & names ----------------

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: &str, t: T, mutable: bool) {
        self.scopes
            .last_mut()
            .unwrap_or(&mut HashMap::new())
            .insert(name.to_string(), (t, mutable));
    }

    fn define_global(&mut self, name: &str, t: T, mutable: bool) {
        if self.scopes.is_empty() {
            self.scopes.push(HashMap::new());
        }
        self.scopes[0].insert(name.to_string(), (t, mutable));
    }

    fn lookup_cell(&self, name: &str) -> Option<Cell> {
        for scope in self.scopes.iter().rev() {
            if let Some(c) = scope.get(name) {
                return Some(c.clone());
            }
        }
        if self.fns.contains_key(name) {
            return Some((T::Func(Box::new(self.fns[name].clone())), false));
        }
        if let Some(sig) = builtin_sig(name) {
            return Some((T::Func(Box::new(sig)), false));
        }
        if self.structs.contains_key(name) {
            return Some((T::Struct(name.to_string()), false));
        }
        if self.enums.contains_key(name) {
            return Some((T::Enum(name.to_string()), false));
        }
        None
    }

    fn lookup(&self, name: &str) -> Option<T> {
        self.lookup_cell(name).map(|(t, _)| t)
    }

    fn err(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }

    // ---------------- statements ----------------

    fn check_stmt(&mut self, s: &Stmt) {
        match &s.kind {
            StmtKind::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                let vt = self.check_expr(value);
                let declared = ty.as_ref().map(resolve_type);
                if let Some(d) = &declared {
                    if !assignable(d, &vt) {
                        self.err(format!(
                            "cannot bind `{name}` as `{}`; initializer produces `{}`",
                            d.describe(),
                            vt.describe()
                        ));
                    }
                }
                let t = declared.unwrap_or(vt);
                if self.scopes.is_empty() {
                    self.define_global(name, t, *mutable);
                } else {
                    self.define(name, t, *mutable);
                }
            }
            StmtKind::Expr(e) => {
                let _ = self.check_expr(e);
            }
            StmtKind::Fn {
                name,
                params,
                ret,
                body,
                ..
            } => {
                if !self.fns.contains_key(name) {
                    let sig = FuncSig::fixed(
                        params
                            .iter()
                            .map(|p| p.ty.as_ref().map(resolve_type).unwrap_or(T::Any))
                            .collect(),
                        ret.as_ref().map(resolve_type).unwrap_or(T::Any),
                    );
                    self.fns.insert(name.clone(), sig);
                }
                self.push();
                for (i, p) in params.iter().enumerate() {
                    let t = p.ty.as_ref().map(resolve_type).unwrap_or(T::Any);
                    self.define(&p.name, t, false);
                    let _ = i;
                }
                self.ret_stack.push(ret.as_ref().map(resolve_type));
                for b in body {
                    self.check_stmt(b);
                }
                self.ret_stack.pop();
                self.pop();
            }
            StmtKind::Return(e) => {
                if let Some(Some(rt)) = self.ret_stack.last().cloned() {
                    if let Some(e) = e {
                        let t = self.check_expr(e);
                        if !assignable(&rt, &t) {
                            self.err(format!(
                                "return type mismatch: expected `{}`, found `{}`",
                                rt.describe(),
                                t.describe()
                            ));
                        }
                    } else {
                        if !assignable(&rt, &T::Nil) {
                            self.err(format!(
                                "return type mismatch: expected `{}`, found nothing",
                                rt.describe()
                            ));
                        }
                    }
                } else if let Some(e) = e {
                    let _ = self.check_expr(e);
                }
            }
            StmtKind::If { cond, then, alt } => {
                let _ = self.check_expr(cond);
                self.push();
                for b in then {
                    self.check_stmt(b);
                }
                self.pop();
                self.push();
                for b in alt {
                    self.check_stmt(b);
                }
                self.pop();
            }
            StmtKind::While { cond, body } => {
                let _ = self.check_expr(cond);
                self.loop_depth += 1;
                self.push();
                for b in body {
                    self.check_stmt(b);
                }
                self.pop();
                self.loop_depth -= 1;
            }
            StmtKind::For { name, iter, body } => {
                let elem = self.for_elem(iter);
                self.loop_depth += 1;
                self.push();
                self.define(name, elem, false);
                for b in body {
                    self.check_stmt(b);
                }
                self.pop();
                self.loop_depth -= 1;
            }
            StmtKind::Loop { body } => {
                self.loop_depth += 1;
                self.push();
                for b in body {
                    self.check_stmt(b);
                }
                self.pop();
                self.loop_depth -= 1;
            }
            StmtKind::Match { expr, arms } => {
                let st = self.check_expr(expr);
                for arm in arms {
                    self.check_pattern(&arm.pat, &st, arm);
                }
            }
            StmtKind::Block { body, .. } => {
                self.push();
                for b in body {
                    self.check_stmt(b);
                }
                self.pop();
            }
            StmtKind::Test { body, .. } => {
                self.push();
                for b in body {
                    self.check_stmt(b);
                }
                self.pop();
            }
            StmtKind::Struct { .. } | StmtKind::Enum { .. } | StmtKind::Import(_) => {}
            StmtKind::Break | StmtKind::Continue => {
                if self.loop_depth == 0 {
                    self.err(format!(
                        "`{}` outside of a loop",
                        if matches!(s.kind, StmtKind::Break) {
                            "break"
                        } else {
                            "continue"
                        }
                    ));
                }
            }
        }
    }

    fn for_elem(&mut self, iter: &Expr) -> T {
        match iter {
            Expr::Range(l, r) => {
                let _ = self.check_expr(l);
                let _ = self.check_expr(r);
                T::Int
            }
            e => match self.check_expr(e) {
                T::Str => T::Char,
                T::Array(inner) => *inner,
                T::Map => T::Str,
                T::Any => T::Any,
                other => {
                    self.err(format!("cannot iterate over `{}`", other.describe()));
                    T::Any
                }
            },
        }
    }

    fn check_pattern(&mut self, pat: &Pattern, scrutinee: &T, _arm: &ast::Arm) {
        match pat {
            Pattern::Wild => {}
            Pattern::Bind(name) => {
                self.define(name, scrutinee.clone(), false);
            }
            Pattern::Lit(lit) => {
                let lt = match lit {
                    LitVal::Int(_) => T::Int,
                    LitVal::Float(_) => T::Float,
                    LitVal::Str(_) => T::Str,
                    LitVal::Char(_) => T::Char,
                    LitVal::Bool(_) => T::Bool,
                };
                if !matches!(scrutinee, T::Any) && !assignable(&lt, scrutinee) && lt != *scrutinee {
                    self.err(format!(
                        "pattern of type `{}` cannot match a value of type `{}`",
                        lt.describe(),
                        scrutinee.describe()
                    ));
                }
            }
            Pattern::Variant { ty, name, inner } => {
                // Locate the enum.
                let found: Option<(&String, &Vec<(String, Option<T>)>)> = if let Some(t) = ty {
                    self.enums.get(t).map(|v| (t, v))
                } else {
                    self.enums
                        .iter()
                        .find(|(_, v)| v.iter().any(|(n, _)| n == name))
                };
                match found {
                    Some((ety, variants)) => {
                        let payload_t = variants
                            .iter()
                            .find(|(n, _)| n == name)
                            .map(|(_, p)| p.clone())
                            .flatten();
                        if !variants.iter().any(|(n, _)| n == name) {
                            self.err(format!("enum `{ety}` has no variant `{name}`"));
                            return;
                        }
                        if !matches!(scrutinee, T::Any)
                            && !assignable(&T::Enum(ety.clone()), scrutinee)
                        {
                            self.err(format!(
                                "pattern `{ety}.{name}` cannot match a value of type `{}`",
                                scrutinee.describe()
                            ));
                        }
                        if let Some(inner_pat) = inner {
                            match payload_t.unwrap_or(T::Any) {
                                T::Any => self.check_pattern(inner_pat, &T::Any, _arm),
                                t => {
                                    if !matches!(t, T::Any) {
                                        self.check_pattern(inner_pat, &T::Any, _arm);
                                    } else {
                                        self.check_pattern(inner_pat, &t, _arm);
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        // Unknown enum: be permissive if dynamic, else flag it.
                        if !matches!(scrutinee, T::Any) {
                            self.err(format!("unknown enum variant `{name}`"));
                        }
                        if let Some(inner_pat) = inner {
                            self.check_pattern(inner_pat, &T::Any, _arm);
                        }
                    }
                }
            }
        }
    }

    // ---------------- expressions ----------------

    fn check_expr(&mut self, e: &Expr) -> T {
        use T::*;
        match e {
            Expr::Int(_) => Int,
            Expr::Float(_) => Float,
            Expr::Str(_) => Str,
            Expr::Char(_) => Char,
            Expr::Bool(_) => Bool,
            Expr::Ident(n) => {
                if matches!(n.as_str(), "nil" | "null" | "undefined") {
                    Nil
                } else {
                    match self.lookup(n) {
                        Some(t) => t,
                        None => {
                            self.err(format!("unknown variable `{n}`"));
                            Any
                        }
                    }
                }
            }
            Expr::Unary { op, e } => {
                let t = self.check_expr(e);
                match op {
                    '-' | '+' => {
                        if !matches!(t, Int | Float | Any) {
                            self.err(format!("cannot apply `{op}` to `{}`", t.describe()));
                        }
                        t
                    }
                    '!' => Bool,
                    _ => Any,
                }
            }
            Expr::Binary { op, l, r } => self.check_binary(*op, l, r),
            Expr::Assign { target, value } => {
                // Treat as a statement-style assignment that also has a value.
                match &**target {
                    Expr::Ident(n) => match self.lookup_cell(n) {
                        Some((t, mutable)) => {
                            if !mutable {
                                self.err(format!("cannot assign to immutable `{n}`"));
                            }
                            let vt = self.check_expr(value);
                            if !assignable(&t, &vt) {
                                self.err(format!(
                                    "cannot assign `{}` to `{n}` of type `{}`",
                                    vt.describe(),
                                    t.describe()
                                ));
                            }
                            t
                        }
                        None => {
                            let _ = self.check_expr(value);
                            self.err(format!("unknown variable `{n}` in assignment"));
                            Any
                        }
                    },
                    _ => {
                        let _ = self.check_expr(target);
                        self.check_expr(value)
                    }
                }
            }
            Expr::Call { callee, args, .. } => self.check_call(callee, args),
            Expr::Member { base, name } => {
                if let Expr::Ident(m) = &**base {
                    if is_module(m) {
                        return self.module_prop(m, name);
                    }
                }
                let bt = self.check_expr(base);
                self.member_prop(&bt, name)
            }
            Expr::Index { base, index } => {
                let bt = self.check_expr(base);
                let _ = self.check_expr(index);
                match bt {
                    Array(inner) => *inner,
                    Str => Char,
                    Map | Any => Any,
                    other => {
                        self.err(format!("cannot index `{}`", other.describe()));
                        Any
                    }
                }
            }
            Expr::ErrProp(inner) => {
                let t = self.check_expr(inner);
                match t {
                    Res(ok) => *ok,
                    Any | Opt(_) => Any,
                    other => {
                        self.err(format!(
                            "`?` used on `{}`; it only unwraps Result values",
                            other.describe()
                        ));
                        Any
                    }
                }
            }
            Expr::Array(items) => {
                let elem: T = if items.is_empty() {
                    Any
                } else {
                    let types: Vec<T> = items.iter().map(|i| self.check_expr(i)).collect();
                    if types.windows(2).all(|w| w[0] == w[1]) && !matches!(types[0], Any) {
                        types[0].clone()
                    } else if types.iter().all(|t| !matches!(t, Any))
                        && types.iter().all(|t| *t == types[0])
                    {
                        types[0].clone()
                    } else {
                        Any
                    }
                };
                Array(Box::new(elem))
            }
            Expr::Map(_) => Map,
            Expr::StructLit { ty, fields } => {
                let declared: Vec<(String, T)> = self.structs.get(ty).cloned().unwrap_or_default();
                if !declared.is_empty() || self.structs.contains_key(ty) {
                    for (name, expr) in fields {
                        match declared.iter().find(|(n, _)| n == name) {
                            Some((_, ft)) => {
                                let vt = self.check_expr(expr);
                                if !assignable(ft, &vt) {
                                    self.err(format!(
                                        "field `{ty}.{name}` expects `{}`, found `{}`",
                                        ft.describe(),
                                        vt.describe()
                                    ));
                                }
                            }
                            None => {
                                self.err(format!("struct `{ty}` has no field `{name}`"));
                                let _ = self.check_expr(expr);
                            }
                        }
                    }
                } else {
                    self.err(format!("unknown struct `{ty}`"));
                    for (_, expr) in fields {
                        let _ = self.check_expr(expr);
                    }
                }
                Struct(ty.clone())
            }
            Expr::Ctor {
                ty,
                variant,
                payload,
            } => self.check_ctor(ty.as_deref(), variant, payload.as_deref()),
            Expr::Range(l, r) => {
                let _ = self.check_expr(l);
                let _ = self.check_expr(r);
                Any
            }
            Expr::Fn {
                params,
                arrow,
                body,
            } => {
                let sig = if let Some(a) = arrow {
                    let mut s = self.fn_signature(params);
                    self.push();
                    for (i, p) in params.iter().enumerate() {
                        let _ = i;
                        self.define(
                            &p.name,
                            p.ty.as_ref().map(resolve_type).unwrap_or(Any),
                            false,
                        );
                    }
                    self.ret_stack.push(Some(Any));
                    let ret = self.check_expr(a);
                    self.ret_stack.pop();
                    self.pop();
                    s.ret = ret;
                    s
                } else {
                    let s = self.fn_signature(params);
                    self.push();
                    for (i, p) in params.iter().enumerate() {
                        let _ = i;
                        self.define(
                            &p.name,
                            p.ty.as_ref().map(resolve_type).unwrap_or(Any),
                            false,
                        );
                    }
                    // No annotation and no arrow: infer from explicit returns.
                    self.ret_stack.push(None);
                    for b in body {
                        self.check_stmt(b);
                    }
                    self.ret_stack.pop();
                    self.pop();
                    s
                };
                Func(Box::new(sig))
            }
            Expr::If { cond, then, alt } => {
                let _ = self.check_expr(cond);
                let tt = self.block_type(then);
                let at = self.block_type(alt);
                if tt == at {
                    tt
                } else {
                    Any
                }
            }
            Expr::Await(inner) => {
                let _ = self.check_expr(inner);
                Any
            }
        }
    }

    fn fn_signature(&self, params: &[Param]) -> FuncSig {
        FuncSig::fixed(
            params
                .iter()
                .map(|p| p.ty.as_ref().map(resolve_type).unwrap_or(T::Any))
                .collect(),
            T::Any,
        )
    }

    /// Type of the value a statement block "produces" (last expression).
    fn block_type(&mut self, body: &[Stmt]) -> T {
        for s in body {
            self.check_stmt(s);
        }
        match body.last().map(|s| &s.kind) {
            Some(StmtKind::Expr(e)) => self.check_expr(e),
            _ => T::Nil,
        }
    }

    fn check_binary(&mut self, op: BinOp, l: &Expr, r: &Expr) -> T {
        use T::*;
        let lt = self.check_expr(l);
        let rt = self.check_expr(r);
        match op {
            BinOp::And | BinOp::Or => Bool,
            BinOp::Eq | BinOp::Ne => Bool,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                match (&lt, &rt) {
                    (Any, _)
                    | (_, Any)
                    | (Str, _)
                    | (_, Str)
                    | (Char, _)
                    | (_, Char)
                    | (Bool, Bool) => {}
                    (Int | Float, Int | Float) => {}
                    _ => self.err(format!(
                        "cannot compare `{}` with `{}`",
                        lt.describe(),
                        rt.describe()
                    )),
                }
                Bool
            }
            BinOp::Add => match (&lt, &rt) {
                (Any, _) | (_, Any) => Any,
                (Str, _) | (_, Str) | (Char, Char) => Str,
                (Int, Int) => Int,
                _ => Float,
            },
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => match (&lt, &rt) {
                (Any, _) | (_, Any) => Any,
                (Int, Int) => Int,
                _ => Float,
            },
        }
    }

    fn check_call(&mut self, callee: &Expr, args: &[ast::Arg]) -> T {
        use T::*;
        match callee {
            Expr::Member { base, name } => {
                if let Expr::Ident(m) = &**base {
                    if is_module(m) {
                        let full = format!("{m}.{name}");
                        if let Some(sig) = builtin_sig(&full) {
                            return self.apply_sig(&full, &sig, args);
                        }
                        // Unknown module member.
                        if builtin_sig(&full).is_none() {
                            for a in args {
                                let _ = self.check_expr(&a.value);
                            }
                            let _ = self.member_prop(&Any, name);
                            self.err(format!("module `{m}` has no member `{name}`"));
                            return Any;
                        }
                    }
                }
                let bt = self.check_expr(base);
                self.check_method_call(&bt, name, args)
            }
            Expr::Ident(name) => {
                if let Some(sig) = builtin_sig(name) {
                    return self.apply_sig(name, &sig, args);
                }
                if self.fns.contains_key(name) {
                    let sig = self.fns[name].clone();
                    return self.apply_sig(name, &sig, args);
                }
                match self.lookup(name) {
                    Some(Func(sig)) => self.apply_sig(name, &sig, args),
                    Some(t) => {
                        self.err(format!(
                            "`{name}` of type `{}` is not callable",
                            t.describe()
                        ));
                        for a in args {
                            let _ = self.check_expr(&a.value);
                        }
                        Any
                    }
                    None => {
                        if self.structs.contains_key(name) {
                            self.err(format!("`{name}` is a struct, not a function"));
                        } else if self.enums.contains_key(name) {
                            self.err(format!("`{name}` is an enum, not a function"));
                        } else {
                            self.err(format!("unknown function `{name}`"));
                        }
                        for a in args {
                            let _ = self.check_expr(&a.value);
                        }
                        Any
                    }
                }
            }
            other => {
                let ct = self.check_expr(other);
                match ct {
                    Func(sig) => self.apply_sig("<closure>", &sig, args),
                    Any => {
                        for a in args {
                            let _ = self.check_expr(&a.value);
                        }
                        Any
                    }
                    t => {
                        self.err(format!("`{}` is not callable", t.describe()));
                        for a in args {
                            let _ = self.check_expr(&a.value);
                        }
                        Any
                    }
                }
            }
        }
    }

    fn check_method_call(&mut self, base: &T, name: &str, args: &[ast::Arg]) -> T {
        use T::*;
        if let Some(sig) = method_sig(base, name) {
            return self.apply_sig(name, &sig, args);
        }
        match base {
            Any => {
                self.err(format!("type `any` has no method `{name}`"));
                Any
            }
            Enum(e) => {
                if let Some(en) = self.enums.get(e).cloned() {
                    match en.iter().find(|(n, _)| n == name) {
                        Some((_, pt)) => {
                            // Enum variant constructor: `Status.Failed("boom")`.
                            let expected: Option<T> = pt.clone();
                            match (expected, args.len()) {
                                (Some(pt), 1) => {
                                    let at = self.check_expr(&args[0].value);
                                    if !assignable(&pt, &at) {
                                        self.err(format!(
                                            "variant `{e}.{name}` expects `{}`, found `{}`",
                                            pt.describe(),
                                            at.describe()
                                        ));
                                    }
                                }
                                (Some(_), 0) => self.err(format!(
                                    "variant `{e}.{name}` requires a payload argument"
                                )),
                                (Some(_), n) => self.err(format!(
                                    "variant `{e}.{name}` expects a single payload, got {n} argument(s)"
                                )),
                                (None, 0) => {}
                                (None, n) => self.err(format!(
                                    "variant `{e}.{name}` takes no payload, got {n} argument(s)"
                                )),
                            }
                            Enum(e.to_string())
                        }
                        None => {
                            self.err(format!("enum `{e}` has no variant `{name}`"));
                            Any
                        }
                    }
                } else {
                    Any
                }
            }
            Struct(s) => {
                let field_t: Option<T> = self
                    .structs
                    .get(s)
                    .and_then(|fields| fields.iter().find(|(n, _)| n == name))
                    .map(|(_, t)| t.clone());
                if let Some(ft) = field_t {
                    // Field access that is being *called*: must be a function.
                    if let Func(sig) = ft {
                        return self.apply_sig(name, &sig, args);
                    }
                    self.err(format!("field `{s}.{name}` is not a function"));
                    return Any;
                }
                self.err(format!("struct `{s}` has no method `{name}`"));
                Any
            }
            other => {
                self.err(format!("`{}` has no method `{name}`", other.describe()));
                Any
            }
        }
    }

    fn apply_sig(&mut self, name: &str, sig: &FuncSig, args: &[ast::Arg]) -> T {
        let n = args.len();
        if !sig.variadic && n != sig.params.len() {
            self.err(format!(
                "`{name}` expects {} argument(s), got {n}",
                sig.params.len()
            ));
        } else if sig.variadic && n < sig.params.len() {
            self.err(format!(
                "`{name}` expects at least {} argument(s), got {n}",
                sig.params.len()
            ));
        }
        for (i, a) in args.iter().enumerate() {
            let at = self.check_expr(&a.value);
            let expect = if sig.variadic {
                sig.params.last().cloned().unwrap_or(T::Any)
            } else if i < sig.params.len() {
                sig.params[i].clone()
            } else {
                T::Any
            };
            if !assignable(&expect, &at) {
                self.err(format!(
                    "argument {i} of `{name}` expects `{}`, found `{}`",
                    expect.describe(),
                    at.describe()
                ));
            }
        }
        // `?`-style Result unwrapping helpers return the wrapped type.
        if matches!(sig.ret, T::Res(_)) && name.ends_with("?.unwrap_parent") {
            return T::Any;
        }
        sig.ret.clone()
    }

    fn module_prop(&mut self, module: &str, name: &str) -> T {
        use T::*;
        match (module, name) {
            ("math", "pi") => Float,
            ("os", "args") => Array(Box::new(Str)),
            ("Buffer", _) | ("Bytes", _) => Any,
            (_, _) => Any,
        }
    }

    fn member_prop(&mut self, base: &T, name: &str) -> T {
        use T::*;
        match base {
            Str => match name {
                "length" => Int,
                _ => Any,
            },
            Array(inner) => match name {
                "length" => Int,
                "sort" => Array(inner.clone()),
                "isEmpty" => Bool,
                _ => Any,
            },
            Map => match name {
                "keys" => Array(Box::new(Str)),
                "values" => Array(Box::new(Any)),
                "length" | "len" => Int,
                _ => Any, // stored map keys are dynamic
            },
            Opt(inner) => match name {
                "is_some" | "is_none" => Bool,
                "unwrap" => (**inner).clone(),
                _ => Any,
            },
            Res(inner) => match name {
                "is_ok" | "is_err" => Bool,
                "unwrap" => (**inner).clone(),
                "ok" | "err" => Opt(Box::new((**inner).clone())),
                _ => Any,
            },
            ProcResult => match name {
                "stdout" | "stderr" => Str,
                "status" => Int,
                "ok" => Bool,
                _ => Any,
            },
            Struct(s) => match self
                .structs
                .get(s)
                .and_then(|f| f.iter().find(|(n, _)| n == name))
            {
                Some((_, t)) => t.clone(),
                None => {
                    self.err(format!("struct `{s}` has no member `{name}`"));
                    Any
                }
            },
            Enum(e) => match self.enums.get(e) {
                Some(en) if en.iter().any(|(n, _)| n == name) => Enum(e.clone()),
                _ => Any,
            },
            Any | Func(_) | Nil => Any,
            other => {
                self.err(format!("`{}` has no member `{name}`", other.describe()));
                Any
            }
        }
    }

    fn check_ctor(&mut self, ty: Option<&str>, variant: &str, payload: Option<&Expr>) -> T {
        use T::*;
        if let Some(t) = ty {
            let en = self.enums.get(t).cloned();
            if let Some(en) = en {
                let payload_t: Option<T> = en
                    .iter()
                    .find(|(n, _)| n == variant)
                    .and_then(|(_, p)| p.clone());
                if let Some(pt) = payload_t {
                    if let Some(p) = payload {
                        let vt = self.check_expr(p);
                        if !assignable(&pt, &vt) {
                            self.err(format!(
                                "variant `{t}.{variant}` expects `{}`, found `{}`",
                                pt.describe(),
                                vt.describe()
                            ));
                        }
                    }
                    return Enum(t.to_string());
                }
                self.err(format!("enum `{t}` has no variant `{variant}`"));
                if let Some(p) = payload {
                    let _ = self.check_expr(p);
                }
                return Enum(t.to_string());
            }
            self.err(format!("unknown enum `{t}`"));
            return Any;
        }
        // Keyword constructors: Some/None/Ok/Err.
        match (variant, payload) {
            ("Some" | "Ok", Some(p)) => {
                let pt = self.check_expr(p);
                if variant == "Some" {
                    Opt(Box::new(pt))
                } else {
                    Res(Box::new(pt))
                }
            }
            ("Some" | "Ok", None) => {
                if variant == "Some" {
                    Opt(Box::new(Any))
                } else {
                    Res(Box::new(Any))
                }
            }
            ("None", None) => Opt(Box::new(Any)),
            ("None", Some(p)) => {
                let _ = self.check_expr(p);
                Opt(Box::new(Any))
            }
            ("Err", Some(p)) => Res(Box::new(self.check_expr(p))),
            ("Err", None) => Res(Box::new(Any)),
            _ => {
                self.err(format!("unknown variant `{variant}`"));
                Any
            }
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

/// Type of the values produced by `members` on a base type — fallback path.
pub fn check_program(program: &Program) -> Vec<String> {
    Checker::new().check(program)
}

/// Convenience wrapper used by lib/main.
pub fn typecheck(program: &Program) -> Result<(), Vec<String>> {
    let errors = check_program(program);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn method_sig(base: &T, name: &str) -> Option<FuncSig> {
    use T::*;
    match base {
        Str => match name {
            "trim" | "trim_start" | "trim_end" | "upper" | "lower" | "reverse" => {
                Some(FuncSig::fixed(vec![], Str))
            }
            "repeat" => Some(FuncSig::fixed(vec![Int], Str)),
            "replace" => Some(FuncSig::fixed(vec![Str, Str], Str)),
            "starts_with" | "ends_with" | "contains" => Some(FuncSig::fixed(vec![Str], Bool)),
            "char_at" => Some(FuncSig::fixed(vec![Int], Str)),
            "split" => Some(FuncSig::fixed(vec![Str], Array(Box::new(Str)))),
            "chars" => Some(FuncSig::fixed(vec![], Array(Box::new(Char)))),
            "len" | "length" => Some(FuncSig::fixed(vec![], Int)),
            // Generic string escape hatch.
            _ => None,
        },
        Array(_) => match name {
            "push" => Some(FuncSig::fixed(vec![Any], Nil)),
            "pop" => Some(FuncSig::fixed(vec![], Any)),
            "contains" => Some(FuncSig::fixed(vec![Any], Bool)),
            "join" => Some(FuncSig::fixed(vec![Str], Str)),
            "sort" => Some(FuncSig::fixed(vec![], Array(Box::new(Any)))),
            _ => None,
        },
        Opt(inner) => match name {
            "unwrap" => Some(FuncSig::fixed(vec![], (**inner).clone())),
            "unwrap_or" => Some(FuncSig::fixed(vec![(**inner).clone()], (**inner).clone())),
            "is_some" | "is_none" => Some(FuncSig::fixed(vec![], Bool)),
            _ => None,
        },
        Res(inner) => match name {
            "unwrap" => Some(FuncSig::fixed(vec![], (**inner).clone())),
            "ok" | "err" => Some(FuncSig::fixed(vec![], Opt(Box::new((**inner).clone())))),
            "is_ok" | "is_err" => Some(FuncSig::fixed(vec![], Bool)),
            _ => None,
        },
        Map => match name {
            "keys" => Some(FuncSig::fixed(vec![], Array(Box::new(Str)))),
            "values" => Some(FuncSig::fixed(vec![], Array(Box::new(Any)))),
            "len" | "length" => Some(FuncSig::fixed(vec![], Int)),
            "get" => Some(FuncSig::fixed(vec![Str], Any)),
            "set" => Some(FuncSig::fixed(vec![Str, Any], Nil)),
            _ => None,
        },
        ProcResult => match name {
            "stdout" | "stderr" => Some(FuncSig::fixed(vec![], Str)),
            "status" => Some(FuncSig::fixed(vec![], Int)),
            "ok" => Some(FuncSig::fixed(vec![], Bool)),
            _ => None,
        },
        Any | Func(_) | Enum(_) | Struct(_) | Nil => None,
        _ => None,
    }
}

/// Turn a Vec of error strings into a single error value (used by lib).
pub fn to_error(errors: Vec<String>, file: &str) -> UsglError {
    let joined = format!("{}:\n  {}", file, errors.join("\n  "));
    UsglError::Type { message: joined }
}
