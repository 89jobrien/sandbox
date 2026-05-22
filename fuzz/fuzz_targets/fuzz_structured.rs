#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use sandbox::limits::ExecutionLimits;
use sandbox::Shell;

/// Structured fuzzer: generates shell-like command sequences from
/// structured data rather than raw bytes. This explores deeper
/// execution paths than pure byte fuzzing.
#[derive(Arbitrary, Debug)]
struct FuzzInput {
    commands: Vec<FuzzCommand>,
}

#[derive(Arbitrary, Debug)]
enum FuzzCommand {
    Echo(String),
    Assign(VarName, String),
    If { cond_true: bool },
    For { items: Vec<String> },
    While { iterations: u8 },
    Redirect(String),
    Pipe,
    Sequence,
    Cd(String),
    Touch(String),
    Mkdir(String),
    Cat(String),
    True,
    False,
    Exit,
}

#[derive(Arbitrary, Debug)]
struct VarName {
    first: char,
    rest: Vec<char>,
}

impl VarName {
    fn to_string(&self) -> String {
        let first = if self.first.is_alphabetic() {
            self.first
        } else {
            'X'
        };
        let rest: String = self
            .rest
            .iter()
            .take(10)
            .filter(|c| c.is_alphanumeric() || **c == '_')
            .collect();
        format!("{first}{rest}")
    }
}

impl FuzzInput {
    fn to_shell_script(&self) -> String {
        let mut parts = Vec::new();
        for cmd in &self.commands {
            let s = match cmd {
                FuzzCommand::Echo(msg) => {
                    let safe: String = msg.chars().take(30).filter(|c| c.is_alphanumeric() || *c == ' ').collect();
                    format!("echo {safe}")
                }
                FuzzCommand::Assign(name, val) => {
                    let safe_val: String = val.chars().take(20).filter(|c| c.is_alphanumeric()).collect();
                    format!("{}={safe_val}", name.to_string())
                }
                FuzzCommand::If { cond_true } => {
                    if *cond_true {
                        "if true; then echo y; fi".to_string()
                    } else {
                        "if false; then echo y; else echo n; fi".to_string()
                    }
                }
                FuzzCommand::For { items } => {
                    let words: Vec<String> = items
                        .iter()
                        .take(5)
                        .map(|s| {
                            s.chars().take(10).filter(|c| c.is_alphanumeric()).collect::<String>()
                        })
                        .filter(|s| !s.is_empty())
                        .collect();
                    if words.is_empty() {
                        "true".to_string()
                    } else {
                        format!("for i in {}; do echo $i; done", words.join(" "))
                    }
                }
                FuzzCommand::While { iterations } => {
                    let n = (*iterations as usize).min(5);
                    // Use a counted for-loop instead of while to stay bounded
                    let words: Vec<String> = (0..n).map(|i| i.to_string()).collect();
                    format!("for _i in {}; do echo loop; done", words.join(" "))
                }
                FuzzCommand::Redirect(name) => {
                    let safe: String = name.chars().take(15).filter(|c| c.is_alphanumeric() || *c == '.').collect();
                    if safe.is_empty() {
                        "true".to_string()
                    } else {
                        format!("echo data > /{safe}")
                    }
                }
                FuzzCommand::Pipe => "echo hello".to_string(),
                FuzzCommand::Sequence => "true".to_string(),
                FuzzCommand::Cd(dir) => {
                    let safe: String = dir.chars().take(20).filter(|c| c.is_alphanumeric() || *c == '/').collect();
                    format!("cd /{safe}")
                }
                FuzzCommand::Touch(f) => {
                    let safe: String = f.chars().take(15).filter(|c| c.is_alphanumeric() || *c == '.').collect();
                    if safe.is_empty() { "true".to_string() } else { format!("touch /{safe}") }
                }
                FuzzCommand::Mkdir(d) => {
                    let safe: String = d.chars().take(15).filter(|c| c.is_alphanumeric()).collect();
                    if safe.is_empty() { "true".to_string() } else { format!("mkdir -p /{safe}") }
                }
                FuzzCommand::Cat(f) => {
                    let safe: String = f.chars().take(15).filter(|c| c.is_alphanumeric() || *c == '.').collect();
                    if safe.is_empty() { "true".to_string() } else { format!("cat /{safe}") }
                }
                FuzzCommand::True => "true".to_string(),
                FuzzCommand::False => "false".to_string(),
                FuzzCommand::Exit => "true".to_string(), // skip exit to keep session alive
            };
            parts.push(s);
        }
        parts.join("; ")
    }
}

fuzz_target!(|input: FuzzInput| {
    let script = input.to_shell_script();
    if script.is_empty() || script.len() > 2000 {
        return;
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let mut shell = Shell::builder()
            .cwd("/")
            .limits(ExecutionLimits {
                max_commands: 100,
                max_loop_iterations: 20,
                max_total_loop_iters: 200,
                max_stdout_bytes: 8192,
                max_stderr_bytes: 4096,
                max_input_bytes: 4096,
                max_parser_fuel: 10_000,
                max_ast_depth: 30,
                ..Default::default()
            })
            .build();
        let _ = shell.exec(&script).await;
    });
});
