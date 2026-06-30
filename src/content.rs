use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) struct ContentSegment {
    pub(crate) role: String,
    pub(crate) text: String,
}

pub(crate) fn extract_claude_or_pi_segments(
    content: &Value,
    visible_role: &str,
    include_all: bool,
) -> Vec<ContentSegment> {
    match content {
        Value::String(text) => vec![ContentSegment {
            role: visible_role.to_string(),
            text: text.clone(),
        }],
        Value::Array(blocks) => {
            extract_claude_or_pi_array_segments(blocks, visible_role, include_all)
        }
        _ => Vec::new(),
    }
}

fn extract_claude_or_pi_array_segments(
    blocks: &[Value],
    visible_role: &str,
    include_all: bool,
) -> Vec<ContentSegment> {
    let mut visible_text = Vec::new();
    let mut hidden_segments = Vec::new();

    for block in blocks {
        if let Some(text) = visible_block_text(block) {
            visible_text.push(text);
            continue;
        }
        if include_all {
            hidden_segments.extend(hidden_block_segment(block));
        }
    }

    let mut segments = Vec::new();
    if !visible_text.is_empty() {
        segments.push(ContentSegment {
            role: visible_role.to_string(),
            text: visible_text.join("\n"),
        });
    }
    segments.extend(hidden_segments);
    segments
}

fn visible_block_text(block: &Value) -> Option<String> {
    if block.get("type").and_then(Value::as_str) != Some("text") {
        return None;
    }

    block.get("text")?.as_str().map(ToString::to_string)
}

fn hidden_block_segment(block: &Value) -> Option<ContentSegment> {
    let block_type = block
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    match block_type {
        "thinking" => block.get("thinking")?.as_str().map(|text| ContentSegment {
            role: "reasoning".to_string(),
            text: text.to_string(),
        }),
        "tool_use" | "toolCall" => tool_input_text(block).as_deref().map(tool_segment),
        "tool_result" | "toolResult" => block
            .get("content")
            .map(value_to_text)
            .map(|text| tool_segment(text.as_str())),
        _ => None,
    }
}

fn tool_input_text(block: &Value) -> Option<String> {
    block
        .get("input")
        .or_else(|| block.get("arguments"))
        .or_else(|| block.get("args"))
        .map(Value::to_string)
}

fn tool_segment(text: &str) -> ContentSegment {
    ContentSegment {
        role: "tool".to_string(),
        text: text.to_string(),
    }
}

pub(crate) fn extract_codex_segment(payload: &Value, include_all: bool) -> ContentSegment {
    let payload_type = payload
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    match payload_type {
        "message" => extract_codex_message(payload),
        "reasoning" if include_all => extract_codex_reasoning(payload),
        "function_call" if include_all => extract_codex_function_call(payload),
        "function_call_output" if include_all => extract_codex_function_output(payload),
        _ => ContentSegment {
            role: String::new(),
            text: String::new(),
        },
    }
}

fn extract_codex_message(payload: &Value) -> ContentSegment {
    let role = json_string(payload, "role");
    let text = payload
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| codex_content_blocks_text(blocks))
        .unwrap_or_default();
    ContentSegment { role, text }
}

fn codex_content_blocks_text(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter_map(codex_content_block_text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn codex_content_block_text(block: &Value) -> Option<String> {
    let block_type = block.get("type").and_then(|value| value.as_str())?;
    match block_type {
        "input_text" | "output_text" | "summary_text" => {
            block.get("text")?.as_str().map(ToString::to_string)
        }
        _ => None,
    }
}

fn extract_codex_reasoning(payload: &Value) -> ContentSegment {
    let text = payload
        .get("summary")
        .and_then(Value::as_array)
        .map(|blocks| codex_content_blocks_text(blocks))
        .unwrap_or_default();
    ContentSegment {
        role: "reasoning".to_string(),
        text,
    }
}

fn extract_codex_function_call(payload: &Value) -> ContentSegment {
    let name = json_string(payload, "name");
    let arguments = payload
        .get("arguments")
        .map(value_to_text)
        .unwrap_or_default();
    ContentSegment {
        role: "tool".to_string(),
        text: format!("{name}: {arguments}"),
    }
}

fn extract_codex_function_output(payload: &Value) -> ContentSegment {
    let output = payload.get("output").map(value_to_text).unwrap_or_default();
    ContentSegment {
        role: "tool".to_string(),
        text: output,
    }
}

fn json_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .to_string()
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
