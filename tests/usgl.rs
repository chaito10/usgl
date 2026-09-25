//! Integration tests for the USGL interpreter (lib API, no external crates).

use std::cell::RefCell;
use std::rc::Rc;

use usgl::run_source_with_output;

fn run_ok(src: &str, expect_exit: i32) -> String {
    let out: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
    let code = run_source_with_output(src, "<test>", out.clone())
        .unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(code, expect_exit, "wrong exit code");
    let text = String::from_utf8(out.borrow().clone()).expect("non-utf8 output");
    text
}

#[test]
fn hello_world() {
    let out = run_ok("println(\"Hello, world!\")", 0);
    assert_eq!(out, "Hello, world!\n");
}

#[test]
fn arithmetic_precedence() {
    let out = run_ok("println(str(2 + 3 * 4))", 0);
    assert_eq!(out, "14\n");
    let out = run_ok("println(str((2 + 3) * 4))", 0);
    assert_eq!(out, "20\n");
}

#[test]
fn let_and_var() {
    let out = run_ok("let a = 1\nvar b = 2\nb = b + a\nprintln(str(b))", 0);
    assert_eq!(out, "3\n");
}

#[test]
fn let_redefinition_rejected() {
    let code = run_source_with_output("let a = 1\nlet a = 2", "<t>", Rc::new(RefCell::new(vec![])));
    let err = code.unwrap_err().to_string();
    assert!(err.contains("already defined"), "got: {err}");
}

#[test]
fn if_else_expression() {
    let out = run_ok(
        "let x = 5\nlet y = if x > 3 { 10 } else { 0 }\nprintln(str(y))",
        0,
    );
    assert_eq!(out, "10\n");
}

#[test]
fn while_loop_break_continue() {
    let out = run_ok(
        "var total = 0\nvar i = 0\nwhile i < 10 {\n    i = i + 1\n    if i % 2 == 0 { continue }\n    if i > 7 { break }\n    total = total + i\n}\nprintln(str(total))",
        0,
    );
    // sums 1+3+5+7 = 16
    assert_eq!(out, "16\n");
}

#[test]
fn for_range_loop() {
    let out = run_ok(
        "var acc = 0\nfor i in 1..5 {\n    acc = acc + i\n}\nprintln(str(acc))",
        0,
    );
    assert_eq!(out, "10\n");
}

#[test]
fn loop_construct() {
    let out = run_ok(
        "var n = 0\nloop {\n    n = n + 1\n    if n == 3 { break }\n}\nprintln(str(n))",
        0,
    );
    assert_eq!(out, "3\n");
}

#[test]
fn structs_and_field_access() {
    let out = run_ok(
        "struct Point { x: int, y: int }\nlet p = Point { x: 3, y: 4 }\nprintln(str(p.x + p.y))",
        0,
    );
    assert_eq!(out, "7\n");
}

#[test]
fn match_on_enum() {
    let src = r#"
enum Status { Pending, Running, Complete, Failed(string) }
let s = Status.Failed("boom")
match s {
    Status.Pending => println("p")
    Status.Failed(msg) => println("f: " + msg)
    _ => println("o")
}
"#;
    let out = run_ok(src, 0);
    assert_eq!(out, "f: boom\n");
}

#[test]
fn functions_recursion() {
    let out = run_ok(
        "fn fib(n: int) -> int {\n    if n < 2 {\n        return n\n    }\n    return fib(n - 1) + fib(n - 2)\n}\nprintln(str(fib(10)))",
        0,
    );
    assert_eq!(out, "55\n");
}

#[test]
fn anonymous_functions_and_higher_order() {
    let out = run_ok(
        "let double = fn(x: int) -> int { x * 2 }\nlet out = [1, 2, 3] |> map(fn(x) => x * 3)\nprintln(str(out))\nprintln(str(double(21)))",
        0,
    );
    assert_eq!(out, "[3, 6, 9]\n42\n");
}

