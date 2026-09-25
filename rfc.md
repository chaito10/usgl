# RFC: Universal Small Systems & Glue Language

**Status:** Draft Proposal  
**Version:** 0.1  
**Working Name:** **USGL — Universal Systems & Glue Language**  
**Alternative names:** `U`, `Nimble`, `Flux`, `Luma`, `Mica`  
**Author:** Community Proposal  
**Date:** 2026-09-23

---

## 1. Abstract

This RFC proposes a new general-purpose programming language designed around one central objective:

> **One small language for systems, embedded software, scripting, automation, application glue, and the browser.**

Existing languages tend to optimize for one or two of these domains:

| Language | Strength | Limitation |
|---|---|---|
| Lua | Tiny, embeddable, scripting | Limited systems-level safety |
| Rust | Systems programming, memory safety | Larger language/toolchain; relatively high complexity |
| Python | High-level glue and automation | Runtime size and performance overhead |
| JavaScript | Browser and application scripting | Runtime-dependent; systems programming limitations |
| C | Small, universal deployment | Manual memory management and safety problems |
| Go | Simple systems programming | Garbage collector/runtime; less suitable for tiny embedded environments |
| Zig | Systems programming, simplicity | Less focused on high-level scripting/glue |
| Wren | Small embeddable scripting | Less suitable as a complete systems language |

USGL proposes a unified alternative:

**Tiny runtime + memory safety + native compilation + scripting + WebAssembly + FFI + single-file deployment.**

The language should be usable from an approximately microcontroller-sized environment up to servers, desktop applications, command-line tools, browser applications, and embedded scripting engines.

---

# 2. Design Philosophy

USGL follows seven principles.

### 2.1 Small

The language specification should be small enough for a developer to understand completely.

The implementation should target:

- small compiler
- small runtime
- minimal standard library
- fast compilation
- minimal dependencies
- static linking where practical

The language should avoid requiring a large virtual machine.

---

### 2.2 Safe by Default

USGL should provide memory safety without requiring a garbage collector.

The preferred model is:

> **Ownership + borrowing + deterministic destruction**

inspired by Rust, but deliberately simplified.

The goal is not to reproduce Rust's entire type system.

---

### 2.3 High-Level When Appropriate

USGL should support concise scripting:

```usgl
files := fs.list("./src")

for file in files {
    if file.ends_with(".rs") {
        println(file)
    }
}
```

The same language should also be capable of lower-level programming:

```usgl
fn checksum(data: &[u8]) -> u32 {
    var result: u32 = 0

    for byte in data {
        result = result + byte
    }

    return result
}
```

---

### 2.4 Universal Runtime

A USGL program should be able to target:

```text
Native
 ├── Linux
 ├── Windows
 ├── macOS
 ├── BSD
 ├── Android
 └── Embedded

Web
 └── WebAssembly
       └── Browser / WASI

Embedded
 └── Host application
       └── USGL module
```

---

### 2.5 Everything Is a Library

Networking, filesystem access, GUI, database access, HTTP, JSON, cryptography, etc. should not be baked deeply into the language.

The core language should remain small.

---

### 2.6 Interoperability First

USGL must be able to communicate with existing ecosystems.

Primary FFI targets:

```text
C
C++
Rust
WASM
JavaScript
Python
```

C ABI compatibility should be a first-class feature.

---

### 2.7 One Language, Multiple Levels

USGL should support three programming styles.

```text
Level 1 — Script
        ↓
Level 2 — Application
        ↓
Level 3 — Systems
```

A developer should not need to change languages when moving between these levels.

---

# 3. Goals

USGL should provide:

- memory safety
- no mandatory garbage collector
- deterministic resource management
- native compilation
- WebAssembly compilation
- browser execution
- embedded execution
- scripting
- high-level collections
- low-level memory access when explicitly requested
- C ABI interoperability
- async programming
- concurrency
- pattern matching
- modules
- package management
- single-file programs
- cross-platform standard library
- small runtime
- fast startup
- predictable performance

---

# 4. Non-Goals

USGL should NOT attempt to:

- replace every specialized language
- provide a massive standard library
- provide a built-in GUI framework
- provide a built-in database engine
- require a garbage collector
- replicate every Rust feature
- replicate every Python feature
- replicate JavaScript semantics
- become a massive enterprise language

The core language should remain intentionally small.

