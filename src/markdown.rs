use std::mem;

use serde_json::{Value, json};

use crate::ProseMirrorError;
use crate::attachment::{attachment_id_from_uri, upgrade_standalone_attachment_links};

type StackFrame = (&'static str, Vec<Value>, Option<Value>);
const MAX_LIST_CONTINUATION_PADDING: usize = crate::MAX_DEPTH * 64;

#[derive(Default)]
struct FrameMeta {
    loose_before: bool,
    last_child_end: Option<usize>,
}

fn push_frame(
    stack: &mut Vec<StackFrame>,
    stack_meta: &mut Vec<FrameMeta>,
    node_type: &'static str,
    attrs: Option<Value>,
    loose_before: bool,
) {
    stack.push((node_type, Vec::new(), attrs));
    stack_meta.push(FrameMeta {
        loose_before,
        last_child_end: None,
    });
}

fn remove_last_mark_type(marks: &mut Vec<Value>, mark_type: &str) {
    if let Some(index) = marks
        .iter()
        .rposition(|m| m.get("type").and_then(|t| t.as_str()) == Some(mark_type))
    {
        marks.remove(index);
    }
}

/// If the top of the stack is an image, append `suffix` to its `alt` in place
/// and return `true`. Avoids the O(N^2) `format!(prev + suffix)` rebuild.
fn try_append_to_image_alt(stack: &mut [StackFrame], suffix: &str) -> bool {
    let Some((node_type, _, attrs)) = stack.last_mut() else {
        return false;
    };
    if *node_type != "image" {
        return false;
    }
    let Some(obj) = attrs.as_mut().and_then(|a| a.as_object_mut()) else {
        return false;
    };
    match obj.get_mut("alt") {
        Some(Value::String(s)) => s.push_str(suffix),
        _ => {
            obj.insert("alt".to_string(), Value::String(suffix.to_string()));
        }
    }
    true
}

fn set_list_item_continuation_indent(stack: &mut [StackFrame], indent: usize) {
    let Some(index) = direct_list_item_index(stack) else {
        return;
    };
    let attrs = stack[index].2.get_or_insert_with(|| json!({}));
    let indent = attrs
        .get("continuationIndent")
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .map_or(indent, |previous| previous.min(indent));
    attrs["continuationIndent"] = json!(indent);
}

fn set_list_item_loose_before_inline(stack: &mut [StackFrame]) {
    let Some(index) = direct_list_item_index(stack) else {
        return;
    };
    let attrs = stack[index].2.get_or_insert_with(|| json!({}));
    attrs["looseBeforeInline"] = json!(true);
}

fn direct_list_item_index(stack: &[StackFrame]) -> Option<usize> {
    let len = stack.len();
    if len == 0 {
        return None;
    }
    let last_type = stack[len - 1].0;
    if last_type == "listItem" || last_type == "taskItem" {
        return Some(len - 1);
    }
    if len >= 2 && last_type == "paragraph" {
        let parent_type = stack[len - 2].0;
        if parent_type == "listItem" || parent_type == "taskItem" {
            return Some(len - 2);
        }
    }
    None
}

fn leading_indent_before(markdown: &str, byte_idx: usize) -> usize {
    let Some(prefix) = markdown.get(..byte_idx) else {
        return 0;
    };
    prefix
        .chars()
        .rev()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum()
}

fn raw_multiline_code_span(markdown: &str, range: &std::ops::Range<usize>) -> Option<String> {
    let raw = markdown.get(range.clone())?;
    let delimiter_len = raw.bytes().take_while(|b| *b == b'`').count();
    if delimiter_len == 0 || raw.len() < delimiter_len * 2 {
        return None;
    }
    let trailing = &raw.as_bytes()[raw.len() - delimiter_len..];
    if !trailing.iter().all(|b| *b == b'`') {
        return None;
    }
    let inner = &raw[delimiter_len..raw.len() - delimiter_len];
    if !inner.contains('\n') && !inner.contains('\r') {
        return None;
    }

    let inner = inner.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = inner.split('\n');
    let mut output = lines.next()?.to_string();
    for line in lines {
        output.push('\n');
        output.push_str(line.trim_start_matches([' ', '\t']));
    }
    Some(output)
}

fn raw_multiline_code_span_continuation_indent(
    markdown: &str,
    range: &std::ops::Range<usize>,
) -> Option<usize> {
    let raw = markdown.get(range.clone())?;
    let delimiter_len = raw.bytes().take_while(|b| *b == b'`').count();
    if delimiter_len == 0 || raw.len() < delimiter_len * 2 {
        return None;
    }
    let inner = &raw[delimiter_len..raw.len() - delimiter_len];
    if !inner.contains('\n') && !inner.contains('\r') {
        return None;
    }
    let inner = inner.replace("\r\n", "\n").replace('\r', "\n");
    inner
        .split('\n')
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|ch| *ch == ' ' || *ch == '\t')
                .map(|ch| if ch == '\t' { 4 } else { 1 })
                .sum()
        })
        .min()
}

fn bullet_list_attrs(markdown: &str, range: &std::ops::Range<usize>) -> Option<Value> {
    let marker = markdown
        .get(range.clone())?
        .chars()
        .find(|ch| !ch.is_whitespace())?;
    if matches!(marker, '*' | '+') {
        Some(json!({ "marker": marker.to_string() }))
    } else {
        None
    }
}

