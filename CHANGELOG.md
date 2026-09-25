# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.2.0-rc.1] - 2026-09-25

### Added
- Phase 2 static type checker (`src/check.rs`): scoped binding analysis with
  mutability tracking, function/builtin/method signatures, struct and enum
  validation, `?`-unwrapping rules, `for` iterable inference, arity checks.
- `us check` now runs the type checker (multi-error output, exit 1).
- `us run` and `us test` hard-gate on a clean type-check before executing.
- `typecheck_source` / `check_source` lib APIs; integration tests for the
  checker (28 tests total).
- Android release builds trimmed to `armv7-linux-androideabi`; README matrix
  now "13-target".

## [0.1.0] - 2026-09-25

### Added
- Phase 1 interpreter MVP: lexer, parser, AST, tree-walking evaluator, REPL,
  canonical formatter, `test` blocks, CLI (`us run/repl/test/check/fmt`).
- Standard library: json, fs, time, os, process, math, strings, Buffer, Bytes.
- Global `map` / `filter`, pipelines, `if`-expressions, Option/Result with `?`.
- CI (`ci.yml`) and 13-target cross-platform release matrix (`release.yml`)
  covering Linux, Windows, macOS, iOS, Android, and WASM (WASI + bare).
- In-browser WASI demo under `www/`.