use sandbox::Shell;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("usage: sandbox-cli <command>");
        eprintln!("       sandbox-cli -c 'echo hello'");
        std::process::exit(1);
    }

    let input = if args[1] == "-c" {
        args[2..].join(" ")
    } else {
        args[1..].join(" ")
    };

    let mut shell = Shell::builder()
        .env("HOME", "/home/user")
        .env("USER", "user")
        .cwd("/home/user")
        .build();

    let output = shell.exec(&input).await?;
    print!("{}", output.stdout);
    if !output.stderr.is_empty() {
        eprint!("{}", output.stderr);
    }
    std::process::exit(output.exit_code);
}