pub(crate) fn markdown_to_prosemirror(
    markdown: &str,
    attachment_uri_scheme: &str,
) -> Result<Value, ProseMirrorError> {
    use pulldown_cmark::{
        Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
    };

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);

    let mut doc_content: Vec<Value> = Vec::new();
    let mut stack: Vec<StackFrame> = Vec::new();
    let mut stack_meta: Vec<FrameMeta> = Vec::new();
    let mut current_marks: Vec<Value> = Vec::new();
    let mut current_text = String::new();

    let flush_text = |text: &mut String, marks: &[Value], content: &mut Vec<Value>| {
        if !text.is_empty() {
            let mut text_node = json!({
                "type": "text",
                "text": Value::String(mem::take(text)),
            });
            if !marks.is_empty() {
                text_node["marks"] = json!(marks);
            }
            content.push(text_node);
        }
    };

    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Some((_, content, _)) = stack.last_mut() {
                    flush_text(&mut current_text, &current_marks, content);
                }
                let loose_before = stack_meta
                    .last()
                    .and_then(|meta| meta.last_child_end)
                    .is_some_and(|previous_end| {
                        has_blank_between(markdown, previous_end, range.start)
                    });

                match tag {
                    Tag::Paragraph => {
                        push_frame(&mut stack, &mut stack_meta, "paragraph", None, loose_before);
                    }
                    Tag::Heading { level, .. } => {
                        let level_num = match level {
                            HeadingLevel::H1 => 1,
                            HeadingLevel::H2 => 2,
                            HeadingLevel::H3 => 3,
                            HeadingLevel::H4 => 4,
                            HeadingLevel::H5 => 5,
                            HeadingLevel::H6 => 6,
                        };
                        push_frame(
                            &mut stack,
                            &mut stack_meta,
                            "heading",
                            Some(json!({ "level": level_num })),
                            loose_before,
                        );
                    }
                    Tag::BlockQuote(_) => {
                        push_frame(
                            &mut stack,
                            &mut stack_meta,
                            "blockquote",
                            None,
                            loose_before,
                        );
                    }
                    Tag::CodeBlock(kind) => {
                        let language = match kind {
                            CodeBlockKind::Fenced(lang) => {
                                let lang_str = lang.as_ref();
                                if lang_str.is_empty() {
                                    None
                                } else {
                                    Some(lang_str.to_string())
                                }
                            }
                            CodeBlockKind::Indented => None,
                        };
                        let attrs = language.map(|l| json!({ "language": l }));
                        push_frame(
                            &mut stack,
                            &mut stack_meta,
                            "codeBlock",
                            attrs,
                            loose_before,
                        );
                    }
                    Tag::List(first_item) => {
                        if first_item.is_some() {
                            let attrs = first_item.map(|n| json!({ "order": n }));
                            push_frame(
                                &mut stack,
                                &mut stack_meta,
                                "orderedList",
                                attrs,
                                loose_before,
                            );
                        } else {
                            let attrs = bullet_list_attrs(markdown, &range);
                            push_frame(
                                &mut stack,
                                &mut stack_meta,
                                "bulletList",
                                attrs,
                                loose_before,
                            );
                        }
                    }
                    Tag::Item => {
                        push_frame(&mut stack, &mut stack_meta, "listItem", None, loose_before);
                    }
                    Tag::Table(alignments) => {
                        let attrs = if alignments.iter().any(|a| *a != Alignment::None) {
                            let values: Vec<&str> = alignments
                                .iter()
                                .map(|alignment| match alignment {
                                    Alignment::None => "none",
                                    Alignment::Left => "left",
                                    Alignment::Center => "center",
                                    Alignment::Right => "right",
                                })
                                .collect();
                            Some(json!({ "alignments": values }))
                        } else {
                            None
                        };
                        push_frame(&mut stack, &mut stack_meta, "table", attrs, loose_before);
                    }
                    Tag::TableHead => {
                        push_frame(
                            &mut stack,
                            &mut stack_meta,
                            "tableRow",
                            Some(json!({ "header": true })),
                            loose_before,
                        );
                    }
                    Tag::TableRow => {
                        push_frame(&mut stack, &mut stack_meta, "tableRow", None, loose_before);
                    }
                    Tag::TableCell => {
                        let is_header = stack.iter().rev().any(|(t, _, attrs)| {
                            *t == "tableRow"
                                && attrs.as_ref().is_some_and(|a| a.get("header").is_some())
                        });
                        let cell_type = if is_header {
                            "tableHeader"
                        } else {
                            "tableCell"
                        };
                        push_frame(&mut stack, &mut stack_meta, cell_type, None, loose_before);
                    }
                    Tag::Emphasis => {
                        current_marks.push(json!({ "type": "italic" }));
                    }
                    Tag::Strong => {
                        current_marks.push(json!({ "type": "bold" }));
                    }
                    Tag::Strikethrough => {
                        current_marks.push(json!({ "type": "strike" }));
                    }
                    Tag::Image {
                        dest_url, title, ..
                    } => {
                        let dest = dest_url.as_ref();

                        let mut attrs = json!({ "alt": "" });

                        if let Some(attachment_id) =
                            attachment_id_from_uri(dest, attachment_uri_scheme)
                        {
                            attrs["attachmentId"] = json!(attachment_id);
                            attrs["src"] = Value::Null;
                        } else {
                            attrs["src"] = json!(dest);
                        }

                        if !title.is_empty() {
                            attrs["title"] = json!(title.as_ref());
                        }

                        push_frame(
                            &mut stack,
                            &mut stack_meta,
                            "image",
                            Some(attrs),
                            loose_before,
                        );
                    }
                    Tag::Link {
                        dest_url, title, ..
                    } => {
                        let mut attrs = json!({ "href": dest_url.as_ref() });
                        if !title.is_empty() {
                            attrs["title"] = json!(title.as_ref());
                        }
                        current_marks.push(json!({ "type": "link", "attrs": attrs }));
                    }
                    _ => {}
                }
            }
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::CodeBlock
                | TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::Image
                | TagEnd::Table
                | TagEnd::TableHead
                | TagEnd::TableRow
                | TagEnd::TableCell => {
                    if let Some((node_type, mut content, attrs)) = stack.pop() {
                        let meta = stack_meta.pop().unwrap_or_default();
                        flush_text(&mut current_text, &current_marks, &mut content);

                        // ProseMirror list items contain block-level nodes.
                        // Tight Markdown list items can arrive as bare inline nodes.
                        if (node_type == "listItem" || node_type == "taskItem")
                            && !content.is_empty()
                            && content.iter().all(|n| {
                                let t = n.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                t == "text" || t == "hardBreak" || t == "image"
                            })
                        {
                            content = vec![json!({
                                "type": "paragraph",
                                "content": content
                            })];
                        }

                        let mut final_attrs = if node_type == "tableRow" { None } else { attrs };
                        if meta.loose_before {
                            let mut attrs = final_attrs.unwrap_or_else(|| json!({}));
                            attrs["looseBefore"] = json!(true);
                            final_attrs = Some(attrs);
                        }
                        if item_has_trailing_blank(markdown, &range) {
                            let mut attrs = final_attrs.unwrap_or_else(|| json!({}));
                            attrs["looseAfter"] = json!(true);
                            final_attrs = Some(attrs);
                        }
                        if let TagEnd::List(true) = tag_end {
                            let mut attrs = final_attrs.unwrap_or_else(|| json!({}));
                            attrs["loose"] = json!(true);
                            final_attrs = Some(attrs);
                        }

                        let is_image = node_type == "image";
                        let mut node = json!({ "type": node_type });
                        if !content.is_empty() {
                            node["content"] = Value::Array(content);
                        }
                        if let Some(a) = final_attrs {
                            node["attrs"] = a;
                        }
                        if is_image && !current_marks.is_empty() {
                            node["marks"] = Value::Array(current_marks.clone());
                        }

                        if let Some((_, parent_content, _)) = stack.last_mut() {
                            parent_content.push(node);
                            if let Some(parent_meta) = stack_meta.last_mut() {
                                parent_meta.last_child_end = Some(range.end);
                            }
                        } else {
                            doc_content.push(node);
                        }
                    }
                }
                TagEnd::Emphasis => {
                    if let Some((_, content, _)) = stack.last_mut() {
                        flush_text(&mut current_text, &current_marks, content);
                    }
                    remove_last_mark_type(&mut current_marks, "italic");
                }
                TagEnd::Strong => {
                    if let Some((_, content, _)) = stack.last_mut() {
                        flush_text(&mut current_text, &current_marks, content);
                    }
                    remove_last_mark_type(&mut current_marks, "bold");
                }
                TagEnd::Strikethrough => {
                    if let Some((_, content, _)) = stack.last_mut() {
                        flush_text(&mut current_text, &current_marks, content);
                    }
                    remove_last_mark_type(&mut current_marks, "strike");
                }
                TagEnd::Link => {
                    if let Some((_, content, _)) = stack.last_mut() {
                        flush_text(&mut current_text, &current_marks, content);
                    }
                    remove_last_mark_type(&mut current_marks, "link");
                }
                _ => {}
            },
            Event::Text(text) => {
                if stack.last().is_some_and(|(node_type, _, _)| {
                    *node_type == "listItem" || *node_type == "taskItem"
                }) && stack_meta
                    .last()
                    .and_then(|meta| meta.last_child_end)
                    .is_some_and(|previous_end| {
                        has_blank_between(markdown, previous_end, range.start)
                    })
                {
                    set_list_item_loose_before_inline(&mut stack);
                }
                if current_text.ends_with('\n') {
                    set_list_item_continuation_indent(
                        &mut stack,
                        leading_indent_before(markdown, range.start),
                    );
                }
                if try_append_to_image_alt(&mut stack, text.as_ref()) {
                    continue;
                }
                current_text.push_str(&text);
            }
            Event::Code(code) => {
                let code_text = raw_multiline_code_span(markdown, &range)
                    .unwrap_or_else(|| code.as_ref().to_string());
                if stack.last().is_some_and(|(node_type, _, _)| {
                    *node_type == "listItem" || *node_type == "taskItem"
                }) && stack_meta
                    .last()
                    .and_then(|meta| meta.last_child_end)
                    .is_some_and(|previous_end| {
                        has_blank_between(markdown, previous_end, range.start)
                    })
                {
                    set_list_item_loose_before_inline(&mut stack);
                }
                if current_text.ends_with('\n') {
                    let source_indent = leading_indent_before(markdown, range.start);
                    let continuation_indent =
                        raw_multiline_code_span_continuation_indent(markdown, &range)
                            .unwrap_or(source_indent);
                    set_list_item_continuation_indent(&mut stack, continuation_indent);
                    if source_indent > continuation_indent {
                        current_text.push_str(&" ".repeat(source_indent - continuation_indent));
                    }
                }
                if try_append_to_image_alt(&mut stack, &code_text) {
                    continue;
                }
                if let Some((_, content, _)) = stack.last_mut() {
                    flush_text(&mut current_text, &current_marks, content);
                    let mut marks = current_marks.clone();
                    marks.push(json!({ "type": "code" }));
                    content.push(json!({
                        "type": "text",
                        "text": code_text,
                        "marks": marks
                    }));
                }
            }
            Event::SoftBreak => {
                if try_append_to_image_alt(&mut stack, " ") {
                    continue;
                }
                current_text.push('\n');
            }
            Event::HardBreak => {
                if try_append_to_image_alt(&mut stack, " ") {
                    continue;
                }
                if let Some((_, content, _)) = stack.last_mut() {
                    flush_text(&mut current_text, &current_marks, content);
                    content.push(json!({ "type": "hardBreak" }));
                }
            }
            Event::Rule => {
                if let Some((_, content, _)) = stack.last_mut() {
                    flush_text(&mut current_text, &current_marks, content);
                    content.push(json!({ "type": "horizontalRule" }));
                } else {
                    doc_content.push(json!({ "type": "horizontalRule" }));
                }
            }
            Event::TaskListMarker(checked) => {
                if let Some((node_type, _, _)) = stack.last_mut()
                    && *node_type == "listItem"
                {
                    *node_type = "taskItem";
                }
                for (node_type, _, _) in stack.iter_mut().rev() {
                    if *node_type == "bulletList" {
                        *node_type = "taskList";
                        break;
                    }
                }
                if let Some((_, _, attrs)) = stack.last_mut() {
                    *attrs = Some(json!({ "checked": checked }));
                }
            }
            _ => {}
        }
    }

    if !current_text.is_empty() {
        let mut text_node = json!({
            "type": "text",
            "text": current_text
        });
        if !current_marks.is_empty() {
            text_node["marks"] = json!(current_marks);
        }
        doc_content.push(json!({
            "type": "paragraph",
            "content": [text_node]
        }));
    }

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

