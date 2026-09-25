use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::ast::{self, *};
use crate::env::Env;
use crate::error::{RtResult, UsglError};
use crate::value::*;

/// A value plus an optional argument name (supports `json.stringify(x, pretty=true)`).
pub type Arg = (Option<String>, Value);

fn sort_values(v: &mut Vec<Value>) {
    v.sort_by(|a, b| values_cmp(a, b).unwrap_or(std::cmp::Ordering::Equal));
}

/// Control-flow signals that cross expression/statement boundaries.
#[derive(Debug)]
pub enum Ctrl {
    Return(Value),
    Break,
    Continue,
    /// The error payload carried by a `?` propagation.
    Propagate(Value),
}

pub enum Abort {
    Err(UsglError),
    Ctrl(Ctrl),
}

impl From<UsglError> for Abort {
    fn from(e: UsglError) -> Self {
        Abort::Err(e)
    }
}

impl From<Abort> for UsglError {
    fn from(a: Abort) -> Self {
        match a {
            Abort::Err(e) => e,
            Abort::Ctrl(c) => UsglError::rt(
                match c {
                    Ctrl::Propagate(e) => err_message(&e),
                    Ctrl::Return(_) => "`return` outside of a function".to_string(),
                    Ctrl::Break => "`break` outside of a loop".to_string(),
                    Ctrl::Continue => "`continue` outside of a loop".to_string(),
                },
                None,
            ),
        }
    }
}

pub type EResult<T> = Result<T, Abort>;

fn rt_err(msg: impl Into<String>) -> Abort {
    Abort::Err(UsglError::rt(msg.into(), None))
}

pub struct Interp {
    pub global: Rc<Env>,
    pub file: String,
    pub out: Rc<RefCell<Vec<u8>>>,
    pub modules: HashMap<String, Rc<Env>>,
    pub cli_args: Vec<String>,
}

impl Interp {
    pub fn new(file: &str) -> Self {
        Self::with_out(file, Rc::new(RefCell::new(Vec::new())))
    }

    pub fn with_out(file: &str, out: Rc<RefCell<Vec<u8>>>) -> Self {
        let global = Env::new(None, Some("global".to_string()));
        let mut interp = Interp {
            global: global.clone(),
            file: file.to_string(),
            out,
            modules: HashMap::new(),
            cli_args: Vec::new(),
        };
        crate::builtins::install(&global, &mut interp);
        interp
    }

    pub fn drain_out(&self) -> Vec<u8> {
        let mut buf = self.out.borrow_mut();
        std::mem::take(&mut *buf)
    }

    pub fn write_out(&self, s: &str) {
        self.out.borrow_mut().extend_from_slice(s.as_bytes());
    }

