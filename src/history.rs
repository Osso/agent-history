use crate::cli::{SearchArgs, Source, SourceFilter};
use crate::content::{extract_claude_or_pi_segments, extract_codex_segment};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct Hit {
    pub source: Source,
    pub session: String,
    pub line_no: usize,
    pub role: String,
    pub timestamp: String,
    pub cwd: String,
    pub text: String,
    pub file: PathBuf,
}

#[derive(Debug)]
pub(crate) struct SessionFile {
    source: Source,
    kind: StorageKind,
    path: PathBuf,
}

#[derive(Clone, Copy, Debug)]
enum StorageKind {
    Live,
    Archive,
}

#[derive(Clone, Debug)]
struct SessionMeta {
    session: String,
    cwd: String,
}

struct SearchContext<'a> {
    args: &'a SearchArgs,
    regex: &'a Regex,
    since: Option<NaiveDate>,
    until: Option<NaiveDate>,
}

struct HitInput<'a> {
    file: &'a SessionFile,
    line_no: usize,
    source: Source,
    session: String,
    role: String,
    timestamp: String,
    cwd: String,
    text: String,
}

pub fn build_regex(args: &SearchArgs) -> Result<Regex, regex::Error> {
    let pattern = if args.ignore_case {
        format!("(?i){}", args.pattern)
    } else {
        args.pattern.clone()
    };
    Regex::new(&pattern)
}

pub fn parse_date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").context("date must be YYYY-MM-DD")
}