pub(crate) fn prosemirror_to_markdown(
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
            node_to_markdown(node, &mut output, "", 0, attachment_uri_scheme)?;
        }
    }

    let trimmed_len = output.trim_end().len();
    output.truncate(trimmed_len);
    Ok(output)
}

fn node_to_markdown(
    node: &Value,
    output: &mut String,
    prefix: &str,
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
            output.push_str(prefix);
            let mut paragraph = String::new();
            inline_content_to_markdown(node, &mut paragraph, depth, scheme)?;
            push_with_escaped_block_starts(output, &paragraph);
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
                output.push_str(prefix);
                output.push('[');
                push_escaped_label(output, filename);
                output.push_str("](");
                push_link_destination(output, &format!("{scheme}{attachment_id}"));
                output.push_str(")\n");
            }
        }
        "heading" => {
            // ProseMirror headings are levels 1..=6; clamp before `repeat`.
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_i64())
                .unwrap_or(1)
                .clamp(1, 6) as usize;
            output.push_str(prefix);
            output.push_str(&"#".repeat(level));
            output.push(' ');
            let mut inline = String::new();
            inline_content_to_markdown(node, &mut inline, depth, scheme)?;
            // ATX headings are single-line; collapse embedded breaks to spaces
            // so a heading does not split into a heading plus paragraph.
            output.push_str(&inline.replace(['\n', '\r'], " "));
            output.push('\n');
        }
        "blockquote" => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for (i, child) in content.iter().enumerate() {
                    if i > 0 {
                        output.push_str(prefix);
                        output.push_str(">\n");
                    }
                    let mut child_output = String::new();
                    node_to_markdown(child, &mut child_output, "", depth + 1, scheme)?;
                    for line in child_output.split_inclusive('\n') {
                        output.push_str(prefix);
                        if line.trim().is_empty() {
                            output.push('>');
                            if line.ends_with('\n') {
                                output.push('\n');
                            }
                        } else {
                            output.push_str("> ");
                            output.push_str(line);
                        }
                    }
                }
            }
        }
        "codeBlock" | "code_block" => {
            let language = node
                .get("attrs")
                .and_then(|a| a.get("language").or_else(|| a.get("params")))
                .and_then(|l| l.as_str())
                .unwrap_or("");
            // Info strings are single-line and cannot contain backticks
            // (a backtick would extend the opening fence); take the first line
            // and drop backticks so the fence cannot be broken or unbalanced.
            let language: String = language
                .split(['\n', '\r'])
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| *c != '`')
                .collect();
            let mut code_text = String::new();
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    if let Some(text) = child.get("text").and_then(|t| t.as_str()) {
                        code_text.push_str(text);
                    }
                }
            }
            let fence = backtick_delimiter(&code_text, 3);
            // The caller-provided prefix must apply to every fence line
            // so nested fences stay inside their parent block.
            let mut body = String::new();
            body.push_str(&fence);
            body.push_str(&language);
            body.push('\n');
            body.push_str(&code_text);
            if !body.ends_with('\n') {
                body.push('\n');
            }
            body.push_str(&fence);
            body.push('\n');
            for line in body.split_inclusive('\n') {
                if line.trim().is_empty() {
                    // Blank lines inside a fence must stay verbatim.
                    output.push_str(line);
                } else {
                    output.push_str(prefix);
                    output.push_str(line);
                }
            }
        }
        "bulletList" | "bullet_list" => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                let marker = bullet_marker(node);
                for (i, child) in content.iter().enumerate() {
                    output.push_str(prefix);
                    output.push_str(&marker);
                    let continuation_prefix = format!(
                        "{prefix}{}",
                        " ".repeat(list_item_continuation_indent(
                            child,
                            marker.len(),
                            prefix.len()
                        ))
                    );
                    list_item_to_markdown(child, output, &continuation_prefix, depth + 1, scheme)?;
                    if i + 1 < content.len() && list_item_loose_after(child) {
                        output.push('\n');
                    }
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
                    // Saturate so a near-max `order` cannot overflow (debug
                    // panic / release wraparound); clamp to the schema's
                    // positive-order invariant.
                    let marker = format!("{}. ", start.max(1).saturating_add(i as i64));
                    output.push_str(prefix);
                    output.push_str(&marker);
                    let continuation_prefix = format!(
                        "{prefix}{}",
                        " ".repeat(list_item_continuation_indent(
                            child,
                            marker.len(),
                            prefix.len()
                        ))
                    );
                    list_item_to_markdown(child, output, &continuation_prefix, depth + 1, scheme)?;
                    if i + 1 < content.len() && list_item_loose_after(child) {
                        output.push('\n');
                    }
                }
            }
        }
        "taskList" | "task_list" => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for (i, child) in content.iter().enumerate() {
                    let checked = child
                        .get("attrs")
                        .and_then(|a| a.get("checked"))
                        .and_then(|c| c.as_bool())
                        .unwrap_or(false);
                    let marker = if checked { "- [x] " } else { "- [ ] " };
                    output.push_str(prefix);
                    output.push_str(marker);
                    let continuation_prefix = format!(
                        "{prefix}{}",
                        " ".repeat(list_item_continuation_indent(
                            child,
                            marker.len(),
                            prefix.len()
                        ))
                    );
                    list_item_to_markdown(child, output, &continuation_prefix, depth + 1, scheme)?;
                    if i + 1 < content.len() && list_item_loose_after(child) {
                        output.push('\n');
                    }
                }
            }
        }
        "horizontalRule" | "horizontal_rule" => {
            output.push_str(prefix);
            output.push_str("---\n");
        }
        "table" => {
            if let Some(rows) = node.get("content").and_then(|c| c.as_array()) {
                let alignments = node
                    .get("attrs")
                    .and_then(|a| a.get("alignments"))
                    .and_then(|a| a.as_array());
                let mut is_first_row = true;
                for row in rows {
                    table_row_to_markdown(
                        row,
                        output,
                        prefix,
                        is_first_row,
                        alignments,
                        depth + 1,
                        scheme,
                    )?;
                    is_first_row = false;
                }
            }
        }
        "hardBreak" | "hard_break" => {
            output.push_str("  \n");
        }
        "image" => {
            let mut image = String::new();
            if render_image_to_markdown(node, &mut image, scheme) {
                output.push_str(prefix);
                output.push_str(&image);
                output.push('\n');
            }
        }
        "text" => {
            text_node_to_markdown(node, output, depth, scheme)?;
        }
        _ => {
            if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                for child in content {
                    node_to_markdown(child, output, prefix, depth + 1, scheme)?;
                }
            }
        }
    }

    Ok(())
}