#[test]
fn json_parse_and_index() {
    let out = run_ok(
        "let d = json.parse(\"{\\\"a\\\": [1, 2], \\\"ok\\\": true}\")?\nprintln(str(d[\"a\"][1]))\nprintln(str(d[\"ok\"]))",
        0,
    );
    assert_eq!(out, "2\ntrue\n");
}

#[test]
fn json_stringify_roundtrip() {
    let out = run_ok(
        "let d = json.parse(\"[1, \\\"x\\\", true]\")?\nprintln(json.stringify(d))",
        0,
    );
    assert_eq!(out, "[1,\"x\",true]\n");
}

#[test]
fn result_propagation_with_question() {
    let src = r#"
fn half(n: int) -> Result[int] {
    if n % 2 != 0 {
        return Err("odd")
    }
    return Ok(n / 2)
}
let v = half(8)?
println(str(v))
"#;
    let out = run_ok(src, 0);
    assert_eq!(out, "4\n");
}

#[test]
fn result_error_is_err() {
    let out = run_ok(
        "fn bad() -> Result[int] {\n    return Err(\"nope\")\n}\nlet r = bad()\nprintln(str(r.is_err))",
        0,
    );
    assert_eq!(out, "true\n");
}

#[test]
fn unwrap_property() {
    let out = run_ok(
        "let o = Some(7)\nprintln(str(o.unwrap))\nlet r = Ok(\"hi\")\nprintln(r.unwrap)",
        0,
    );
    assert_eq!(out, "7\nhi\n");
}

#[test]
fn map_properties() {
    let out = run_ok(
        "let m = { \"b\": 2, \"a\": 1 }\nprintln(str(m.keys.sort))\nprintln(str(m.len))",
        0,
    );
    assert_eq!(out, "[a, b]\n2\n");
}

#[test]
fn array_sort() {
    let out = run_ok("let a = [3, 1, 2]\nprintln(str(a.sort))", 0);
    assert_eq!(out, "[1, 2, 3]\n");
}

#[test]
fn string_methods() {
    let out = run_ok("println(\"  hi  \".trim().upper())", 0);
    assert_eq!(out, "HI\n");
}

#[test]
fn strings_length_property() {
    let out = run_ok("println(str(\"abcd\".length))", 0);
    assert_eq!(out, "4\n");
}

#[test]
fn map_literal_shadow_on_call() {
    // A stored map key wins over the built-in method, matching RFC data-as-code.
    let out = run_ok("let m = { \"keys\": \"stored\" }\nprintln(m.keys)", 0);
    assert_eq!(out, "stored\n");
}

#[test]
fn shell_capture() {
    #[cfg(target_os = "windows")]
    let out = run_ok(
        "println(str(process.run(\"cmd\", [\"/c\", \"echo\", \"us\"]).stdout.trim()))",
        0,
    );
    #[cfg(not(target_os = "windows"))]
    let out = run_ok(
        "println(str(process.run(\"sh\", [\"-c\", \"echo us\"]).stdout.trim()))",
        0,
    );
    assert_eq!(out, "us\n");
}

#[test]
fn check_source_rejects_bad_syntax() {
    let r = usgl::check_source("let = 3", "<t>");
    assert!(r.is_err());
    let r = usgl::check_source("fn a() {\n    return 1\n}", "<t>");
    assert!(r.is_ok());
}

#[test]
fn format_source_is_stable() {
    let src = "let a=[1,2,3]\nif a[0]>0 {\nprintln(\"yes\")\n}\n";
    let once = usgl::format_source(src, "<t>").unwrap();
    let twice = usgl::format_source(&once, "<t>").unwrap();
    assert_eq!(once, twice, "formatter must be idempotent");
}

#[test]
fn tokenize_basics() {
    let toks = usgl::tokenize("let x = 1 + 2", "<t>").unwrap();
    let kinds: Vec<&str> = toks
        .iter()
        .map(|t| match &t.kind {
            usgl::token::Tok::Let => "let",
            usgl::token::Tok::Ident(_) => "ident",
            usgl::token::Tok::Eq => "=",
            usgl::token::Tok::Int(_) => "int",
            usgl::token::Tok::Plus => "+",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["let", "ident", "=", "int", "+", "int", "other"]);
}
