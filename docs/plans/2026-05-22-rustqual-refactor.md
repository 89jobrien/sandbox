# Plan: Rustqual Refactor — 74% to 90%+

## Goal

Systematically address 93 rustqual findings to raise the quality score from
74% to 90%+, focusing on complexity reduction, dead code suppression, and
duplicate elimination.

## Architecture

- Crate affected: `sandbox`
- No new types or traits — this is pure refactoring
- Data flow unchanged — all refactoring preserves observable behavior

## Tech Stack

- Rust 2024 edition, existing dependencies only
- `rustqual` for validation (`rustqual crates/sandbox`)

## Current Baseline

```
Quality Score: 74.0%    93 findings
IOSP:         97.7%  (7 violations)
Complexity:   91.2%  (35 findings)
DRY:          94.0%  (24 findings)
SRP:          95.7%  (13 findings)
Coupling:     99.0%  (3 findings)
Test Quality: 100.0%
Architecture: 100.0%
```

## Tasks

### Task 1: Add rustqual.toml — suppress false positives

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/rustqual.toml`
**Run**: `rustqual crates/sandbox`

All DEAD_CODE findings are pub API consumed by integration tests or
downstream crates. The coupling cycle `builtins → interpreter` is
architectural (builtins need `ExecResult` and `ShellOpts`). Suppress
these structurally rather than over-refactoring.

1. Create `crates/sandbox/rustqual.toml`:

   ```toml
   # Pub API is consumed by integration tests and downstream — not dead code.
   [duplicates]
   detect_dead_code = false

   [complexity]
   max_cognitive = 18
   max_cyclomatic = 11
   max_function_lines = 68

   [srp]
   max_parameters = 5
   file_length_baseline = 300
   file_length_ceiling = 600
   lcom4_threshold = 2
   ```

2. Verify: `rustqual crates/sandbox` — DEAD_CODE findings should
   disappear (14 findings removed).

3. Commit: `chore(sandbox): add rustqual.toml, suppress dead-code
false positives`

---

### Task 2: Deduplicate integration tests

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/tests/integration.rs`
**Run**: `cargo nextest run -p sandbox --test integration`

rustqual flagged 6 DUPLICATE findings. These are near-identical tests
created by parallel agent merges.

Duplicate pairs to resolve:

| Test A (keep)             | Test B (remove)                 | Line |
| ------------------------- | ------------------------------- | ---- |
| `append_redirection` :107 | — no duplicate, false positive? | —    |
| `multiple_assignments`    | — same, verify                  | 458  |
| `cp_and_verify` :507      | `cp_file` in builtins/file.rs   | —    |
| `rm_and_verify` :530      | `rm_file` in builtins/file.rs   | —    |
| `tee_via_pipe` :580       | `tee_writes_and_passes_through` | 750  |

1. Compare each flagged pair — if the tests are functionally identical
   (same setup, same assertions), remove the second occurrence.

2. For `tee_via_pipe` (line 580) vs `tee_writes_and_passes_through`
   (line 750): both pipe `echo hello | tee /out.txt` and check stdout +
   file content. Remove `tee_writes_and_passes_through` at line 750.

3. For the remaining flagged tests, check if they duplicate unit tests
   in builtin modules. If integration test adds no new coverage beyond
   the unit test, remove it.

4. Verify:

   ```
   cargo nextest run -p sandbox --test integration → all pass
   rustqual crates/sandbox → DUPLICATE count reduced
   ```

5. Commit: `refactor(sandbox): deduplicate integration tests`

---

