# Plan: Wire Execution Tracing into Interpreter

## Goal

Emit `TraceEvent` records during command execution so callers can
inspect what ran, how long it took, and which capabilities were used.

## Architecture

- Crates affected: `sandbox`
- Existing types used: `TraceEvent`, `PipelineTrace`, `PipelineNode`
  (all in `crates/sandbox/src/trace.rs`)
- Data flow: `execute_simple()` measures timing and builds a
  `TraceEvent` -> pushes to `Interpreter.trace_events` -> caller
  retrieves via `Shell::trace_events()` / `Shell::take_trace_events()`

## Tech Stack

- Rust 2024 edition
- `std::time::Instant` for duration measurement
- No new dependencies

## Tasks

### Task 1: Add tracing fields to Interpreter

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/interpreter/mod.rs`
**Run**: `cargo check -p sandbox`

1. Write failing test:

   ```rust
   // No test needed — this is a struct field addition.
   // cargo check confirms compilation.
   ```

2. Implement:

   Add two fields to `Interpreter`:

   ```rust
   pub tracing_enabled: bool,
   pub trace_events: Vec<crate::trace::TraceEvent>,
   ```

3. Verify:

   ```
   cargo check -p sandbox
   ```

4. Do NOT commit yet — Task 2 depends on this.

### Task 2: Initialize tracing fields in ShellBuilder

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/lib.rs`
**Run**: `cargo check -p sandbox`

1. Add `tracing: bool` field to `ShellBuilder` (default `false`).

2. Add builder method:

   ```rust
   pub fn tracing(mut self, enabled: bool) -> Self {
       self.tracing = enabled;
       self
   }
   ```

3. In both `build()` and `build_with_state()`, pass the field through:

   ```rust
   tracing_enabled: self.tracing,
   trace_events: Vec::new(),
   ```

4. Verify:

   ```
   cargo check -p sandbox
   ```

5. Do NOT commit yet — Task 3 depends on this.

### Task 3: Emit TraceEvent in execute_simple

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/interpreter/mod.rs`
**Run**: `cargo nextest run -p sandbox`

1. Write failing test:

   ```rust
   #[tokio::test]
   async fn trace_events_emitted() {
       let mut shell = Shell::builder()
           .cwd("/")
           .tracing(true)
           .build();

       shell.exec("echo hello").await.unwrap();
       let events = shell.trace_events();
       assert_eq!(events.len(), 1);
       assert_eq!(events[0].command, "echo");
       assert_eq!(events[0].args, vec!["hello"]);
       assert_eq!(events[0].exit_code, 0);
       assert!(events[0].duration_us > 0 || events[0].duration_us == 0);
   }
   ```

   Run: `cargo nextest run -p sandbox -- trace_events_emitted`
   Expected: FAIL (no `trace_events()` method yet, no emission)

2. Implement in `execute_simple`, wrap the body with timing:

   ```rust
   async fn execute_simple(&mut self, sc: &SimpleCommand)
       -> ShellResult<ExecResult>
   {
       let start = std::time::Instant::now();

       // ... existing body unchanged ...

       let result = /* existing result */;

       if self.tracing_enabled && !sc.words.is_empty() {
           let caps_used: Vec<Cap> = if let Some(b)
               = self.builtins.get(&expanded_words[0])
           {
               b.required_capabilities().to_vec()
           } else {
               vec![]
           };

           self.trace_events.push(crate::trace::TraceEvent {
               timestamp: std::time::SystemTime::now(),
               command: expanded_words[0].clone(),
               args: expanded_words[1..].to_vec(),
               exit_code: result.exit_code,
               duration_us: start.elapsed().as_micros() as u64,
               capabilities_used: caps_used,
           });
       }

       // return result as before
   }
   ```

3. Verify:

   ```
   cargo nextest run -p sandbox    -> all green
   cargo clippy -p sandbox -- -D warnings  -> zero warnings
   ```

4. Do NOT commit yet — Task 4 completes the public API.

### Task 4: Expose trace accessors on Shell

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/lib.rs`
**Run**: `cargo nextest run -p sandbox`

1. Write failing test:

   ```rust
   #[tokio::test]
   async fn trace_not_emitted_when_disabled() {
       let mut shell = Shell::builder().cwd("/").build();
       shell.exec("echo hello").await.unwrap();
       assert!(shell.trace_events().is_empty());
   }

   #[tokio::test]
   async fn take_trace_events_clears() {
       let mut shell = Shell::builder()
           .cwd("/")
           .tracing(true)
           .build();

       shell.exec("echo a").await.unwrap();
       let events = shell.take_trace_events();
       assert_eq!(events.len(), 1);
       assert!(shell.trace_events().is_empty());
   }
   ```

   Run: `cargo nextest run -p sandbox -- trace_not_emitted`
   Expected: FAIL

2. Implement on `Shell`:

   ```rust
   pub fn trace_events(&self) -> &[trace::TraceEvent] {
       &self.interpreter.trace_events
   }

   pub fn take_trace_events(&mut self) -> Vec<trace::TraceEvent> {
       std::mem::take(&mut self.interpreter.trace_events)
   }
   ```

3. Verify:

   ```
   cargo nextest run -p sandbox    -> all green
   cargo clippy -p sandbox -- -D warnings  -> zero warnings
   ```

4. Run: `git branch --show-current`
   Verify output matches expected branch.
   Commit: `git commit -m "feat(sandbox): wire execution tracing into interpreter"`

### Task 5: Pipeline tracing (stretch)

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/interpreter/mod.rs`
**Run**: `cargo nextest run -p sandbox`

1. Write failing test:

   ```rust
   #[tokio::test]
   async fn trace_pipeline_stages() {
       let mut shell = Shell::builder()
           .cwd("/")
           .tracing(true)
           .build();

       shell.exec("echo hello | cat").await.unwrap();
       let events = shell.trace_events();
       assert_eq!(events.len(), 2);
       assert_eq!(events[0].command, "echo");
       assert_eq!(events[1].command, "cat");
   }
   ```

   Run: `cargo nextest run -p sandbox -- trace_pipeline`
   Expected: FAIL (pipeline stages each call `execute_simple`,
   so this may already pass after Task 3 — verify)

2. If already passing, this task is a no-op. If not, ensure each
   pipeline stage goes through `execute_simple` so tracing fires.

3. Verify:

   ```
   cargo nextest run -p sandbox    -> all green
   cargo clippy -p sandbox -- -D warnings  -> zero warnings
   ```

4. Commit only if changes were needed:
   `git commit -m "feat(sandbox): trace pipeline stages"`

## Risks

- `Instant::now()` timing in tests is non-deterministic — assert
  `duration_us >= 0` rather than exact values.
- `required_capabilities()` borrows the builtin; we look it up again
  after execution. This is a cheap hashmap lookup, acceptable.
