pub mod ast;
pub mod builtins;
pub mod env;
pub mod error;
pub mod eval;
pub mod fmt;
pub mod lexer;
pub mod parser;
pub mod repl;
pub mod token;
pub mod value;

use std::cell::RefCell;
use std::rc::Rc;

use ast::Program;
use error::RtResult;
use eval::Interp;
use lexer::Lexer;
use token::Token;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parse `source` as a USGL program.
pub fn parse_source(source: &str, file: &str) -> RtResult<Program> {
    parser::parse(source, file)
}

/// Parse + run and turn the result into a process exit code (0 = ok).
pub fn run_source(source: &str, file: &str) -> RtResult<i32> {
    let mut interp = Interp::new(file);
    run_in(&mut interp, source, file, true)
}

/// Like `run_source` but sends `println!` output to a caller-owned buffer.
pub fn run_source_with_output(source: &str, file: &str, out: Rc<RefCell<Vec<u8>>>) -> RtResult<i32> {
    let mut interp = Interp::with_out(file, out);
    run_in(&mut interp, source, file, true)
}

fn run_in(interp: &mut Interp, source: &str, file: &str, run_main: bool) -> RtResult<i32> {
    let program = parse_source(source, file)?;
    interp.run(&program, run_main)
}

/// Syntax-check only.
pub fn check_source(source: &str, file: &str) -> RtResult<usize> {
    let program = parse_source(source, file)?;
    Ok(program.stmts.len())
}

/// Parse and print the canonical USGL formatting for `source`.
pub fn format_source(source: &str, file: &str) -> RtResult<String> {
    let program = parse_source(source, file)?;
    Ok(fmt::format_program(&program))
}

/// Tokenize without building an AST.
pub fn tokenize(source: &str, file: &str) -> Result<Vec<Token>, error::UsglError> {
    Lexer::new(source, file).tokenize()
}

/// Render an `eval::Abort` as a single-line user-facing message.
pub fn abort_message(a: &eval::Abort) -> String {
    use eval::Ctrl;
    match a {
        eval::Abort::Err(e) => format!("{}", e),
        eval::Abort::Ctrl(c) => format!(
            "control flow escaped: {}",
            match c {
                Ctrl::Return(_) => "`return` outside of a function",
                Ctrl::Break => "`break` outside of a loop",
                Ctrl::Continue => "`continue` outside of a loop",
                Ctrl::Propagate(_) => "`?` outside a Result-returning function",
            }
        ),
    }
}