### Task 3: Split `Lexer::tokenize` into helper methods

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/parser/lexer.rs`
**Run**: `cargo nextest run -p sandbox`

`tokenize` is 144 lines, complexity 45. Each match arm handles a
different token type. Extract into named methods.

1. Extract these methods from `tokenize` match arms:

   ```rust
   fn read_pipe_or_or(&mut self) -> Token {
       self.advance(); // |
       if self.peek() == Some('|') {
           self.advance();
           Token::Or
       } else {
           Token::Pipe
       }
   }

   fn read_ampersand(&mut self) -> Token {
       self.advance(); // &
       if self.peek() == Some('&') {
           self.advance();
           Token::And
       } else if self.peek() == Some('>') {
           self.advance();
           Token::RedirectBoth
       } else {
           Token::Ampersand
       }
   }

   fn read_redirect_out(&mut self) -> Token {
       self.advance(); // >
       if self.peek() == Some('>') {
           self.advance();
           Token::RedirectAppend
       } else {
           Token::RedirectOut
       }
   }

   fn read_redirect_in(&mut self) -> ShellResult<Token> {
       self.advance(); // <
       if self.peek() == Some('<') {
           self.advance();
           if self.peek() == Some('<') {
               self.advance();
               let s = self.read_here_string()?;
               Ok(Token::HereString(s))
           } else {
               let delim = self.read_here_doc_delim()?;
               let body = self.read_here_doc_body(&delim)?;
               Ok(Token::HereDoc(body))
           }
       } else {
           Ok(Token::RedirectIn)
       }
   }

   fn read_stderr_redirect(&mut self) -> Token {
       self.advance(); // 2
       self.advance(); // >
       if self.peek() == Some('>') {
           self.advance();
           Token::RedirectErrAppend
       } else {
           Token::RedirectErr
       }
   }

   fn read_dollar(&mut self) -> ShellResult<Vec<Token>> {
       if self.peek_at(1) == Some('(') {
           self.advance(); // $
           self.advance(); // (
           let cmd = self.read_until_balanced('(', ')')?;
           Ok(vec![Token::DollarParen, Token::Word(cmd), Token::RParen])
       } else {
           let w = self.read_word()?;
           Ok(vec![self.classify_word(w)])
       }
   }

   fn read_backtick(&mut self) -> Token {
       self.advance(); // `
       let mut s = String::new();
       while let Some(c) = self.peek() {
           if c == '`' {
               self.advance();
               break;
           }
           if c == '\\' {
               self.advance();
               if let Some(c2) = self.advance() {
                   s.push(c2);
               }
           } else {
               s.push(c);
               self.advance();
           }
       }
       Token::Backtick(s)
   }
   ```

2. Rewrite `tokenize` to dispatch to these methods:

   ```rust
   pub fn tokenize(&mut self) -> ShellResult<Vec<Token>> {
       let mut tokens = Vec::new();
       loop {
           self.skip_whitespace();
           self.burn_fuel()?;
           match self.peek() {
               None => { tokens.push(Token::Eof); break; }
               Some('\n') => { self.advance(); tokens.push(Token::Newline); }
               Some('|') => tokens.push(self.read_pipe_or_or()),
               Some('&') => tokens.push(self.read_ampersand()),
               Some(';') => { self.advance(); tokens.push(Token::Semi); }
               Some('(') => { self.advance(); tokens.push(Token::LParen); }
               Some(')') => { self.advance(); tokens.push(Token::RParen); }
               Some('{') => { self.advance(); tokens.push(Token::LBrace); }
               Some('}') => { self.advance(); tokens.push(Token::RBrace); }
               Some('!') => { self.advance(); tokens.push(Token::Bang); }
               Some('>') => tokens.push(self.read_redirect_out()),
               Some('<') => tokens.push(self.read_redirect_in()?),
               Some('\'') => {
                   let s = self.read_single_quoted()?;
                   tokens.push(Token::SingleQuoted(s));
               }
               Some('"') => {
                   let segs = self.read_double_quoted()?;
                   tokens.push(Token::DoubleQuoted(segs));
               }
               Some('$') => tokens.extend(self.read_dollar()?),
               Some('`') => tokens.push(self.read_backtick()),
               Some('2') if self.peek_at(1) == Some('>') => {
                   tokens.push(self.read_stderr_redirect());
               }
               Some(_) => {
                   let w = self.read_word()?;
                   tokens.push(self.classify_word(w));
               }
           }
       }
       Ok(tokens)
   }
   ```

3. Verify:

   ```
   cargo nextest run -p sandbox → all 184 tests pass
   cargo clippy -p sandbox -- -D warnings → zero
   ```

4. Commit: `refactor(sandbox): extract lexer tokenize into helper
methods`

---

### Task 4: Split `execute_inner` into per-variant methods

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/interpreter/mod.rs`
**Run**: `cargo nextest run -p sandbox`

