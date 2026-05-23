# Plan: Subshell Isolation for (...) Groups

## Goal

Execute `Command::Subshell(inner)` in an isolated variable/function
scope so that variable assignments, function definitions, cwd changes,
and shell option changes inside `(...)` do not leak to the parent.
The VFS is intentionally shared (matching bash semantics).

## Architecture

- Crate affected: `sandbox`
- Files: `interpreter/mod.rs`, `fs.rs`
- Data flow: on `Subshell`, save env/vars/cwd/functions/params/opts,
  execute inner, restore saved state, return exit code

## Tech Stack

- Rust 2024 edition
- No new dependencies

## Tasks

### Task 1: Write failing tests for subshell isolation

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/lib.rs`
**Run**: `cargo nextest run -p sandbox -- subshell`

1. Write failing tests:

   ```rust
   #[tokio::test]
   async fn subshell_isolates_vars() {
       let mut shell = Shell::builder().cwd("/").build();
       shell.exec("X=outer").await.unwrap();
       shell.exec("(X=inner)").await.unwrap();
       let output = shell.exec("echo $X").await.unwrap();
       assert_eq!(output.stdout, "outer\n");
   }

   #[tokio::test]
   async fn subshell_isolates_cwd() {
       let mut shell = Shell::builder().cwd("/").build();
       shell.exec("mkdir -p /tmp/sub").await.unwrap();
       shell.exec("(cd /tmp/sub)").await.unwrap();
       assert_eq!(shell.cwd(), "/");
   }

   #[tokio::test]
   async fn subshell_isolates_functions() {
       let mut shell = Shell::builder().cwd("/").build();
       shell.exec("(function foo { echo bar; })").await.unwrap();
       let output = shell.exec("foo").await.unwrap();
       assert_eq!(output.exit_code, 127);
   }

   #[tokio::test]
   async fn subshell_isolates_env() {
       let mut shell = Shell::builder().cwd("/").build();
       shell.exec("export A=1").await.unwrap();
       shell.exec("(export A=2)").await.unwrap();
       let output = shell.exec("echo $A").await.unwrap();
       assert_eq!(output.stdout, "1\n");
   }

   #[tokio::test]
   async fn subshell_shares_fs() {
       let mut shell = Shell::builder().cwd("/").build();
       shell.exec("(echo hello > /sub.txt)").await.unwrap();
       let output = shell.exec("cat /sub.txt").await.unwrap();
       assert_eq!(output.stdout, "hello\n");
   }

   #[tokio::test]
   async fn subshell_exit_code_propagates() {
       let mut shell = Shell::builder().cwd("/").build();
       let output = shell.exec("(false)").await.unwrap();
       assert_eq!(output.exit_code, 1);
   }
   ```

   Run: `cargo nextest run -p sandbox -- subshell_isolates`
   Expected: `subshell_isolates_vars`, `subshell_isolates_cwd`,
   `subshell_isolates_functions`, `subshell_isolates_env` FAIL.
   `subshell_shares_fs` and `subshell_exit_code_propagates` PASS.

2. Do NOT commit yet.

### Task 2: Implement subshell isolation in execute_inner

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/interpreter/mod.rs`
**Run**: `cargo nextest run -p sandbox`

1. Replace the `Command::Subshell` arm:

   ```rust
   Command::Subshell(inner) => {
       // Save parent state
       let saved_env = self.env.clone();
       let saved_vars = self.vars.clone();
       let saved_cwd = self.cwd.clone();
       let saved_functions = self.functions.clone();
       let saved_params = self.positional_params.clone();
       let saved_opts = self.shell_opts.clone();

       let result = self.execute(inner).await;

       // Restore parent state (VFS is shared intentionally)
       self.env = saved_env;
       self.vars = saved_vars;
       self.cwd = saved_cwd;
       self.functions = saved_functions;
       self.positional_params = saved_params;
       self.shell_opts = saved_opts;

       // Propagate exit code
       match result {
           Ok(r) => {
               self.last_exit_code = r.exit_code;
               Ok(r)
           }
           Err(e) => Err(e),
       }
   }
   ```

2. Verify:

   ```
   cargo nextest run -p sandbox    -> all green
   cargo clippy -p sandbox -- -D warnings  -> zero warnings
   ```

3. Run: `git branch --show-current`
   Commit: `git commit -m "feat(sandbox): subshell isolation for (...) groups"`

## Risks

- Cloning `HashMap` state on every subshell is O(n) in state size.
  Acceptable for a virtual shell; optimize later if profiling shows it.
- `last_exit_code` is intentionally NOT saved/restored — the parent
  sees the subshell's exit code (matching bash).
- `counters` are intentionally NOT forked — subshell commands still
  count against global limits (preventing resource exhaustion via
  nested subshells).
