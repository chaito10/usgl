use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use crate::ast::Param;
use crate::env::Env;

/// Runtime value.
#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Char(char),
    Str(Rc<str>),
    Bytes(Rc<Vec<u8>>),
    Array(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<Vec<(String, Value)>>>),
    Struct(Rc<RefCell<StructData>>),
    Enum {
        ty: Option<String>,
        variant: String,
        payload: Option<Box<Value>>,
    },
    Opt(OptV),
    Res(Res),
    Range {
        start: i64,
        end: i64,
    },
    Function(Rc<Function>),
    Module(Rc<Env>),
    File(Rc<RefCell<UsglFile>>),
    ProcResult(Rc<ProcResult>),
    TypeInfo {
        name: String,
        kind: TypeKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone)]
pub struct StructData {
    pub name: String,
    pub fields: Vec<(String, Value)>,
}

#[derive(Clone)]
pub enum OptV {
    None,
    Some(Box<Value>),
}

impl PartialEq for OptV {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (OptV::None, OptV::None) => true,
            (OptV::Some(a), OptV::Some(b)) => values_equal(a, b),
            _ => false,
        }
    }
}

#[derive(Clone)]
pub enum Res {
    Ok(Box<Value>),
    Err(Box<Value>),
}

impl PartialEq for Res {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Res::Ok(a), Res::Ok(b)) => values_equal(a, b),
            (Res::Err(a), Res::Err(b)) => values_equal(a, b),
            _ => false,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        values_equal(self, other)
    }
}

/// A file opened through `fs.open`.
pub struct UsglFile {
    pub path: String,
    pub handle: Option<std::fs::File>,
    pub writable: bool,
}

impl Drop for UsglFile {
    fn drop(&mut self) {
        // std::fs::File closes itself on drop; this is the deterministic
        // resource cleanup promised by the RFC ("file automatically closed").
        self.handle.take();
    }
}

#[derive(Debug, Clone)]
pub struct ProcResult {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
}

pub struct Function {
    pub kind: FuncKind,
    pub name: Option<String>,
}

pub enum FuncKind {
    Closure(Rc<ClosureData>),
    Builtin(&'static str),
}

pub struct ClosureData {
    pub params: Vec<Param>,
    pub arrow: Option<crate::ast::Expr>,
    pub body: Vec<crate::ast::Stmt>,
    pub env: Rc<Env>,
}

/// Human-facing rendering used by `println` / string concatenation.
pub fn display(v: &Value) -> String {
    match v {
        Value::Nil => String::new(),
        Value::Bool(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format_float(*f),
        Value::Char(c) => c.to_string(),
        Value::Str(s) => s.to_string(),
        Value::Bytes(b) => format!("Bytes[{}]", b.len()),
        Value::Array(a) => {
            let a = a.borrow();
            let mut out = String::from("[");
            for (i, v) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&display(v));
            }
            out.push(']');
            out
        }
        Value::Map(m) => {
            let m = m.borrow();
            let mut out = String::from("{ ");
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{}: {}", k, display(v));
            }
            if !m.is_empty() {
                out.push(' ');
            }
            out.push('}');
            out
        }
        Value::Struct(s) => {
            let s = s.borrow();
            let mut out = format!("{} {{ ", s.name);
            for (i, (k, v)) in s.fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "{}: {}", k, display(v));
            }
            if !s.fields.is_empty() {
                out.push(' ');
            }
            out.push('}');
            out
        }
        Value::Enum {
            ty,
            variant,
            payload,
        } => {
            let mut out = String::new();
            if let Some(t) = ty {
                out.push_str(t);
                out.push('.');
            }
            out.push_str(variant);
            if let Some(p) = payload {
                let _ = write!(out, "({})", display(p));
            }
            out
        }
        Value::Opt(o) => match o {
            OptV::None => "None".into(),
            OptV::Some(v) => format!("Some({})", display(v)),
        },
        Value::Res(r) => match r {
            Res::Ok(v) => format!("Ok({})", display(v)),
            Res::Err(v) => format!("Err({})", display(v)),
        },
        Value::Range { start, end } => format!("{}..{}", start, end),
        Value::Function(f) => match &f.name {
            Some(n) => format!("<function {}>", n),
            None => "<function>".into(),
        },
        Value::Module(m) => format!("<module {}>", m.name()),
        Value::File(f) => format!("<file {}>", f.borrow().path),
        Value::ProcResult(p) => format!("<process status={}>", p.status),
        Value::TypeInfo { name, .. } => format!("<type {}>", name),
    }
}