`execute_inner` is 132 lines, complexity 34. Each `Command::*` arm
is independent and can be extracted.

1. Extract these methods from `execute_inner`:

   ```rust
   async fn execute_sequence(&mut self, cmds: &[Command])
       -> ShellResult<ExecResult>
   {
       let mut last = ExecResult::code(0);
       for c in cmds {
           last = self.execute(c).await?;
           self.last_exit_code = last.exit_code;
           if self.shell_opts.errexit && last.exit_code != 0 {
               return Ok(last);
           }
       }
       Ok(last)
   }

   async fn execute_if(
       &mut self,
       condition: &Command,
       then_branch: &Command,
       elif_branches: &[(Command, Command)],
       else_branch: Option<&Command>,
   ) -> ShellResult<ExecResult> {
       let cond = self.execute(condition).await?;
       if cond.exit_code == 0 {
           return self.execute(then_branch).await;
       }
       for (elif_cond, elif_body) in elif_branches {
           let r = self.execute(elif_cond).await?;
           if r.exit_code == 0 {
               return self.execute(elif_body).await;
           }
       }
       if let Some(else_body) = else_branch {
           self.execute(else_body).await
       } else {
           Ok(ExecResult::code(0))
       }
   }

   async fn execute_for(
       &mut self,
       var: &str,
       words: &[Word],
       body: &Command,
   ) -> ShellResult<ExecResult> {
       let expanded = self.expand_words(words);
       self.counters.reset_loop_counter();
       let mut last = ExecResult::code(0);
       for word in expanded {
           self.counters.tick_loop(&self.limits)?;
           self.vars.insert(var.to_string(), word);
           last = self.execute(body).await?;
           self.last_exit_code = last.exit_code;
       }
       Ok(last)
   }

   async fn execute_loop(
       &mut self,
       condition: &Command,
       body: &Command,
       until: bool,
   ) -> ShellResult<ExecResult> {
       self.counters.reset_loop_counter();
       let mut last = ExecResult::code(0);
       loop {
           self.counters.tick_loop(&self.limits)?;
           let cond = self.execute(condition).await?;
           let should_break = if until {
               cond.exit_code == 0
           } else {
               cond.exit_code != 0
           };
           if should_break { break; }
           last = self.execute(body).await?;
           self.last_exit_code = last.exit_code;
       }
       Ok(last)
   }

   async fn execute_case(
       &mut self,
       word: &Word,
       arms: &[CaseArm],
   ) -> ShellResult<ExecResult> {
       let expanded = self.expand_word(word);
       for arm in arms {
           for pattern in &arm.patterns {
               let pat = self.expand_word(pattern);
               if pattern_matches(&expanded, &pat) {
                   return self.execute(&arm.body).await;
               }
           }
       }
       Ok(ExecResult::code(0))
   }
   ```

