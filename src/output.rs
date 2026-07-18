use crate::backend::SearchResponse;
use crate::cli::SearchArgs;
use anyhow::Result;
use std::io::{IsTerminal, Write};

struct Style {
    enabled: bool,
}

impl Style {
    fn paint(&self, code: &str, value: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{value}\x1b[0m")
        } else {
            value.to_string()
        }
    }
}

pub fn print_response(response: &SearchResponse, args: &SearchArgs) -> Result<()> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();

    if args.json {
        output.write_all(&response.raw_output)?;
        return Ok(());
    }

    let style = Style {
        enabled: !args.no_color && std::io::stdout().is_terminal(),
    };
    for record in &response.records {
        writeln!(
            output,
            "{} {} {} {}",
            style.paint("33", &record.score.to_string()),
            style.paint("36", &record.record_type),
            style.paint("35", &record.session_id),
            record.path
        )?;
        writeln!(output, "{}", record.text)?;
    }
    Ok(())
}
