use proptest::prelude::*;

use sandbox::Shell;
use sandbox::capabilities::CapabilitySet;
use sandbox::limits::ExecutionLimits;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

// ── Parser never panics on arbitrary input ──────────────────────────

proptest! {
    #[test]
    fn parser_does_not_panic(input in "\\PC{0,500}") {
        let _ = sandbox::parser::Parser::parse(&input, 10_000, 50);
    }

    #[test]
    fn parser_bounded_ascii(input in "[a-zA-Z0-9 ;|&$=(){}<>\"'!\\-_./\\\\#\\n\\t]{0,300}") {
        let _ = sandbox::parser::Parser::parse(&input, 10_000, 50);
    }
}

// ── Shell.exec never panics on arbitrary input ──────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn shell_exec_does_not_panic(input in "\\PC{0,200}") {
        let rt = rt();
        rt.block_on(async {
            let mut shell = Shell::builder()
                .cwd("/")
                .limits(ExecutionLimits {
                    max_commands: 100,
                    max_loop_iterations: 50,
                    max_total_loop_iters: 200,
                    max_stdout_bytes: 4096,
                    max_input_bytes: 1024,
                    ..Default::default()
                })
                .build();
            let _ = shell.exec(&input).await;
        });
    }
}

// ── Echo roundtrip: echo $X always outputs the value of X ───────────

proptest! {
    #[test]
    fn echo_roundtrip(value in "[a-zA-Z0-9_]{1,50}") {
        let rt = rt();
        rt.block_on(async {
            let mut shell = Shell::builder().cwd("/").build();
            shell.exec(&format!("X={value}")).await.unwrap();
            let out = shell.exec("echo $X").await.unwrap();
            assert_eq!(out.stdout, format!("{value}\n"));
        });
    }
}

// ── Variable assignment roundtrip ───────────────────────────────────

proptest! {
    #[test]
    fn var_assign_roundtrip(
        name in "[a-zA-Z_][a-zA-Z0-9_]{0,20}",
        value in "[a-zA-Z0-9]{0,50}"
    ) {
        let rt = rt();
        rt.block_on(async {
            let mut shell = Shell::builder().cwd("/").build();
            shell.exec(&format!("{name}={value}")).await.unwrap();
            assert_eq!(shell.vars().get(&name).unwrap(), &value);
        });
    }
}

// ── Export roundtrip ────────────────────────────────────────────────

proptest! {
    #[test]
    fn export_roundtrip(
        name in "[a-zA-Z_][a-zA-Z0-9_]{0,20}",
        value in "[a-zA-Z0-9]{0,50}"
    ) {
        let rt = rt();
        rt.block_on(async {
            let mut shell = Shell::builder().cwd("/").build();
            shell.exec(&format!("export {name}={value}")).await.unwrap();
            assert_eq!(shell.env().get(&name).unwrap(), &value);
        });
    }
}

// ── For-loop iteration count matches word count ─────────────────────

proptest! {
    #[test]
    fn for_loop_count(count in 1usize..20) {
        let rt = rt();
        rt.block_on(async {
            let words: Vec<String> = (0..count).map(|i| format!("w{i}")).collect();
            let input = format!(
                "for i in {}; do echo $i; done",
                words.join(" ")
            );
            let mut shell = Shell::builder().cwd("/").build();
            let out = shell.exec(&input).await.unwrap();
            let lines: Vec<&str> = out.stdout.lines().collect();
            assert_eq!(lines.len(), count);
            for (i, line) in lines.iter().enumerate() {
                assert_eq!(*line, words[i]);
            }
        });
    }
}

// ── File write/read roundtrip ───────────────────────────────────────

proptest! {
    #[test]
    fn file_roundtrip(content in "[a-zA-Z0-9 ]{1,100}") {
        let rt = rt();
        rt.block_on(async {
            let mut shell = Shell::builder().cwd("/").build();
            shell.exec(&format!("printf '%s' '{content}' > /test.txt"))
                .await
                .unwrap();
            let out = shell.exec("cat /test.txt").await.unwrap();
            assert_eq!(out.stdout, content);
        });
    }
}

// ── Sequence produces output from all commands ──────────────────────

proptest! {
    #[test]
    fn sequence_all_outputs(count in 1usize..10) {
        let rt = rt();
        rt.block_on(async {
            let cmds: Vec<String> = (0..count).map(|i| format!("echo line{i}")).collect();
            let input = cmds.join("; ");
            let mut shell = Shell::builder().cwd("/").build();
            let out = shell.exec(&input).await.unwrap();
            let lines: Vec<&str> = out.stdout.lines().collect();
            assert_eq!(lines.len(), count);
        });
    }
}

// ── Capability denial is consistent ─────────────────────────────────

proptest! {
    #[test]
    fn empty_caps_always_deny_fs(input in "(cat|ls|mkdir|touch|rm|cp|mv) /[a-z]+") {
        let rt = rt();
        rt.block_on(async {
            let mut shell = Shell::builder()
                .cwd("/")
                .capabilities(CapabilitySet::empty())
                .build();
            let result = shell.exec(&input).await;
            assert!(result.is_err());
        });
    }
}

// ── Limits are enforced: loop never exceeds max ─────────────────────

proptest! {
    #[test]
    fn loop_limit_enforced(max_iters in 1usize..20) {
        let rt = rt();
        rt.block_on(async {
            let mut shell = Shell::builder()
                .cwd("/")
                .limits(ExecutionLimits {
                    max_loop_iterations: max_iters,
                    ..Default::default()
                })
                .build();
            let result = shell.exec("while true; do echo x; done").await;
            assert!(result.is_err());
        });
    }
}

// ── Nested depth limit ──────────────────────────────────────────────

proptest! {
    #[test]
    fn deep_nesting_rejected(depth in 10usize..50) {
        let open = "if true; then ".repeat(depth);
        let close = " fi".repeat(depth);
        let input = format!("{open}echo deep;{close}");
        // With max_ast_depth=5, deep nesting should fail
        let result = sandbox::parser::Parser::parse(&input, 100_000, 5);
        assert!(result.is_err());
    }
}
