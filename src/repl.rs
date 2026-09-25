use std::env;
use std::io::{BufRead, Write};
use std::rc::Rc;

use crate::ast::StmtKind;
use crate::env::Env;
use crate::error::UsglError;
use crate::eval::Interp;
use crate::parser::parse;
use crate::value::repr;

/// Interactive REPL.
///
/// Input is accumulated line by line until it forms a complete program, then
/// evaluated against the persistent global scope (top-level `let`/`fn` rebind
/// because the REPL env allows redefinition).
pub fn repl() -> Result<(), UsglError> {
    let mut interp = Interp::new("<repl>");
    interp.global.allow_redefine();
    let global: Rc<Env> = interp.global.clone();

    println!("USGL {} — interactive. `exit` to quit.", crate::VERSION);
    let stdin = std::io::stdin();
    let mut source = String::new();
    loop {
        let prompt = if source.is_empty() { "> " } else { "... " };
        print!("{}", prompt);
        let _ = std::io::stdout().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("error reading input: {}", e);
                break;
            }
        }
        source.push_str(&line);
        if needs_more(&source) {
            continue;
        }
        let input = source.trim().to_string();
        source.clear();
        if input.is_empty() {
            continue;
        }
        if input == "exit" || input == ":q" || input == ":quit" {
            break;
        }
        if input == "help" || input == ":help" {
            println!("multi-line input continues with `... `; Ctrl-D or `exit` quits.");
            continue;
        }
        if cmd_clear(&input) {
            interp = reset_interp();
            continue;
        }

        let program = match parse(&input, "<repl>") {
            Ok(p) => p,
            Err(e) => {
                eprintln!("parse error: {}", e);
                continue;
            }
        };
        for stmt in &program.stmts {
            match interp.stmt(stmt, &global) {
                Ok(v) => {
                    if matches!(stmt.kind, StmtKind::Expr(_)) && !v.is_nil() {
                        println!("=> {}", repr(&v));
                    }
                }
                Err(a) => eprintln!("error: {}", crate::abort_message(&a)),
            }
        }
        let out = interp.drain_out();
        if !out.is_empty() {
            let _ = std::io::stdout().write_all(&out);
        }
    }
    println!();
    Ok(())
}

fn reset_interp() -> Interp {
    Interp::new("<repl>")
}

fn cmd_clear(input: &str) -> bool {
    matches!(input, ":clear" | ":reset" | ":c")
}

/// Heuristically decide whether `src` is complete: the delimiter stack must be
/// empty. Strings (`"..."`), chars (`'c'`), and comments are skipped.
fn needs_more(src: &str) -> bool {
    let mut stack: Vec<char> = Vec::new();
    let mut it = src.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '(' | '[' | '{' => stack.push(c),
            ')' | ']' | '}' => {
                let open = stack.pop();
                if !matching(open, c) {
                    return false;
                }
            }
            '"' => {
                let mut esc = false;
                for c2 in it.by_ref() {
                    if c2 == '"' && !esc {
                        break;
                    }
                    if c2 == '\\' && !esc {
                        esc = true;
                    } else {
                        esc = false;
                    }
                }
            }
            '\'' => {
                // char literal: 'x' or '\n'
                let mut n = 0;
                for c2 in it.by_ref() {
                    n += 1;
                    if c2 == '\'' && n <= 6 {
                        break;
                    }
                }
            }
            '#' => {
                while it.next().is_some_and(|c| c != '\n') {}
            }
            '/' => {
                if it.peek() == Some(&'/') {
                    it.next();
                    while it.next().is_some_and(|c| c != '\n') {}
                }
            }
            _ => {}
        }
    }
    !stack.is_empty()
}

fn matching(open: Option<char>, close: char) -> bool {
    matches!(
        (open, close),
        (Some('('), ')') | (Some('['), ']') | (Some('{'), '}')
    )
}

/// Line-editing with GNU readline when available; falls back to plain stdin.
/// (Kept as a separate point so a readline crate can be swapped in later.)
#[allow(dead_code)]
fn readline(_prompt: &str) -> Option<String> {
    None
}

fn _env_discard(_: env::Args) {}