    pub fn file_dir(&self) -> String {
        match Path::new(&self.file).parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().to_string(),
            _ => ".".to_string(),
        }
    }

    // ================= high-level =================

    /// Run the top-level statements of a program in the given environment.
    pub fn exec_in_env(&mut self, program: &Program, env: &Rc<Env>) -> EResult<()> {
        for stmt in &program.stmts {
            self.stmt(stmt, env)?;
        }
        Ok(())
    }

    /// Execute a program: top-level statements, then `fn main` if requested.
    /// Returns an exit code.
    pub fn run(&mut self, program: &Program, run_main: bool) -> RtResult<i32> {
        match self.stmt_run(program, run_main) {
            Ok(code) => Ok(code),
            Err(Abort::Err(e)) => Err(e),
            Err(Abort::Ctrl(c)) => Err(match c {
                Ctrl::Propagate(e) => UsglError::rt(err_message(&e), None),
                Ctrl::Return(_) => UsglError::rt("`return` outside of a function", None),
                Ctrl::Break => UsglError::rt("`break` outside of a loop", None),
                Ctrl::Continue => UsglError::rt("`continue` outside of a loop", None),
            }),
        }
    }

    fn stmt_run(&mut self, program: &Program, run_main: bool) -> EResult<i32> {
        let global = self.global.clone();
        self.exec_in_env(program, &global)?;

        if !run_main {
            return Ok(0);
        }

        if let Some(main) = ast::find_main(program) {
            let mainfn = self.closure_from_decl(main, &self.global)?;
            let value = self.call_value(&Value::Function(mainfn), Vec::new())?;
            if let Value::Res(Res::Err(e)) = value {
                self.write_out(&format!("error: {}\n", err_message(&e)));
                return Ok(1);
            }
        }
        Ok(0)
    }

    // ================= statements =================

    pub fn stmt(&mut self, s: &Stmt, env: &Rc<Env>) -> EResult<Value> {
        match &s.kind {
            StmtKind::Let { name, mutable, value, .. } => {
                let v = self.expr(value, env)?;
                env.define(name, v, *mutable).map_err(Abort::Err)?;
                Ok(Value::Nil)
            }
            StmtKind::Expr(e) => self.expr(e, env),
            StmtKind::Fn { .. } => {
                let f = self.closure_from_decl(s, env)?;
                let name = match &s.kind {
                    StmtKind::Fn { name, .. } => name.clone(),
                    _ => unreachable!(),
                };
                env.define(&name, Value::Function(f), false).map_err(Abort::Err)?;
                Ok(Value::Nil)
            }
            StmtKind::Return(v) => {
                let val = match v {
                    Some(e) => self.expr(e, env)?,
                    None => Value::Nil,
                };
                Err(Abort::Ctrl(Ctrl::Return(val)))
            }
            StmtKind::If { cond, then, alt } => {
                let c = self.expr(cond, env)?;
                if c.is_truthy() {
                    self.block(then, env)
                } else if !alt.is_empty() {
                    self.block(alt, env)
                } else {
                    Ok(Value::Nil)
                }
            }
            StmtKind::While { cond, body } => {
                let scope = Env::new(Some(env.clone()), Some("<while>".to_string()));
                loop {
                    let c = self.expr(cond, &scope)?;
                    if !c.is_truthy() {
                        return Ok(Value::Nil);
                    }
                    match self.run_loop_body(body, &scope) {
                        Ok(()) => {}
                        Err(Abort::Ctrl(Ctrl::Break)) => return Ok(Value::Nil),
                        Err(abort) => return Err(abort),
                    }
                }
            }
            StmtKind::For { name, iter, body } => {
                let iterable = self.expr(iter, env)?;
                let items = self.iter_values(iterable)?;
                for item in items {
                    let scope = Env::new(Some(env.clone()), Some("<for>".to_string()));
                    scope.define(name, item, false).map_err(Abort::Err)?;
                    match self.run_loop_body(body, &scope) {
                        Ok(()) => {}
                        Err(Abort::Ctrl(Ctrl::Break)) => return Ok(Value::Nil),
                        Err(abort) => return Err(abort),
                    }
                }
                Ok(Value::Nil)
            }
            StmtKind::Loop { body } => {
                let scope = Env::new(Some(env.clone()), Some("<loop>".to_string()));
                loop {
                    match self.run_loop_body(body, &scope) {
                        Ok(()) => {}
                        Err(Abort::Ctrl(Ctrl::Break)) => return Ok(Value::Nil),
                        Err(abort) => return Err(abort),
                    }
                }
            }
            StmtKind::Match { expr, arms } => {
                let value = self.expr(expr, env)?;
                for arm in arms {
                    if let Some(bindings) = try_match(&arm.pat, &value) {
                        let scope = Env::new(Some(env.clone()), Some("<match>".to_string()));
                        for (bname, bval) in bindings {
                            scope.define(&bname, bval, false).map_err(Abort::Err)?;
                        }
                        return self.block(&arm.body, &scope);
                    }
                }
                Err(rt_err(format!(
                    "no match arm matched for value `{}`",
                    display(&value)
                )))
            }
            StmtKind::Struct { name, .. } => {
                env.define(
                    name,
                    Value::TypeInfo { name: name.clone(), kind: TypeKind::Struct },
                    false,
                )
                .map_err(Abort::Err)?;
                Ok(Value::Nil)
            }
            StmtKind::Enum { name, .. } => {
                env.define(
                    name,
                    Value::TypeInfo { name: name.clone(), kind: TypeKind::Enum },
                    false,
                )
                .map_err(Abort::Err)?;
                Ok(Value::Nil)
            }
            StmtKind::Import(parts) => {
                let module = self.resolve_module(parts, env)?;
                let bind_name = parts.last().cloned().unwrap_or_default();
                env.define(&bind_name, module, false).map_err(Abort::Err)?;
                Ok(Value::Nil)
            }
            StmtKind::Test { .. } => Ok(Value::Nil),
            StmtKind::Break => Err(Abort::Ctrl(Ctrl::Break)),
            StmtKind::Continue => Err(Abort::Ctrl(Ctrl::Continue)),
            StmtKind::Block { body, .. } => self.block(body, env),
        }
    }

    fn run_loop_body(&mut self, body: &[Stmt], env: &Rc<Env>) -> EResult<()> {
        match self.block(body, env) {
            Ok(_) => Ok(()),
            Err(Abort::Ctrl(Ctrl::Continue)) => Ok(()),
            other => other.map(|_| ()),
        }
    }

    fn block(&mut self, stmts: &[Stmt], env: &Rc<Env>) -> EResult<Value> {
        let scope = Env::new(Some(env.clone()), Some("<block>".to_string()));
        let mut last = Value::Nil;
        for s in stmts {
            last = self.stmt(s, &scope)?;
        }
        Ok(last)
    }

    fn closure_from_decl(&self, s: &Stmt, env: &Rc<Env>) -> EResult<Rc<Function>> {
        match &s.kind {
            StmtKind::Fn { name, params, body, is_async: _, .. } => {
                let data = Rc::new(ClosureData {
                    params: params.clone(),
                    arrow: None,
                    body: body.clone(),
                    env: env.clone(),
                });
                Ok(Rc::new(Function {
                    kind: FuncKind::Closure(data),
                    name: Some(name.clone()),
                }))
            }
            _ => Err(rt_err("not a function declaration")),
        }
    }

    fn iter_values(&self, v: Value) -> EResult<Vec<Value>> {
        match v {
            Value::Array(a) => Ok(a.borrow().clone()),
            Value::Str(s) => Ok(s.chars().map(|c| Value::Str(Rc::from(c.to_string()))).collect()),
            Value::Range { start, end } => {
                if start >= end {
                    Ok(Vec::new())
                } else {
                    Ok((start..end).map(Value::Int).collect())
                }
            }
            Value::Map(m) => Ok(m.borrow().iter().map(|(k, _)| Value::Str(Rc::from(k.as_str()))).collect()),
            other => Err(rt_err(format!(
                "cannot iterate over `{}`",
                other.type_name()
            ))),
        }
    }

    // ================= module resolution =================

    fn resolve_module(&mut self, parts: &[String], env: &Rc<Env>) -> EResult<Value> {
        let full = parts.join(".");
        if let Some(m) = self.modules.get(&full) {
            return Ok(Value::Module(m.clone()));
        }
        // std::fs access happens before evaluating the module so we can import
        // files next to the importing source.
        let path = Path::new(&self.file_dir()).join(parts.join("/")).with_extension("us");
        if path.exists() {
            let source = std::fs::read_to_string(&path).map_err(|e| {
                rt_err(format!("cannot read module `{}`: {}", full, e))
            })?;
            let program = crate::parser::parse(&source, &path.to_string_lossy())
                .map_err(Abort::Err)?;
            let root = Env::new(None, Some(full.clone()));
            self.modules.insert(full.clone(), root.clone());
            let prev_file = self.file.clone();
            self.file = path.to_string_lossy().to_string();
            self.exec_in_env(&program, &root)?;
            self.file = prev_file;
            return Ok(Value::Module(root));
        }
        let _ = env;
        Err(rt_err(format!(
            "module `{}` not found (Phase 1 provides: fs, json, math, strings, time, os, process, Buffer, Bytes)",
            full
        )))
    }

    // ================= expressions =================

    pub fn expr(&mut self, e: &Expr, env: &Rc<Env>) -> EResult<Value> {
        match e {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Str(s) => Ok(Value::Str(Rc::from(s.as_str()))),
            Expr::Char(c) => Ok(Value::Char(*c)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Ident(name) => env
                .get(name)
                .ok_or_else(|| rt_err(format!("unknown variable `{}`", name))),
            Expr::Unary { op, e } => {
                let v = self.expr(e, env)?;
                match op {
                    '-' => match v {
                        Value::Int(i) => Ok(Value::Int(i.wrapping_neg())),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        other => Err(rt_err(format!("cannot negate `{}`", other.type_name()))),
                    },
                    '!' => Ok(Value::Bool(!v.is_truthy())),
                    _ => Err(rt_err("unsupported unary operator")),
                }
            }
            Expr::Binary { op, l, r } => self.binary(*op, l, r, env),
            Expr::Assign { target, value } => {
                let v = self.expr(value, env)?;
                match &**target {
                    Expr::Ident(name) => {
                        env.assign(name, v.clone()).map_err(Abort::Err)?;
                        Ok(v)
                    }
                    Expr::Member { base, name } => {
                        let basev = self.expr(base, env)?;
                        let mut slot = None;
                        match &basev {
                            Value::Struct(s) => {
                                let mut s = s.borrow_mut();
                                if let Some((_, fv)) = s.fields.iter_mut().find(|(k, _)| k == name) {
                                    *fv = v.clone();
                                    slot = Some(());
                                }
                            }
                            Value::Map(m) => {
                                let mut m = m.borrow_mut();
                                if let Some((_, fv)) = m.iter_mut().find(|(k, _)| k == name) {
                                    *fv = v.clone();
                                    slot = Some(());
                                } else {
                                    m.push((name.clone(), v.clone()));
                                    slot = Some(());
                                }
                            }
                            other => {
                                return Err(rt_err(format!(
                                    "cannot assign to member of `{}`",
                                    other.type_name()
                                )))
                            }
                        }
                        if slot.is_some() {
                            Ok(v)
                        } else {
                            Err(rt_err(format!("type has no member `{}` (missing field target)", name)))
                        }
                    }
                    Expr::Index { base, index } => {
                        let basev = self.expr(base, env)?;
                        let idx = self.expr(index, env)?;
                        match &basev {
                            Value::Array(a) => {
                                let mut a = a.borrow_mut();
                                let i = as_int(&idx)?;
                                if i < 0 || i as usize >= a.len() {
                                    return Err(rt_err(format!("array index {} out of bounds", i)));
                                }
                                a[i as usize] = v.clone();
                                Ok(v)
                            }
                            Value::Map(m) => {
                                let mut m = m.borrow_mut();
                                let key = display(&idx);
                                if let Some((_, fv)) = m.iter_mut().find(|(k, _)| k == &key) {
                                    *fv = v.clone();
                                } else {
                                    m.push((key, v.clone()));
                                }
                                Ok(v)
                            }
                            Value::Bytes(b) => {
                                let mut b = b.as_ref().clone();
                                let i = as_int(&idx)?;
                                if i < 0 || i as usize >= b.len() {
                                    return Err(rt_err(format!("Bytes index {} out of bounds", i)));
                                }
                                let byte = as_int(&v)? as u8;
                                b[i as usize] = byte;
                                Ok(Value::Bytes(Rc::new(b)))
                            }
                            other => Err(rt_err(format!(
                                "cannot index-assign into `{}`",
                                other.type_name()
                            ))),
                        }
                    }
                    _ => Err(rt_err("invalid assignment target")),
                }
            }
            Expr::Call { callee, args, .. } => {
                let args = self.eval_args(args, env)?;
                self.call_expr(callee, args, env)
            }
            Expr::Member { base, name } => {
                let basev = self.expr(base, env)?;
                self.read_member(basev, name)
            }
            Expr::Index { base, index } => {
                let basev = self.expr(base, env)?;
                let idx = self.expr(index, env)?;
                self.index_value(basev, idx)
            }
            Expr::ErrProp(inner) => {
                let v = self.expr(inner, env)?;
                match v {
                    Value::Res(Res::Ok(x)) => Ok(*x),
                    Value::Res(Res::Err(e)) => Err(Abort::Ctrl(Ctrl::Propagate(*e))),
                    other => Err(rt_err(format!(
                        "`?` used on `{}`; it only unwraps Result values",
                        other.type_name()
                    ))),
                }
            }
            Expr::Array(items) => {
                let mut vals = Vec::with_capacity(items.len());
                for it in items {
                    vals.push(self.expr(it, env)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(vals))))
            }
            Expr::Map(fields) => {
                let mut vals = Vec::with_capacity(fields.len());
                for (k, v) in fields {
                    vals.push((k.clone(), self.expr(v, env)?));
                }
                Ok(Value::Map(Rc::new(RefCell::new(vals))))
            }
            Expr::StructLit { ty, fields } => {
                let mut vals = Vec::with_capacity(fields.len());
                for (k, v) in fields {
                    vals.push((k.clone(), self.expr(v, env)?));
                }
                Ok(Value::Struct(Rc::new(RefCell::new(StructData {
                    name: ty.clone(),
                    fields: vals,
                }))))
            }
            Expr::Ctor { ty, variant, payload } => {
                let payload = match payload {
                    Some(p) => Some(Box::new(self.expr(p, env)?)),
                    None => None,
                };
                match variant.as_str() {
                    "Some" => Ok(Value::Opt(OptV::Some(Box::new(*payload.unwrap_or(Box::new(Value::Nil)))))),
                    "None" => Ok(Value::Opt(OptV::None)),
                    "Ok" => Ok(Value::Res(Res::Ok(payload.unwrap_or(Box::new(Value::Nil))))),
                    "Err" => Ok(Value::Res(Res::Err(payload.unwrap_or(Box::new(Value::Nil))))),
                    _ => Ok(Value::Enum { ty: ty.clone(), variant: variant.clone(), payload }),
                }
            }
            Expr::Range(l, r) => {
                let s = as_int(&self.expr(l, env)?)?;
                let e = as_int(&self.expr(r, env)?)?;
                Ok(Value::Range { start: s, end: e })
            }
            Expr::Fn { params, arrow, body } => {
                let data = Rc::new(ClosureData {
                    params: params.clone(),
                    arrow: arrow.as_ref().map(|b| (**b).clone()),
                    body: body.clone(),
                    env: env.clone(),
                });
                Ok(Value::Function(Rc::new(Function { kind: FuncKind::Closure(data), name: None })))
            }
            Expr::Await(inner) => self.expr(inner, env),
            Expr::If { cond, then, alt } => {
                let c = self.expr(cond, env)?;
                if c.is_truthy() {
                    self.block(then, env)
                } else if !alt.is_empty() {
                    self.block(alt, env)
                } else {
                    Ok(Value::Nil)
                }
            }
        }
    }

    fn binary(&mut self, op: BinOp, l: &Expr, r: &Expr, env: &Rc<Env>) -> EResult<Value> {
        use BinOp::*;
        match op {
            And => {
                let lv = self.expr(l, env)?;
                if !lv.is_truthy() {
                    return Ok(Value::Bool(false));
                }
                return Ok(Value::Bool(self.expr(r, env)?.is_truthy()));
            }
            Or => {
                let lv = self.expr(l, env)?;
                if lv.is_truthy() {
                    return Ok(Value::Bool(true));
                }
                return Ok(Value::Bool(self.expr(r, env)?.is_truthy()));
            }
            _ => {}
        }
        let lv = self.expr(l, env)?;
        let rv = self.expr(r, env)?;
        match op {
            Add => add_values(&lv, &rv),
            Sub => num_bin(&lv, &rv, |a, b| a - b, |a, b| a - b),
            Mul => num_bin(&lv, &rv, |a, b| a.checked_mul(b).unwrap_or(0), |a, b| a * b),
            Div => {
                if is_zero(&rv) {
                    return Err(rt_err("division by zero"));
                }
                num_bin(&lv, &rv, |a, b| a.wrapping_div(b), |a, b| a / b)
            }
            Mod => {
                if is_zero(&rv) {
                    return Err(rt_err("modulo by zero"));
                }
                num_bin(&lv, &rv, |a, b| a.wrapping_rem(b), |a, b| a % b)
            }
            Eq => Ok(Value::Bool(values_equal(&lv, &rv))),
            Ne => Ok(Value::Bool(!values_equal(&lv, &rv))),
            Lt | Le | Gt | Ge => {
                let ord = values_cmp(&lv, &rv)
                    .ok_or_else(|| rt_err(format!("cannot order `{}` and `{}`", lv.type_name(), rv.type_name())))?;
                let b = match op {
                    Lt => ord.is_lt(),
                    Le => ord.is_le(),
                    Gt => ord.is_gt(),
                    Ge => ord.is_ge(),
                    _ => unreachable!(),
                };
                Ok(Value::Bool(b))
            }
            And | Or => unreachable!(),
        }
    }

    fn eval_args(&mut self, args: &[ast::Arg], env: &Rc<Env>) -> EResult<Vec<Arg>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let v = self.expr(&a.value, env)?;
            out.push((a.name.clone(), v));
        }
        Ok(out)
    }

    fn call_expr(&mut self, callee: &Expr, args: Vec<Arg>, env: &Rc<Env>) -> EResult<Value> {
        // Method call: receiver.name(...)
        if let Expr::Member { base, name } = callee {
            let basev = self.expr(base, env)?;
            return self.dispatch_call(basev, name, args);
        }
        let calv = self.expr(callee, env)?;
        self.call_value(&calv, args)
    }

    /// Decide how `base.name(args)` is evaluated.
    fn dispatch_call(&mut self, base: Value, name: &str, args: Vec<Arg>) -> EResult<Value> {
        match &base {
            Value::Map(m) => {
                // A stored value shadows built-in methods.
                if m.borrow().iter().any(|(k, _)| k == name) {
                    let member = self.read_member(base.clone(), name)?;
                    return self.call_value(&member, args);
                }
crate::builtins::call_method(&base, name, self, args).map_err(Abort::Err)
            }
            Value::Struct(s) => {
                let field = s
                    .borrow()
                    .fields
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| rt_err(format!("struct has no field `{}`", name)))?;
                self.call_value(&field, args)
            }
            Value::Module(m) => {
                let member = m
                    .get(name)
                    .ok_or_else(|| rt_err(format!("module `{}` has no member `{}`", m.name(), name)))?;
                self.call_value(&member, args)
            }
            Value::TypeInfo { name: ty, kind: TypeKind::Enum } => {
                if args.len() > 1 {
                    return Err(rt_err(format!("enum variant `{}.{}` takes at most one payload", ty, name)));
                }
                let payload = args.into_iter().next().map(|(_, v)| Box::new(v));
                Ok(Value::Enum { ty: Some(ty.clone()), variant: name.to_string(), payload })
            }
            Value::TypeInfo { name: ty, kind: TypeKind::Struct } => Err(rt_err(format!(
                "`{}` is a struct type; construct values with `{} {{ ... }}`",
                ty, ty
            ))),
            _ => crate::builtins::call_method(&base, name, self, args).map_err(Abort::Err),
        }
    }

    pub fn call_value(&mut self, callee: &Value, args: Vec<Arg>) -> EResult<Value> {
        match callee {
            Value::Function(f) => match &f.kind {
                FuncKind::Builtin(name) => crate::builtins::call_builtin(name, self, args).map_err(Abort::Err),
                FuncKind::Closure(c) => self.call_closure(f, c, args),
            },
            Value::Module(m) => {
                if args.is_empty() {
                    Err(rt_err(format!("module `{}` is not callable", m.name())))
                } else {
                    Err(rt_err(format!("module `{}` is not callable", m.name())))
                }
            }
            role => Err(rt_err(format!("`{}` is not callable", role.type_name()))),
        }
    }

    fn call_closure(
        &mut self,
        f: &Rc<Function>,
        c: &Rc<ClosureData>,
        args: Vec<Arg>,
    ) -> EResult<Value> {
        let scope = Env::new(Some(c.env.clone()), Some(f.name.as_deref().unwrap_or("<anon>").to_string()));

        let mut positional = Vec::new();
        let mut named: HashMap<String, Value> = HashMap::new();
        for (n, v) in args {
            match n {
                Some(n) => {
                    named.insert(n, v);
                }
                None => positional.push(v),
            }
        }
        if positional.len() > c.params.len() {
            let name = f.name.as_deref().unwrap_or("<anonymous>");
            return Err(rt_err(format!(
                "too many arguments to function `{}` (got {}, expected {})",
                name,
                positional.len(),
                c.params.len()
            )));
        }

        for (i, p) in c.params.iter().enumerate() {
            let value = if i < positional.len() {
                positional[i].clone()
            } else if let Some(v) = named.remove(&p.name) {
                v
            } else {
                let name = f.name.as_deref().unwrap_or("<anonymous>");
                return Err(rt_err(format!(
                    "missing argument `{}` in call to `{}`",
                    p.name, name
                )));
            };
            scope.define(&p.name, value, false).map_err(Abort::Err)?;
        }
        if let Some(extra) = named.keys().next() {
            let name = f.name.as_deref().unwrap_or("<anonymous>");
            return Err(rt_err(format!("unknown argument `{}` in call to `{}`", extra, name)));
        }

        if let Some(arrow) = &c.arrow {
            return self.expr(arrow, &scope);
        }
        match self.block(&c.body, &scope) {
            Ok(v) => Ok(v),
            Err(Abort::Ctrl(Ctrl::Return(v))) => Ok(v),
            Err(other) => Err(other),
        }
    }

    fn read_member(&mut self, base: Value, name: &str) -> EResult<Value> {
        match &base {
            Value::Module(m) => m
                .get(name)
                .ok_or_else(|| rt_err(format!("module `{}` has no member `{}`", m.name(), name))),
            Value::Struct(s) => s
                .borrow()
                .fields
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| rt_err(format!("struct has no field `{}`", name))),
            Value::Map(m) => {
                let m = m.borrow();
                if let Some((_, v)) = m.iter().find(|(k, _)| k == name) {
                    return Ok(v.clone());
                }
                match name {
                    "keys" => Ok(Value::Array(Rc::new(RefCell::new(
                        m.iter().map(|(k, _)| Value::Str(Rc::from(k.as_str()))).collect(),
                    )))),
                    "values" => Ok(Value::Array(Rc::new(RefCell::new(
                        m.iter().map(|(_, v)| v.clone()).collect(),
                    )))),
                    "len" | "length" => Ok(Value::Int(m.len() as i64)),
                    _ => Err(rt_err(format!("map has no key `{}`", name))),
                }
            }
            Value::Str(s) => match name {
                "length" => Ok(Value::Int(s.chars().count() as i64)),
                _ => Err(rt_err(format!("String has no member `{}`", name))),
            },
            Value::Array(a) => match name {
                "length" => Ok(Value::Int(a.borrow().len() as i64)),
                "sort" => {
                    let mut v = a.borrow().clone();
                    sort_values(&mut v);
                    Ok(Value::Array(Rc::new(RefCell::new(v))))
                }
                "isEmpty" => Ok(Value::Bool(a.borrow().is_empty())),
                _ => Err(rt_err(format!("Array has no member `{}`", name))),
            },
            Value::Bytes(b) => match name {
                "length" => Ok(Value::Int(b.len() as i64)),
                _ => Err(rt_err(format!("Bytes has no member `{}`", name))),
            },
            Value::File(f) => match name {
                "path" => Ok(Value::Str(Rc::from(f.borrow().path.as_str()))),
                "name" => {
                    let p = f.borrow().path.clone();
                    let n = Path::new(&p).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or(p);
                    Ok(Value::Str(Rc::from(n.as_str())))
                }
                _ => Err(rt_err(format!("File has no member `{}`", name))),
            },
            Value::ProcResult(p) => match name {
                "stdout" => Ok(Value::Str(Rc::from(p.stdout.as_str()))),
                "stderr" => Ok(Value::Str(Rc::from(p.stderr.as_str()))),
                "status" => Ok(Value::Int(p.status as i64)),
                "ok" => Ok(Value::Bool(p.status == 0)),
                _ => Err(rt_err(format!("process result has no member `{}`", name))),
            },
            Value::Opt(o) => match name {
                "is_some" => Ok(Value::Bool(matches!(o, OptV::Some(_)))),
                "is_none" => Ok(Value::Bool(matches!(o, OptV::None))),
                "unwrap" => match o {
                    OptV::Some(v) => Ok((**v).clone()),
                    OptV::None => Err(rt_err("called unwrap on None")),
                },
                _ => Err(rt_err(format!("Option has no member `{}`", name))),
            },
            Value::Res(r) => match name {
                "is_ok" => Ok(Value::Bool(matches!(r, Res::Ok(_)))),
                "is_err" => Ok(Value::Bool(matches!(r, Res::Err(_)))),
                "unwrap" => match r {
                    Res::Ok(v) => Ok((**v).clone()),
                    Res::Err(e) => Err(rt_err(format!(
                        "called unwrap on error: {}",
                        err_message(e)
                    ))),
                },
                "ok" => match r {
                    Res::Ok(v) => Ok(Value::Opt(OptV::Some(v.clone()))),
                    Res::Err(_) => Ok(Value::Opt(OptV::None)),
                },
                "err" => match r {
                    Res::Ok(_) => Ok(Value::Opt(OptV::None)),
                    Res::Err(e) => Ok(Value::Opt(OptV::Some(e.clone()))),
                },
                _ => Err(rt_err(format!("Result has no member `{}`", name))),
            },
            Value::TypeInfo { name: ty, kind: TypeKind::Enum } => Ok(Value::Enum {
                ty: Some(ty.clone()),
                variant: name.to_string(),
                payload: None,
            }),
            Value::TypeInfo { name: ty, kind: TypeKind::Struct } => Err(rt_err(format!(
                "`{}` is a struct type; it has no member `{}`",
                ty, name
            ))),
            other => Err(rt_err(format!("`{}` has no member `{}`", other.type_name(), name))),
        }
    }

    fn index_value(&mut self, base: Value, index: Value) -> EResult<Value> {
        match &base {
            Value::Array(a) => {
                let i = as_int(&index)?;
                let a = a.borrow();
                if i < 0 || i as usize >= a.len() {
                    return Err(rt_err(format!("array index {} out of bounds (len {})", i, a.len())));
                }
                Ok(a[i as usize].clone())
            }
            Value::Str(s) => {
                let i = as_int(&index)?;
                let chars: Vec<char> = s.chars().collect();
                if i < 0 || i as usize >= chars.len() {
                    return Err(rt_err(format!("string index {} out of bounds", i)));
                }
                Ok(Value::Str(Rc::from(chars[i as usize].to_string())))
            }
            Value::Map(m) => {
                let key = display(&index);
                let m = m.borrow();
                m.iter()
                    .find(|(k, _)| k == &key)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| rt_err(format!("map has no key `{}`", key)))
            }
            Value::Struct(s) => {
                let key = display(&index);
                s.borrow()
                    .fields
                    .iter()
                    .find(|(k, _)| k == &key)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| rt_err(format!("struct has no field `{}`", key)))
            }
            Value::Bytes(b) => {
                let i = as_int(&index)?;
                if i < 0 || i as usize >= b.len() {
                    return Err(rt_err(format!("Bytes index {} out of bounds", i)));
                }
                Ok(Value::Int(b[i as usize] as i64))
            }
            other => Err(rt_err(format!("cannot index into `{}`", other.type_name()))),
        }
    }
}

