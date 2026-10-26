use clap::Parser;
use cowmark_cli::cli::{Cli, run};
use cowmark_cli::ops::Context;

fn main() {
    let cli = Cli::parse();
    let context = Context::from_env(cli.root.clone());
    let code = run(cli, &context, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(code);
}