pub fn collect_session_files(args: &SearchArgs) -> Vec<SessionFile> {
    let mut files = Vec::new();
    if includes_source(args.source, Source::Claude) {
        files.extend(collect_claude_files(args));
    }
    if includes_source(args.source, Source::Codex) {
        files.extend(collect_codex_files(args));
    }
    if includes_source(args.source, Source::Pi) {
        files.extend(collect_pi_files(args));
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

pub fn search_files(
    files: &[SessionFile],
    args: &SearchArgs,
    regex: &Regex,
    since: Option<NaiveDate>,
    until: Option<NaiveDate>,
) -> Vec<Hit> {
    let context = SearchContext {
        args,
        regex,
        since,
        until,
    };

    if args.files_with_matches {
        return search_files_by_matching_file(files, &context);
    }

    let mut hits = Vec::new();
    for file in files {
        let remaining = remaining_match_capacity(args, hits.len());
        if remaining == Some(0) {
            break;
        }

        let mut file_hits = search_file(file, &context);
        if let Some(limit) = remaining {
            file_hits.truncate(limit);
        }
        hits.extend(file_hits);
    }

    hits
}

fn search_files_by_matching_file(files: &[SessionFile], context: &SearchContext<'_>) -> Vec<Hit> {
    let mut hits = Vec::new();
    for file in files {
        if context.args.max_count == Some(hits.len()) {
            break;
        }

        if let Some(hit) = search_file(file, context).into_iter().next() {
            hits.push(hit);
        }
    }
    hits
}

fn includes_source(filter: SourceFilter, source: Source) -> bool {
    matches!(filter, SourceFilter::All)
        || matches!(
            (filter, source),
            (SourceFilter::Claude, Source::Claude)
                | (SourceFilter::Codex, Source::Codex)
                | (SourceFilter::Pi, Source::Pi)
        )
}

fn collect_claude_files(args: &SearchArgs) -> Vec<SessionFile> {
    let live_dir = home_dir().join(".claude/projects");
    let archive_dir = home_dir().join(".claude/archive");
    let mut files = Vec::new();

    if !args.archive_only {
        files.extend(collect_walked_files(
            Source::Claude,
            StorageKind::Live,
            &live_dir,
            args,
            is_jsonl_file,
        ));
    }
    if !args.live_only {
        files.extend(collect_walked_files(
            Source::Claude,
            StorageKind::Archive,
            &archive_dir,
            args,
            is_zstd_jsonl_file,
        ));
    }

    files
}

fn collect_codex_files(args: &SearchArgs) -> Vec<SessionFile> {
    let live_dir = home_dir().join(".codex/sessions");
    let archive_dir = home_dir().join(".codex/archived_sessions");
    let mut files = Vec::new();

    if !args.archive_only {
        files.extend(collect_walked_files(
            Source::Codex,
            StorageKind::Live,
            &live_dir,
            args,
            is_jsonl_file,
        ));
    }
    if !args.live_only {
        files.extend(collect_walked_files(
            Source::Codex,
            StorageKind::Archive,
            &archive_dir,
            args,
            is_jsonl_file,
        ));
    }

    files
}

fn collect_pi_files(args: &SearchArgs) -> Vec<SessionFile> {
    if args.archive_only {
        return Vec::new();
    }

    let sessions_dir = home_dir().join(".config/pi/agent/sessions");
    collect_walked_files(
        Source::Pi,
        StorageKind::Live,
        &sessions_dir,
        args,
        is_jsonl_file,
    )
}

fn collect_walked_files(
    source: Source,
    kind: StorageKind,
    root: &Path,
    args: &SearchArgs,
    file_matches: fn(&Path) -> bool,
) -> Vec<SessionFile> {
    if !root.exists() {
        return Vec::new();
    }

    walkdir::WalkDir::new(root)
        .into_iter()
        .flatten()
        .map(|entry| entry.into_path())
        .filter(|path| path.is_file() && file_matches(path))
        .filter(|path| date_filename_matches(path, args))
        .filter(|path| path_encoded_filters_match(source, root, path, args))
        .map(|path| SessionFile { source, kind, path })
        .collect()
}

fn is_jsonl_file(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("jsonl")
}

fn is_zstd_jsonl_file(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|value| value.ends_with(".jsonl.zst"))
}

fn date_filename_matches(path: &Path, args: &SearchArgs) -> bool {
    let Some(date) = first_filename_date(path) else {
        return true;
    };

    let since = args.since.as_deref().and_then(parse_date_for_pruning);
    let until = args.until.as_deref().and_then(parse_date_for_pruning);
    since.is_none_or(|minimum| date >= minimum) && until.is_none_or(|maximum| date <= maximum)
}

fn first_filename_date(path: &Path) -> Option<NaiveDate> {
    let filename = path.file_name()?.to_str()?;
    for start in 0..filename.len().saturating_sub(9) {
        let end = start + 10;
        let candidate = filename.get(start..end)?;
        if let Ok(date) = NaiveDate::parse_from_str(candidate, "%Y-%m-%d") {
            return Some(date);
        }
    }
    None
}

fn parse_date_for_pruning(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn path_encoded_filters_match(source: Source, root: &Path, path: &Path, args: &SearchArgs) -> bool {
    let relative_path = path.strip_prefix(root).unwrap_or(path);
    let relative = relative_path.to_string_lossy();

    if source == Source::Pi && path_project_filter_can_prune(&relative, args.project.as_deref()) {
        return false;
    }

    if source == Source::Pi && !path_contains_optional_filter(&relative, args.session.as_deref()) {
        return false;
    }

    if source == Source::Claude
        && claude_encoded_project_path(relative_path).is_some_and(|project_path| {
            claude_project_filter_can_prune(&project_path, args.project.as_deref())
        })
    {
        return false;
    }

    if source == Source::Claude
        && !path_contains_optional_filter(&relative, args.session.as_deref())
    {
        return false;
    }

    true
}

fn claude_encoded_project_path(path: &Path) -> Option<String> {
    let first_component = path.components().next()?.as_os_str().to_str()?;
    if first_component.is_empty() {
        return None;
    }

    Some(first_component.replace('-', "/"))
}

fn path_project_filter_can_prune(path: &str, filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return false;
    };

    !filter.contains('/') && !path.contains(filter)
}

fn claude_project_filter_can_prune(path: &str, filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return false;
    };

    !filter.contains('-') && !path.contains(filter)
}

fn path_contains_optional_filter(path: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|value| path.contains(value))
}

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".to_string()))
}

fn remaining_match_capacity(args: &SearchArgs, current_hits: usize) -> Option<usize> {
    args.max_count
        .map(|max_count| max_count.saturating_sub(current_hits))
}

fn search_file(file: &SessionFile, context: &SearchContext<'_>) -> Vec<Hit> {
    let Ok(reader) = open_lines(file) else {
        return Vec::new();
    };

    match file.source {
        Source::Claude => search_claude_file(file, reader, context),
        Source::Codex => search_codex_file(file, reader, context),
        Source::Pi => search_pi_file(file, reader, context),
    }
}