// ---------- pattern matching ----------

fn try_match(pat: &Pattern, value: &Value) -> Option<Vec<(String, Value)>> {
    match pat {
        Pattern::Wild => Some(Vec::new()),
        Pattern::Lit(l) => {
            let target = match l {
                LitVal::Int(n) => Value::Int(*n),
                LitVal::Float(f) => Value::Float(*f),
                LitVal::Str(s) => Value::Str(Rc::from(s.as_str())),
                LitVal::Char(c) => Value::Char(*c),
                LitVal::Bool(b) => Value::Bool(*b),
            };
            if values_equal(&target, value) {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::Bind(name) => Some(vec![(name.clone(), value.clone())]),
        Pattern::Variant { ty, name, inner } => match name.as_str() {
            "Some" => match value {
                Value::Opt(OptV::Some(v)) => bind_inner(inner, v),
                _ => None,
            },
            "None" => match value {
                Value::Opt(OptV::None) => Some(Vec::new()),
                _ => None,
            },
            "Ok" => match value {
                Value::Res(Res::Ok(v)) => bind_inner(inner, v),
                _ => None,
            },
            "Err" => match value {
                Value::Res(Res::Err(v)) => bind_inner(inner, v),
                _ => None,
            },
            other => match value {
                Value::Enum { ty: vty, variant, payload } => {
                    if variant != other {
                        return None;
                    }
                    if let Some(t) = ty {
                        if Some(t.as_str()) != vty.as_deref() {
                            return None;
                        }
                    }
                    match (inner, payload) {
                        (None, None) => Some(Vec::new()),
                        (Some(_), Some(v)) => bind_inner(inner, v),
                        _ => None,
                    }
                }
                _ => None,
            },
        },
    }
}

fn bind_inner(inner: &Option<Box<Pattern>>, value: &Value) -> Option<Vec<(String, Value)>> {
    match inner {
        Some(p) => try_match(p, value),
        None => Some(Vec::new()),
    }
}

// ---------- numeric helpers ----------

fn as_int(v: &Value) -> EResult<i64> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(rt_err(format!("expected an integer index, found `{}`", other.type_name()))),
    }
}

fn is_zero(v: &Value) -> bool {
    matches!(v, Value::Int(0) | Value::Float(0.0))
}

fn num_bin(
    l: &Value,
    r: &Value,
    int_fn: impl Fn(i64, i64) -> i64,
    float_fn: impl Fn(f64, f64) -> f64,
) -> EResult<Value> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(int_fn(*a, *b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(float_fn(*a as f64, *b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(float_fn(*a, *b as f64))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_fn(*a, *b))),
        _ => Err(rt_err(format!(
            "cannot apply arithmetic to `{}` and `{}`",
            l.type_name(),
            r.type_name()
        ))),
    }
}

fn add_values(l: &Value, r: &Value) -> EResult<Value> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.wrapping_add(*b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        _ => Ok(Value::Str(Rc::from(format!("{}{}", display(l), display(r))))),
    }
}