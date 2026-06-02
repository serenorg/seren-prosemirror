use serde_json::{Value, json};

use crate::ProseMirrorError;
use crate::attachment::upgrade_standalone_attachment_links;

pub(crate) fn prosemirror_to_org(
    doc: &Value,
    attachment_uri_scheme: &str,
) -> Result<String, ProseMirrorError> {
    let mut output = String::new();

    if doc.get("type").and_then(|t| t.as_str()) != Some("doc") {
        return Err(ProseMirrorError::InvalidStructure(
            "Root must be doc".to_string(),
        ));
    }

    if let Some(content) = doc.get("content").and_then(|c| c.as_array()) {
        for (i, node) in content.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }
            node_to_org(node, &mut output, 0, 0, attachment_uri_scheme)?;
        }
    }

    let trimmed_len = output.trim_end().len();
    output.truncate(trimmed_len);
    Ok(output)
}

fn node_to_org(
    node: &Value,
    output: &mut String,
    indent: usize,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    if depth >= crate::MAX_DEPTH {
        return Err(ProseMirrorError::InvalidStructure(
            "Document nesting exceeds the maximum depth".to_string(),
        ));
    }
    let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match node_type {
        "paragraph" => {
            let mut paragraph = String::new();
            inline_content_to_org(node, &mut paragraph, scheme)?;
            push_org_paragraph_text(output, &paragraph);
            output.push('\n');
        }
        "attachment" => {
            let attrs = node.get("attrs");
            let filename = attrs
                .and_then(|a| a.get("filename"))
                .and_then(|v| v.as_str())
                .unwrap_or("attachment");

            if let Some(attachment_id) = attrs
                .and_then(|a| a.get("attachmentId"))
                .and_then(|v| v.as_str())
            {
                output.push_str(&"  ".repeat(indent));
                output.push_str(&format!(
                    "[[{}][{}]]\n",
                    org_escape_target(&format!("{scheme}{attachment_id}")),
                    org_escape_description(filename)
                ));
            }
        }
        "heading" => {
            // Clamp to the 1..=6 heading range before allocation.
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_i64())
                .unwrap_or(1)
                .clamp(1, 6) as usize;
            output.push_str(&"*".repeat(level));
            output.push(' ');
            let mut inline = String::new();
            inline_content_to_org(node, &mut inline, scheme)?;
            output.push_str(&inline.replace(['\n', '\r'], " "));
            output.push('\n');
        }
        "blockquote" => {
            output.push_str("#+BEGIN_QUOTE\n");
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    node_to_org(child, output, 0, depth + 1, scheme)?;
                }
            }
            output.push_str("#+END_QUOTE\n");
        }
        "codeBlock" | "code_block" => {
            let language = node
                .get("attrs")
                .and_then(|a| a.get("language").or_else(|| a.get("params")))
                .and_then(|l| l.as_str())
                .unwrap_or("");
            let language = language.split(['\n', '\r']).next().unwrap_or("");
            output.push_str("#+BEGIN_SRC ");
            output.push_str(language);
            output.push('\n');
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(text) = child.get("text").and_then(|t| t.as_str()) {
                        push_org_source_text(output, text);
                    }
                }
            }
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("#+END_SRC\n");
        }
        "bulletList" | "bullet_list" => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    output.push_str(&"  ".repeat(indent));
                    output.push_str("- ");
                    org_list_item_to_org(child, output, indent, depth, scheme)?;
                }
            }
        }
        "orderedList" | "ordered_list" => {
            let start = node
                .get("attrs")
                .and_then(|a| a.get("order"))
                .and_then(|o| o.as_i64())
                .unwrap_or(1);
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for (i, child) in content.iter().enumerate() {
                    output.push_str(&"  ".repeat(indent));
                    output.push_str(&format!("{}. ", start.max(1).saturating_add(i as i64)));
                    org_list_item_to_org(child, output, indent, depth, scheme)?;
                }
            }
        }
        "taskList" | "task_list" => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    let checked = child
                        .get("attrs")
                        .and_then(|a| a.get("checked"))
                        .and_then(|c| c.as_bool())
                        .unwrap_or(false);
                    output.push_str(&"  ".repeat(indent));
                    output.push_str(if checked { "- [X] " } else { "- [ ] " });
                    org_list_item_to_org(child, output, indent, depth, scheme)?;
                }
            }
        }
        "horizontalRule" | "horizontal_rule" => {
            output.push_str("-----\n");
        }
        "table" => {
            if let Some(rows) = node.get("content").and_then(|c| c.as_array()) {
                let mut is_first_row = true;
                for row in rows {
                    org_table_row_to_org(row, output, is_first_row, scheme)?;
                    is_first_row = false;
                }
            }
        }
        "hardBreak" | "hard_break" => {
            output.push('\n');
        }
        "image" => {
            let mut image = String::new();
            if render_image_to_org(node, &mut image, scheme) {
                output.push_str(&image);
                output.push('\n');
            }
        }
        _ => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    node_to_org(child, output, indent, depth + 1, scheme)?;
                }
            }
        }
    }

    Ok(())
}

