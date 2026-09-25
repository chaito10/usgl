use std::process::ExitCode;

const USAGE: &str = "\
USGL — Universal Systems & Glue Language (Phase 1 interpreter)

Usage:
  us <file.us> [args...]      run a program
  us run <file.us> [args...]  run a program (same as above)
  us repl                     start the interactive REPL
  us test [files...]          run `test` blocks (default: all *.us in cwd)
  us check <file.us>          syntax-check without running
  us fmt [-w] <file.us>       print canonical formatting (_w_ = write back)
  us version | --version      print version
  us help | --help            print this help
";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("{}", USAGE);
        return ExitCode::from(1);
    }
    let sub = args[1].clone();
    let rest = &args[2..];

    match sub.as_str() {
        "repl" => {
            let _ = usgl::repl::repl();
            ExitCode::SUCCESS
        }
        "test" | "tests" => ExitCode::from(run_tests(rest)),
        "check" | "parse" | "lint" => match rest.first() {
            Some(path) => ExitCode::from(check(path)),
            None => {
                eprintln!("usage: us {} <file.us>", sub);
                ExitCode::from(1)
            }
        },
        "fmt" => {
            if rest.is_empty() {
                eprintln!("usage: us fmt [-w] <file.us>");
                return ExitCode::from(1);
            }
            let (write_back, path) = if rest[0] == "-w" {
                (true, rest.get(1))
            } else if rest[0] == "--write" {
                (true, rest.get(1))
            } else {
                (false, rest.first())
            };
            match path {
                Some(p) => ExitCode::from(fmt_file(p, write_back)),
                None => {
                    eprintln!("usage: us fmt [-w] <file.us>");
                    ExitCode::from(1)
                }
            }
        }
        "run" => match rest.first() {
            Some(path) => ExitCode::from(run_file(path, &rest[1..])),
            None => {
                eprintln!("usage: us run <file.us> [args...]");
                ExitCode::from(1)
            }
        },
        "version" | "--version" | "-V" => {
            println!("us {}", usgl::VERSION);
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            print!("{}", USAGE);
            ExitCode::SUCCESS
        }
        _ => {
            // Treat it as a program file; remaining args go to the program.
            args.remove(0);
            let path = args[0].clone();
            let prog_args = &args[1..];
            ExitCode::from(run_file(&path, prog_args))
        }
    }
}

fn run_file(path: &str, prog_args: &[String]) -> u8 {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path, e);
            return 1;
        }
    };
    let mut interp = usgl::eval::Interp::new(path);
    interp.cli_args = prog_args.to_vec();
    let code = match usgl::parser::parse(&source, path) {
        Ok(program) => match interp.run(&program, true) {
            Ok(code) => Some(code),
            Err(e) => {
                eprintln!("{}", e);
                Some(1)
            }
        },
        Err(e) => {
            eprintln!("{}", e);
            Some(1)
        }
    };
    flush_out(&interp);
    code.unwrap_or(1) as u8
}

fn flush_out(interp: &usgl::eval::Interp) {
    use std::io::Write;
    let bytes = interp.drain_out();
    if !bytes.is_empty() {
        let _ = std::io::stdout().lock().write_all(&bytes);
        let _ = std::io::stdout().lock().flush();
    }
}

fn check(path: &str) -> u8 {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path, e);
            return 1;
        }
    };
    match usgl::parser::parse(&source, path) {
        Ok(_) => {
            println!("OK  {}", path);
            0
        }
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    }
}

fn fmt_file(path: &str, write_back: bool) -> u8 {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path, e);
            return 1;
        }
    };
    let formatted = match usgl::format_source(&source, path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };
    if write_back {
        if let Err(e) = std::fs::write(path, formatted) {
            eprintln!("error: cannot write `{}`: {}", path, e);
            return 1;
        }
        0
    } else {
        print!("{}", formatted);
        0
    }
}

fn run_tests(files: &[String]) -> u8 {
    let paths: Vec<String> = if files.is_empty() {
        match std::fs::read_dir(".") {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "us"))
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            Err(e) => {
                eprintln!("error: cannot list cwd: {}", e);
                return 1;
            }
        }
    } else {
        files.to_vec()
    };
    if paths.is_empty() {
        println!("no .us files found in cwd");
        return 0;
    }

    let mut total_pass = 0usize;
    let mut total_fail = 0usize;
    let mut exit = 0u8;
    for path in &paths {
        let (p, f) = run_test_file(path);
        total_pass += p;
        total_fail += f;
        if f > 0 {
            exit = 1;
        }
    }
    println!("\n{} passed, {} failed", total_pass, total_fail);
    exit
}

fn run_test_file(path: &str) -> (usize, usize) {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path, e);
            return (0, 1);
        }
    };
    let program = match usgl::parser::parse(&source, path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return (0, 1);
        }
    };
    let tests = usgl::ast::collect_tests(&program);
    if tests.is_empty() {
        return (0, 0);
    }
    let mut interp = usgl::eval::Interp::new(path);
    let global = interp.global.clone();

    // Define top-level fns/structs/enums so tests can reference them.
    let mut decls = Vec::new();
    for stmt in &program.stmts {
        decls.push(stmt.clone());
    }
    let decl_program = usgl::ast::Program {
        stmts: decls,
        eof_comments: Vec::new(),
    };
    if let Err(a) = interp.exec_in_env(&decl_program, &global) {
        eprintln!("{}: setup error: {}", path, usgl::abort_message(&a));
        return (0, tests.len());
    }

    let mut pass = 0usize;
    let mut fail = 0usize;
    for (name, body, depth) in &tests {
        let env = usgl::env::Env::new(Some(global.clone()), Some("<test>".to_string()));
        let tp = usgl::ast::Program {
            stmts: body.clone(),
            eof_comments: Vec::new(),
        };
        match interp.exec_in_env(&tp, &env) {
            Ok(_) => {
                pass += 1;
                println!("  ok  {} {}", "  ".repeat(*depth), name);
            }
            Err(a) => {
                fail += 1;
                println!("  FAIL {} {}", "  ".repeat(*depth), name);
                eprintln!("       {}", usgl::abort_message(&a));
            }
        }
    }
    flush_out(&interp);
    println!("{path}: {} passed, {} failed", pass, fail);
    (pass, fail)
}