fn open_lines(file: &SessionFile) -> Result<Box<dyn BufRead>> {
    let handle =
        File::open(&file.path).with_context(|| format!("opening {}", file.path.display()))?;
    let reader: Box<dyn BufRead> = match file.kind {
        StorageKind::Live => Box::new(BufReader::new(handle)),
        StorageKind::Archive => open_archive_reader(file, handle)?,
    };
    Ok(reader)
}

fn open_archive_reader(file: &SessionFile, handle: File) -> Result<Box<dyn BufRead>> {
    if is_zstd_jsonl_file(&file.path) {
        return Ok(Box::new(BufReader::new(zstd::stream::read::Decoder::new(
            handle,
        )?)));
    }

    Ok(Box::new(BufReader::new(handle)))
}

fn search_claude_file(
    file: &SessionFile,
    reader: Box<dyn BufRead>,
    context: &SearchContext<'_>,
) -> Vec<Hit> {
    reader
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| parse_json_line(line_index, line))
        .flat_map(|(line_no, value)| claude_hits(file, line_no, value, context))
        .collect()
}

fn claude_hits(
    file: &SessionFile,
    line_no: usize,
    value: Value,
    context: &SearchContext<'_>,
) -> Vec<Hit> {
    let Some(role) = value.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    if role != "user" && role != "assistant" {
        return Vec::new();
    }

    let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
    else {
        return Vec::new();
    };

    let timestamp = json_string(&value, "timestamp");
    let cwd = json_string(&value, "cwd");
    let session = json_string(&value, "sessionId");
    extract_claude_or_pi_segments(content, role, context.args.all)
        .into_iter()
        .filter_map(|segment| {
            build_hit(
                HitInput {
                    file,
                    line_no,
                    source: Source::Claude,
                    session: session.clone(),
                    role: segment.role,
                    timestamp: timestamp.clone(),
                    cwd: cwd.clone(),
                    text: segment.text,
                },
                context,
            )
        })
        .collect()
}

fn search_codex_file(
    file: &SessionFile,
    mut reader: Box<dyn BufRead>,
    context: &SearchContext<'_>,
) -> Vec<Hit> {
    let Some((meta, first_search_line)) = read_codex_meta(&mut reader) else {
        return Vec::new();
    };

    if !project_matches(&meta.cwd, context.args) || !session_matches(&meta.session, context.args) {
        return Vec::new();
    }

    let mut hits = collect_first_codex_hit(file, first_search_line, &meta, context);
    let parsed_lines = reader
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| parse_json_line(line_index + 1, line));

    hits.extend(
        parsed_lines.filter_map(|(line_no, value)| codex_hit(file, line_no, value, &meta, context)),
    );
    hits
}

fn collect_first_codex_hit(
    file: &SessionFile,
    first_search_line: Option<(usize, Value)>,
    meta: &SessionMeta,
    context: &SearchContext<'_>,
) -> Vec<Hit> {
    let Some((line_no, value)) = first_search_line else {
        return Vec::new();
    };

    codex_hit(file, line_no, value, meta, context)
        .into_iter()
        .collect()
}

fn read_codex_meta(reader: &mut dyn BufRead) -> Option<(SessionMeta, Option<(usize, Value)>)> {
    let mut buffer = String::new();
    if reader.read_line(&mut buffer).ok()? == 0 {
        return None;
    }

    let value = serde_json::from_str::<Value>(buffer.trim_end()).ok()?;
    if value.get("type").and_then(|item| item.as_str()) != Some("session_meta") {
        return Some((
            SessionMeta {
                session: String::new(),
                cwd: String::new(),
            },
            Some((1, value)),
        ));
    }

    let payload = value.get("payload")?;
    let meta = SessionMeta {
        session: json_string(payload, "id"),
        cwd: json_string(payload, "cwd"),
    };
    Some((meta, None))
}

fn codex_hit(
    file: &SessionFile,
    line_no: usize,
    value: Value,
    meta: &SessionMeta,
    context: &SearchContext<'_>,
) -> Option<Hit> {
    if value.get("type")?.as_str()? != "response_item" {
        return None;
    }

    let payload = value.get("payload")?;
    let segment = extract_codex_segment(payload, context.args.all);
    let timestamp = json_string(&value, "timestamp");
    build_hit(
        HitInput {
            file,
            line_no,
            source: Source::Codex,
            session: meta.session.clone(),
            role: segment.role,
            timestamp,
            cwd: meta.cwd.clone(),
            text: segment.text,
        },
        context,
    )
}