---

# 5. Proposed Language Model

USGL uses a combination of:

```text
Static typing
+
Type inference
+
Ownership
+
Borrowing
+
Automatic resource management
+
Generics
+
Algebraic data types
+
Optional dynamic values
```

Example:

```usgl
fn main() {
    let name = "USGL"
    println("Hello, " + name)
}
```

The compiler infers:

```text
name : String
```

when possible.

Explicit types remain available:

```usgl
let port: u16 = 8080
```

---

# 6. Memory Management

Memory safety is a fundamental requirement.

USGL should use:

### Stack allocation

For local values:

```usgl
fn add(a: i32, b: i32) -> i32 {
    return a + b
}
```

### Ownership

```usgl
let data = Buffer.new(1024)

process(data)

# data cannot be used here if ownership moved
```

### Borrowing

```usgl
fn process(data: &[u8]) {
    ...
}
```

### Mutable borrowing

```usgl
fn modify(data: &mut [u8]) {
    ...
}
```

### Automatic destruction

Resources should be released automatically when their owner leaves scope.

```usgl
{
    let file = fs.open("data.txt")
    file.write("hello")
}
# file automatically closed
```

---

# 7. Simplified Ownership Model

USGL should deliberately simplify Rust.

The language should avoid exposing unnecessary lifetime complexity to normal programmers.

Most code should look like:

```usgl
fn read_file(path: String) -> Result<String> {
    let file = fs.open(path)?
    return file.read_all()
}
```

Advanced lifetime annotations should exist only for low-level library authors.

---

# 8. Type System

Core types:

```text
bool

i8
i16
i32
i64

u8
u16
u32
u64

f32
f64

char

String

Bytes

Array[T]

Map[K,V]

Option[T]

Result[T,E]
```

Example:

```usgl
let age: u32 = 32

let names: Array[String] = [
    "Alice",
    "Bob",
    "Charlie"
]
```

---

# 9. Structs

```usgl
struct User {
    id: u64
    name: String
    active: bool
}
```

Usage:

```usgl
let user = User {
    id: 1,
    name: "Alice",
    active: true
}
```

---

# 10. Enums

```usgl
enum Status {
    Pending
    Running
    Complete
    Failed(String)
}
```

Pattern matching:

```usgl
match status {
    Status.Pending => println("Waiting")
    Status.Running => println("Running")
    Status.Complete => println("Done")
    Status.Failed(error) => println(error)
}
```

---

# 11. Functions

Functions are first-class values.

```usgl
fn square(x: i32) -> i32 {
    return x * x
}
```

Short form:

```usgl
let square = fn(x) => x * x
```

Higher-order programming:

```usgl
let result = numbers.map(fn(x) => x * 2)
```

---

# 12. Error Handling

USGL should avoid exceptions as the primary error mechanism.

Use:

```usgl
Result[T,E]
```

Example:

```usgl
fn read_config() -> Result[Config, Error] {
    let data = fs.read("config.json")?
    return json.parse[Config](data)
}
```

The `?` operator propagates errors.

---

# 13. Optional Values

```usgl
fn find_user(id: u64) -> Option[User]
```

Usage:

```usgl
match find_user(10) {
    Some(user) => println(user.name)
    None => println("Not found")
}
```

---

# 14. Dynamic Values

Although USGL is statically typed, dynamic values should be available for scripting.

```usgl
let value: Any = json.parse(data)
```

This allows dynamic structures:

```usgl
value["users"][0]["name"]
```

Dynamic programming should be possible without turning the entire language dynamic.

---

# 15. Modules

Modules should be extremely simple.

```usgl
import fs
import net.http
import json
```

Local modules:

```text
project/
    main.us
    database.us
    network.us
```

```usgl
import database
import network
```

---

# 16. Package System

The package manager should be built into the toolchain.

Proposed commands:

```bash
us init
us add http
us remove http
us build
us run
us test
us fmt
us check
us publish
```

Package manifest:

```toml
name = "myapp"
version = "0.1.0"

[dependencies]
http = "1.2"
json = "2.0"
```

---

# 17. Single-File Programming

A major design goal is zero-friction scripting.

This should work:

```bash
us hello.us
```

Example:

```usgl
#!/usr/bin/env us

println("Hello World")
```

No project required.

No configuration required.

No package manifest required.

No build directory required.

