# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with
code in this repository.

## What This Is

A standalone virtual bash interpreter with a sandboxed in-memory filesystem.
No real OS commands are executed — everything runs inside a `vfs::MemoryFS`.
Rust 2024 edition.

## Build & Test

```bash
cargo build                           # build all crates
cargo test                            # run all tests
cargo test -p sandbox                 # library tests only
cargo test -p sandbox -- <test_name>  # single test
cargo clippy --workspace              # lint
```

Benchmarks live in `sandbox-bench` (currently a placeholder):

```bash
cargo test -p sandbox-bench
```

## Workspace Layout

Three crates under `crates/`:

| Crate           | Role                                             |
| --------------- | ------------------------------------------------ |
| `sandbox`       | Core library: parser, interpreter, VFS, builtins |
| `sandbox-cli`   | Thin CLI binary (`sandbox-cli -c 'echo hello'`)  |
| `sandbox-bench` | Benchmark harness (dev-dependencies only)        |

## Architecture

Execution flows through a linear pipeline:

```
input string -> Lexer -> Parser -> AST (Command) -> Interpreter -> ShellOutput
```

### Key modules in `crates/sandbox/src/`

- **`parser/lexer.rs`** — tokenizes shell input (words, operators, quotes,
  heredocs, redirections)
- **`parser/ast.rs`** — AST types: `Command` enum (Simple, Pipeline, If, For,
  While, Until, Case, FunctionDef, Subshell, Group, Assignment, etc.)
- **`parser/mod.rs`** — recursive-descent parser with fuel and depth limits
- **`interpreter/mod.rs`** — async executor that walks the AST; handles
  control flow, redirections, pipelines, variable/function scoping
- **`interpreter/expansion.rs`** — variable expansion (`$VAR`,
  `${VAR:-default}`, `$?`, positional params, command substitution)
- **`interpreter/hooks.rs`** — `ExecHandler` trait for intercepting unknown
  commands (default returns `None`; implement to add custom command dispatch)
- **`builtins/`** — built-in commands split by category:
  - `core.rs` — echo, printf, true, false, exit, return, test/[, read, set,
    shift, source, eval, type, command
  - `file.rs` — cat, head, tail, wc, tee, touch, cp, mv, rm, ls, mkdir,
    basename, dirname, realpath, find, grep, sort, uniq, tr, cut, sed
  - `nav.rs` — cd, pwd
  - `vars.rs` — export, unset, env, declare, local
  - `flow.rs` — break, continue (stub)
- **`fs.rs`** — `SandboxFs` wrapping `vfs::MemoryFS`; all paths normalized
  to prevent traversal escapes
- **`capabilities.rs`** — capability-based permission model (`Cap` enum:
  ReadFs, WriteFs, RealFs, Network, Exec, EnvRead, EnvWrite, Signal);
  builtins declare required capabilities via `required_capabilities()`
- **`limits.rs`** — `ExecutionLimits` (commands, loops, depth, output bytes,
  input size, timeouts) with hard caps that cannot be exceeded
- **`trace.rs`** — `TraceEvent` / `PipelineTrace` structs for execution
  tracing (data types only, not yet wired)
- **`snapshot.rs`** — placeholder for future state serialization
- **`error.rs`** — `ShellError` / `ShellResult` types

### Public API

Entry point is `Shell::builder()` which returns a `ShellBuilder`. Configure
with `.env()`, `.cwd()`, `.limits()`, `.capabilities()`, `.fs()`,
`.exec_handler()`, `.builtins()`, then `.build()`. Call `shell.exec(input)`
(async) to run commands.

Custom builtins implement the `Builtin` trait (`name()` +
`async execute(Context) -> ShellResult<ExecResult>`).

### Design Invariants

- The VFS is fully in-memory; no host filesystem access unless `Cap::RealFs`
  is granted (not yet implemented).
- All user-facing limits are clamped to hard caps at build time
  (`ExecutionLimits::clamped()`).
- The interpreter is `async` (tokio) to support future timeout enforcement;
  current builtins are CPU-bound but wrapped in async.
- Subshell (`(...)`) currently shares parent context — true isolation is
  deferred.
