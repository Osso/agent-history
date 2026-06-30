use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(
    name = "agent-history",
    about = "Search local Claude Code, Codex, and Pi session history"
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
    /// Regex pattern to search for
    pub pattern: String,

    /// Session history source to search
    #[arg(long, value_enum, default_value_t = SourceFilter::All)]
    pub source: SourceFilter,

    /// Filter by message role
    #[arg(long)]
    pub role: Option<String>,

    /// Filter by cwd/project substring
    #[arg(long)]
    pub project: Option<String>,

    /// Filter by session id substring
    #[arg(long)]
    pub session: Option<String>,

    /// Only messages on or after YYYY-MM-DD
    #[arg(long)]
    pub since: Option<String>,

    /// Only messages on or before YYYY-MM-DD
    #[arg(long)]
    pub until: Option<String>,

    /// Case-insensitive regex
    #[arg(short = 'i', long)]
    pub ignore_case: bool,

    /// Stop after N matches
    #[arg(short = 'm', long)]
    pub max_count: Option<usize>,

    /// List matching session files only, not turns
    #[arg(short = 'l', long)]
    pub files_with_matches: bool,

    /// Search only live session files
    #[arg(long, conflicts_with = "archive_only")]
    pub live_only: bool,

    /// Search only archived session files
    #[arg(long)]
    pub archive_only: bool,

    /// Include hidden reasoning/tool payloads
    #[arg(long)]
    pub all: bool,

    /// Output JSON hit records
    #[arg(long)]
    pub json: bool,

    /// Disable color output
    #[arg(long)]
    pub no_color: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum SourceFilter {
    All,
    Claude,
    Codex,
    Pi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Claude,
    Codex,
    Pi,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Claude => "claude",
            Source::Codex => "codex",
            Source::Pi => "pi",
        }
    }
}
