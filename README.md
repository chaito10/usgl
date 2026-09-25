# USGL — Universal Systems & Glue Language

A from-scratch implementation of the USGL language described in
[`rfc.md`](rfc.md). This repository is **Phase 1**: a dependency-free
interpreter written in Rust with a `us` command-line interface.

> No external crates. Only the standard library is used.

```
┌──────────────┐   lexer    ┌─────────┐   parser   ┌──────┐   eval    ┌──────────┐
│  .us source  │ ────────►  │ Tokens  │ ────────►  │ AST  │ ──────►  │  Values  │
└──────────────┘            └─────────┘            └──────┘           └──────────┘
```

## Build & test

```sh
cargo build            # builds target/debug/us.exe
cargo test             # 26+ integration tests (lib API)
```

## Usage

```sh
us examples/smoke.us          # run a script
us run examples/tests.us      # run a script (same as above)
us repl                       # interactive REPL
us test [FILE]                # run `test "name" { ... }` blocks (`.us` in cwd if omitted)
us check FILE                 # syntax-check only
us fmt FILE                   # pretty-print to stdout
us fmt -w FILE                # rewrite the file in place
us version
us help
```

Exit code is `0` on success; a failed assertion or an unhandled error exits
non-zero.

## What works today (Phase 1)

| Area | Status | Notes |
| --- | --- | --- |
| `let` / `var` / `const` | yes | immutability is a parse-time idea; runtime enforces nothing |
| Numbers, strings, chars, bools, nil | yes | `i64` ints, `f64` floats |
| Arrays & maps | yes | `.sort`, `.keys`, `.values`, `.len`, indexing, `[]` literal |
| Structs | yes | field access, struct literals |
| Enums | yes | `Status.Failed(x)` constructors, `match` with bindings + `_` |
| `fn`, return type arrow | yes | recursion, closures, arrow bodies `fn(x) => x * 2` |
| Higher-order: `map` / `filter` | yes | global (pipeline-friendly) |
| Pipelines `a \|> f(x)` | yes | rewritten at parse time into `f(a, x)` |
| `if` / `else if` / `else` | yes | both as statements and expressions (`let y = if ...`) |
| `while`, `for .. in ..`, `loop` | yes | `break` / `continue` |
| `?` error propagation | yes | unwraps `Result`, else aborts with the error |
| Option / Result | yes | `Some` / `None` / `Ok` / `Err`, `.unwrap`, `.is_ok`, `.ok` |
| String methods | yes | `trim`, `upper`, `lower`, `split`, `replace`, `contains`, `length` … |
| JSON module | yes | `json.parse` (static `Result`) + `json.stringify` (compact / `pretty=true`) |
| fs module | yes | `read`, `write`, `append`, `exists`, `list`, `directories`, `glob`, `mkdir`, `remove` |
| process module | yes | `process.run`, captured stdout/stderr/status |
| os / time / math / Bytes / Buffer | partial | `os.args`, `time.now`, `math.*` unhappy when mixed types |
| `import` | yes | relative `.us` files resolved from the importing file's directory |
| `test "name" { }` blocks | yes | `us test` runs them and reports pass/fail |
| REPL | yes | multi-line input, `:clear`, `:reset`, prints `=> value` |
| `us fmt` | yes | canonical formatting; idempotent |
| Type checking | no | types are parsed into the AST and retained for Phase 2 |
| Concurrency / channels / async | no | parsed/awaited as placeholders where possible |
| FFI / extern | no | reserved |

## Pipeline details

`let names = users |> map(fn(u) => u.name)` is accepted and desugared at parse
time to `map(users, fn(u) => u.name)`, which the formatter will print back.
Source `|>` is the canonical spelling and round-trips through `us fmt`.

## `test` blocks

Intended for trailing assertion suites in a script:

```usgl
test "fibonacci" {
    assert_eq(fib(10), 55)
}
```

`us test` runs every `test` block in an isolated scope, reporting
`ok`/`FAIL` per suite. `assert_eq` aborts the suite on mismatch.

## Project layout

```
src/
  lexer.rs     tokenization (newline-significant)
  parser.rs    recursive-descent → AST
  ast.rs       AST + typed params/fields/variants
  eval.rs      tree-walking interpreter, closures, control flow
  value.rs     Value, Function, TypeInfo, ordering, equality
  env.rs       scope chain (modules are envs too)
  builtins*.rs stdlib modules (json, fs, time, os, process, math, strings)
  fmt.rs       canonical formatter
  repl.rs      interactive loop
  token.rs     token kinds + spans
  error.rs     TargetResult<_, UsglError> spans
  main.rs      `us` CLI
tests/usgl.rs  integration tests (lib API)
examples/     runnable scripts + test suites
```

## Phase 2 (planned)

Static type checking against the parsed type annotations, a bytecode compiler or
WASM target, marks/threads, FFI, and a linker for multi-file projects.