fn longest_backtick_run(s: &str) -> usize {
    let mut longest_run = 0;
    let mut current_run = 0;
    for byte in s.bytes() {
        if byte == b'`' {
            current_run += 1;
            longest_run = longest_run.max(current_run);
        } else {
            current_run = 0;
        }
    }
    longest_run
}

fn backtick_delimiter(code: &str, min_len: usize) -> String {
    "`".repeat(min_len.max(longest_backtick_run(code) + 1))
}

fn list_item_to_markdown(
    node: &Value,
    output: &mut String,
    continuation_prefix: &str,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    if depth >= crate::MAX_DEPTH {
        return Err(ProseMirrorError::InvalidStructure(
            "Document nesting exceeds the maximum depth".to_string(),
        ));
    }

    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
        let mut first_line = true;
        let mut line_has_inline_content = false;
        let mut previous_block_type: Option<&str> = None;
        let mut previous_block_loose_after = false;

        for child in content {
            let node_type = child.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match node_type {
                "paragraph" => {
                    if !first_line {
                        if previous_block_type == Some("paragraph")
                            || node_loose_before(child)
                            || previous_block_loose_after
                        {
                            output.push('\n');
                        }
                        output.push_str(continuation_prefix);
                    }
                    let mut paragraph = String::new();
                    inline_content_to_markdown(child, &mut paragraph, depth + 1, scheme)?;
                    let paragraph = escape_markdown_block_starts(&paragraph);
                    push_with_continuation_prefix(output, &paragraph, continuation_prefix);
                    output.push('\n');
                    first_line = false;
                    line_has_inline_content = false;
                    previous_block_type = Some("paragraph");
                    previous_block_loose_after = node_loose_after(child);
                }
                "text" | "hardBreak" | "hard_break" => {
                    if !first_line && !line_has_inline_content {
                        if list_item_loose_before_inline(node) || previous_block_loose_after {
                            output.push('\n');
                        }
                        output.push_str(continuation_prefix);
                    }
                    text_node_to_markdown(child, output, depth + 1, scheme)?;
                    first_line = false;
                    line_has_inline_content = true;
                    previous_block_type = None;
                    previous_block_loose_after = false;
                }
                _ => {
                    if first_line || line_has_inline_content || node_loose_before(child) {
                        output.push('\n');
                    }
                    node_to_markdown(child, output, continuation_prefix, depth + 1, scheme)?;
                    first_line = false;
                    line_has_inline_content = false;
                    previous_block_type = Some(node_type);
                    previous_block_loose_after = node_loose_after(child);
                }
            }
        }

        if first_line || line_has_inline_content {
            output.push('\n');
        }
    }
    Ok(())
}