---

# 18. System Programming

USGL should expose low-level functionality when explicitly requested.

Example:

```usgl
unsafe fn read_register(address: usize) -> u32 {
    return *(address as *u32)
}
```

Unsafe operations must be explicitly marked.

```usgl
unsafe {
    ...
}
```

Safe code remains the default.

---

# 19. Unsafe Boundary

Unsafe code should be isolated.

```usgl
fn safe_api() {
    unsafe {
        low_level_operation()
    }
}
```

The compiler should clearly identify unsafe sections.

This allows:

```text
Safe application
        ↓
Small unsafe boundary
        ↓
Hardware / OS / FFI
```

---

# 20. Embedded Programming

USGL should support environments with:

- no operating system
- no filesystem
- no network
- limited RAM
- limited flash
- no heap

Example:

```usgl
@no_std
@no_heap

fn main() {
    gpio.write(LED, true)
}
```

A microcontroller program should not require the full USGL runtime.

---

# 21. Runtime Profiles

USGL should define runtime profiles.

### Micro

```text
No heap
No OS
Minimal runtime
Static allocation
```

### Embedded

```text
Optional heap
Hardware APIs
RTOS integration
```

### Native

```text
Filesystem
Networking
Threads
Processes
Dynamic libraries
```

### Web

```text
WebAssembly
Browser APIs
JavaScript interop
```

### Server

```text
Networking
Async
Database
Concurrency
```

---

# 22. Garbage Collection

A garbage collector should NOT be required.

However, optional garbage-collected containers could be supported.

For example:

```usgl
gc {
    let graph = Graph.new()
}
```

This should be considered an advanced runtime feature rather than a language requirement.

The default model remains deterministic memory management.

---

# 23. Concurrency

USGL should provide lightweight concurrency.

Example:

```usgl
spawn {
    server.run()
}
```

Channels:

```usgl
let channel = Channel.new[String]()

spawn {
    channel.send("hello")
}

println(channel.receive())
```

The exact runtime implementation may use:

```text
OS threads
async tasks
event loops
green threads
WebAssembly workers
```

depending on the target.

---

# 24. Async Programming

Async syntax should be simple.

```usgl
async fn download(url: String) -> Result[Bytes] {
    let response = await http.get(url)?
    return await response.bytes()
}
```

The same syntax should work across native and WASM targets where supported.

---

# 25. Scripting

USGL should provide Python-like convenience.

Example:

```usgl
for file in fs.glob("*.txt") {
    let text = fs.read(file)
    println(file + ": " + text.length)
}
```

Pipeline style:

```usgl
let result =
    users
    |> filter(fn(u) => u.active)
    |> map(fn(u) => u.email)
    |> sort()
```

---

# 26. Shell Integration

USGL should be usable as a shell scripting language.

Example:

```usgl
let files = shell("git ls-files")

for file in files.lines() {
    println(file)
}
```

A safer structured API should also exist:

```usgl
let result = process.run(
    "git",
    ["status", "--short"]
)
```

---

# 27. Browser Support

The primary browser compilation target should be WebAssembly.

```bash
us build --target wasm
```

Generated:

```text
app.wasm
app.js
```

Browser usage:

```html
<script type="module">
    import { init } from "./app.js"
    await init()
</script>
```

USGL should provide JavaScript interoperability.

```usgl
js.window.alert("Hello")
```

---

# 28. WASI

USGL should support WASI for portable server-side WebAssembly.

Targets:

```text
wasm32-wasi
wasm32-browser
wasm32-component
```

This allows the same application to execute:

```text
Browser
Server
Edge
Cloud
Desktop sandbox
Plugin systems
```

---

# 29. Cross Compilation

Example:

```bash
us build --target linux-x64
us build --target windows-x64
us build --target macos-arm64
us build --target wasm
us build --target riscv64
us build --target arm64
```

Cross compilation should be a core feature rather than an external collection of tools.

---

# 30. Foreign Function Interface

C ABI compatibility is mandatory.

Example:

```usgl
extern fn printf(format: *u8, ...)
```

Libraries could expose:

```text
libc
libssl
libsqlite
libuv
libgit2
GPU APIs
OS APIs
```

This is critical for universal adoption.

---

# 31. Native Interoperability

USGL should be able to call existing libraries instead of reinventing everything.

Architecture:

```text
USGL
  |
  +-- C ABI
  |
  +-- WASM
  |
  +-- JavaScript
  |
  +-- OS APIs
```

---

# 32. Standard Library

The core standard library should be divided into layers.

### Core

```text
collections
strings
math
option
result
iterators
memory
```

### System

```text
fs
process
thread
time
os
env
```

### Network

```text
tcp
udp
http
websocket
dns
```

### Data

```text
json
toml
yaml
csv
```

### Optional

```text
sqlite
crypto
regex
compression
```

The core compiler should not require all modules.

---

# 33. CLI

The official tool should be called:

```text
us
```

Commands:

```bash
us run
us build
us test
us check
us fmt
us lint
us doc
us add
us remove
us update
us publish
us repl
```

REPL:

```bash
$ us repl

> let x = 10
> x * 2
20
```

---

# 34. Toolchain Architecture

Proposed architecture:

```text
                 USGL Source
                      |
                      v
               ┌─────────────┐
               │    Parser   │
               └──────┬──────┘
                      |
                      v
               ┌─────────────┐
               │ Type Checker│
               └──────┬──────┘
                      |
                      v
               ┌─────────────┐
               │   MIR/IR    │
               └──────┬──────┘
                      |
          ┌───────────┼───────────┐
          v           v           v
       Native       WASM       Embedded
       LLVM/         WASM       backend
       Cranelift
```

The compiler should ideally be written in USGL itself after bootstrapping.

---

# 35. Compiler Implementation

Initial bootstrap options:

```text
Phase 1: Rust implementation

Phase 2: Self-hosting USGL compiler

Phase 3: Minimal bootstrap compiler
```

The final ecosystem should avoid depending permanently on a large host language.

---

# 36. Runtime Size Targets

The following are proposed engineering targets rather than guarantees.

| Component | Target |
|---|---:|
| Minimal compiler executable | < 20 MB |
| Minimal native runtime | < 1 MB |
| Micro runtime | < 100 KB |
| Simple CLI binary | < 2 MB |
| Browser/WASM runtime | < 500 KB |
| Startup time | milliseconds |
| Dependency count | minimal |

Exact sizes should be benchmarked during implementation.

---

# 37. Language Complexity Target

The language should intentionally limit the number of concepts.

Target:

```text
~30 core language concepts
```

A programmer familiar with C/Python/Rust/JavaScript should be able to become productive quickly.

---

# 38. Syntax Philosophy

Syntax should combine familiar concepts from existing languages without copying their complexity.

Example complete application:

```usgl
import http
import json

struct User {
    name: String
    age: u32
}

async fn main() -> Result {
    let response = await http.get("https://example.com/users")?
    let users = json.parse[Array[User]](
        await response.text()?
    )?

    for user in users {
        println(user.name)
    }

    return Ok
}
```

---

# 39. Application Example

HTTP server:

```usgl
import http

fn main() {
    let server = http.server(":8080")

    server.get("/", fn(request) {
        return "Hello from USGL"
    })

    server.run()
}
```

Compile:

```bash
us build
```

Run:

```bash
./server
```

---

# 40. Embedded Example

```usgl
@no_std
@no_heap

import hal.gpio

fn main() {
    gpio.configure_output(LED)

    loop {
        gpio.toggle(LED)
        delay_ms(500)
    }
}
```

---

# 41. Automation Example

```usgl
import fs
import process

for project in fs.directories("./projects") {
    let result = process.run(
        "git",
        ["-C", project, "status", "--short"]
    )

    if result.stdout != "" {
        println(project + " has changes")
    }
}
```

This demonstrates the intended "glue language" role.

---

# 42. Web Example

```usgl
fn greet(name: String) -> String {
    return "Hello " + name
}
```

Compile:

```bash
us build --target wasm
```

The same function should be usable from JavaScript.

---

# 43. Interchangeable Execution

A central USGL goal is:

```text
write once

        ↓

┌──────────────────────────────┐
│           USGL               │
└──────────────────────────────┘

 ↓          ↓          ↓
Native     WASM      Embedded
 ↓          ↓          ↓
Linux     Browser    MCU
Windows   Server     RTOS
macOS     Edge       Device
```

Not every program will be portable to every target, but the language and core semantics should remain consistent.

---

# 44. Component Model

USGL should eventually support WebAssembly Components.

A USGL module could expose:

```text
functions
records
resources
interfaces
```

This would make USGL suitable for plugin architectures.

Example:

```text
Application
    |
    +-- USGL plugin
    |
    +-- Rust plugin
    |
    +-- Python plugin
    |
    +-- JavaScript plugin
```

---

# 45. Plugin Architecture

USGL should be suitable as an embedded scripting language.

Example host:

```text
C/C++ Application
        |
        v
    USGL Runtime
        |
        +-- user scripts
        +-- plugins
        +-- automation
        +-- configuration
```

This is one of the primary use cases for the language.

---

# 46. Configuration as Code

USGL should be usable instead of JSON/YAML/TOML for complex configuration.

Example:

```usgl
server {
    port = 8080
    workers = 4

    tls {
        enabled = true
        certificate = "server.crt"
    }
}
```

Because configuration is executable, advanced configurations can use functions and conditionals.

---

# 47. Security Model

USGL should support capability-based execution.

Instead of giving an application unrestricted access:

```text
Program
  |
  +-- filesystem: /data
  +-- network: example.com
  +-- environment: limited
```

A sandboxed program could be executed with:

```bash
us run --allow-read=/data \
       --allow-net=example.com
```

This is especially important for:

- plugins
- browser applications
- serverless functions
- AI agents
- downloaded scripts
- automation

---

# 48. AI-Agent Friendly

USGL should be designed to be easy for both humans and AI systems to generate.

Reasons:

- small syntax
- explicit types
- predictable formatting
- deterministic builds
- clear errors
- simple package system
- single-file programs
- easy sandboxing
- strong tooling

Example:

```usgl
fn main() {
    let result = http.get(url)?

    if result.status == 200 {
        println(result.text()?)
    }

    return Ok
}
```

---

# 49. Reproducible Builds

Builds should support:

```bash
us lock
us build --locked
```

The lockfile records:

```text
dependency
version
source
checksum
compiler version
target
```

---

# 50. Formatting

One official formatter:

```bash
us fmt
```

There should be minimal formatting configuration.

The language should follow the philosophy:

> **There should be one obvious way to format USGL code.**

---

# 51. Testing

Testing should be built into the language tooling.

```usgl
test "addition" {
    assert(add(2, 3) == 5)
}
```

Run:

```bash
us test
```

---

# 52. Documentation

Documentation comments:

```usgl
/// Calculates the checksum of a byte buffer.
fn checksum(data: &[u8]) -> u32 {
    ...
}
```

Generate documentation:

```bash
us doc
```

---

# 53. Debugging

The toolchain should provide:

```bash
us debug
us stacktrace
us profile
us trace
```

Native debugging should integrate with existing debuggers where possible.

---

# 54. Package Registry

A decentralized or federated package model should be considered.

Possible sources:

```text
official registry
Git repositories
local paths
IPFS/content-addressed sources
WASM components
```

Example:

```bash
us add github.com/example/http
```

However, the package registry must not be a mandatory centralized dependency.

---

# 55. Source Compatibility

USGL should maintain a strong stability policy.

Possible editions:

```text
USGL 1
USGL 2
USGL 3
```

Breaking language changes should be rare.

---

# 56. Performance Goals

USGL should target:

### Native

Performance approaching compiled systems languages for appropriate workloads.

### Script

Fast startup and reasonable performance without JIT requirements.

### Embedded

Predictable memory usage.

### WASM

Low startup overhead and compact binaries.

The language should prioritize predictable performance over extreme benchmark optimization.

---

# 57. Minimal Runtime Philosophy

The runtime should be modular.

Instead of:

```text
Application
    |
    v
500 MB runtime
```

the goal is:

```text
Application
    |
    v
only required runtime components
```

For example:

```text
hello world
→ strings + stdout

embedded firmware
→ core + hardware abstraction

HTTP server
→ core + async + networking
```

---

# 58. Proposed File Extensions

Source:

```text
.us
```

Package manifest:

```text
us.toml
```

Lock file:

```text
us.lock
```

Compiled WebAssembly:

```text
.wasm
```

---

# 59. Proposed Keywords

Initial keywords:

```text
fn
let
var
const

if
else
for
while
loop

match

struct
enum

import
export

return

async
await
spawn

unsafe

trait
impl

true
false

Some
None
Ok
Err
```

The exact keyword set should remain small.

