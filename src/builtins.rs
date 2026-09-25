use std::cell::RefCell;
use std::rc::Rc;

use crate::env::Env;
use crate::error::{RtResult, UsglError};
use crate::eval::{Abort, Arg, Ctrl, Interp};
use crate::value::*;

fn builtin(name: &'static str) -> Value {
    Value::Function(Rc::new(Function {
        kind: FuncKind::Builtin(name),
        name: Some(name.to_string()),
    }))
}

fn rt_err(msg: impl Into<String>) -> UsglError {
    UsglError::rt(msg.into(), None)
}

fn positional(args: &[Arg]) -> Vec<Value> {
    args.iter()
        .filter_map(|(n, v)| if n.is_none() { Some(v.clone()) } else { None })
        .collect()
}

fn all(args: &[Arg]) -> Vec<Value> {
    args.iter().map(|(_, v)| v.clone()).collect()
}

fn named<'a>(args: &'a [Arg], key: &str) -> Option<&'a Value> {
    args.iter()
        .find(|(n, _)| n.as_deref() == Some(key))
        .map(|(_, v)| v)
}

fn one(args: &[Arg], fname: &str) -> RtResult<Value> {
    let p = positional(args);
    if p.is_empty() {
        Err(rt_err(format!("missing argument to `{}`", fname)))
    } else {
        Ok(p[0].clone())
    }
}

fn seed_arg(v: &Value) -> RtResult<String> {
    match v {
        Value::Str(s) => Ok(s.to_string()),
        _ => Err(rt_err(format!(
            "expected String, found `{}`",
            v.type_name()
        ))),
    }
}

fn int_arg(v: &Value) -> RtResult<i64> {
    match v {
        Value::Int(i) => Ok(*i),
        _ => Err(rt_err(format!(
            "expected an integer, found `{}`",
            v.type_name()
        ))),
    }
}

fn num_arg(v: &Value) -> RtResult<f64> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        _ => Err(rt_err(format!(
            "expected a number, found `{}`",
            v.type_name()
        ))),
    }
}

// =====================================================================
// install
// =====================================================================