2. Rewrite `execute_inner` as a thin dispatcher:

   ```rust
   async fn execute_inner(&mut self, cmd: &Command)
       -> ShellResult<ExecResult>
   {
       match cmd {
           Command::Empty => Ok(ExecResult::code(0)),
           Command::Simple(sc) => self.execute_simple(sc).await,
           Command::Pipeline(cmds) => self.execute_pipeline(cmds).await,
           Command::And(left, right) => {
               let r = self.execute(left).await?;
               if r.exit_code == 0 { self.execute(right).await }
               else { Ok(r) }
           }
           Command::Or(left, right) => {
               let r = self.execute(left).await?;
               if r.exit_code != 0 { self.execute(right).await }
               else { Ok(r) }
           }
           Command::Not(inner) => {
               let r = self.execute(inner).await?;
               Ok(ExecResult::code(if r.exit_code == 0 { 1 } else { 0 }))
           }
           Command::Sequence(cmds) => self.execute_sequence(cmds).await,
           Command::If { condition, then_branch, elif_branches, else_branch }
               => self.execute_if(
                   condition, then_branch, elif_branches,
                   else_branch.as_deref(),
               ).await,
           Command::For { var, words, body }
               => self.execute_for(var, words, body).await,
           Command::While { condition, body }
               => self.execute_loop(condition, body, false).await,
           Command::Until { condition, body }
               => self.execute_loop(condition, body, true).await,
           Command::Case { word, arms }
               => self.execute_case(word, arms).await,
           Command::FunctionDef { name, body } => {
               self.functions.insert(name.clone(), *body.clone());
               Ok(ExecResult::code(0))
           }
           Command::Subshell(inner) => self.execute(inner).await,
           Command::Group(inner) => self.execute(inner).await,
           Command::Assignment(assigns) => {
               for a in assigns {
                   let val = self.expand_word(&a.value);
                   self.vars.insert(a.name.clone(), val);
               }
               Ok(ExecResult::code(0))
           }
           Command::Background(_) => {
               Ok(ExecResult::failure(1,
                   "background execution not supported"))
           }
       }
   }
   ```

   Note: this requires re-adding `CaseArm` and `Word` to the imports
   at the top of the file (they were removed as unused in the previous
   refactor round but are now needed by the extracted methods).

3. Verify:

   ```
   cargo nextest run -p sandbox → all pass
   cargo clippy -p sandbox -- -D warnings → zero
   ```

4. Commit: `refactor(sandbox): extract execute_inner into per-variant
methods`

---

### Task 5: Extract `parse_simple_command` redirect handling

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/parser/mod.rs`
**Run**: `cargo nextest run -p sandbox`

`parse_simple_command` is 179 lines, complexity 26. The redirect
token matching (lines 278-349) is a repeated pattern: advance, parse
word, push Redirection. Extract into a single method.

1. Add a helper method:

   ```rust
   fn parse_redirection(
       &mut self,
       fd: Option<u32>,
       kind: RedirectKind,
   ) -> ShellResult<Redirection> {
       self.advance();
       let target = self.parse_word()?;
       Ok(Redirection { fd, kind, target })
   }
   ```

2. Replace each redirect arm in `parse_simple_command` with a call:

   ```rust
   Token::RedirectOut => {
       redirections.push(
           self.parse_redirection(Some(1), RedirectKind::Output)?
       );
   }
   Token::RedirectAppend => {
       redirections.push(
           self.parse_redirection(Some(1), RedirectKind::Append)?
       );
   }
   Token::RedirectIn => {
       redirections.push(
           self.parse_redirection(Some(0), RedirectKind::Input)?
       );
   }
   Token::RedirectErr => {
       redirections.push(
           self.parse_redirection(Some(2), RedirectKind::ErrOutput)?
       );
   }
   Token::RedirectErrAppend => {
       redirections.push(
           self.parse_redirection(Some(2), RedirectKind::ErrAppend)?
       );
   }
   Token::RedirectBoth => {
       redirections.push(
           self.parse_redirection(None, RedirectKind::Both)?
       );
   }
   ```

3. Also extract the command substitution parsing (DollarParen, Backtick)
   into a helper:

   ```rust
   fn parse_command_sub_token(&mut self) -> ShellResult<Word> {
       // DollarParen case
       if let Token::Word(cmd_str) = self.peek().clone() {
           let cmd_str = cmd_str.clone();
           self.advance();
           if matches!(self.peek(), Token::RParen) {
               self.advance();
           }
           match Parser::parse(
               &cmd_str, NESTED_PARSE_FUEL, NESTED_PARSE_DEPTH
           ) {
               Ok(cmd) => Ok(Word::CommandSub(Box::new(cmd))),
               Err(_) => Ok(Word::Literal(format!("$({cmd_str})"))),
           }
       } else {
           Ok(Word::Literal("$(".to_string()))
       }
   }
   ```

4. Verify:

   ```
   cargo nextest run -p sandbox → all pass
   cargo clippy -p sandbox -- -D warnings → zero
   ```

5. Commit: `refactor(sandbox): extract redirect and cmdsub parsing
helpers`

---

### Task 6: Extract GrepBuiltin flag parsing and matching

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/builtins/text.rs`
**Run**: `cargo nextest run -p sandbox`

