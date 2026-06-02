use serde_json::Value;

/// Extract plain text from a ProseMirror document for search indexing.
pub fn extract_plain_text(doc: &Value) -> String {
    let mut text = String::new();
    extract_text_recursive(doc, &mut text, 0);
    text.trim().to_string()
}

fn extract_text_recursive(node: &Value, output: &mut String, depth: usize) {
    if depth >= crate::MAX_DEPTH {
        return;
    }
    let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match node_type {
        "text" => {
            if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
                output.push_str(text);
            }
        }
        "paragraph" | "heading" | "blockquote" | "codeBlock" | "code_block" | "listItem"
        | "list_item" | "taskItem" | "task_item" | "tableCell" | "table_cell" | "tableHeader"
        | "table_header" => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    extract_text_recursive(child, output, depth + 1);
                }
            }
            output.push('\n');
        }
        "hardBreak" | "hard_break" => {
            output.push('\n');
        }
        "image" => {
            if let Some(alt) = node
                .get("attrs")
                .and_then(|a| a.get("alt"))
                .and_then(|v| v.as_str())
                .filter(|alt| !alt.is_empty())
            {
                output.push_str(alt);
                output.push('\n');
            }
        }
        "attachment" => {
            if let Some(filename) = node
                .get("attrs")
                .and_then(|a| a.get("filename"))
                .and_then(|v| v.as_str())
                .filter(|v| !v.trim().is_empty())
            {
                output.push_str(filename);
                output.push('\n');
            }
        }
        _ => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    extract_text_recursive(child, output, depth + 1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn nested(depth: usize, leaf: &str) -> Value {
        let mut node = json!({ "type": "text", "text": leaf });
        for _ in 0..depth {
            node = json!({ "type": "blockquote", "content": [node] });
        }
        node
    }

    #[test]
    fn extracts_shallow_text() {
        assert!(extract_plain_text(&nested(5, "shallow")).contains("shallow"));
    }

    #[test]
    fn stops_recursing_past_a_depth_limit() {
        assert!(!extract_plain_text(&nested(200, "deep")).contains("deep"));
    }
}
