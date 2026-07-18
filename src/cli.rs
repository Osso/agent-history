use clap::{Parser, Subcommand};

pub const DEFAULT_MAX_COUNT: usize = 5;

#[derive(Parser, Debug)]
#[command(
    name = "agent-history",
    about = "Retrieve ranked session history from claude-memory"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Search(SearchArgs),
}

#[derive(Parser, Debug)]
pub struct SearchArgs {
    /// Plain-text retrieval query
    pub query: String,

    /// Return at most N ranked results
    #[arg(short = 'm', long, default_value_t = DEFAULT_MAX_COUNT)]
    pub max_count: usize,

    /// Output backend NDJSON records unchanged
    #[arg(long)]
    pub json: bool,

    /// Disable color output
    #[arg(long)]
    pub no_color: bool,
}