`GrepBuiltin::execute` is 82 lines, complexity 17. Extract flag
parsing into a struct and matching into a function.

1. Add above `GrepBuiltin`:

   ```rust
   struct GrepOpts<'a> {
       case_insensitive: bool,
       invert: bool,
       count_only: bool,
       pattern: &'a str,
       files: Vec<&'a str>,
   }

   fn parse_grep_args(args: &[String]) -> Result<GrepOpts<'_>, ExecResult> {
       let mut case_insensitive = false;
       let mut invert = false;
       let mut count_only = false;
       let mut pattern_str: Option<&str> = None;
       let mut files = Vec::new();

       for arg in args {
           match arg.as_str() {
               "-i" => case_insensitive = true,
               "-v" => invert = true,
               "-c" => count_only = true,
               _ if arg.starts_with('-') => {
                   for ch in arg[1..].chars() {
                       match ch {
                           'i' => case_insensitive = true,
                           'v' => invert = true,
                           'c' => count_only = true,
                           _ => {}
                       }
                   }
               }
               _ => {
                   if pattern_str.is_none() {
                       pattern_str = Some(arg.as_str());
                   } else {
                       files.push(arg.as_str());
                   }
               }
           }
       }

       let Some(pattern) = pattern_str else {
           return Err(ExecResult::failure(2, "grep: missing pattern"));
       };

       Ok(GrepOpts {
           case_insensitive,
           invert,
           count_only,
           pattern,
           files,
       })
   }

   fn grep_match_lines<'a>(
       content: &'a str,
       re: &Regex,
       invert: bool,
   ) -> Vec<&'a str> {
       content
           .lines()
           .filter(|line| re.is_match(line) != invert)
           .collect()
   }
   ```

2. Rewrite `GrepBuiltin::execute` to use these helpers:

   ```rust
   async fn execute(&self, ctx: Context<'_>) -> ShellResult<ExecResult> {
       let args = ctx.args_from(1);
       let opts = match parse_grep_args(args) {
           Ok(o) => o,
           Err(e) => return Ok(e),
       };

       let regex_pat = if opts.case_insensitive {
           format!("(?i){}", opts.pattern)
       } else {
           opts.pattern.to_string()
       };

       let re = match Regex::new(&regex_pat) {
           Ok(r) => r,
           Err(e) => return Ok(ExecResult::failure(
               2, format!("grep: invalid pattern: {e}")
           )),
       };

       let content = match read_input(&ctx, &opts.files) {
           Ok(c) => c,
           Err(e) => return Ok(e),
       };

       let matched = grep_match_lines(&content, &re, opts.invert);

       if opts.count_only {
           return Ok(ExecResult {
               exit_code: if matched.is_empty() { 1 } else { 0 },
               stdout: format!("{}\n", matched.len()),
               stderr: String::new(),
           });
       }

       if matched.is_empty() {
           Ok(ExecResult::code(1))
       } else {
           let mut output = matched.join("\n");
           output.push('\n');
           Ok(ExecResult::success(output))
       }
   }
   ```

3. Verify:

   ```
   cargo nextest run -p sandbox → all pass
   cargo clippy -p sandbox -- -D warnings → zero
   ```

4. Commit: `refactor(sandbox): extract grep flag parsing and matching`

---