fn item_has_trailing_blank(markdown: &str, range: &std::ops::Range<usize>) -> bool {
    let Some(text) = markdown.get(range.clone()) else {
        return false;
    };
    let mut line_breaks = 0;
    for ch in text.chars().rev() {
        match ch {
            ' ' | '\t' | '\r' => {}
            '\n' => {
                line_breaks += 1;
                if line_breaks >= 2 {
                    return true;
                }
            }
            _ => return false,
        }
    }
    false
}

fn has_blank_between(markdown: &str, previous_end: usize, current_start: usize) -> bool {
    let Some(before_current) = markdown.get(..previous_end) else {
        return false;
    };
    let Some(gap) = markdown.get(previous_end..current_start) else {
        return false;
    };

    let mut line_breaks = usize::from(before_current.ends_with('\n'));
    for ch in gap.chars() {
        match ch {
            ' ' | '\t' | '\r' => {}
            '\n' => {
                line_breaks += 1;
                if line_breaks >= 2 {
                    return true;
                }
            }
            _ => return false,
        }
    }
    false
}

fn node_loose_before(node: &Value) -> bool {
    node.get("attrs")
        .and_then(|a| a.get("looseBefore"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn node_loose_after(node: &Value) -> bool {
    node.get("attrs")
        .and_then(|a| a.get("looseAfter"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn bullet_marker(node: &Value) -> String {
    match node
        .get("attrs")
        .and_then(|a| a.get("marker"))
        .and_then(|v| v.as_str())
    {
        Some("*") => "* ".to_string(),
        Some("+") => "+ ".to_string(),
        _ => "- ".to_string(),
    }
}

fn list_item_loose_after(node: &Value) -> bool {
    node.get("attrs")
        .and_then(|a| a.get("looseAfter"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn list_item_loose_before_inline(node: &Value) -> bool {
    node.get("attrs")
        .and_then(|a| a.get("looseBeforeInline"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn list_item_continuation_indent(node: &Value, fallback: usize, prefix_len: usize) -> usize {
    node.get("attrs")
        .and_then(|a| a.get("continuationIndent"))
        .and_then(|v| v.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .map(|value| value.saturating_sub(prefix_len))
        .map(|value| value.min(MAX_LIST_CONTINUATION_PADDING))
        .unwrap_or(fallback)
}

fn push_with_continuation_prefix(output: &mut String, text: &str, continuation_prefix: &str) {
    let mut parts = text.split('\n').peekable();
    let mut first = true;
    while let Some(part) = parts.next() {
        if !first {
            output.push_str(continuation_prefix);
        }
        output.push_str(part);
        if parts.peek().is_some() {
            output.push('\n');
        }
        first = false;
    }
}

fn push_with_escaped_block_starts(output: &mut String, text: &str) {
    let escaped = escape_markdown_block_starts(text);
    output.push_str(&escaped);
}

fn escape_markdown_block_starts(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (body, newline) = line
            .strip_suffix('\n')
            .map_or((line, ""), |body| (body, "\n"));
        if let Some(delimiter_index) = ordered_list_marker_delimiter_index(body) {
            escaped.push_str(&body[..delimiter_index]);
            escaped.push('\\');
            escaped.push_str(&body[delimiter_index..]);
        } else if markdown_line_needs_leading_escape(body) {
            escaped.push('\\');
            escaped.push_str(body);
        } else {
            escaped.push_str(body);
        }
        escaped.push_str(newline);
    }
    escaped
}

fn markdown_line_needs_leading_escape(line: &str) -> bool {
    markdown_line_starts_heading(line)
        || line.starts_with('>')
        || line.starts_with("```")
        || line.starts_with("~~~")
        || matches!(line, "-" | "*" | "+")
        || line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("+ ")
        || is_thematic_break_line(line)
}

fn markdown_line_starts_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&hashes)
        && line
            .as_bytes()
            .get(hashes)
            .is_none_or(|next| next.is_ascii_whitespace())
}

/// A line of 3+ of the same `-`, `*`, or `_` (ignoring spaces/tabs) parses as
/// a thematic break, silently turning paragraph text into a horizontal rule.
fn is_thematic_break_line(line: &str) -> bool {
    let mut marker = None;
    let mut count = 0;
    for ch in line.chars() {
        match ch {
            ' ' | '\t' => {}
            '-' | '*' | '_' => {
                if marker.is_some_and(|m| m != ch) {
                    return false;
                }
                marker = Some(ch);
                count += 1;
            }
            _ => return false,
        }
    }
    count >= 3
}

fn ordered_list_marker_delimiter_index(line: &str) -> Option<usize> {
    let delimiter_index = line.find(['.', ')'])?;
    let marker = &line[..delimiter_index];
    let rest = &line[delimiter_index + 1..];

    if marker.is_empty()
        || marker.len() > 9
        || !marker.bytes().all(|b| b.is_ascii_digit())
        || !rest.starts_with(' ')
    {
        return None;
    }

    Some(delimiter_index)
}

fn inline_content_to_markdown(
    node: &Value,
    output: &mut String,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
        render_inline_nodes(content, output, depth + 1, scheme)?;
    }
    Ok(())
}

fn text_node_to_markdown(
    node: &Value,
    output: &mut String,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    render_inline_nodes(std::slice::from_ref(node), output, depth, scheme)
}

fn render_inline_nodes(
    nodes: &[Value],
    output: &mut String,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    render_inline_run(nodes, 0, output, depth, scheme)
}

/// Render a run of inline nodes, treating the mark at index `skip` as the next
/// unconsumed mark. Marks are peeled by index rather than by cloning each node
/// with its first mark stripped, so heavily marked text stays linear in the
/// number of marks instead of quadratic.
fn render_inline_run(
    nodes: &[Value],
    skip: usize,
    output: &mut String,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    if depth >= crate::MAX_DEPTH {
        return Err(ProseMirrorError::InvalidStructure(
            "Document nesting exceeds the maximum depth".to_string(),
        ));
    }

    let mut i = 0;
    while i < nodes.len() {
        if let Some(mark) = mark_at(&nodes[i], skip) {
            let mut j = i + 1;
            while j < nodes.len() && mark_at(&nodes[j], skip) == Some(mark) {
                j += 1;
            }
            render_marked_run(mark, &nodes[i..j], skip + 1, output, depth + 1, scheme)?;
            i = j;
            continue;
        }

        if nodes[i].get("type").and_then(|t| t.as_str()) == Some("text") {
            let mut text = String::new();
            let mut j = i;
            while j < nodes.len()
                && mark_at(&nodes[j], skip).is_none()
                && nodes[j].get("type").and_then(|t| t.as_str()) == Some("text")
            {
                if let Some(value) = nodes[j].get("text").and_then(|t| t.as_str()) {
                    text.push_str(value);
                }
                j += 1;
            }
            push_escaped_markdown_text(output, &text);
            i = j;
            continue;
        }

        render_unmarked_inline_node(&nodes[i], output, depth, scheme)?;
        i += 1;
    }
    Ok(())
}

fn mark_at(node: &Value, idx: usize) -> Option<&Value> {
    node.get("marks")
        .and_then(|m| m.as_array())
        .and_then(|marks| marks.get(idx))
}

fn render_unmarked_inline_node(
    node: &Value,
    output: &mut String,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");

    if node_type == "hardBreak" || node_type == "hard_break" {
        output.push_str("  \n");
        return Ok(());
    }

    if node_type == "image" {
        render_image_to_markdown(node, output, scheme);
        return Ok(());
    }

    if node_type != "text" {
        if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
            render_inline_nodes(content, output, depth + 1, scheme)?;
        }
        return Ok(());
    }

    let text = node.get("text").and_then(|t| t.as_str()).unwrap_or("");
    push_escaped_markdown_text(output, text);
    Ok(())
}

fn render_image_to_markdown(node: &Value, output: &mut String, scheme: &str) -> bool {
    let attrs = node.get("attrs");
    let alt = attrs
        .and_then(|a| a.get("alt"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let title = attrs
        .and_then(|a| a.get("title"))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty());

    if let Some(attachment_id) = attrs
        .and_then(|a| a.get("attachmentId"))
        .and_then(|v| v.as_str())
    {
        output.push_str("![");
        push_escaped_label(output, alt);
        output.push_str("](");
        push_link_destination(output, &format!("{scheme}{attachment_id}"));
        push_markdown_title(output, title);
        output.push(')');
        true
    } else if let Some(src) = attrs.and_then(|a| a.get("src")).and_then(|v| v.as_str()) {
        output.push_str("![");
        push_escaped_label(output, alt);
        output.push_str("](");
        push_link_destination(output, src);
        push_markdown_title(output, title);
        output.push(')');
        true
    } else {
        false
    }
}

fn render_marked_run(
    mark: &Value,
    nodes: &[Value],
    skip: usize,
    output: &mut String,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    let mark_type = mark.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match mark_type {
        "bold" | "strong" => {
            render_delimited_mark_expel_whitespace("**", "**", nodes, skip, output, depth, scheme)?
        }
        "italic" | "em" => {
            render_delimited_mark_expel_whitespace("*", "*", nodes, skip, output, depth, scheme)?
        }
        "code" => render_code_mark(nodes, output, depth)?,
        "strike" | "strikethrough" => {
            render_delimited_mark("~~", "~~", nodes, skip, output, depth, scheme)?
        }
        "underline" => render_delimited_mark("<u>", "</u>", nodes, skip, output, depth, scheme)?,
        "link" => {
            let href = mark
                .get("attrs")
                .and_then(|a| a.get("href"))
                .and_then(|h| h.as_str())
                .unwrap_or("");
            let title = mark
                .get("attrs")
                .and_then(|a| a.get("title"))
                .and_then(|t| t.as_str())
                .filter(|t| !t.is_empty());
            let mut raw_inner = String::new();
            render_inline_plain_text(nodes, &mut raw_inner, depth)?;
            if title.is_none() && raw_inner == href && is_autolink_href(href) {
                output.push('<');
                output.push_str(href);
                output.push('>');
            } else {
                let mut inner = String::new();
                render_inline_run(nodes, skip, &mut inner, depth, scheme)?;
                output.push('[');
                output.push_str(&inner);
                output.push_str("](");
                push_link_destination(output, href);
                push_markdown_title(output, title);
                output.push(')');
            }
        }
        _ => render_inline_run(nodes, skip, output, depth, scheme)?,
    }
    Ok(())
}

fn render_delimited_mark(
    open: &str,
    close: &str,
    nodes: &[Value],
    skip: usize,
    output: &mut String,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    output.push_str(open);
    render_inline_run(nodes, skip, output, depth, scheme)?;
    output.push_str(close);
    Ok(())
}

fn render_delimited_mark_expel_whitespace(
    open: &str,
    close: &str,
    nodes: &[Value],
    skip: usize,
    output: &mut String,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    let mut inner = String::new();
    render_inline_run(nodes, skip, &mut inner, depth, scheme)?;
    let trimmed_start = inner.trim_start_matches(char::is_whitespace);
    let leading_len = inner.len() - trimmed_start.len();
    let trimmed = trimmed_start.trim_end_matches(char::is_whitespace);
    let trailing_len = trimmed_start.len() - trimmed.len();
    if trimmed.is_empty() {
        output.push_str(&inner);
        return Ok(());
    }
    output.push_str(&inner[..leading_len]);
    output.push_str(open);
    output.push_str(trimmed);
    output.push_str(close);
    output.push_str(&inner[inner.len() - trailing_len..]);
    Ok(())
}

fn render_code_mark(
    nodes_without_mark: &[Value],
    output: &mut String,
    depth: usize,
) -> Result<(), ProseMirrorError> {
    let mut inner = String::new();
    render_inline_plain_text(nodes_without_mark, &mut inner, depth)?;
    let delimiter = backtick_delimiter(&inner, 1);
    output.push_str(&delimiter);
    if inner.starts_with('`')
        || inner.starts_with(' ')
        || inner.ends_with('`')
        || inner.ends_with(' ')
    {
        output.push(' ');
        output.push_str(&inner);
        output.push(' ');
    } else {
        output.push_str(&inner);
    }
    output.push_str(&delimiter);
    Ok(())
}

fn render_inline_plain_text(
    nodes: &[Value],
    output: &mut String,
    depth: usize,
) -> Result<(), ProseMirrorError> {
    if depth >= crate::MAX_DEPTH {
        return Err(ProseMirrorError::InvalidStructure(
            "Document nesting exceeds the maximum depth".to_string(),
        ));
    }

    for node in nodes {
        let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match node_type {
            "text" => {
                if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
                    output.push_str(text);
                }
            }
            "hardBreak" | "hard_break" => output.push('\n'),
            _ => {
                if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                    render_inline_plain_text(content, output, depth + 1)?;
                }
            }
        }
    }
    Ok(())
}

fn push_escaped_markdown_text(output: &mut String, text: &str) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let bracket_escape_starts = markdown_bracket_escape_starts(text);
    let mut bracket_escape_index = 0;
    for (idx, &(byte_idx, ch)) in chars.iter().enumerate() {
        let prev = idx.checked_sub(1).map(|i| chars[i].1);
        let next = chars.get(idx + 1).map(|(_, c)| *c);
        let should_escape = match ch {
            '\\' | '`' | '*' => true,
            '_' => should_escape_underscore(prev, next),
            '[' => {
                let should_escape =
                    bracket_escape_starts.get(bracket_escape_index) == Some(&byte_idx);
                if should_escape {
                    bracket_escape_index += 1;
                }
                should_escape
            }
            '<' => next.is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?')),
            '~' => prev == Some('~') || next == Some('~'),
            _ => false,
        };
        if should_escape {
            output.push('\\');
        }
        output.push(ch);
    }
}

fn markdown_bracket_escape_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut pending = Vec::new();
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '[' => pending.push(idx),
            ']' => {
                let rest = &text[idx + ch.len_utf8()..];
                if rest.starts_with('(') || rest.starts_with('[') {
                    starts.append(&mut pending);
                } else {
                    pending.clear();
                }
            }
            _ => {}
        }
    }

    starts.append(&mut pending);
    starts
}

fn should_escape_underscore(prev: Option<char>, next: Option<char>) -> bool {
    if prev == Some('[') {
        return false;
    }
    !(prev.is_some_and(|c| c.is_alphanumeric()) && next.is_some_and(|c| c.is_alphanumeric()))
}

/// Escape Markdown link/image labels and collapse line endings.
fn push_escaped_label(output: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '\\' | '[' | ']' => {
                output.push('\\');
                output.push(ch);
            }
            '\n' | '\r' => output.push(' '),
            _ => output.push(ch),
        }
    }
}

/// Emit a Markdown link/image destination so it round-trips and cannot break
/// out of the surrounding `()`. Destinations with whitespace, parentheses,
/// angle brackets, a backslash, or control characters use the angle-bracket
/// form `<...>`; line endings (disallowed inside a destination) are dropped.
fn push_link_destination(output: &mut String, dest: &str) {
    let needs_angle = dest.bytes().any(|b| {
        b.is_ascii_whitespace()
            || b.is_ascii_control()
            || matches!(b, b'(' | b')' | b'<' | b'>' | b'\\')
    });
    if !needs_angle {
        output.push_str(dest);
        return;
    }
    output.push('<');
    for ch in dest.chars() {
        match ch {
            '\n' | '\r' => {}
            '<' | '>' | '\\' => {
                output.push('\\');
                output.push(ch);
            }
            _ => output.push(ch),
        }
    }
    output.push('>');
}

fn push_markdown_title(output: &mut String, title: Option<&str>) {
    let Some(title) = title else {
        return;
    };
    output.push_str(" \"");
    for ch in title.chars() {
        match ch {
            '"' | '\\' => {
                output.push('\\');
                output.push(ch);
            }
            '\n' | '\r' => output.push(' '),
            c if c.is_ascii_control() => output.push(' '),
            _ => output.push(ch),
        }
    }
    output.push('"');
}

fn is_autolink_href(href: &str) -> bool {
    // CommonMark autolinks forbid whitespace, control characters, and `<`/`>`;
    // reject those so the raw `<href>` emission cannot break out of the link.
    if href
        .bytes()
        .any(|b| b.is_ascii_whitespace() || b.is_ascii_control() || matches!(b, b'<' | b'>'))
    {
        return false;
    }
    href.contains("://") || href.contains('@')
}

fn table_row_to_markdown(
    row: &Value,
    output: &mut String,
    prefix: &str,
    is_first_row: bool,
    alignments: Option<&Vec<Value>>,
    depth: usize,
    scheme: &str,
) -> Result<(), ProseMirrorError> {
    if depth >= crate::MAX_DEPTH {
        return Err(ProseMirrorError::InvalidStructure(
            "Document nesting exceeds the maximum depth".to_string(),
        ));
    }

    if let Some(cells) = row.get("content").and_then(|c| c.as_array()) {
        output.push_str(prefix);
        output.push('|');
        for cell in cells {
            let mut rendered_cell = String::new();
            inline_content_to_markdown(cell, &mut rendered_cell, depth + 1, scheme)?;
            if rendered_cell.is_empty() {
                output.push_str(" |");
            } else {
                output.push(' ');
                output.push_str(&rendered_cell.replace('|', "\\|"));
                output.push_str(" |");
            }
        }
        output.push('\n');

        if is_first_row {
            output.push_str(prefix);
            output.push('|');
            for (i, _) in cells.iter().enumerate() {
                let marker = alignments
                    .and_then(|values| values.get(i))
                    .and_then(|value| value.as_str())
                    .map(|alignment| match alignment {
                        "left" => " :--- |",
                        "center" => " :---: |",
                        "right" => " ---: |",
                        _ => " --- |",
                    })
                    .unwrap_or(" --- |");
                output.push_str(marker);
            }
            output.push('\n');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEME: &str = "serentest://attachment/";

    #[test]
    fn split_text_nodes_cannot_assemble_unintended_link() {
        let doc = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [
                    { "type": "text", "text": "[literal" },
                    { "type": "text", "text": "](not-a-link)" }
                ]
            }]
        });

        let markdown = prosemirror_to_markdown(&doc, SCHEME).unwrap();
        let reparsed = markdown_to_prosemirror(&markdown, SCHEME).unwrap();
        let inline = reparsed["content"][0]["content"].as_array().unwrap();

        assert_eq!(inline.len(), 1);
        assert_eq!(inline[0]["text"], "[literal](not-a-link)");
        assert!(inline[0].get("marks").is_none());
    }

    #[test]
    fn huge_continuation_indent_attr_is_bounded() {
        let doc = json!({
            "type": "doc",
            "content": [{
                "type": "bulletList",
                "content": [{
                    "type": "listItem",
                    "attrs": { "continuationIndent": u64::MAX },
                    "content": [{
                        "type": "paragraph",
                        "content": [{ "type": "text", "text": "bounded" }]
                    }]
                }]
            }]
        });

        let markdown = prosemirror_to_markdown(&doc, SCHEME).unwrap();

        assert!(markdown.len() < MAX_LIST_CONTINUATION_PADDING + 32);
        assert!(markdown.starts_with("- bounded"));
    }

    #[test]
    fn arbitrary_attr_types_do_not_panic() {
        // Markdown-origin attrs ignore wrong-typed values and bound large values.
        let doc = json!({
            "type": "doc",
            "content": [
                { "type": "heading", "attrs": { "level": u64::MAX }, "content": [{ "type": "text", "text": "h" }] },
                { "type": "heading", "attrs": { "level": "huge" }, "content": [{ "type": "text", "text": "h" }] },
                {
                    "type": "orderedList",
                    "attrs": { "order": i64::MAX },
                    "content": [{
                        "type": "listItem",
                        "attrs": { "continuationIndent": "lots", "looseAfter": "yes" },
                        "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "a" }] }]
                    }]
                },
                {
                    "type": "bulletList",
                    "attrs": { "marker": 42 },
                    "content": [{
                        "type": "listItem",
                        "attrs": { "continuationIndent": u64::MAX, "loose": 1, "looseBefore": [] },
                        "content": [
                            { "type": "paragraph", "content": [{ "type": "text", "text": "b" }] },
                            { "type": "paragraph", "content": [{ "type": "text", "text": "c" }] }
                        ]
                    }]
                },
                {
                    "type": "table",
                    "attrs": { "alignments": "not-an-array" },
                    "content": [{ "type": "tableRow", "content": [
                        { "type": "tableCell", "content": [{ "type": "text", "text": "x" }] }
                    ] }]
                },
                {
                    "type": "table",
                    "attrs": { "alignments": [1, 2, 3] },
                    "content": [{ "type": "tableRow", "attrs": { "header": true }, "content": [
                        { "type": "tableHeader", "content": [{ "type": "text", "text": "h" }] }
                    ] }]
                }
            ]
        });

        let markdown = prosemirror_to_markdown(&doc, SCHEME).unwrap();
        // Large continuationIndent values stay bounded.
        assert!(markdown.len() < MAX_LIST_CONTINUATION_PADDING * 4);
        // Re-parsing the rendered Markdown must also not panic.
        markdown_to_prosemirror(&markdown, SCHEME).unwrap();
    }

    #[test]
    fn nested_large_continuation_indent_stays_bounded() {
        let mut node = json!({
            "type": "bulletList",
            "content": [{
                "type": "listItem",
                "attrs": { "continuationIndent": u64::MAX },
                "content": [
                    { "type": "paragraph", "content": [{ "type": "text", "text": "leaf" }] },
                    { "type": "paragraph", "content": [{ "type": "text", "text": "cont" }] }
                ]
            }]
        });
        for _ in 0..8 {
            node = json!({
                "type": "bulletList",
                "content": [{
                    "type": "listItem",
                    "attrs": { "continuationIndent": u64::MAX },
                    "content": [
                        node,
                        { "type": "paragraph", "content": [{ "type": "text", "text": "cont" }] }
                    ]
                }]
            });
        }
        let doc = json!({ "type": "doc", "content": [node] });
        let markdown = prosemirror_to_markdown(&doc, SCHEME).unwrap();
        assert!(markdown.len() < 1_000_000);
    }

    #[test]
    fn cross_mark_bracket_run_cannot_assemble_link() {
        // A `[` and `](url)` split across a bold boundary must not become a link.
        let doc = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [
                    { "type": "text", "text": "[" },
                    { "type": "text", "text": "x", "marks": [{ "type": "bold" }] },
                    { "type": "text", "text": "](https://example.invalid)" }
                ]
            }]
        });
        let markdown = prosemirror_to_markdown(&doc, SCHEME).unwrap();
        let reparsed = markdown_to_prosemirror(&markdown, SCHEME).unwrap();
        assert!(
            !reparsed.to_string().contains("\"link\""),
            "synthesized a link across the bold boundary: {markdown:?}"
        );
    }

    #[test]
    fn multibyte_text_near_metacharacters_does_not_panic() {
        let doc = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "café[x](y) 日本語](z) ✓[ unclosed _ä_ ~ñ~" }]
            }]
        });
        let markdown = prosemirror_to_markdown(&doc, SCHEME).unwrap();
        markdown_to_prosemirror(&markdown, SCHEME).unwrap();
    }
}
