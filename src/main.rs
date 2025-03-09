use clap::{Parser, Subcommand};

mod config;
mod context;
mod openrouter;

use context::Context;

#[derive(Subcommand)]
enum Command {
    /// Run a task
    #[clap(name = "run", alias = "")]
    Run,
    /// Login using OpenRouter
    Login,
}

#[derive(Parser)]
#[clap(version, author, about, long_about = None)]
struct Cli {
    /// Enable trace logging
    #[clap(long)]
    trace: bool,
    /// Enable debug logging
    #[clap(long)]
    debug: bool,
    #[clap(subcommand)]
    command: Option<Command>,
}

pub fn main() {
    let cli = Cli::parse();
    let mut builder = env_logger::Builder::from_default_env();
    builder
        .format_timestamp(None)
        .format_level(false)
        .format_target(false);

    if cli.trace {
        builder.filter_level(log::LevelFilter::Trace);
    } else if cli.debug {
        builder.filter_level(log::LevelFilter::Debug);
    } else {
        builder.filter_level(log::LevelFilter::Warn);
    }

    builder.init();

    match cli.command.unwrap_or(Command::Run) {
        Command::Run => {
            let config = config::Config::load_or_create().expect("Failed to load config");
            let Some(openrouter_key) = config.openrouter_key else {
                eprintln!("OpenRouter API key is not set.");
                eprintln!("Run `minion login` to authenticate with OpenRouter.");
                std::process::exit(1);
            };

            let _ctx = Context { openrouter_key };
        }
        Command::Login => {
            tokio::runtime::Runtime::new()
                .expect("Failed to create runtime")
                .block_on(async {
                    let config = config::Config::load_or_create().expect("Failed to load config");
                    openrouter::login_flow(config)
                        .await
                        .expect("Failed to start login flow");
                });
        }
    }
}
