mod cli;
mod content;
mod history;
mod output;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command, SearchArgs};

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Search(args) => run_search(args),
    }
}

fn run_search(args: SearchArgs) -> Result<()> {
    let regex = history::build_regex(&args).context("invalid regex")?;
    let since = args.since.as_deref().map(history::parse_date).transpose()?;
    let until = args.until.as_deref().map(history::parse_date).transpose()?;
    let files = history::collect_session_files(&args);
    let hits = history::search_files(&files, &args, &regex, since, until);

    if hits.is_empty() {
        std::process::exit(1);
    }

    output::print_hits(&hits, &args, &regex)
}