/// Debug-ish representation including quotes for strings.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("{:?}", s),
        _ => display(v),
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.type_name())
    }
}

impl Value {
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }

    /// Truthiness used by `if`/`while`/`&&`/`||`/`!`.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            _ => true,
        }
    }

    /// Stable machine-readable type name used by `type_of`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Char(_) => "char",
            Value::Str(_) => "string",
            Value::Bytes(_) => "bytes",
            Value::Array(_) => "array",
            Value::Map(_) => "map",
            Value::Struct(_) => "struct",
            Value::Enum { .. } => "enum",
            Value::Opt(_) => "option",
            Value::Res(_) => "result",
            Value::Range { .. } => "range",
            Value::Function(_) => "function",
            Value::Module(_) => "module",
            Value::File(_) => "file",
            Value::ProcResult(_) => "process_result",
            Value::TypeInfo { .. } => "type",
        }
    }
}

pub fn format_float(f: f64) -> String {
    if f == f.trunc() && f.is_finite() && f.abs() < 1e15 {
        format!("{:.1}", f)
    } else {
        format!("{}", f)
    }
}

/// Structural equality used by `==`.
pub fn values_equal(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Nil, Nil) => true,
        (Bool(x), Bool(y)) => x == y,
        (Int(x), Int(y)) => x == y,
        (Float(x), Float(y)) => x == y,
        (Int(_), Float(_)) => (as_f64(a) - as_f64(b)).abs() < 1e-9,
        (Float(_), Int(_)) => (as_f64(a) - as_f64(b)).abs() < 1e-9,
        (Char(x), Char(y)) => x == y,
        (Str(x), Str(y)) => x == y,
        (Bytes(x), Bytes(y)) => x == y,
        (Array(x), Array(y)) => {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Map(x), Map(y)) => {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.iter().any(|(k2, v2)| k == k2 && values_equal(v, v2)))
        }
        (Struct(x), Struct(y)) => {
            let (x, y) = (x.borrow(), y.borrow());
            x.name == y.name
                && x.fields.len() == y.fields.len()
                && x.fields.iter().all(|(k, v)| {
                    y.fields
                        .iter()
                        .any(|(k2, v2)| k == k2 && values_equal(v, v2))
                })
        }
        (
            Enum {
                ty: t1,
                variant: v1,
                payload: p1,
            },
            Enum {
                ty: t2,
                variant: v2,
                payload: p2,
            },
        ) => {
            t1 == t2
                && v1 == v2
                && p1
                    .as_ref()
                    .zip(p2.as_ref())
                    .map_or(true, |(a, b)| values_equal(a, b))
        }
        (Opt(o1), Opt(o2)) => match (o1, o2) {
            (OptV::None, OptV::None) => true,
            (OptV::Some(a), OptV::Some(b)) => values_equal(a, b),
            _ => false,
        },
        (Res(r1), Res(r2)) => match (r1, r2) {
            (crate::value::Res::Ok(a), crate::value::Res::Ok(b)) => values_equal(a, b),
            (crate::value::Res::Err(a), crate::value::Res::Err(b)) => values_equal(a, b),
            _ => false,
        },
        (Range { start: s1, end: e1 }, Range { start: s2, end: e2 }) => s1 == s2 && e1 == e2,
        _ => false,
    }
}

/// Ordering used by `<` / `>` / sort.
pub fn values_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    use crate::value::Value::*;
    match (a, b) {
        (Int(x), Int(y)) => Some(x.cmp(y)),
        (Float(x), Float(y)) => x.partial_cmp(y),
        (Int(x), Float(y)) => (*x as f64).partial_cmp(y),
        (Float(x), Int(y)) => x.partial_cmp(&(*y as f64)),
        (Str(x), Str(y)) => Some(x.cmp(y)),
        (Bool(x), Bool(y)) => Some(x.cmp(y)),
        (Char(x), Char(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn as_f64(v: &Value) -> f64 {
    match v {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

/// Extract a human-readable error message from an error payload value.
pub fn err_message(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        Value::Struct(_) => display(v),
        other => display(other),
    }
}