pub fn install(global: &Rc<Env>, interp: &mut Interp) {
    for name in [
        "println",
        "print",
        "assert",
        "assert_eq",
        "shell",
        "type_of",
        "str",
        "len",
        "map",
        "filter",
    ] {
        let _ = global.define(name, builtin(name), false);
    }

    let mut modules: Vec<(&str, Vec<&'static str>)> = Vec::new();
    modules.push((
        "fs",
        vec![
            "fs.read",
            "fs.write",
            "fs.append",
            "fs.exists",
            "fs.list",
            "fs.directories",
            "fs.glob",
            "fs.open",
            "fs.remove",
            "fs.mkdir",
        ],
    ));
    modules.push(("json", vec!["json.parse", "json.stringify"]));
    modules.push((
        "math",
        vec![
            "math.sqrt",
            "math.cbrt",
            "math.abs",
            "math.floor",
            "math.ceil",
            "math.round",
            "math.trunc",
            "math.pow",
            "math.min",
            "math.max",
            "math.sin",
            "math.cos",
            "math.tan",
            "math.asin",
            "math.acos",
            "math.atan",
            "math.log",
            "math.log2",
            "math.log10",
            "math.exp",
            "math.pi",
        ],
    ));
    modules.push((
        "strings",
        vec![
            "strings.upper",
            "strings.lower",
            "strings.trim",
            "strings.trim_start",
            "strings.trim_end",
            "strings.reverse",
            "strings.repeat",
            "strings.replace",
            "strings.starts_with",
            "strings.ends_with",
            "strings.contains",
            "strings.char_at",
            "strings.length",
            "strings.split",
            "strings.join",
            "strings.chars",
        ],
    ));
    modules.push(("time", vec!["time.sleep", "time.millis", "time.now"]));
    modules.push((
        "os",
        vec!["os.name", "os.env", "os.args", "os.cwd", "os.exit"],
    ));
    modules.push(("process", vec!["process.run", "process.shell"]));
    modules.push(("Buffer", vec!["Buffer.new"]));
    modules.push(("Bytes", vec!["Bytes.new"]));

    for (mname, fns) in modules {
        let env = Env::new(None, Some(mname.to_string()));
        for f in fns {
            let _ = env.define(
                f.strip_prefix(mname).unwrap().trim_start_matches('.'),
                builtin(f),
                false,
            );
        }
        interp.modules.insert(mname.to_string(), env.clone());
        // Bind the module itself so `fs.list(...)` works without an import,
        // matching the RFC's single-file scripting goal.
        let _ = global.define(mname, Value::Module(env), false);
    }
}

// =====================================================================
// global builtins
// =====================================================================

pub fn call_builtin(name: &str, interp: &mut Interp, args: Vec<Arg>) -> RtResult<Value> {
    match name {
        "println" => {
            let parts: Vec<String> = all(&args).iter().map(display).collect();
            interp.write_out(&format!("{}\n", parts.join(" ")));
            Ok(Value::Nil)
        }
        "print" => {
            let parts: Vec<String> = all(&args).iter().map(display).collect();
            interp.write_out(&parts.join(" "));
            Ok(Value::Nil)
        }
        "assert" => {
            let p = all(&args);
            if p.is_empty() {
                return Err(rt_err("assert expects a condition"));
            }
            if !p[0].is_truthy() {
                let msg = if p.len() > 1 {
                    display(&p[1])
                } else {
                    "assertion failed".into()
                };
                return Err(rt_err(msg));
            }
            Ok(Value::Nil)
        }
        "assert_eq" => {
            let p = all(&args);
            if p.len() < 2 {
                return Err(rt_err("assert_eq expects two values"));
            }
            if !values_equal(&p[0], &p[1]) {
                let msg = if p.len() > 2 {
                    display(&p[2])
                } else {
                    format!("assertion failed: {} != {}", repr(&p[0]), repr(&p[1]))
                };
                return Err(rt_err(msg));
            }
            Ok(Value::Nil)
        }
        "shell" => {
            let cmd = seed_arg(&one(&args, "shell")?)?;
            let out = run_shell(&cmd)?;
            Ok(Value::Str(Rc::from(out.as_str())))
        }
        "type_of" => {
            let v = one(&args, "type_of")?;
            Ok(Value::Str(Rc::from(v.type_name())))
        }
        "str" => {
            let v = one(&args, "str")?;
            Ok(Value::Str(Rc::from(display(&v).as_str())))
        }
        "len" => {
            let v = one(&args, "len")?;
            let n = match &v {
                Value::Str(s) => s.chars().count() as i64,
                Value::Array(a) => a.borrow().len() as i64,
                Value::Map(m) => m.borrow().len() as i64,
                Value::Bytes(b) => b.len() as i64,
                _ => {
                    return Err(rt_err(format!(
                        "`len` does not apply to `{}`",
                        v.type_name()
                    )))
                }
            };
            Ok(Value::Int(n))
        }
        "map" | "filter" => {
            let p = positional(&args);
            if p.len() < 2 {
                return Err(rt_err(format!("{} (array, function)", name)));
            }
            match &p[0] {
                Value::Array(a) => ho_apply(interp, a, &p[1], name),
                other => Err(rt_err(format!(
                    "`{}` expects an array as its first argument, found `{}`",
                    name,
                    other.type_name()
                ))),
            }
        }

        // ---------------- fs ----------------
        "fs.read" => {
            let p = positional(&args);
            let path = seed_arg(p.first().ok_or_else(|| rt_err("fs.read(path)"))?)?;
            match std::fs::read_to_string(&path) {
                Ok(s) => Ok(Value::Res(Res::Ok(Box::new(Value::Str(Rc::from(
                    s.as_str(),
                )))))),
                Err(e) => Ok(err_result(format!("fs.read({:?}): {}", path, e))),
            }
        }
        "fs.write" => {
            let p = positional(&args);
            if p.len() < 2 {
                return Err(rt_err("fs.write(path, content)"));
            }
            let path = seed_arg(&p[0])?;
            let content = display(&p[1]);
            match std::fs::write(&path, content) {
                Ok(()) => Ok(ok_result(Value::Nil)),
                Err(e) => Ok(err_result(format!("fs.write({:?}): {}", path, e))),
            }
        }
        "fs.append" => {
            let p = positional(&args);
            if p.len() < 2 {
                return Err(rt_err("fs.append(path, content)"));
            }
            let path = seed_arg(&p[0])?;
            let content = display(&p[1]);
            use std::io::Write;
            let r = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut f| f.write_all(content.as_bytes()));
            match r {
                Ok(()) => Ok(ok_result(Value::Nil)),
                Err(e) => Ok(err_result(format!("fs.append({:?}): {}", path, e))),
            }
        }
        "fs.exists" => {
            let path = seed_arg(&one(&args, "fs.exists")?)?;
            Ok(Value::Bool(std::path::Path::new(&path).exists()))
        }
        "fs.list" => {
            let dir = seed_arg(&one(&args, "fs.list")?)?;
            let mut names: Vec<String> = Vec::new();
            let entries = std::fs::read_dir(&dir)
                .map_err(|e| rt_err(format!("fs.list({:?}): {}", dir, e)))?;
            for en in entries {
                let en = en.map_err(|e| rt_err(format!("fs.list({:?}): {}", dir, e)))?;
                names.push(en.file_name().to_string_lossy().to_string());
            }
            names.sort();
            let vals: Vec<Value> = names.into_iter().map(|n| join_dir(&dir, &n)).collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        "fs.directories" => {
            let dir = seed_arg(&one(&args, "fs.directories")?)?;
            let mut names: Vec<String> = Vec::new();
            let entries = std::fs::read_dir(&dir)
                .map_err(|e| rt_err(format!("fs.directories({:?}): {}", dir, e)))?;
            for en in entries {
                let en = en.map_err(|e| rt_err(format!("fs.directories({:?}): {}", dir, e)))?;
                if en.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    names.push(en.file_name().to_string_lossy().to_string());
                }
            }
            names.sort();
            let vals: Vec<Value> = names.into_iter().map(|n| join_dir(&dir, &n)).collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        "fs.glob" => {
            let pat = seed_arg(&one(&args, "fs.glob")?)?;
            let (dir, fpat) = split_glob(&pat);
            let mut matches: Vec<String> = Vec::new();
            let entries = std::fs::read_dir(&dir)
                .map_err(|e| rt_err(format!("fs.glob({:?}): {}", pat, e)))?;
            for en in entries {
                let en = en.map_err(|e| rt_err(format!("fs.glob({:?}): {}", pat, e)))?;
                let name = en.file_name().to_string_lossy().to_string();
                if glob_match(&fpat, &name) {
                    if let Value::Str(p) = join_dir(&dir, &name) {
                        matches.push(p.to_string());
                    }
                }
            }
            matches.sort();
            let vals: Vec<Value> = matches
                .into_iter()
                .map(|n| Value::Str(Rc::from(n.as_str())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        "fs.open" => {
            let path = seed_arg(&one(&args, "fs.open")?)?;
            let handle = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&path)
                .map_err(|e| rt_err(format!("fs.open({:?}): {}", path, e)))?;
            Ok(Value::File(Rc::new(RefCell::new(UsglFile {
                path: path.clone(),
                handle: Some(handle),
                writable: true,
            }))))
        }
        "fs.remove" => {
            let path = seed_arg(&one(&args, "fs.remove")?)?;
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(ok_result(Value::Nil)),
                Err(e) => Ok(err_result(format!("fs.remove({:?}): {}", path, e))),
            }
        }
        "fs.mkdir" => {
            let path = seed_arg(&one(&args, "fs.mkdir")?)?;
            match std::fs::create_dir_all(&path) {
                Ok(()) => Ok(ok_result(Value::Nil)),
                Err(e) => Ok(err_result(format!("fs.mkdir({:?}): {}", path, e))),
            }
        }

        // ---------------- json ----------------
        "json.parse" => {
            let s = seed_arg(&one(&args, "json.parse")?)?;
            match crate::builtins::json::parse(&s) {
                Ok(v) => Ok(Value::Res(Res::Ok(Box::new(v)))),
                Err(e) => Ok(err_result(format!("json.parse: {}", e))),
            }
        }
        "json.stringify" => {
            let p = positional(&args);
            if p.is_empty() {
                return Err(rt_err("json.stringify(value)"));
            }
            let pretty = matches!(named(&args, "pretty"), Some(Value::Bool(true)));
            match crate::builtins::json::stringify(&p[0], pretty) {
                Ok(s) => Ok(Value::Str(Rc::from(s.as_str()))),
                Err(e) => Err(rt_err(e)),
            }
        }

        // ---------------- math ----------------
        "math.pi" => Ok(Value::Float(std::f64::consts::PI)),
        _ if name.starts_with("math.") => {
            let f = math_fn(name)?;
            let p = positional(&args);
            match f {
                MathFn::Nullary => Ok(Value::Float(f_nullary(name)?)),
                MathFn::Unary => {
                    let x = num_arg(
                        p.first()
                            .ok_or_else(|| rt_err(format!("{} expects one argument", name)))?,
                    )?;
                    Ok(Value::Float(f_unary(name, x)?))
                }
                MathFn::Binary => {
                    if p.len() < 2 {
                        return Err(rt_err(format!("{} expects two arguments", name)));
                    }
                    let a = num_arg(&p[0])?;
                    let b = num_arg(&p[1])?;
                    Ok(Value::Float(f_binary(name, a, b)?))
                }
                MathFn::All => {
                    let xs: Vec<f64> = p.iter().map(num_arg).collect::<RtResult<_>>()?;
                    if name == "math.min" {
                        Ok(Value::Float(xs.into_iter().fold(f64::INFINITY, f64::min)))
                    } else if name == "math.max" {
                        Ok(Value::Float(
                            xs.into_iter().fold(f64::NEG_INFINITY, f64::max),
                        ))
                    } else {
                        Err(rt_err(format!("unknown math function `{}`", name)))
                    }
                }
            }
        }

        // ---------------- strings ----------------
        "strings.upper" | "strings.lower" | "strings.trim" | "strings.trim_start"
        | "strings.trim_end" | "strings.reverse" => {
            let s = seed_arg(&one(&args, name)?)?;
            Ok(Value::Str(Rc::from(str_unary(name, &s).as_str())))
        }
        "strings.length" => {
            let s = seed_arg(&one(&args, "strings.length")?)?;
            Ok(Value::Int(s.chars().count() as i64))
        }
        "strings.repeat" => {
            let p = positional(&args);
            if p.len() < 2 {
                return Err(rt_err("strings.repeat(s, n)"));
            }
            let s = seed_arg(&p[0])?;
            let n = int_arg(&p[1])?;
            if n < 0 || n > 100_000 {
                return Err(rt_err("invalid repeat count"));
            }
            Ok(Value::Str(Rc::from(s.repeat(n as usize).as_str())))
        }
        "strings.replace" => {
            let p = positional(&args);
            if p.len() < 3 {
                return Err(rt_err("strings.replace(s, old, new)"));
            }
            let s = seed_arg(&p[0])?;
            let o = seed_arg(&p[1])?;
            let n = seed_arg(&p[2])?;
            Ok(Value::Str(Rc::from(s.replace(&o, &n).as_str())))
        }
        "strings.starts_with" | "strings.ends_with" | "strings.contains" => {
            let p = positional(&args);
            if p.len() < 2 {
                return Err(rt_err(format!("{} expects two arguments", name)));
            }
            let s = seed_arg(&p[0])?;
            let sub = seed_arg(&p[1])?;
            let b = match name {
                "strings.starts_with" => s.starts_with(&sub),
                "strings.ends_with" => s.ends_with(&sub),
                _ => s.contains(&sub),
            };
            Ok(Value::Bool(b))
        }
        "strings.char_at" => {
            let p = positional(&args);
            if p.len() < 2 {
                return Err(rt_err("strings.char_at(s, i)"));
            }
            let s = seed_arg(&p[0])?;
            let i = int_arg(&p[1])?;
            let chars: Vec<char> = s.chars().collect();
            if i < 0 || i as usize >= chars.len() {
                return Err(rt_err(format!("string index {} out of bounds", i)));
            }
            Ok(Value::Str(Rc::from(chars[i as usize].to_string())))
        }
        "strings.split" => {
            let p = positional(&args);
            if p.len() < 2 {
                return Err(rt_err("strings.split(s, sep)"));
            }
            let s = seed_arg(&p[0])?;
            let sep = seed_arg(&p[1])?;
            Ok(Value::Array(Rc::new(RefCell::new(
                s.split(&sep).map(|x| Value::Str(Rc::from(x))).collect(),
            ))))
        }
        "strings.join" => {
            let p = positional(&args);
            if p.len() < 2 {
                return Err(rt_err("strings.join(items, sep)"));
            }
            let sep = display(&p[1]);
            match &p[0] {
                Value::Array(a) => {
                    let parts: Vec<String> = a.borrow().iter().map(display).collect();
                    Ok(Value::Str(Rc::from(parts.join(&sep).as_str())))
                }
                _ => Err(rt_err("strings.join expects an Array")),
            }
        }
        "strings.chars" => {
            let s = seed_arg(&one(&args, "strings.chars")?)?;
            let vals: Vec<Value> = s
                .chars()
                .map(|c| Value::Str(Rc::from(c.to_string())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }

        // ---------------- time ----------------
        "time.sleep" => {
            let ms = int_arg(&one(&args, "time.sleep")?)?;
            if ms < 0 {
                return Err(rt_err(
                    "time.sleep expects a non-negative number of milliseconds",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
            Ok(Value::Nil)
        }
        "time.millis" => {
            let d = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            Ok(Value::Int(d.as_millis() as i64))
        }
        "time.now" => Ok(Value::Str(Rc::from(crate::builtins::time::now().as_str()))),

        // ---------------- os ----------------
        "os.name" => Ok(Value::Str(Rc::from(os_name().as_str()))),
        "os.env" => {
            let k = seed_arg(&one(&args, "os.env")?)?;
            match std::env::var(&k) {
                Ok(v) => Ok(Value::Opt(OptV::Some(Box::new(Value::Str(Rc::from(
                    v.as_str(),
                )))))),
                Err(_) => Ok(Value::Opt(OptV::None)),
            }
        }
        "os.args" => {
            let vals: Vec<Value> = interp
                .cli_args
                .iter()
                .map(|a| Value::Str(Rc::from(a.as_str())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        "os.cwd" => Ok(Value::Str(Rc::from(
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
                .as_str(),
        ))),
        "os.exit" => {
            let code = int_arg(&one(&args, "os.exit")?)?;
            Err(UsglError::Exit(code as i32))
        }

        // ---------------- process ----------------
        "process.run" => {
            let p = positional(&args);
            if p.len() < 1 {
                return Err(rt_err("process.run(cmd, args)"));
            }
            let cmd = seed_arg(&p[0])?;
            let mut process_args: Vec<String> = Vec::new();
            if let Some(Value::Array(a)) = p.get(1) {
                for v in a.borrow().iter() {
                    process_args.push(seed_arg(v)?);
                }
            }
            run_process(&cmd, &process_args)
        }
        "process.shell" => {
            let cmd = seed_arg(&one(&args, "process.shell")?)?;
            let out = run_shell(&cmd)?;
            Ok(Value::Str(Rc::from(out.as_str())))
        }

        // ---------------- Buffer / Bytes ----------------
        "Bytes.new" | "Buffer.new" => {
            let n = int_arg(&one(&args, name)?)?;
            if n < 0 || n > 1_000_000_000 {
                return Err(rt_err("invalid buffer size"));
            }
            Ok(Value::Bytes(Rc::new(vec![0; n as usize])))
        }

        other => Err(rt_err(format!("unknown builtin `{}`", other))),
    }
}

// =====================================================================
// methods
// =====================================================================

pub fn call_method(
    receiver: &Value,
    name: &str,
    interp: &mut Interp,
    args: Vec<Arg>,
) -> RtResult<Value> {
    match receiver {
        Value::Str(s) => str_method(s, name, &args),
        Value::Array(a) => array_method(a, name, interp, &args),
        Value::Map(m) => map_method(m, name, &args),
        Value::Bytes(b) => bytes_method(b, name, &args),
        Value::File(f) => file_method(f, name, &args),
        Value::Opt(o) => opt_method(o, name, &args),
        Value::Res(r) => res_method(r, name, &args),
        _ => Err(rt_err(format!(
            "`{}` has no method `{}`",
            receiver.type_name(),
            name
        ))),
    }
}

fn str_method(s: &Rc<str>, name: &str, args: &[Arg]) -> RtResult<Value> {
    let p = positional(args);
    match name {
        "len" => Ok(Value::Int(s.chars().count() as i64)),
        "upper" => Ok(Value::Str(Rc::from(s.to_uppercase().as_str()))),
        "lower" => Ok(Value::Str(Rc::from(s.to_lowercase().as_str()))),
        "trim" => Ok(Value::Str(Rc::from(s.trim()))),
        "trim_start" => Ok(Value::Str(Rc::from(s.trim_start()))),
        "trim_end" => Ok(Value::Str(Rc::from(s.trim_end()))),
        "split" => {
            let sep = seed_arg(p.first().ok_or_else(|| rt_err("split(separator)"))?)?;
            let vals: Vec<Value> = s.split(&sep).map(|x| Value::Str(Rc::from(x))).collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        "lines" => {
            let mut vals: Vec<Value> = Vec::new();
            for line in s.lines() {
                vals.push(Value::Str(Rc::from(line)));
            }
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        "chars" => {
            let vals: Vec<Value> = s
                .chars()
                .map(|c| Value::Str(Rc::from(c.to_string())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        "starts_with" => {
            let sub = seed_arg(p.first().ok_or_else(|| rt_err("starts_with(value)"))?)?;
            Ok(Value::Bool(s.starts_with(&sub)))
        }
        "ends_with" => {
            let sub = seed_arg(p.first().ok_or_else(|| rt_err("ends_with(value)"))?)?;
            Ok(Value::Bool(s.ends_with(&sub)))
        }
        "contains" => {
            let sub = seed_arg(p.first().ok_or_else(|| rt_err("contains(value)"))?)?;
            Ok(Value::Bool(s.contains(&sub)))
        }
        "replace" => {
            if p.len() < 2 {
                return Err(rt_err("replace(old, new)"));
            }
            let o = seed_arg(&p[0])?;
            let n = seed_arg(&p[1])?;
            Ok(Value::Str(Rc::from(s.replace(&o, &n).as_str())))
        }
        "repeat" => {
            let n = int_arg(p.first().ok_or_else(|| rt_err("repeat(n)"))?)?;
            if n < 0 || n > 1_000_000 {
                return Err(rt_err("invalid repeat count"));
            }
            Ok(Value::Str(Rc::from(s.repeat(n as usize).as_str())))
        }
        "reverse" => Ok(Value::Str(Rc::from(
            s.chars().rev().collect::<String>().as_str(),
        ))),
        "char_at" => {
            let i = int_arg(p.first().ok_or_else(|| rt_err("char_at(i)"))?)?;
            let chars: Vec<char> = s.chars().collect();
            if i < 0 || i as usize >= chars.len() {
                return Err(rt_err(format!("string index {} out of bounds", i)));
            }
            Ok(Value::Str(Rc::from(chars[i as usize].to_string())))
        }
        "index_of" => {
            let sub = seed_arg(p.first().ok_or_else(|| rt_err("index_of(value)"))?)?;
            match s.find(&sub) {
                Some(i) => Ok(Value::Opt(OptV::Some(Box::new(Value::Int(i as i64))))),
                None => Ok(Value::Opt(OptV::None)),
            }
        }
        _ => Err(rt_err(format!("String has no method `{}`", name))),
    }
}

fn array_method(
    a: &Rc<RefCell<Vec<Value>>>,
    name: &str,
    interp: &mut Interp,
    args: &[Arg],
) -> RtResult<Value> {
    // Modifications may need to happen while other reads run; keep simple.
    if name == "push" || name == "pop" || name == "sort_in_place" {
        let mut a = a.borrow_mut();
        return match name {
            "push" => {
                let v = one_positional(args)?.clone();
                a.push(v);
                Ok(Value::Nil)
            }
            "pop" => match a.pop() {
                Some(v) => Ok(v),
                None => Err(rt_err("pop() on empty array")),
            },
            _ => unreachable!(),
        };
    }

    let a = &*a.borrow();
    match name {
        "len" => Ok(Value::Int(a.len() as i64)),
        "is_empty" => Ok(Value::Bool(a.is_empty())),
        "first" => Ok(a
            .first()
            .map(|v| OptV::Some(Box::new(v.clone())))
            .unwrap_or(OptV::None))
        .map(Value::Opt),
        "last" => Ok(a
            .last()
            .map(|v| OptV::Some(Box::new(v.clone())))
            .unwrap_or(OptV::None))
        .map(Value::Opt),
        "contains" => {
            let v = one_positional(args)?.clone();
            Ok(Value::Bool(a.iter().any(|x| values_equal(x, &v))))
        }
        "index_of" => {
            let v = one_positional(args)?.clone();
            match a.iter().position(|x| values_equal(x, &v)) {
                Some(i) => Ok(Value::Opt(OptV::Some(Box::new(Value::Int(i as i64))))),
                None => Ok(Value::Opt(OptV::None)),
            }
        }
        "join" => {
            let sep = display(&one_positional(args)?);
            let parts: Vec<String> = a.iter().map(display).collect();
            Ok(Value::Str(Rc::from(parts.join(&sep).as_str())))
        }
        "map" | "filter" => {
            let f = one_positional(args)?.clone();
            if !matches!(f, Value::Function(_)) {
                return Err(rt_err(format!("{} expects a function", name)));
            }
            let mut result = Vec::with_capacity(a.len());
            for item in a.iter() {
                let applied = match interp.call_value(&f, vec![(None, item.clone())]) {
                    Ok(v) => v,
                    Err(Abort::Ctrl(Ctrl::Propagate(e))) => {
                        return Err(rt_err(err_message(&e)));
                    }
                    Err(Abort::Err(e)) => return Err(e),
                    Err(Abort::Ctrl(Ctrl::Return(v))) => v,
                    Err(Abort::Ctrl(_)) => {
                        return Err(rt_err("unexpected control flow in closure"))
                    }
                };
                if name == "map" {
                    result.push(applied);
                } else if applied.is_truthy() {
                    result.push(item.clone());
                }
            }
            Ok(Value::Array(Rc::new(RefCell::new(result))))
        }
        "foreach" => {
            let f = one_positional(args)?.clone();
            for item in a.iter() {
                match interp.call_value(&f, vec![(None, item.clone())]) {
                    Ok(_) => {}
                    Err(Abort::Ctrl(Ctrl::Propagate(e))) => return Err(rt_err(err_message(&e))),
                    Err(Abort::Err(e)) => return Err(e),
                    Err(Abort::Ctrl(_)) => {
                        return Err(rt_err("unexpected control flow in closure"))
                    }
                }
            }
            Ok(Value::Nil)
        }
        "find" => {
            let f = one_positional(args)?.clone();
            for item in a.iter() {
                let applied = interp.call_value(&f, vec![(None, item.clone())])?;
                if applied.is_truthy() {
                    return Ok(Value::Opt(OptV::Some(Box::new(item.clone()))));
                }
            }
            Ok(Value::Opt(OptV::None))
        }
        "sort" => {
            let mut items = a.clone();
            items.sort_by(|x, y| values_cmp(x, y).unwrap_or(std::cmp::Ordering::Equal));
            Ok(Value::Array(Rc::new(RefCell::new(items))))
        }
        "reverse" => {
            let mut items = a.clone();
            items.reverse();
            Ok(Value::Array(Rc::new(RefCell::new(items))))
        }
        _ => Err(rt_err(format!("Array has no method `{}`", name))),
    }
}

fn one_positional(args: &[Arg]) -> RtResult<Value> {
    positional(args)
        .into_iter()
        .next()
        .ok_or_else(|| rt_err("missing argument"))
}

/// Global `map(arr, f)` / `filter(arr, f)` (pipeline-friendly).
fn ho_apply(
    interp: &mut Interp,
    items: &Rc<RefCell<Vec<Value>>>,
    f: &Value,
    name: &str,
) -> RtResult<Value> {
    if !matches!(f, Value::Function(_)) {
        return Err(rt_err(format!("{} expects a function", name)));
    }
    let mut result = Vec::with_capacity(items.borrow().len());
    for item in items.borrow().iter() {
        let applied = match interp.call_value(f, vec![(None, item.clone())]) {
            Ok(v) => v,
            Err(Abort::Ctrl(Ctrl::Propagate(e))) => return Err(rt_err(err_message(&e))),
            Err(Abort::Err(e)) => return Err(e),
            Err(Abort::Ctrl(Ctrl::Return(v))) => v,
            Err(Abort::Ctrl(_)) => return Err(rt_err("unexpected control flow in closure")),
        };
        if name == "map" {
            result.push(applied);
        } else if applied.is_truthy() {
            result.push(item.clone());
        }
    }
    Ok(Value::Array(Rc::new(RefCell::new(result))))
}

fn map_method(m: &Rc<RefCell<Vec<(String, Value)>>>, name: &str, args: &[Arg]) -> RtResult<Value> {
    let mut m = m.borrow_mut();
    match name {
        "len" => Ok(Value::Int(m.len() as i64)),
        "keys" => Ok(Value::Array(Rc::new(RefCell::new(
            m.iter()
                .map(|(k, _)| Value::Str(Rc::from(k.as_str())))
                .collect(),
        )))),
        "values" => Ok(Value::Array(Rc::new(RefCell::new(
            m.iter().map(|(_, v)| v.clone()).collect(),
        )))),
        "get" => {
            let k = display(&one_positional(args)?);
            Ok(Value::Opt(
                m.iter()
                    .find(|(kk, _)| kk == &k)
                    .map(|(_, v)| OptV::Some(Box::new(v.clone())))
                    .unwrap_or(OptV::None),
            ))
        }
        "has" => {
            let k = display(&one_positional(args)?);
            Ok(Value::Bool(m.iter().any(|(kk, _)| kk == &k)))
        }
        "set" => {
            let p = positional(args);
            if p.len() < 2 {
                return Err(rt_err("set(key, value)"));
            }
            let k = display(&p[0]);
            if let Some((_, vv)) = m.iter_mut().find(|(kk, _)| kk == &k) {
                *vv = p[1].clone();
            } else {
                m.push((k, p[1].clone()));
            }
            Ok(Value::Nil)
        }
        "delete" => {
            let k = display(&one_positional(args)?);
            m.retain(|(kk, _)| kk != &k);
            Ok(Value::Nil)
        }
        _ => Err(rt_err(format!("Map has no method `{}`", name))),
    }
}

fn bytes_method(b: &Rc<Vec<u8>>, name: &str, args: &[Arg]) -> RtResult<Value> {
    match name {
        "len" => Ok(Value::Int(b.len() as i64)),
        "get" => {
            let i = int_arg(&one_positional(args)?)?;
            if i < 0 || i as usize >= b.len() {
                return Err(rt_err(format!("Bytes index {} out of bounds", i)));
            }
            Ok(Value::Int(b[i as usize] as i64))
        }
        "to_array" => {
            let vals: Vec<Value> = b.iter().map(|x| Value::Int(*x as i64)).collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        _ => Err(rt_err(format!("Bytes has no method `{}`", name))),
    }
}

fn file_method(f: &Rc<RefCell<UsglFile>>, name: &str, args: &[Arg]) -> RtResult<Value> {
    use std::io::{Read, Write};
    let mut f = f.borrow_mut();
    match name {
        "write" => {
            let handle = f.handle.as_mut().ok_or_else(|| rt_err("file is closed"))?;
            let content = display(&one_positional(args)?);
            handle
                .write_all(content.as_bytes())
                .map_err(|e| rt_err(format!("write failed: {}", e)))?;
            Ok(Value::Nil)
        }
        "write_line" => {
            let handle = f.handle.as_mut().ok_or_else(|| rt_err("file is closed"))?;
            let content = display(&one_positional(args)?);
            writeln!(handle, "{}", content).map_err(|e| rt_err(format!("write failed: {}", e)))?;
            Ok(Value::Nil)
        }
        "read_all" => {
            let handle = f.handle.as_mut().ok_or_else(|| rt_err("file is closed"))?;
            let mut buf = String::new();
            match handle.read_to_string(&mut buf) {
                Ok(_) => Ok(Value::Res(Res::Ok(Box::new(Value::Str(Rc::from(
                    buf.as_str(),
                )))))),
                Err(e) => Ok(err_result(format!("read failed: {}", e))),
            }
        }
        "read_line" => {
            let handle = f.handle.as_mut().ok_or_else(|| rt_err("file is closed"))?;
            use std::io::BufRead;
            let mut br = std::io::BufReader::new(&mut *handle);
            let mut buf = String::new();
            match br.read_line(&mut buf) {
                Ok(0) => Ok(Value::Res(Res::Err(Box::new(Value::Str(Rc::from(
                    "end of file",
                )))))),
                Ok(_) => {
                    while buf.ends_with('\n') || buf.ends_with('\r') {
                        buf.pop();
                    }
                    Ok(Value::Res(Res::Ok(Box::new(Value::Str(Rc::from(
                        buf.as_str(),
                    ))))))
                }
                Err(e) => Ok(err_result(format!("read failed: {}", e))),
            }
        }
        "close" => {
            f.handle.take();
            Ok(Value::Nil)
        }
        _ => Err(rt_err(format!("File has no method `{}`", name))),
    }
}

fn opt_method(o: &OptV, name: &str, args: &[Arg]) -> RtResult<Value> {
    match name {
        "is_some" => Ok(Value::Bool(matches!(o, OptV::Some(_)))),
        "is_none" => Ok(Value::Bool(matches!(o, OptV::None))),
        "unwrap" => match o {
            OptV::Some(v) => Ok((**v).clone()),
            OptV::None => Err(rt_err("called unwrap() on None")),
        },
        "unwrap_or" => match o {
            OptV::Some(v) => Ok((**v).clone()),
            OptV::None => Ok(one_positional(args)?.clone()),
        },
        "expect" => match o {
            OptV::Some(v) => Ok((**v).clone()),
            OptV::None => {
                let msg = display(&one_positional(args)?);
                Err(rt_err(format!("{} (unwrap on None)", msg)))
            }
        },
        _ => Err(rt_err(format!("Option has no method `{}`", name))),
    }
}

fn res_method(r: &Res, name: &str, args: &[Arg]) -> RtResult<Value> {
    match name {
        "is_ok" => Ok(Value::Bool(matches!(r, Res::Ok(_)))),
        "is_err" => Ok(Value::Bool(matches!(r, Res::Err(_)))),
        "unwrap" => match r {
            Res::Ok(v) => Ok((**v).clone()),
            Res::Err(e) => Err(rt_err(format!(
                "called unwrap() on error: {}",
                err_message(e)
            ))),
        },
        "unwrap_or" => match r {
            Res::Ok(v) => Ok((**v).clone()),
            Res::Err(_) => Ok(one_positional(args)?.clone()),
        },
        "expect" => match r {
            Res::Ok(v) => Ok((**v).clone()),
            Res::Err(_) => {
                let msg = display(&one_positional(args)?);
                Err(rt_err(msg))
            }
        },
        "ok" => match r {
            Res::Ok(v) => Ok(Value::Opt(OptV::Some(v.clone()))),
            Res::Err(_) => Ok(Value::Opt(OptV::None)),
        },
        "err" => match r {
            Res::Ok(_) => Ok(Value::Opt(OptV::None)),
            Res::Err(e) => Ok(Value::Opt(OptV::Some(e.clone()))),
        },
        _ => Err(rt_err(format!("Result has no method `{}`", name))),
    }
}

// =====================================================================
// helpers
// =====================================================================

fn ok_result(v: Value) -> Value {
    Value::Res(Res::Ok(Box::new(v)))
}

fn err_result(msg: impl Into<String>) -> Value {
    Value::Res(Res::Err(Box::new(Value::Str(Rc::from(
        msg.into().as_str(),
    )))))
}

fn join_dir(dir: &str, name: &str) -> Value {
    let joined = if dir.ends_with('/') || dir.ends_with('\\') {
        format!("{}{}", dir, name)
    } else {
        format!("{}/{}", dir, name)
    };
    Value::Str(Rc::from(joined.as_str()))
}

fn split_glob(pattern: &str) -> (String, String) {
    let trimmed = pattern.trim().trim_start_matches("./");
    let mut dir = String::new();
    let mut name = String::new();
    for (i, part) in trimmed.split('/').enumerate() {
        if i + 1 < trimmed.split('/').count() {
            if !dir.is_empty() {
                dir.push('/');
            }
            dir.push_str(part);
        } else {
            name = part.to_string();
        }
    }
    if dir.is_empty() {
        dir = ".".to_string();
    }
    (dir, name)
}

fn glob_match(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(c), Some(d)) => c == d && go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    go(&p, &n)
}

fn run_shell(cmd: &str) -> RtResult<String> {
    use std::process::Command;
    let output = if cfg!(windows) {
        Command::new("cmd").args(["/C", cmd]).output()
    } else {
        Command::new("sh").args(["-c", cmd]).output()
    };
    let out = output.map_err(|e| rt_err(format!("failed to run shell command: {}", e)))?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn run_process(cmd: &str, args: &[String]) -> RtResult<Value> {
    use std::process::Command;
    let output = Command::new(cmd).args(args).output();
    let out = output.map_err(|e| rt_err(format!("failed to run `{}`: {}", cmd, e)))?;
    Ok(Value::ProcResult(Rc::new(ProcResult {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        status: out.status.code().unwrap_or(-1),
    })))
}

fn os_name() -> String {
    match std::env::consts::OS {
        "windows" => "windows".into(),
        "linux" => "linux".into(),
        "macos" => "macos".into(),
        other => other.into(),
    }
}

// ---- math helpers ----

enum MathFn {
    Nullary,
    Unary,
    Binary,
    All,
}

fn math_fn(name: &str) -> RtResult<MathFn> {
    Ok(match name {
        "math.pi" => MathFn::Nullary,
        "math.abs" | "math.floor" | "math.ceil" | "math.round" | "math.trunc" | "math.sqrt"
        | "math.cbrt" | "math.sin" | "math.cos" | "math.tan" | "math.asin" | "math.acos"
        | "math.atan" | "math.log" | "math.log2" | "math.log10" | "math.exp" => MathFn::Unary,
        "math.pow" | "math.atan2" | "math.fmod" => MathFn::Binary,
        "math.min" | "math.max" => MathFn::All,
        _ => return Err(rt_err(format!("unknown math function `{}`", name))),
    })
}

fn f_nullary(name: &str) -> RtResult<f64> {
    match name {
        "math.pi" => Ok(std::f64::consts::PI),
        _ => Err(rt_err(format!("unknown math function `{}`", name))),
    }
}

fn f_unary(name: &str, x: f64) -> RtResult<f64> {
    let r = match name {
        "math.abs" => x.abs(),
        "math.floor" => x.floor(),
        "math.ceil" => x.ceil(),
        "math.round" => x.round(),
        "math.trunc" => x.trunc(),
        "math.sqrt" => {
            if x < 0.0 {
                return Err(rt_err("sqrt of a negative number"));
            }
            x.sqrt()
        }
        "math.cbrt" => x.cbrt(),
        "math.sin" => x.sin(),
        "math.cos" => x.cos(),
        "math.tan" => x.tan(),
        "math.asin" => x.asin(),
        "math.acos" => x.acos(),
        "math.atan" => x.atan(),
        "math.log" => {
            if x <= 0.0 {
                return Err(rt_err("log of a non-positive number"));
            }
            x.ln()
        }
        "math.log2" => {
            if x <= 0.0 {
                return Err(rt_err("log2 of a non-positive number"));
            }
            x.log2()
        }
        "math.log10" => {
            if x <= 0.0 {
                return Err(rt_err("log10 of a non-positive number"));
            }
            x.log10()
        }
        "math.exp" => x.exp(),
        _ => return Err(rt_err(format!("unknown math function `{}`", name))),
    };
    Ok(r)
}

fn f_binary(name: &str, a: f64, b: f64) -> RtResult<f64> {
    let r = match name {
        "math.pow" => a.powf(b),
        "math.atan2" => a.atan2(b),
        "math.fmod" => a % b,
        _ => return Err(rt_err(format!("unknown math function `{}`", name))),
    };
    Ok(r)
}

fn str_unary(name: &str, s: &str) -> String {
    match name {
        "strings.upper" => s.to_uppercase(),
        "strings.lower" => s.to_lowercase(),
        "strings.trim" => s.trim().to_string(),
        "strings.trim_start" => s.trim_start().to_string(),
        "strings.trim_end" => s.trim_end().to_string(),
        "strings.reverse" => s.chars().rev().collect(),
        _ => s.to_string(),
    }
}

// ---- submodules ----

pub mod json;
pub mod time;