fn push_org_source_text(output: &mut String, text: &str) {
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if body.trim().eq_ignore_ascii_case("#+END_SRC") {
            output.push(',');
        }
        output.push_str(line);
    }
}

fn push_org_paragraph_text(output: &mut String, text: &str) {
    for line in text.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if org_line_starts_structure(body) {
            output.push(',');
        }
        output.push_str(line);
    }
}

fn org_line_starts_structure(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let upper = trimmed.to_uppercase();
    if matches!(
        upper.as_str(),
        "#+BEGIN_SRC" | "#+END_SRC" | "#+BEGIN_QUOTE" | "#+END_QUOTE"
    ) || upper.starts_with("#+BEGIN_SRC ")
    {
        return true;
    }
    if trimmed.starts_with("-----") || trimmed.strip_prefix("- ").is_some() {
        return true;
    }
    if parse_org_ordered_marker(trimmed).is_some() {
        return true;
    }
    if trimmed.starts_with('|') && trimmed.ends_with('|') {
        return true;
    }
    let star_count = trimmed.chars().take_while(|&c| c == '*').count();
    star_count > 0 && trimmed.as_bytes().get(star_count) == Some(&b' ')
}

fn org_list_item_to_org(
    node: &Value,
    output: &mut String,
    indent: usize,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
        for (i, child) in content.iter().enumerate() {
            if i == 0 {
                let mut paragraph = String::new();
                inline_content_to_org(child, &mut paragraph, scheme)?;
                push_org_paragraph_text(output, &paragraph);
                output.push('\n');
            } else {
                node_to_org(child, output, indent + 1, depth + 1, scheme)?;
            }
        }
    }
    Ok(())
}

fn inline_content_to_org(
    node: &Value,
    output: &mut String,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
        for child in content {
            text_node_to_org(child, output, scheme)?;
        }
    }
    Ok(())
}

fn text_node_to_org(
    node: &Value,
    output: &mut String,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");

    if node_type == "hardBreak" || node_type == "hard_break" {
        output.push('\n');
        return Ok(());
    }

    if node_type == "image" {
        render_image_to_org(node, output, scheme);
        return Ok(());
    }

    if node_type != "text" {
        return Ok(());
    }

    let text = node.get("text").and_then(|t| t.as_str()).unwrap_or("");
    let marks = node.get("marks").and_then(|m| m.as_array());

    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut is_link = false;
    let mut link_href = String::new();

    if let Some(marks) = marks {
        for mark in marks {
            let mark_type = mark.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match mark_type {
                "bold" | "strong" => {
                    prefix.push('*');
                    suffix.insert(0, '*');
                }
                "italic" | "em" => {
                    prefix.push('/');
                    suffix.insert(0, '/');
                }
                "code" => {
                    prefix.push('~');
                    suffix.insert(0, '~');
                }
                "strike" | "strikethrough" => {
                    prefix.push('+');
                    suffix.insert(0, '+');
                }
                "underline" => {
                    prefix.push('_');
                    suffix.insert(0, '_');
                }
                "link" => {
                    is_link = true;
                    if let Some(href) = mark
                        .get("attrs")
                        .and_then(|a| a.get("href"))
                        .and_then(|h| h.as_str())
                    {
                        link_href = href.to_string();
                    }
                }
                _ => {}
            }
        }
    }

    if is_link {
        output.push_str("[[");
        output.push_str(&org_escape_target(&link_href));
        output.push_str("][");
        output.push_str(&prefix);
        output.push_str(&org_escape_description(text));
        output.push_str(&suffix);
        output.push_str("]]");
    } else {
        output.push_str(&prefix);
        output.push_str(text);
        output.push_str(&suffix);
    }

    Ok(())
}