fn search_pi_file(
    file: &SessionFile,
    mut reader: Box<dyn BufRead>,
    context: &SearchContext<'_>,
) -> Vec<Hit> {
    let Some(meta) = read_pi_meta(&mut reader) else {
        return Vec::new();
    };

    if !project_matches(&meta.cwd, context.args) || !session_matches(&meta.session, context.args) {
        return Vec::new();
    }

    reader
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| parse_json_line(line_index + 1, line))
        .flat_map(|(line_no, value)| pi_hits(file, line_no, value, &meta, context))
        .collect()
}

fn read_pi_meta(reader: &mut dyn BufRead) -> Option<SessionMeta> {
    let mut buffer = String::new();
    if reader.read_line(&mut buffer).ok()? == 0 {
        return None;
    }

    let value = serde_json::from_str::<Value>(buffer.trim_end()).ok()?;
    if value.get("type").and_then(|item| item.as_str()) != Some("session") {
        return Some(SessionMeta {
            session: String::new(),
            cwd: String::new(),
        });
    }

    Some(SessionMeta {
        session: json_string(&value, "id"),
        cwd: json_string(&value, "cwd"),
    })
}

fn pi_hits(
    file: &SessionFile,
    line_no: usize,
    value: Value,
    meta: &SessionMeta,
    context: &SearchContext<'_>,
) -> Vec<Hit> {
    if value.get("type").and_then(Value::as_str) != Some("message") {
        return Vec::new();
    }

    let Some(message) = value.get("message") else {
        return Vec::new();
    };
    let role = json_string(message, "role");
    let timestamp = json_string(&value, "timestamp");
    let Some(content) = message.get("content") else {
        return Vec::new();
    };

    extract_claude_or_pi_segments(content, &role, context.args.all)
        .into_iter()
        .filter_map(|segment| {
            build_hit(
                HitInput {
                    file,
                    line_no,
                    source: Source::Pi,
                    session: meta.session.clone(),
                    role: segment.role,
                    timestamp: timestamp.clone(),
                    cwd: meta.cwd.clone(),
                    text: segment.text,
                },
                context,
            )
        })
        .collect()
}

fn build_hit(input: HitInput<'_>, context: &SearchContext<'_>) -> Option<Hit> {
    if input.text.is_empty() || !context.regex.is_match(&input.text) {
        return None;
    }
    if !role_matches(&input.role, context.args) || !project_matches(&input.cwd, context.args) {
        return None;
    }
    if !session_matches(&input.session, context.args)
        || !date_matches(&input.timestamp, context.since, context.until)
    {
        return None;
    }

    Some(Hit {
        source: input.source,
        session: input.session,
        line_no: input.line_no,
        role: input.role,
        timestamp: input.timestamp,
        cwd: input.cwd,
        text: input.text,
        file: input.file.path.clone(),
    })
}

fn role_matches(role: &str, args: &SearchArgs) -> bool {
    args.role.as_deref().is_none_or(|filter| role == filter)
}

fn project_matches(cwd: &str, args: &SearchArgs) -> bool {
    args.project
        .as_deref()
        .is_none_or(|filter| cwd.contains(filter))
}

fn session_matches(session: &str, args: &SearchArgs) -> bool {
    args.session
        .as_deref()
        .is_none_or(|filter| session.contains(filter))
}

fn date_matches(timestamp: &str, since: Option<NaiveDate>, until: Option<NaiveDate>) -> bool {
    if timestamp.is_empty() {
        return since.is_none() && until.is_none();
    }

    let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) else {
        return since.is_none() && until.is_none();
    };

    let date = parsed.naive_utc().date();
    since.is_none_or(|minimum| date >= minimum) && until.is_none_or(|maximum| date <= maximum)
}

fn parse_json_line(line_index: usize, line: std::io::Result<String>) -> Option<(usize, Value)> {
    let line = line.ok()?;
    let value = serde_json::from_str::<Value>(&line).ok()?;
    Some((line_index + 1, value))
}

fn json_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_string()
}
