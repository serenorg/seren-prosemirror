use serde_json::Value;

use crate::ProseMirrorError;

/// Supported node types in the ProseMirror schema.
///
/// Accepts both `snake_case` (ProseMirror reference schema naming) and
/// `camelCase` (Tiptap extension naming) so UI + MCP/Markdown workflows can
/// interoperate cleanly.
const SUPPORTED_NODES: &[&str] = &[
    "doc",
    "paragraph",
    "text",
    "heading",
    "blockquote",
    "attachment",
    "image",
    "codeBlock",
    "code_block",
    "bulletList",
    "bullet_list",
    "orderedList",
    "ordered_list",
    "listItem",
    "list_item",
    "taskList",
    "task_list",
    "taskItem",
    "task_item",
    "horizontalRule",
    "horizontal_rule",
    "hardBreak",
    "hard_break",
    "table",
    "tableRow",
    "table_row",
    "tableCell",
    "table_cell",
    "tableHeader",
    "table_header",
];

/// Supported mark types (supports both Tiptap and ProseMirror naming).
const SUPPORTED_MARKS: &[&str] = &[
    "bold",
    "strong",
    "italic",
    "em",
    "code",
    "strike",
    "strikethrough",
    "underline",
    "link",
];

/// Validate that a ProseMirror document uses only supported nodes and marks,
/// within a bounded nesting depth.
pub fn validate_document(doc: &Value) -> Result<(), ProseMirrorError> {
    if doc.get("type").and_then(|t| t.as_str()) != Some("doc") {
        return Err(ProseMirrorError::InvalidStructure(
            "Root must be doc".to_string(),
        ));
    }

    validate_node(doc, 0)
}

fn validate_node(node: &Value, depth: usize) -> Result<(), ProseMirrorError> {
    if depth >= crate::MAX_DEPTH {
        return Err(ProseMirrorError::InvalidStructure(
            "Document nesting exceeds the maximum depth".to_string(),
        ));
    }

    if !node.is_object() {
        return Err(ProseMirrorError::InvalidStructure(
            "Node must be an object".to_string(),
        ));
    }

    let node_type = node
        .get("type")
        .and_then(|t| t.as_str())
        .ok_or_else(|| ProseMirrorError::InvalidStructure("Node missing type".to_string()))?;

    if !SUPPORTED_NODES.contains(&node_type) {
        return Err(ProseMirrorError::UnsupportedNode(node_type.to_string()));
    }

    validate_node_attrs(node, node_type)?;

    if let Some(marks_value) = node.get("marks") {
        let marks = marks_value.as_array().ok_or_else(|| {
            ProseMirrorError::InvalidStructure("Node marks must be an array".to_string())
        })?;

        for mark in marks {
            if !mark.is_object() {
                return Err(ProseMirrorError::InvalidStructure(
                    "Mark must be an object".to_string(),
                ));
            }

            let mark_type = mark.get("type").and_then(|t| t.as_str()).ok_or_else(|| {
                ProseMirrorError::InvalidStructure("Mark missing type".to_string())
            })?;

            if !SUPPORTED_MARKS.contains(&mark_type) {
                return Err(ProseMirrorError::UnsupportedMark(mark_type.to_string()));
            }

            validate_mark_attrs(mark, mark_type)?;
        }
    }

    if let Some(content_value) = node.get("content") {
        let content = content_value.as_array().ok_or_else(|| {
            ProseMirrorError::InvalidStructure("Node content must be an array".to_string())
        })?;

        for child in content {
            validate_node(child, depth + 1)?;
        }
    }

    Ok(())
}

fn validate_node_attrs(node: &Value, node_type: &str) -> Result<(), ProseMirrorError> {
    if let Some(attrs) = node.get("attrs")
        && !attrs.is_object()
        && !attrs.is_null()
    {
        return Err(ProseMirrorError::InvalidStructure(
            "Node attrs must be an object".to_string(),
        ));
    }

    match node_type {
        "text" if node.get("text").and_then(|t| t.as_str()).is_none() => {
            return Err(ProseMirrorError::InvalidStructure(
                "Text node missing text".to_string(),
            ));
        }
        "heading" => {
            if let Some(level) = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_i64())
                && !(1..=6).contains(&level)
            {
                return Err(ProseMirrorError::InvalidStructure(
                    "Heading level must be between 1 and 6".to_string(),
                ));
            }
        }
        "orderedList" | "ordered_list" => {
            if let Some(order) = node
                .get("attrs")
                .and_then(|a| a.get("order"))
                .and_then(|o| o.as_i64())
                && order < 1
            {
                return Err(ProseMirrorError::InvalidStructure(
                    "Ordered list order must be positive".to_string(),
                ));
            }
        }
        "taskItem" | "task_item" => {
            if let Some(checked) = node.get("attrs").and_then(|a| a.get("checked"))
                && !checked.is_boolean()
            {
                return Err(ProseMirrorError::InvalidStructure(
                    "Task item checked attr must be a boolean".to_string(),
                ));
            }
        }
        "attachment"
            if node
                .get("attrs")
                .and_then(|a| a.get("attachmentId"))
                .and_then(|id| id.as_str())
                .filter(|id| !id.trim().is_empty())
                .is_none() =>
        {
            return Err(ProseMirrorError::InvalidStructure(
                "Attachment node missing attachmentId".to_string(),
            ));
        }
        _ => {}
    }

    Ok(())
}

fn validate_mark_attrs(mark: &Value, mark_type: &str) -> Result<(), ProseMirrorError> {
    if let Some(attrs) = mark.get("attrs")
        && !attrs.is_object()
        && !attrs.is_null()
    {
        return Err(ProseMirrorError::InvalidStructure(
            "Mark attrs must be an object".to_string(),
        ));
    }

    if mark_type == "link"
        && mark
            .get("attrs")
            .and_then(|a| a.get("href"))
            .and_then(|href| href.as_str())
            .filter(|href| !href.trim().is_empty())
            .is_none()
    {
        return Err(ProseMirrorError::InvalidStructure(
            "Link mark missing href".to_string(),
        ));
    }

    Ok(())
}