/// Render a table cell's text, descending through block wrappers (the standard
/// Tiptap `tableCell > paragraph > text` shape) down to inline nodes. Bounded
/// by `MAX_DEPTH` so deeply nested cells cannot recurse without limit.
fn org_cell_text(
    cell: &Value,
    output: &mut String,
    scheme: &str,
    depth: usize,
) -> Result<(), ProseMirrorError> {
    if depth >= crate::MAX_DEPTH {
        return Ok(());
    }
    if let Some(content) = cell.get("content").and_then(|c| c.as_array()) {
        for child in content {
            match child.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "text" | "image" | "hardBreak" | "hard_break" => {
                    text_node_to_org(child, output, scheme)?
                }
                _ => org_cell_text(child, output, scheme, depth + 1)?,
            }
        }
    }
    Ok(())
}

/// Neutralize Org link descriptions and collapse line endings.
fn org_escape_description(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            ']' => out.push(')'),
            '[' => out.push('('),
            '\n' | '\r' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

/// Neutralize an Org link target. Targets are delimited by `[`/`]`, so
/// percent-encode those and drop line endings to prevent breakout.
fn org_escape_target(target: &str) -> String {
    let mut out = String::with_capacity(target.len());
    for ch in target.chars() {
        match ch {
            '[' => out.push_str("%5B"),
            ']' => out.push_str("%5D"),
            '\n' | '\r' => {}
            _ => out.push(ch),
        }
    }
    out
}

fn render_image_to_org(node: &Value, output: &mut String, scheme: &str) -> bool {
    let attrs = node.get("attrs");
    let alt = attrs
        .and_then(|a| a.get("alt"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if let Some(attachment_id) = attrs
        .and_then(|a| a.get("attachmentId"))
        .and_then(|v| v.as_str())
    {
        output.push_str(&format!(
            "[[{}][{}]]",
            org_escape_target(&format!("{scheme}{attachment_id}")),
            org_escape_description(alt)
        ));
        true
    } else if let Some(src) = attrs.and_then(|a| a.get("src")).and_then(|v| v.as_str()) {
        if alt.is_empty() {
            output.push_str(&format!("[[{}]]", org_escape_target(src)));
        } else {
            output.push_str(&format!(
                "[[{}][{}]]",
                org_escape_target(src),
                org_escape_description(alt)
            ));
        }
        true
    } else {
        false
    }
}

fn org_table_row_to_org(
    row: &Value,
    output: &mut String,
    is_first_row: bool,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    if let Some(cells) = row.get("content").and_then(|c| c.as_array()) {
        output.push('|');
        for cell in cells {
            output.push(' ');
            let mut rendered = String::new();
            org_cell_text(cell, &mut rendered, scheme, 0)?;
            // A literal '|' would split the cell on re-import; escape it.
            output.push_str(&rendered.replace('|', "\\|"));
            output.push_str(" |");
        }
        output.push('\n');

        if is_first_row {
            output.push('|');
            for _ in cells {
                output.push_str("---+");
            }
            output.pop();
            output.push('|');
            output.push('\n');
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Org-mode to ProseMirror
// ---------------------------------------------------------------------------

#[derive(PartialEq, Clone, Copy)]
enum OrgListKind {
    Bullet,
    Ordered,
    Task,
}

struct OrgPendingList {
    kind: OrgListKind,
    items: Vec<Value>,
    order_start: i64,
}

/// Buffered table rows: each row is its cells (each cell being inline content
/// nodes) plus whether the row is a header row.
type OrgPendingTable = Vec<(Vec<Vec<Value>>, bool)>;

/// Emit any buffered list as a single list node. Consecutive Org list lines are
/// accumulated so they become one list with several items rather than a run of
/// single-item lists.
fn flush_org_list(pending: &mut Option<OrgPendingList>, out: &mut Vec<Value>) {
    let Some(list) = pending.take() else {
        return;
    };
    let node_type = match list.kind {
        OrgListKind::Bullet => "bulletList",
        OrgListKind::Ordered => "orderedList",
        OrgListKind::Task => "taskList",
    };
    let mut node = json!({ "type": node_type, "content": list.items });
    if list.kind == OrgListKind::Ordered && list.order_start != 1 {
        node["attrs"] = json!({ "order": list.order_start });
    }
    out.push(node);
}

fn push_org_list_item(
    pending: &mut Option<OrgPendingList>,
    out: &mut Vec<Value>,
    kind: OrgListKind,
    item: Value,
    order_start: i64,
) {
    match pending {
        Some(list) if list.kind == kind => list.items.push(item),
        _ => {
            flush_org_list(pending, out);
            *pending = Some(OrgPendingList {
                kind,
                items: vec![item],
                order_start,
            });
        }
    }
}

/// Emit any buffered table rows as a single table node, so a multi-row Org table
/// becomes one table rather than one table per row.
fn flush_org_table(pending: &mut Option<OrgPendingTable>, out: &mut Vec<Value>) {
    let Some(rows) = pending.take() else {
        return;
    };
    if rows.is_empty() {
        return;
    }
    let row_nodes: Vec<Value> = rows
        .into_iter()
        .map(|(cells, is_header)| {
            let cell_type = if is_header {
                "tableHeader"
            } else {
                "tableCell"
            };
            let cell_nodes: Vec<Value> = cells
                .into_iter()
                .map(|content| json!({ "type": cell_type, "content": content }))
                .collect();
            let mut row = json!({ "type": "tableRow", "content": cell_nodes });
            if is_header {
                row["attrs"] = json!({ "header": true });
            }
            row
        })
        .collect();
    out.push(json!({ "type": "table", "content": row_nodes }));
}

/// A `|---+---|` style rule line separating header rows from body rows.
fn is_org_table_separator(trimmed: &str) -> bool {
    let inner: String = trimmed
        .trim_matches('|')
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    !inner.is_empty() && inner.bytes().all(|b| matches!(b, b'-' | b'+'))
}

/// Split a `| a | b |` row into per-cell inline content, honoring `\|` escapes.
fn split_org_row(trimmed: &str) -> Vec<Vec<Value>> {
    let inner = trimmed.trim_matches('|');
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cur.push('|');
            }
            '|' => {
                cells.push(parse_org_inline(cur.trim()));
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    cells.push(parse_org_inline(cur.trim()));
    cells
}

/// Parse a leading ordered-list marker of any width (`1. `, `10. `, `3) `),
/// returning the start number and the remaining text.
fn parse_org_ordered_marker(trimmed: &str) -> Option<(i64, &str)> {
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let (number, rest) = trimmed.split_at(digits);
    let text = rest
        .strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))?;
    let number = number.parse::<i64>().ok()?;
    Some((number, text))
}

pub(crate) fn org_to_prosemirror(
    org: &str,
    attachment_uri_scheme: &str,
) -> Result<Value, ProseMirrorError> {
    let mut doc_content: Vec<Value> = Vec::new();
    let mut in_src_block = false;
    let mut src_language = String::new();
    let mut src_content = String::new();
    let mut in_quote_block = false;
    let mut quote_content: Vec<Value> = Vec::new();
    let mut pending_list: Option<OrgPendingList> = None;
    let mut pending_table: Option<OrgPendingTable> = None;

    for line in org.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with("#+BEGIN_SRC") {
            flush_org_list(&mut pending_list, &mut doc_content);
            flush_org_table(&mut pending_table, &mut doc_content);
            in_src_block = true;
            src_language = trimmed
                .get("#+BEGIN_SRC".len()..)
                .unwrap_or("")
                .trim()
                .to_string();
            src_content.clear();
            continue;
        }
        if in_src_block {
            if upper == "#+END_SRC" {
                in_src_block = false;
                let mut code_block = json!({
                    "type": "codeBlock",
                    "content": [{
                        "type": "text",
                        "text": src_content.trim_end()
                    }]
                });
                if !src_language.is_empty() {
                    code_block["attrs"] = json!({ "language": src_language });
                }
                doc_content.push(code_block);
                continue;
            }
            if !src_content.is_empty() {
                src_content.push('\n');
            }
            if let Some(rest) = line.strip_prefix(',')
                && rest.trim().eq_ignore_ascii_case("#+END_SRC")
            {
                src_content.push_str(rest);
            } else {
                src_content.push_str(line);
            }
            continue;
        }

        if upper == "#+BEGIN_QUOTE" {
            flush_org_list(&mut pending_list, &mut doc_content);
            flush_org_table(&mut pending_table, &mut doc_content);
            in_quote_block = true;
            quote_content.clear();
            continue;
        }
        if upper == "#+END_QUOTE" {
            in_quote_block = false;
            doc_content.push(json!({
                "type": "blockquote",
                "content": quote_content.clone()
            }));
            continue;
        }
        if in_quote_block {
            let line = org_unescape_generated_line(line).unwrap_or(line);
            quote_content.push(json!({
                "type": "paragraph",
                "content": parse_org_inline(line)
            }));
            continue;
        }

        if trimmed.starts_with('*') {
            let star_count = trimmed.chars().take_while(|&c| c == '*').count();
            if star_count > 0 && trimmed.as_bytes().get(star_count) == Some(&b' ') {
                flush_org_list(&mut pending_list, &mut doc_content);
                flush_org_table(&mut pending_table, &mut doc_content);
                let level = star_count.min(6);
                let text = trimmed[star_count..].trim();
                doc_content.push(json!({
                    "type": "heading",
                    "attrs": { "level": level },
                    "content": parse_org_inline(text)
                }));
                continue;
            }
        }

        if trimmed.starts_with("-----") {
            flush_org_list(&mut pending_list, &mut doc_content);
            flush_org_table(&mut pending_table, &mut doc_content);
            doc_content.push(json!({ "type": "horizontalRule" }));
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- ") {
            flush_org_table(&mut pending_table, &mut doc_content);
            if let Some(text) = rest.strip_prefix("[ ] ") {
                let item = json!({
                    "type": "taskItem",
                    "attrs": { "checked": false },
                    "content": [{ "type": "paragraph", "content": parse_org_inline(text) }]
                });
                push_org_list_item(
                    &mut pending_list,
                    &mut doc_content,
                    OrgListKind::Task,
                    item,
                    1,
                );
            } else if let Some(text) = rest
                .strip_prefix("[X] ")
                .or_else(|| rest.strip_prefix("[x] "))
            {
                let item = json!({
                    "type": "taskItem",
                    "attrs": { "checked": true },
                    "content": [{ "type": "paragraph", "content": parse_org_inline(text) }]
                });
                push_org_list_item(
                    &mut pending_list,
                    &mut doc_content,
                    OrgListKind::Task,
                    item,
                    1,
                );
            } else {
                let item = json!({
                    "type": "listItem",
                    "content": [{ "type": "paragraph", "content": parse_org_inline(rest) }]
                });
                push_org_list_item(
                    &mut pending_list,
                    &mut doc_content,
                    OrgListKind::Bullet,
                    item,
                    1,
                );
            }
            continue;
        }

        if let Some((number, text)) = parse_org_ordered_marker(trimmed) {
            flush_org_table(&mut pending_table, &mut doc_content);
            let item = json!({
                "type": "listItem",
                "content": [{ "type": "paragraph", "content": parse_org_inline(text) }]
            });
            push_org_list_item(
                &mut pending_list,
                &mut doc_content,
                OrgListKind::Ordered,
                item,
                number,
            );
            continue;
        }

        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            flush_org_list(&mut pending_list, &mut doc_content);
            if is_org_table_separator(trimmed) {
                if let Some(rows) = pending_table.as_mut() {
                    for row in rows.iter_mut() {
                        row.1 = true;
                    }
                }
                continue;
            }
            let cells = split_org_row(trimmed);
            pending_table
                .get_or_insert_with(Vec::new)
                .push((cells, false));
            continue;
        }

        flush_org_list(&mut pending_list, &mut doc_content);
        flush_org_table(&mut pending_table, &mut doc_content);
        if !trimmed.is_empty() {
            let paragraph = org_unescape_generated_line(trimmed).unwrap_or(trimmed);
            doc_content.push(json!({
                "type": "paragraph",
                "content": parse_org_inline(paragraph)
            }));
        }
    }

    flush_org_list(&mut pending_list, &mut doc_content);
    flush_org_table(&mut pending_table, &mut doc_content);

    if doc_content.is_empty() {
        doc_content.push(json!({
            "type": "paragraph",
            "content": []
        }));
    }

    upgrade_standalone_attachment_links(&mut doc_content, attachment_uri_scheme);

    Ok(json!({
        "type": "doc",
        "content": doc_content
    }))
}

fn org_unescape_generated_line(line: &str) -> Option<&str> {
    let rest = line.strip_prefix(',')?;
    org_line_starts_structure(rest).then_some(rest.trim())
}

fn push_org_run(text: &mut String, marks: &[Value], out: &mut Vec<Value>) {
    if text.is_empty() {
        return;
    }
    let mut node = json!({ "type": "text", "text": std::mem::take(text) });
    if !marks.is_empty() {
        node["marks"] = json!(marks);
    }
    out.push(node);
}

fn mark_active(marks: &[Value], mark_type: &str) -> bool {
    marks
        .iter()
        .any(|m| m.get("type").and_then(|t| t.as_str()) == Some(mark_type))
}

fn org_emphasis_mark(c: char) -> Option<&'static str> {
    match c {
        '*' => Some("bold"),
        '/' => Some("italic"),
        '_' => Some("underline"),
        '+' => Some("strike"),
        _ => None,
    }
}

/// Org emphasis may only open at line start or after whitespace/`-({'"` and must
/// be followed by a non-whitespace character, so markers embedded in identifiers
/// (`some_var_name`) or arithmetic (`2 * 3`) are not mistaken for formatting.
fn org_emphasis_can_open(prev: Option<char>, next: Option<char>) -> bool {
    let pre_ok = match prev {
        None => true,
        Some(c) => c.is_whitespace() || matches!(c, '-' | '(' | '{' | '\'' | '"'),
    };
    let post_ok = next.is_some_and(|c| !c.is_whitespace());
    pre_ok && post_ok
}

fn org_emphasis_can_close(prev: Option<char>) -> bool {
    prev.is_some_and(|c| !c.is_whitespace())
}

fn parse_org_inline(text: &str) -> Vec<Value> {
    let chars: Vec<char> = text.chars().collect();
    let mut result: Vec<Value> = Vec::new();
    let mut current_text = String::new();
    let mut marks: Vec<Value> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Org link: [[target]] or [[target][description]].
        if c == '[' && chars.get(i + 1) == Some(&'[') {
            push_org_run(&mut current_text, &marks, &mut result);
            i += 2;
            let mut target = String::new();
            let mut description = String::new();
            let mut in_target = true;
            while i < chars.len() {
                let lc = chars[i];
                if lc == ']' && chars.get(i + 1) == Some(&'[') {
                    in_target = false;
                    i += 2;
                    continue;
                }
                if lc == ']' && chars.get(i + 1) == Some(&']') {
                    i += 2;
                    break;
                }
                if in_target {
                    target.push(lc);
                } else {
                    description.push(lc);
                }
                i += 1;
            }
            let display = if description.is_empty() {
                target.clone()
            } else {
                description
            };
            result.push(json!({
                "type": "text",
                "text": display,
                "marks": [{ "type": "link", "attrs": { "href": target } }]
            }));
            continue;
        }

        // Verbatim/code span `~...~`: the contents are literal, so no inner
        // marker is interpreted.
        if c == '~' {
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1).copied();
            if org_emphasis_can_open(prev, next)
                && let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == '~')
                && close > i + 1
            {
                push_org_run(&mut current_text, &marks, &mut result);
                let literal: String = chars[i + 1..close].iter().collect();
                let mut code_marks = marks.clone();
                code_marks.push(json!({ "type": "code" }));
                result.push(json!({ "type": "text", "text": literal, "marks": code_marks }));
                i = close + 1;
                continue;
            }
            current_text.push('~');
            i += 1;
            continue;
        }

        // Emphasis: *bold* /italic/ _underline_ +strike+.
        if let Some(mark_type) = org_emphasis_mark(c) {
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1).copied();
            if mark_active(&marks, mark_type) {
                if org_emphasis_can_close(prev) {
                    push_org_run(&mut current_text, &marks, &mut result);
                    marks.retain(|m| m.get("type").and_then(|t| t.as_str()) != Some(mark_type));
                    i += 1;
                    continue;
                }
            } else if org_emphasis_can_open(prev, next) {
                push_org_run(&mut current_text, &marks, &mut result);
                marks.push(json!({ "type": mark_type }));
                i += 1;
                continue;
            }
            current_text.push(c);
            i += 1;
            continue;
        }

        current_text.push(c);
        i += 1;
    }

    push_org_run(&mut current_text, &marks, &mut result);
    result
}
