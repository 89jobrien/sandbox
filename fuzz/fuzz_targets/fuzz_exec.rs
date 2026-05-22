#![no_main]
use libfuzzer_sys::fuzz_target;
use sandbox::limits::ExecutionLimits;
use sandbox::Shell;

// Fuzz full execution with tight limits to keep runs fast.
// Goal: no panics from any input. Errors are expected and fine.
fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        if input.len() > 500 {
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
                    max_commands: 50,
                    max_loop_iterations: 20,
                    max_total_loop_iters: 100,
                    max_stdout_bytes: 4096,
                    max_stderr_bytes: 4096,
                    max_input_bytes: 1024,
                    max_parser_fuel: 5_000,
                    max_ast_depth: 20,
                    ..Default::default()
                })
                .build();
            let _ = shell.exec(input).await;
        });
    }
});
