use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
pub struct SearchRecord {
    #[serde(rename = "type")]
    pub record_type: String,
    pub text: String,
    #[serde(rename = "source")]
    pub _source: String,
    pub path: String,
    pub session_id: String,
    pub score: f32,
}

pub struct SearchResponse {
    pub raw_output: Vec<u8>,
    pub records: Vec<SearchRecord>,
}

pub fn query_claude_memory(query: &str, limit: usize) -> Result<SearchResponse> {
    let output = Command::new("claude-memory")
        .args(["search", "--json", "--limit"])
        .arg(limit.to_string())
        .arg(query)
        .output()
        .context("failed to spawn claude-memory")?;

    if !output.status.success() {
        let status = output
            .status
            .code()
            .map_or_else(|| "signal".to_string(), |code| code.to_string());
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("claude-memory exited with status {status}: {stderr}");
    }

    let records = parse_records(&output.stdout)?;
    Ok(SearchResponse {
        raw_output: output.stdout,
        records,
    })
}

fn parse_records(output: &[u8]) -> Result<Vec<SearchRecord>> {
    let text = std::str::from_utf8(output).context("claude-memory output was not UTF-8")?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("invalid claude-memory JSON record"))
        .collect()
}
