#![no_main]
use libfuzzer_sys::fuzz_target;

// Fuzz the parser with arbitrary byte strings.
// Goal: no panics, no infinite loops (fuel-limited).
fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        let _ = sandbox::parser::Parser::parse(input, 10_000, 50);
    }
});
