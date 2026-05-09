mod commands;
mod output;
mod tui;

use clap::Parser;
use commands::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let opts = &cli.global;

    match cli.command {
        Commands::Login => commands::login::run(opts).await,
        Commands::Logout(args) => commands::logout::run(&args, opts).await,
        Commands::Whoami => commands::whoami::run(opts).await,
        Commands::Course(sub) => commands::course::run(sub, opts).await,
        Commands::Assignment(args) => commands::assignment::run(args, opts).await,
        Commands::File(sub) => commands::file::run(sub, opts).await,
        Commands::Announcement(sub) => commands::announcement::run(sub, opts).await,
        Commands::Discussion(sub) => commands::discussion::run(sub, opts).await,
        Commands::Module(sub) => commands::module::run(sub, opts).await,
    }
}
