use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "pandar", about = "Pandar operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Print CLI version")]
    Version,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Version => println!("{}", env!("CARGO_PKG_VERSION")),
    }

    Ok(())
}