---

# 60. Core Traits / Interfaces

A lightweight interface mechanism should be supported.

```usgl
trait Printable {
    fn print(self)
}
```

Implementation:

```usgl
impl Printable for User {
    fn print(self) {
        println(self.name)
    }
}
```

Traits should remain simpler than Rust's full trait system.

---

# 61. Generics

Generic programming:

```usgl
fn first[T](items: &[T]) -> Option[T] {
    if items.len() == 0 {
        return None
    }

    return Some(items[0])
}
```

Generic syntax should remain predictable.

---

# 62. Metaprogramming

USGL should initially avoid a complex macro system.

Instead, prefer:

```text
generics
traits
compile-time functions
code generation tools
```

A limited macro mechanism can be introduced later if necessary.

---

# 63. Reflection

Reflection should not be mandatory in the core runtime.

Optional metadata can be enabled:

```usgl
@reflect
struct User {
    name: String
}
```

This keeps embedded builds small.

---

# 64. Interoperability with Python

Python integration should be possible through bindings.

Example:

```text
Python
  |
  +-- USGL native library
```

USGL should also be able to act as an embedded scripting engine inside Python applications where practical.

---

# 65. Interoperability with JavaScript

For browser builds:

```usgl
export fn greet(name: String) -> String
```

JavaScript:

```javascript
const result = usgl.greet("Alice");
```

---

# 66. Interoperability with Rust

Rust applications should be able to embed USGL through a small runtime API.

```text
Rust host
    |
    v
USGL interpreter/runtime
    |
    v
USGL scripts
```

This makes USGL useful for plugin systems.

---

# 67. Interpreter + Compiler

USGL should support two execution modes.

### Interpreter

```bash
us script.us
```

Advantages:

- fast startup
- scripting
- REPL
- embedded use
- development

### Native compiler

```bash
us build
```

Advantages:

- performance
- standalone binaries
- embedded
- production deployment

The same language semantics should be shared.

---

# 68. Optional JIT

A JIT should not be required.

It may be added later for:

- long-running scripts
- dynamic workloads
- AI agents
- interactive applications

---

# 69. Implementation Strategy

### Phase 0 — Specification

Define:

- syntax
- type system
- ownership
- module system
- ABI
- runtime model

### Phase 1 — Interpreter

Implement:

```text
lexer
parser
AST
interpreter
REPL
basic standard library
```

### Phase 2 — Native Compiler

Implement:

```text
type checker
MIR
native backend
linker integration
```

### Phase 3 — WASM

Add:

```text
wasm32
WASI
JavaScript bindings
browser runtime
```

### Phase 4 — Embedded

Add:

```text
no_std
no_heap
static allocation
bare-metal targets
```

### Phase 5 — Self Hosting

Rewrite the compiler in USGL.

---

# 70. Reference Implementation

A possible implementation stack:

```text
Frontend
   ↓
Parser
   ↓
Type checker
   ↓
MIR
   ↓
Cranelift / LLVM / custom backend
   ↓
Native
```

For WASM:

```text
MIR
 ↓
WASM backend
 ↓
WASI / Browser
```

The implementation should avoid making LLVM mandatory for tiny targets.

---

# 71. Example: Universal Utility

A file converter:

```usgl
import fs
import json

fn main() {
    let input = fs.read("data.json")?
    let data = json.parse(input)?

    let output = json.stringify(data, pretty=true)

    fs.write("output.json", output)?

    return Ok
}
```

This same program can potentially target:

```text
Linux
Windows
macOS
WASM/WASI
```

without changing its core logic.

---

# 72. Example: Embedded Host

A C application:

```text
┌──────────────────────────┐
│ Existing C Application   │
│                          │
│   ┌──────────────────┐   │
│   │ USGL Runtime     │   │
│   │                  │   │
│   │ script.us        │   │
│   └──────────────────┘   │
└──────────────────────────┘
```

The application can expose APIs:

```text
gpio.write()
sensor.read()
network.send()
device.configure()
```

USGL then becomes the application's glue layer.

---

# 73. The "Universal Glue" Use Case

The primary distinction from traditional systems languages is that USGL should be equally comfortable doing:

```text
C library binding
        +
filesystem automation
        +
HTTP requests
        +
JSON processing
        +
CLI tools
        +
embedded firmware
        +
WebAssembly
        +
application scripting
```

