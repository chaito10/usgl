# USGL — Universal Systems & Glue Language

A from-scratch implementation of the USGL language described in
[`rfc.md`](rfc.md). This repository is **Phase 1**: a dependency-free
interpreter written in Rust with a `us` command-line interface,
CI-tested and cross-released for Linux, macOS, Windows, Android, iOS,
and the web (WASM).

> No external crates. Only the standard library is used.

[![CI](https://github.com/chaito10/usgl/actions/workflows/ci.yml/badge.svg)](https://github.com/chaito10/usgl/actions/workflows/ci.yml)
[![Release](https://github.com/chaito10/usgl/actions/workflows/release.yml/badge.svg)](https://github.com/chaito10/usgl/actions/workflows/release.yml)

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
.github/workflows/
  ci.yml        fmt check + tests (ubuntu/windows/macos) + wasm smoke
  release.yml   16-target cross-build matrix + GitHub release
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
www/          in-browser WASI demo (runs the real us.wasm)
```

## CI / release matrix

`.github/workflows/ci.yml` runs on every push/PR: `cargo fmt --check`, a release
test suite on ubuntu/windows/macos, and a WASI build smoked under `wasmtime`.

`.github/workflows/release.yml` builds every target below and publishes them to a
GitHub Release when a `v*` tag is pushed (or on `workflow_dispatch`):

| Platform | Targets (arch) | Built with |
| --- | --- | --- |
| Linux | x86_64, i686, aarch64, arm, armv7, riscv64 | `cross` (GNU) |
| Windows | x86_64, i686, aarch64 | `cargo-xwin` (MSVC `.exe`) |
| macOS | x86_64, aarch64, **universal2** | native macOS runner + `lipo` |
| Android | aarch64, armv7, i686, x86_64 | `cross` (NDK images) |
| iOS | aarch64 (device), x86_64 + arm64 (sim) | native macOS runner |
| Web | `us.wasm` (WASI), `us-bare.wasm` | `cargo build --target wasm32-*` |

The release job also attaches `SHA256SUMS.txt` and the `www/` browser bundle.

## Running in a browser

The `www/` demo loads `us.wasm` (the WASI build) and runs it with
`@wasmer/wasi` — same binary as the CLI. After a release it is served from the
release assets; locally you can reproduce it with:

```sh
cargo build --release --target wasm32-wasip1
cp target/wasm32-wasip1/release/us.wasm www/us.wasm
python -m http.server 8000 --directory www   # open http://localhost:8000
```

Or run the wasm headlessly:

```sh
bash$ wasmtime run us.wasm my_script.us
```

## Phase 2 (planned)

Static type checking against the parsed type annotations, a bytecode compiler or
WASM target, marks/threads, FFI, and a linker for multi-file projects.