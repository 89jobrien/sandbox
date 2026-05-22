# sandbox

A virtual bash interpreter with a sandboxed in-memory filesystem. No real OS
commands are executed — everything runs inside a `vfs::MemoryFS`. Designed for
embedding in applications that need to evaluate shell-like scripts safely.

## Usage

### As a library

```rust
use sandbox::{Shell, ShellOutput};

#[tokio::main]
async fn main() {
    let mut shell = Shell::builder()
        .env("HOME", "/home/user")
        .cwd("/home/user")
        .build();

    let output = shell.exec("echo hello world").await.unwrap();
    assert_eq!(output.stdout, "hello world\n");
    assert_eq!(output.exit_code, 0);
}
```

### CLI

```bash
cargo run -p sandbox-cli -- -c 'for i in a b c; do echo $i; done'
```

## Supported Shell Features

**Control flow:** `if/elif/else/fi`, `for/do/done`, `while/do/done`,
`until/do/done`, `case/esac`, `&&`, `||`, `!`, `;` sequences

**Functions:** `function name { ... }` and `name() { ... }`

**Pipelines:** `cmd1 | cmd2 | cmd3`

**Redirections:** `>`, `>>`, `<`, `2>`, `2>>`, `&>`, heredocs (`<<`),
herestrings (`<<<`)

**Expansion:** `$VAR`, `${VAR}`, `${VAR:-default}`, `${VAR:+alt}`,
`${#VAR}`, `$?`, `$0`..`$9`, `$@`, `$#`, command substitution (`$(...)`,
`` `...` ``), double-quoted interpolation

**Assignments:** `VAR=value`, `export VAR=value`, prefix assignments

**Grouping:** `{ ...; }`, `(...)`

## Built-in Commands

| Category   | Commands                                     |
| ---------- | -------------------------------------------- |
| Core       | echo, printf, cat, read, head, tail, test, [ |
| Navigation | cd, pwd, ls                                  |
| File       | mkdir, rm, cp, mv, touch                     |
| Flow       | true, false, exit                            |
| Variables  | export, set, unset                           |

Unknown commands return exit code 127. Implement the `ExecHandler` trait to
intercept and handle external commands, or register custom builtins via
`shell.register_builtin(impl Builtin)`.

## Capabilities

The shell uses a capability-based permission model. Each `Shell` instance has
a `CapabilitySet` that controls what operations are allowed:

| Capability | Controls                          |
| ---------- | --------------------------------- |
| ReadFs     | Reading files and listing dirs    |
| WriteFs    | Writing, creating, removing files |
| EnvRead    | Reading environment variables     |
| EnvWrite   | Modifying environment variables   |
| RealFs     | Host filesystem access (future)   |
| Network    | Network operations (future)       |
| Exec       | Spawning real processes (future)  |
| Signal     | Signal handling (future)          |

Default set: `ReadFs`, `WriteFs`, `EnvRead`, `EnvWrite`. Restrict with:

```rust
use sandbox::capabilities::{Cap, CapabilitySet};

let shell = Shell::builder()
    .capabilities(CapabilitySet::new([Cap::ReadFs, Cap::EnvRead]))
    .build();
```

## Execution Limits

All limits have hard caps that cannot be exceeded:

| Limit                | Default | Hard cap  |
| -------------------- | ------- | --------- |
| Commands             | 10,000  | 1,000,000 |
| Loop iterations      | 10,000  | 1,000,000 |
| AST depth            | 100     | 100       |
| Parser fuel (tokens) | 100,000 | 1,000,000 |
| Stdout               | 1 MB    | 100 MB    |
| Input size           | 10 MB   | 100 MB    |
| VFS size             | 100 MB  | 1 GB      |
| Timeout              | 30s     | 3,600s    |

```rust
use sandbox::limits::ExecutionLimits;

let shell = Shell::builder()
    .limits(ExecutionLimits {
        max_loop_iterations: 100,
        ..Default::default()
    })
    .build();
```

## Custom Builtins

```rust
use async_trait::async_trait;
use sandbox::builtins::{Builtin, Context};
use sandbox::error::ShellResult;
use sandbox::interpreter::hooks::ExecResult;

struct Hello;

#[async_trait]
impl Builtin for Hello {
    fn name(&self) -> &str { "hello" }

    async fn execute(&self, _ctx: Context<'_>) -> ShellResult<ExecResult> {
        Ok(ExecResult::success("hello from custom\n"))
    }
}

let mut shell = Shell::builder().build();
shell.register_builtin(Hello);
```

## License

MIT OR Apache-2.0