A developer should not need:

```text
C + Python + JavaScript + Bash + Rust
```

for ordinary integration work.

USGL attempts to cover the common intersection.

---

# 74. Design Tradeoff

The language intentionally chooses:

```text
Smallness
        >
Language feature count

Safety
        >
Raw unrestricted memory access

Portability
        >
Platform-specific optimization

Predictability
        >
Runtime magic
```

However, unsafe and platform-specific mechanisms remain available through explicit boundaries.

---

# 75. Comparison

| Capability | Lua | Python | Rust | JS | USGL |
|---|---:|---:|---:|---:|---:|
| Small runtime | ★★★★★ | ★★ | ★★★ | ★★ | ★★★★★ |
| Memory safety | ★★ | ★★★★ | ★★★★★ | ★★★★ | ★★★★★ |
| Systems programming | ★★ | ★★ | ★★★★★ | ★ | ★★★★★ |
| Scripting | ★★★★★ | ★★★★★ | ★★★ | ★★★★★ | ★★★★★ |
| Embedded | ★★★★★ | ★★ | ★★★★ | ★★ | ★★★★★ |
| Browser | ★ | ★ | ★★★ | ★★★★★ | ★★★★★ |
| WASM | ★★ | ★★ | ★★★★★ | ★★★★★ | ★★★★★ |
| C interoperability | ★★★★★ | ★★★★ | ★★★★★ | ★★ | ★★★★★ |
| Deterministic memory | ★★★ | ★ | ★★★★★ | ★ | ★★★★★ |
| Single-file programs | ★★★★★ | ★★★★★ | ★★★ | ★★★★★ | ★★★★★ |
| Cross-platform | ★★★★★ | ★★★★★ | ★★★★★ | ★★★★★ | ★★★★★ |

These are design-target comparisons, not benchmark measurements.

---

# 76. Core Identity

USGL should not attempt to become:

> "Rust but easier."

Nor:

> "Python but faster."

Nor:

> "JavaScript for embedded systems."

Its identity should instead be:

> **A minimal, safe, portable programming language that can move between the system layer and the scripting layer without changing languages.**

---

# 77. Motto

Possible project motto:

> **One language. Every layer.**

Alternative:

> **Small enough for a device. Powerful enough for a system. Simple enough for a script.**

---

# 78. Proposed Initial MVP

The first implementation should NOT attempt the entire vision.

MVP:

```text
✓ Lexer
✓ Parser
✓ Interpreter
✓ REPL
✓ Variables
✓ Functions
✓ Structs
✓ Enums
✓ Pattern matching
✓ Arrays
✓ Strings
✓ Map
✓ Option
✓ Result
✓ Modules
✓ Basic ownership
✓ Automatic resource cleanup
✓ C FFI
✓ CLI
✓ Formatter
✓ Test framework
✓ Native compiler
✓ WASM target
```

Defer:

```text
✗ complex macros
✗ advanced reflection
✗ JIT
✗ distributed runtime
✗ built-in GUI
✗ huge standard library
```

---

# 79. Success Criteria

USGL can be considered successful if a developer can use one language to write:

### Tiny utility

```text
<100 lines
```

### CLI application

```text
native binary
```

### Automation script

```text
single .us file
```

### Web application component

```text
WASM
```

### Server

```text
native/WASI
```

### Embedded component

```text
no_std/no_heap
```

### Plugin

```text
embedded USGL runtime
```

without fundamentally changing the language.

---

# 80. Final Proposal

USGL proposes a fourth category between existing language families:

```text
        HIGH LEVEL
             │
       Python / JS
             │
             │
          ┌─────┐
          │USGL │
          └─────┘
             │
             │
        Rust / Zig
             │
             │
        C / Assembly
             │
        LOW LEVEL
```

The language should combine:

```text
Lua
  → small + embeddable

Python
  → expressive + glue-friendly

Rust
  → memory safety + systems capability

JavaScript
  → ubiquitous portable execution

WASM
  → universal sandboxed target
```

while deliberately keeping the language and runtime substantially smaller than the ecosystems it connects.

The fundamental design objective is therefore:

> **USGL is a small, memory-safe, cross-platform language for writing everything from a five-line automation script to an embedded system component, native application, server, or WebAssembly module.**

---

## Proposed slogan

**USGL — One Language. Every Layer.**