use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn upgrade_standalone_attachment_links(
    nodes: &mut [Value],
    attachment_uri_scheme: &str,
) {
    upgrade_links_at_depth(nodes, attachment_uri_scheme, 0);
}

fn upgrade_links_at_depth(nodes: &mut [Value], attachment_uri_scheme: &str, depth: usize) {
    // Best-effort transform: leave overly deep links unchanged.
    if depth >= crate::MAX_DEPTH {
        return;
    }
    for node in nodes.iter_mut() {
        if let Some(content) = node.get_mut("content").and_then(|c| c.as_array_mut()) {
            upgrade_links_at_depth(content, attachment_uri_scheme, depth + 1);
        }

        if let Some(upgraded) = paragraph_as_attachment_node(node, attachment_uri_scheme) {
            *node = upgraded;
        }
    }
}

fn paragraph_as_attachment_node(node: &Value, attachment_uri_scheme: &str) -> Option<Value> {
    if node.get("type").and_then(|t| t.as_str()) != Some("paragraph") {
        return None;
    }

    let content = node.get("content")?.as_array()?;
    if content.len() != 1 {
        return None;
    }

    let text_node = content.first()?;
    if text_node.get("type").and_then(|t| t.as_str()) != Some("text") {
        return None;
    }

    let text = text_node.get("text")?.as_str()?.trim();
    if text.is_empty() {
        return None;
    }

    let marks = text_node.get("marks")?.as_array()?;
    if marks.len() != 1 {
        return None;
    }

    let link_mark = marks.first()?;
    if link_mark.get("type").and_then(|t| t.as_str()) != Some("link") {
        return None;
    }

    let href = link_mark
        .get("attrs")
        .and_then(|a| a.get("href"))
        .and_then(|h| h.as_str())?;

    let attachment_id = attachment_id_from_uri(href, attachment_uri_scheme)?;

    if attachment_id.is_empty() || Uuid::parse_str(attachment_id).is_err() {
        return None;
    }

    Some(json!({
        "type": "attachment",
        "attrs": {
            "attachmentId": attachment_id,
            "filename": text,
        }
    }))
}

pub(crate) fn attachment_id_from_uri<'a>(
    uri: &'a str,
    attachment_uri_scheme: &str,
) -> Option<&'a str> {
    if attachment_uri_scheme.is_empty() {
        return None;
    }

    let attachment_id = uri
        .strip_prefix(attachment_uri_scheme)?
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim();

    if attachment_id.is_empty() {
        None
    } else {
        Some(attachment_id)
    }
}
