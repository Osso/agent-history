mod backend;
mod cli;
mod output;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, SearchArgs};

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Search(args) => run_search(args),
    }
}

fn run_search(args: SearchArgs) -> Result<()> {
    let response = backend::query_claude_memory(&args.query, args.max_count)?;
    if response.records.is_empty() {
        std::process::exit(1);
    }
    output::print_response(&response, &args)
}
