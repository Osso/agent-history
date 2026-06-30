use crate::cli::SearchArgs;
use crate::history::Hit;
use anyhow::Result;
use regex::Regex;
use std::collections::HashSet;
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

    fn highlight(&self, regex: &Regex, value: &str) -> String {
        if self.enabled {
            regex.replace_all(value, "\x1b[1;31m$0\x1b[0m").into_owned()
        } else {
            value.to_string()
        }
    }
}

pub fn print_hits(hits: &[Hit], args: &SearchArgs, regex: &Regex) -> Result<()> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();

    if args.files_with_matches {
        print_matching_files(hits, args, &mut output);
        return Ok(());
    }

    let style = Style {
        enabled: !args.no_color && std::io::stdout().is_terminal(),
    };
    for hit in hits {
        print_hit(hit, args, regex, &style, &mut output)?;
    }
    Ok(())
}

fn print_matching_files(hits: &[Hit], args: &SearchArgs, output: &mut impl Write) {
    let mut seen = HashSet::new();
    let mut count = 0usize;
    for hit in hits {
        if !seen.insert(hit.file.clone()) {
            continue;
        }
        writeln!(output, "{}", hit.file.display()).ok();
        count += 1;
        if args.max_count == Some(count) {
            break;
        }
    }
}

fn print_hit(
    hit: &Hit,
    args: &SearchArgs,
    regex: &Regex,
    style: &Style,
    output: &mut impl Write,
) -> Result<()> {
    if args.json {
        writeln!(output, "{}", serde_json::to_string(hit)?)?;
        return Ok(());
    }

    let session_label = format!("{}:{}", hit.source.label(), hit.session);
    writeln!(
        output,
        "{}:{} {} {} {}",
        style.paint("35", &session_label),
        style.paint("33", &hit.line_no.to_string()),
        style.paint("36", &hit.role),
        style.paint("32", &hit.timestamp),
        hit.cwd
    )?;

    for line in hit.text.lines().filter(|line| regex.is_match(line)) {
        writeln!(output, "  {}", style.highlight(regex, line))?;
    }
    writeln!(output)?;
    Ok(())
}