### Task 7: Extract Ls formatting and evaluate_test magic number

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/builtins/nav.rs`,
`crates/sandbox/src/builtins/core.rs`
**Run**: `cargo nextest run -p sandbox`

`Ls::execute` has complexity 25 and nesting depth 5. `evaluate_test`
has magic number `3`.

1. In `nav.rs`, extract entry formatting:

   ```rust
   fn format_entry(entry: &crate::fs::DirEntry, long_format: bool)
       -> String
   {
       if long_format {
           let kind = if entry.is_dir { "d" } else { "-" };
           format!(
               "{}rwxr-xr-x  1 user  group  {:>8}  {}\n",
               kind, entry.size, entry.name
           )
       } else {
           format!("{}\n", entry.name)
       }
   }
   ```

2. Replace the inner loop in `Ls::execute`:

   ```rust
   for entry in &entries {
       output.push_str(&format_entry(entry, long_format));
   }
   ```

3. In `core.rs`, add constant for binary test arg count:

   ```rust
   /// Expected arg count for binary test operators (-eq, -gt, etc.)
   const BINARY_TEST_ARGC: usize = 3;
   ```

4. Replace `if args.len() == 3` with `if args.len() == BINARY_TEST_ARGC`.

5. Verify:

   ```
   cargo nextest run -p sandbox → all pass
   cargo clippy -p sandbox -- -D warnings → zero
   ```

6. Commit: `refactor(sandbox): extract ls formatting, name test magic
number`

---

### Task 8: Add IOSP inline suppressions for I/O boundary functions

**Crate**: `sandbox`
**File(s)**: `crates/sandbox/src/builtins/core.rs`,
`crates/sandbox/src/builtins/file.rs`,
`crates/sandbox/src/fs.rs`
**Run**: `rustqual crates/sandbox`

The remaining IOSP VIOLATION findings are on functions that inherently
mix logic with I/O calls — `Test_::execute`, `BracketTest::execute`,
`evaluate_test`, `Find::execute`, `find_walk`,
`SandboxFs::remove_recursive`, `SandboxFs::walk_all_recursive`.

These are I/O boundary functions where splitting would not improve
readability.

1. Add `// qual:allow(iosp) reason: "I/O boundary"` above each:
   - `core.rs`: above `Test_::execute`, `BracketTest::execute`,
     `evaluate_test`
   - `file.rs`: above `Find::execute`, `find_walk`
   - `fs.rs`: above `remove_recursive`, `walk_all_recursive`

2. Verify: `rustqual crates/sandbox` — IOSP violations should drop
   to 0.

3. Commit: `chore(sandbox): suppress IOSP findings on I/O boundary
functions`

---

### Task 9: Final validation and baseline

**Crate**: `sandbox`
**File(s)**: (none — validation only)
**Run**: `cargo nextest run --workspace && rustqual crates/sandbox`

1. Run full test suite:

   ```
   cargo nextest run --workspace → all pass
   cargo clippy --workspace -- -D warnings → zero
   ```

2. Run rustqual and verify score >= 90%.

3. Save baseline:

   ```
   rustqual crates/sandbox --save-baseline docs/rustqual-baseline.json
   ```

4. Commit: `chore(sandbox): save rustqual baseline at 90%+`

## Projected Finding Reduction

| Task      | Findings removed | Category                   |
| --------- | ---------------- | -------------------------- |
| 1         | ~14              | DEAD_CODE (config)         |
| 2         | ~6               | DUPLICATE                  |
| 3         | ~8               | Complexity, nesting, LONG  |
| 4         | ~3               | Complexity, LONG           |
| 5         | ~4               | Complexity, LONG, BP       |
| 6         | ~2               | Complexity, LONG           |
| 7         | ~4               | Complexity, nesting, MAGIC |
| 8         | ~7               | IOSP violations            |
| **Total** | **~48**          | 93 → ~45 → score ~90%      |

## Risk

- **Test breakage**: All tasks are pure refactoring — observable
  behavior is unchanged. Each task runs the full test suite.
- **Over-splitting**: Tasks 3-6 extract methods only where the
  resulting functions are independently meaningful. No
  single-use-throwaway extractions.
- **IOSP suppression drift**: Task 8 suppressions are documented
  with reasons. Future code changes should re-evaluate whether the
  suppression is still warranted.

## Parallelization

Tasks 1-2 are independent of Tasks 3-8. Within Tasks 3-8, each
targets a different file and can run in parallel:

- Slot A: Task 3 (lexer.rs) + Task 5 (parser/mod.rs)
- Slot B: Task 4 (interpreter/mod.rs)
- Slot C: Task 6 (text.rs) + Task 7 (nav.rs + core.rs)
- Slot D: Task 8 (suppressions across files)
- Sequential: Task 9 (final validation)
