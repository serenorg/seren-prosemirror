use super::*;

fn pm() -> ProseMirror {
    ProseMirror::new("serentest://attachment/")
}

fn assert_markdown_roundtrips(md: &str) {
    let p = pm();
    let doc = p.markdown_to_prosemirror(md).unwrap();
    let back = p.prosemirror_to_markdown(&doc).unwrap();
    assert_eq!(back, md);
}

#[test]
fn test_markdown_to_prosemirror_simple() {
    let doc = pm().markdown_to_prosemirror("Hello **world**!").unwrap();
    assert_eq!(doc["type"], "doc");
    assert!(!doc["content"].as_array().unwrap().is_empty());
}

#[test]
fn test_prosemirror_to_markdown_roundtrip() {
    let p = pm();
    let md = "# Heading\n\nSome **bold** and *italic* text.";
    let doc = p.markdown_to_prosemirror(md).unwrap();
    let back = p.prosemirror_to_markdown(&doc).unwrap();

    assert!(back.contains("# Heading"));
    assert!(back.contains("bold"));
    assert!(back.contains("italic"));
}

#[test]
fn test_extract_plain_text() {
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "Hello world"
            }]
        }]
    });
    assert_eq!(extract_plain_text(&doc), "Hello world");
}

#[test]
fn test_validate_document() {
    let valid = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{ "type": "text", "text": "Hello" }]
        }]
    });
    assert!(validate_document(&valid).is_ok());

    let invalid = json!({
        "type": "doc",
        "content": [{ "type": "unsupported_node" }]
    });
    assert!(validate_document(&invalid).is_err());
}

#[test]
fn alternate_mark_names_validate_and_render() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [
                { "type": "text", "text": "strong", "marks": [{ "type": "strong" }] },
                { "type": "text", "text": " and " },
                { "type": "text", "text": "em", "marks": [{ "type": "em" }] }
            ]
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();

    validate_document(&doc).unwrap();
    assert_eq!(md, "**strong** and *em*");
}

#[test]
fn validate_document_rejects_excessive_nesting() {
    let mut node = json!({ "type": "text", "text": "deep" });
    for _ in 0..200 {
        node = json!({ "type": "blockquote", "content": [node] });
    }
    let doc = json!({ "type": "doc", "content": [node] });
    let err = validate_document(&doc).unwrap_err();
    assert!(
        format!("{err:?}").contains("maximum depth"),
        "expected a max-depth error, got {err:?}"
    );
}

#[test]
fn prosemirror_to_markdown_rejects_excessive_nesting() {
    let mut node = json!({ "type": "text", "text": "deep" });
    for _ in 0..200 {
        node = json!({ "type": "blockquote", "content": [node] });
    }
    let doc = json!({ "type": "doc", "content": [node] });
    let err = pm().prosemirror_to_markdown(&doc).unwrap_err();
    assert!(
        format!("{err:?}").contains("maximum depth"),
        "expected a max-depth error, got {err:?}"
    );
}

#[test]
fn prosemirror_to_markdown_rejects_excessive_inline_nesting() {
    let mut node = json!({ "type": "text", "text": "deep" });
    for _ in 0..200 {
        node = json!({ "type": "inlineWrapper", "content": [node] });
    }
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [node],
        }],
    });
    let err = pm().prosemirror_to_markdown(&doc).unwrap_err();
    assert!(
        format!("{err:?}").contains("maximum depth"),
        "expected a max-depth error, got {err:?}"
    );
}

#[test]
fn prosemirror_to_markdown_rejects_excessive_mark_nesting() {
    let marks = (0..200)
        .map(|_| json!({ "type": "bold" }))
        .collect::<Vec<_>>();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "deep",
                "marks": marks,
            }],
        }],
    });
    let err = pm().prosemirror_to_markdown(&doc).unwrap_err();
    assert!(
        format!("{err:?}").contains("maximum depth"),
        "expected a max-depth error, got {err:?}"
    );
}

#[cfg(feature = "orgmode")]
#[test]
fn prosemirror_to_org_rejects_excessive_nesting() {
    let mut node = json!({ "type": "text", "text": "deep" });
    for _ in 0..200 {
        node = json!({ "type": "blockquote", "content": [node] });
    }
    let doc = json!({ "type": "doc", "content": [node] });
    let err = pm().prosemirror_to_org(&doc).unwrap_err();
    assert!(
        format!("{err:?}").contains("maximum depth"),
        "expected a max-depth error, got {err:?}"
    );
}

#[test]
fn validate_document_rejects_non_doc_root() {
    let invalid = json!({
        "type": "paragraph",
        "content": [{ "type": "text", "text": "Hello" }]
    });

    assert_eq!(
        validate_document(&invalid),
        Err(ProseMirrorError::InvalidStructure(
            "Root must be doc".to_string()
        ))
    );
}

#[test]
fn validate_document_rejects_malformed_content_and_marks() {
    let invalid_content = json!({
        "type": "doc",
        "content": "not-an-array"
    });
    assert_eq!(
        validate_document(&invalid_content),
        Err(ProseMirrorError::InvalidStructure(
            "Node content must be an array".to_string()
        ))
    );

    let invalid_mark = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "Hello",
                "marks": [{ "type": "link" }]
            }]
        }]
    });
    assert_eq!(
        validate_document(&invalid_mark),
        Err(ProseMirrorError::InvalidStructure(
            "Link mark missing href".to_string()
        ))
    );
}

#[test]
fn test_empty_document() {
    let doc = empty_document();
    assert_eq!(doc["type"], "doc");
    assert!(doc["content"].as_array().unwrap().len() == 1);
}

#[test]
fn test_attachment_uri_scheme() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "attachment",
            "attrs": {
                "attachmentId": "123e4567-e89b-12d3-a456-426614174000",
                "filename": "file.pdf"
            }
        }]
    });

    let md = p.prosemirror_to_markdown(&doc).unwrap();
    assert!(
        md.contains("serentest://attachment/123e4567-e89b-12d3-a456-426614174000"),
        "Expected attachment URI in markdown, got: {}",
        md
    );
    assert!(md.contains("file.pdf"));
}

#[test]
fn test_attachment_roundtrip_via_link() {
    let p = pm();
    let md = "[file.pdf](serentest://attachment/123e4567-e89b-12d3-a456-426614174000)";
    let doc = p.markdown_to_prosemirror(md).unwrap();

    let first = &doc["content"][0];
    assert_eq!(first["type"], "attachment");
    assert_eq!(
        first["attrs"]["attachmentId"],
        "123e4567-e89b-12d3-a456-426614174000"
    );
}

#[test]
fn empty_attachment_scheme_does_not_capture_links() {
    let p = ProseMirror::new("");
    let md = "[file.pdf](123e4567-e89b-12d3-a456-426614174000)";
    let doc = p.markdown_to_prosemirror(md).unwrap();

    assert_eq!(doc["content"][0]["type"], "paragraph");
}

#[test]
fn attachment_image_uri_uses_attachment_attrs() {
    let p = pm();
    let md = "![diagram](serentest://attachment/123e4567-e89b-12d3-a456-426614174000)";
    let doc = p.markdown_to_prosemirror(md).unwrap();

    let image = &doc["content"][0]["content"][0];
    assert_eq!(image["type"], "image");
    assert_eq!(
        image["attrs"]["attachmentId"],
        "123e4567-e89b-12d3-a456-426614174000"
    );
    assert!(image["attrs"]["src"].is_null());
}

#[test]
fn empty_attachment_scheme_does_not_capture_images() {
    let p = ProseMirror::new("");
    let md = "![diagram](123e4567-e89b-12d3-a456-426614174000)";
    let doc = p.markdown_to_prosemirror(md).unwrap();

    let image = &doc["content"][0]["content"][0];
    assert_eq!(image["type"], "image");
    assert_eq!(
        image["attrs"]["src"],
        "123e4567-e89b-12d3-a456-426614174000"
    );
    assert!(image["attrs"].get("attachmentId").is_none());
}

#[test]
fn attachment_image_uri_without_id_keeps_src() {
    let p = pm();
    let md = "![diagram](serentest://attachment/)";
    let doc = p.markdown_to_prosemirror(md).unwrap();

    let image = &doc["content"][0]["content"][0];
    assert_eq!(image["type"], "image");
    assert_eq!(image["attrs"]["src"], "serentest://attachment/");
    assert!(image["attrs"].get("attachmentId").is_none());
}

#[test]
fn image_markdown_roundtrips() {
    assert_markdown_roundtrips("![diagram](https://example.com/diagram.png)");
}

#[test]
fn attachment_image_markdown_roundtrips() {
    assert_markdown_roundtrips(
        "![diagram](serentest://attachment/123e4567-e89b-12d3-a456-426614174000)",
    );
}

#[cfg(feature = "orgmode")]
#[test]
fn org_attachment_link_roundtrips_as_attachment_node() {
    let p = pm();
    let org = "[[serentest://attachment/123e4567-e89b-12d3-a456-426614174000][file.pdf]]";
    let doc = p.org_to_prosemirror(org).unwrap();

    assert_eq!(doc["content"][0]["type"], "attachment");
    assert_eq!(
        doc["content"][0]["attrs"]["attachmentId"],
        "123e4567-e89b-12d3-a456-426614174000"
    );
    assert_eq!(doc["content"][0]["attrs"]["filename"], "file.pdf");
}

#[cfg(feature = "orgmode")]
#[test]
fn org_export_preserves_inline_attachment_image_uri() {
    let p = pm();
    let doc = p
        .markdown_to_prosemirror(
            "![diagram](serentest://attachment/123e4567-e89b-12d3-a456-426614174000)",
        )
        .unwrap();
    let org = p.prosemirror_to_org(&doc).unwrap();

    assert_eq!(
        org,
        "[[serentest://attachment/123e4567-e89b-12d3-a456-426614174000][diagram]]"
    );
}

#[test]
fn test_markdown_bold_in_ordered_list() {
    let p = pm();
    let md =
        "1. **Create** the initial design\n2. **Fund** the development\n3. **Deploy** and test";
    let doc = p.markdown_to_prosemirror(md).unwrap();
    let back = p.prosemirror_to_markdown(&doc).unwrap();

    assert!(
        back.contains("**Create**"),
        "Bold 'Create' lost. Got:\n{}",
        back
    );
    assert!(
        back.contains("**Fund**"),
        "Bold 'Fund' lost. Got:\n{}",
        back
    );
    assert!(
        back.contains("**Deploy**"),
        "Bold 'Deploy' lost. Got:\n{}",
        back
    );
    assert!(
        back.contains("the initial design"),
        "Text after bold lost. Got:\n{}",
        back
    );
}

#[test]
fn test_markdown_bold_in_bullet_list() {
    let p = pm();
    let md = "- **First** item\n- **Second** item";
    let doc = p.markdown_to_prosemirror(md).unwrap();
    let back = p.prosemirror_to_markdown(&doc).unwrap();

    assert!(
        back.contains("**First**"),
        "Bold 'First' lost. Got:\n{}",
        back
    );
    assert!(
        back.contains("item"),
        "Text after bold lost. Got:\n{}",
        back
    );
}

#[test]
fn test_different_uri_schemes() {
    let swarm = ProseMirror::new("seren-bounty://attachment/");
    let notes = ProseMirror::new("seren-notes://attachment/");

    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "attachment",
            "attrs": {
                "attachmentId": "abc-123",
                "filename": "test.txt"
            }
        }]
    });

    let swarm_md = swarm.prosemirror_to_markdown(&doc).unwrap();
    let notes_md = notes.prosemirror_to_markdown(&doc).unwrap();

    assert!(swarm_md.contains("seren-bounty://attachment/abc-123"));
    assert!(notes_md.contains("seren-notes://attachment/abc-123"));
}

#[test]
fn tight_ordered_list_with_nested_bullet_roundtrips() {
    assert_markdown_roundtrips(
        "1. Resolve target database with MCP:\n   - project: `alpaca-short-trader`",
    );
}

#[test]
fn plus_bullet_marker_roundtrips() {
    assert_markdown_roundtrips("1. Use flags:\n   + `--flag` keeps the source marker.");
}

#[test]
fn loose_ordered_list_spacing_roundtrips() {
    assert_markdown_roundtrips("1. First\n\n2. Second");
}

#[test]
fn loose_list_item_nested_list_spacing_roundtrips() {
    assert_markdown_roundtrips(
        "2. Parent paragraph.\n\n   1. Nested first.\n   2. Nested second.\n\n   Follow-up paragraph.",
    );
}

#[test]
fn loose_list_item_nested_list_then_inline_text_roundtrips() {
    assert_markdown_roundtrips(
        "2. Parent paragraph.\n\n   1. Nested first.\n   2. Nested second.\n\n   Follow-up text.",
    );
}

#[test]
fn ordered_list_marker_width_drives_continuation_indent() {
    assert_markdown_roundtrips("10. Resolve target database with MCP:\n    - project: `foo`");
}

#[test]
fn ordered_list_source_continuation_indent_roundtrips() {
    assert_markdown_roundtrips("11. Basic health check passes.\n   metadata continues.");
}

#[test]
fn multiline_inline_code_in_list_item_roundtrips() {
    assert_markdown_roundtrips("3. Run:\n   `cmd one\n   cmd two`\n   then continue.");
}

#[test]
fn multiline_inline_code_after_list_soft_break_roundtrips() {
    assert_markdown_roundtrips(
        "4. The agent persists the pair with\n   `record-created-market --polymarket-condition-id \"$POLY_CID\"\n   --prophet-market-id \"$PROPHET_MID\"`.",
    );
}

#[test]
fn multiline_inline_code_preserves_asymmetric_indent() {
    assert_markdown_roundtrips(
        "4. The agent persists the pair with\n     `record-created-market --polymarket-condition-id \"$POLY_CID\"\n   --prophet-market-id \"$PROPHET_MID\"`.",
    );
}

#[test]
fn wrapped_list_item_with_code_spans_roundtrips() {
    assert_markdown_roundtrips(
        "1. In 1Password admin, create a vault named `PK Salesforce Skill`\n   and add one login item named `PK Salesforce`. The item must carry\n   `username`, `password`, and a TOTP field.",
    );
}

#[test]
fn list_item_code_fence_then_paragraph_roundtrips() {
    assert_markdown_roundtrips("1. Example:\n   ```json\n   {}\n   ```\n   Then continue.");
}

#[test]
fn bold_with_inline_code_emits_single_strong_run() {
    assert_markdown_roundtrips("2. **Existing `.env` file** - check if it exists");
}

#[test]
fn blockquote_with_inner_ordered_list_keeps_quote_prefix() {
    assert_markdown_roundtrips(
        "> 1. Keep them running\n> 2. Stop all and create a new schedule\n> 3. Cancel",
    );
}

#[test]
fn email_autolink_roundtrips_as_autolink_not_link() {
    assert_markdown_roundtrips("Please send this error to <hello@serendb.com> for support.");
}

/// Tables round-trip with the spaced separator form (`| --- |`).
#[test]
fn table_separator_renders_with_spaces_around_dashes() {
    assert_markdown_roundtrips("| a | b |\n| --- | --- |\n| 1 | 2 |");
}

#[test]
fn table_cell_pipe_is_escaped() {
    assert_markdown_roundtrips(
        "| field | values |\n| --- | --- |\n| side | `\"long\"` \\| `\"short\"` |",
    );
}

#[test]
fn table_cell_pipe_inside_inline_code_roundtrips() {
    assert_markdown_roundtrips("| expression |\n| --- |\n| `left\\|right` |");
}

#[test]
fn table_cell_pipe_inside_link_href_roundtrips() {
    assert_markdown_roundtrips("| link |\n| --- |\n| [docs](https://example.com/a\\|b) |");
}

#[test]
fn empty_table_cell_uses_single_space_form() {
    assert_markdown_roundtrips("| total | amount |\n| --- | --- |\n| label | |");
}

#[test]
fn table_inside_list_item_keeps_continuation_indent() {
    assert_markdown_roundtrips("- table:\n  | a | b |\n  | --- | --- |\n  | 1 | 2 |");
}

/// Code fences inside tight list items keep their continuation indent.
#[test]
fn code_fence_inside_list_item_preserves_indent() {
    assert_markdown_roundtrips("- step:\n  ```sql\n  SELECT 1\n  ```");
}

#[test]
fn code_fence_inside_ordered_list_before_next_item_roundtrips() {
    assert_markdown_roundtrips(
        "2. Use script:\n   ```javascript\n   run()\n   ```\n3. Return result",
    );
}

#[test]
fn code_fence_with_blank_line_inside_list_item_roundtrips() {
    assert_markdown_roundtrips("- step:\n  ```sql\n  SELECT 1\n\n  SELECT 2\n  ```");
}

#[test]
fn code_fence_inside_blockquote_roundtrips() {
    assert_markdown_roundtrips("> ```sql\n> SELECT 1\n> ```");
}

#[test]
fn code_fence_containing_backticks_uses_longer_outer_fence() {
    assert_markdown_roundtrips("````\n```sql\nSELECT 1\n```\n````");
}

#[test]
fn blockquote_paragraph_then_ordered_list_keeps_blank_quote_line() {
    assert_markdown_roundtrips("> Choose one:\n>\n> 1. First\n> 2. Second");
}

#[test]
fn blockquote_paragraph_then_code_fence_keeps_blank_quote_line() {
    assert_markdown_roundtrips("> Example:\n>\n> ```sql\n> SELECT 1\n> ```");
}

/// Inline code at paragraph end needs a closing delimiter.
#[test]
fn trailing_inline_code_at_end_of_paragraph_is_preserved() {
    assert_markdown_roundtrips(
        "This skill moves Claude Code auto-memory out of plaintext markdown files under `~/.claude/projects/*/memory/`.",
    );
}

#[test]
fn inline_code_containing_backtick_uses_longer_delimiter() {
    assert_markdown_roundtrips("Use `` ` `` as the delimiter.");
}

#[test]
fn inline_code_preserves_edge_spaces_with_longer_delimiter() {
    assert_markdown_roundtrips("Keep `  value  ` padded.");
}

#[test]
fn literal_tildes_are_escaped_to_prevent_strikethrough_roundtrip() {
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "use ~~ as a separator"
            }]
        }]
    });

    let markdown = pm().prosemirror_to_markdown(&doc).unwrap();
    let reparsed = pm().markdown_to_prosemirror(&markdown).unwrap();
    assert_eq!(extract_plain_text(&reparsed), "use ~~ as a separator");
}

#[test]
fn validate_document_rejects_empty_attachment_id() {
    let invalid = json!({
        "type": "doc",
        "content": [{
            "type": "attachment",
            "attrs": { "attachmentId": "" }
        }]
    });
    assert_eq!(
        validate_document(&invalid),
        Err(ProseMirrorError::InvalidStructure(
            "Attachment node missing attachmentId".to_string()
        ))
    );
}

#[test]
fn validate_document_rejects_empty_link_href() {
    let invalid = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "hi",
                "marks": [{ "type": "link", "attrs": { "href": "" } }]
            }]
        }]
    });
    assert_eq!(
        validate_document(&invalid),
        Err(ProseMirrorError::InvalidStructure(
            "Link mark missing href".to_string()
        ))
    );
}

#[test]
fn literal_markdown_metacharacters_are_escaped() {
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "**not bold** [literal](not-a-link) <tag>"
            }]
        }]
    });

    let markdown = pm().prosemirror_to_markdown(&doc).unwrap();
    assert_eq!(markdown, r"\*\*not bold\*\* \[literal](not-a-link) \<tag>");

    let reparsed = pm().markdown_to_prosemirror(&markdown).unwrap();
    assert_eq!(
        extract_plain_text(&reparsed),
        "**not bold** [literal](not-a-link) <tag>"
    );
}

#[test]
fn benign_markdown_punctuation_stays_byte_stable() {
    let text = "cost ~$1, snake_case, [N], [_id], and <100 employees";
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{ "type": "text", "text": text }]
        }]
    });

    let markdown = pm().prosemirror_to_markdown(&doc).unwrap();
    assert_eq!(markdown, text);
}

#[test]
fn table_alignment_markers_roundtrip() {
    assert_markdown_roundtrips("| a | b | c |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |");
}

#[test]
fn literal_block_markers_are_escaped_at_paragraph_start() {
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{ "type": "text", "text": "1. not a list" }]
        }, {
            "type": "paragraph",
            "content": [{ "type": "text", "text": "# not a heading" }]
        }]
    });

    let markdown = pm().prosemirror_to_markdown(&doc).unwrap();
    assert_eq!(markdown, "1\\. not a list\n\n\\# not a heading");

    let reparsed = pm().markdown_to_prosemirror(&markdown).unwrap();
    assert_eq!(
        extract_plain_text(&reparsed),
        "1. not a list\n# not a heading"
    );
}

#[test]
fn inline_code_does_not_escape_markdown_metacharacters_inside_code() {
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": "`raw`",
                "marks": [{ "type": "code" }]
            }]
        }]
    });

    assert_eq!(pm().prosemirror_to_markdown(&doc).unwrap(), "`` `raw` ``");
}

#[test]
fn soft_break_inside_paragraph_roundtrips() {
    assert_markdown_roundtrips("This paragraph is intentionally wrapped\nacross two source lines.");
}

#[test]
fn soft_break_inside_marked_text_roundtrips() {
    assert_markdown_roundtrips("This paragraph has **bold text\nwrapped across lines**.");
}

/// List items keep their first paragraph before following blocks.
#[test]
fn list_item_paragraph_with_sibling_block_is_preserved() {
    assert_markdown_roundtrips(
        "- The user has one or more documents.\n\n  Additional context paragraph.",
    );
}

#[test]
fn list_item_paragraph_nested_list_then_paragraph_roundtrips() {
    assert_markdown_roundtrips(
        "- The user has one or more documents.\n  - Review the newest document.\n\n  Additional context paragraph.",
    );
}

#[test]
fn list_item_soft_break_continuation_keeps_indent() {
    assert_markdown_roundtrips(
        "3. Set:\n   - EIP-712 `typed_data`:\n     - compatible fallback\n       `[domainSeparator || hashStruct(message)]`",
    );
}

#[test]
fn list_item_marked_soft_break_continuation_keeps_indent() {
    assert_markdown_roundtrips("- **bold text\n  wrapped across lines**");
}

// ---------------------------------------------------------------------------
// Output escaping / injection hardening
// ---------------------------------------------------------------------------

fn link_doc(text: &str, href: &str) -> Value {
    json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": text,
                "marks": [{ "type": "link", "attrs": { "href": href } }]
            }]
        }]
    })
}

fn first_inline(doc: &Value) -> &Value {
    &doc["content"][0]["content"][0]
}

#[test]
fn link_href_with_paren_breakout_payload_cannot_inject_markup() {
    let p = pm();
    let href = "http://x)![pwn](javascript:alert(1))";
    let md = p.prosemirror_to_markdown(&link_doc("click", href)).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();

    // The whole href stays one link destination; no second (image) node leaks in.
    assert_eq!(
        reparsed["content"][0]["content"].as_array().unwrap().len(),
        1
    );
    let inline = first_inline(&reparsed);
    assert_eq!(inline["marks"][0]["type"], "link");
    assert_eq!(inline["marks"][0]["attrs"]["href"], href);
}

#[test]
fn autolink_href_with_angle_brackets_cannot_inject_raw_html() {
    let p = pm();
    let href = "http://e.com#</a><img src=x onerror=alert(1)>";
    // text == href would previously take the raw `<href>` autolink path.
    let md = p.prosemirror_to_markdown(&link_doc(href, href)).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();

    assert_eq!(
        reparsed["content"][0]["content"].as_array().unwrap().len(),
        1
    );
    let inline = first_inline(&reparsed);
    assert_eq!(inline["marks"][0]["type"], "link");
    assert_eq!(inline["marks"][0]["attrs"]["href"], href);
}

#[test]
fn link_href_with_space_round_trips_via_angle_form() {
    let p = pm();
    let href = "http://x.com/a b";
    let md = p.prosemirror_to_markdown(&link_doc("x", href)).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();
    assert_eq!(first_inline(&reparsed)["marks"][0]["attrs"]["href"], href);
}

#[test]
fn link_url_scheme_is_passed_through_unsanitized_by_design() {
    // The converter is faithful: URL-scheme filtering is the downstream
    // renderer's responsibility (documented in the crate/security docs). It
    // must not silently corrupt the link nor expand it into extra markup.
    let p = pm();
    let md = p
        .prosemirror_to_markdown(&link_doc("click", "javascript:x=1"))
        .unwrap();
    assert_eq!(md, "[click](javascript:x=1)");
}

#[test]
fn link_title_is_preserved_and_prevents_autolink_shortcut() {
    let p = pm();
    let href = "https://example.com";
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": href,
                "marks": [{
                    "type": "link",
                    "attrs": { "href": href, "title": "docs \"quoted\"" }
                }]
            }]
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();
    let inline = first_inline(&reparsed);

    assert!(
        !md.starts_with('<'),
        "title was lost through autolink: {md:?}"
    );
    assert_eq!(inline["marks"][0]["attrs"]["href"], href);
    assert_eq!(inline["marks"][0]["attrs"]["title"], "docs \"quoted\"");
}

#[test]
fn image_alt_and_src_breakout_payloads_cannot_inject_markup() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "image",
            "attrs": { "alt": "a](javascript:alert(1))[x", "src": "http://x)![p](data:,)" }
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();

    let para = &reparsed["content"][0];
    assert_eq!(para["content"].as_array().unwrap().len(), 1);
    let image = &para["content"][0];
    assert_eq!(image["type"], "image");
    assert_eq!(image["attrs"]["alt"], "a](javascript:alert(1))[x");
    assert_eq!(image["attrs"]["src"], "http://x)![p](data:,)");
}

#[test]
fn image_title_is_preserved() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "image",
            "attrs": {
                "alt": "diagram",
                "src": "https://example.com/a b.png",
                "title": "quoted \"title\""
            }
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();
    let image = &reparsed["content"][0]["content"][0];

    assert_eq!(image["attrs"]["src"], "https://example.com/a b.png");
    assert_eq!(image["attrs"]["title"], "quoted \"title\"");
}

#[test]
fn attachment_filename_breakout_payload_cannot_inject_markup() {
    let p = pm();
    let id = "123e4567-e89b-12d3-a456-426614174000";
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "attachment",
            "attrs": { "attachmentId": id, "filename": "f](javascript:alert(1))[x" }
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();

    // The standalone attachment link upgrades back to a single attachment node
    // with the filename preserved verbatim (no injected second link).
    let node = &reparsed["content"][0];
    assert_eq!(node["type"], "attachment");
    assert_eq!(node["attrs"]["attachmentId"], id);
    assert_eq!(node["attrs"]["filename"], "f](javascript:alert(1))[x");
}

// ---------------------------------------------------------------------------
// Adversarial integer attributes (DoS / panic safety)
// ---------------------------------------------------------------------------

fn heading_doc(level: Value) -> Value {
    json!({
        "type": "doc",
        "content": [{ "type": "heading", "attrs": { "level": level }, "content": [{ "type": "text", "text": "hi" }] }]
    })
}

#[test]
fn heading_level_out_of_range_is_clamped_not_allocated() {
    let p = pm();
    let huge = p
        .prosemirror_to_markdown(&heading_doc(json!(i64::MAX)))
        .unwrap();
    assert!(huge.starts_with("###### "), "got: {huge}");
    let negative = p.prosemirror_to_markdown(&heading_doc(json!(-1))).unwrap();
    assert!(negative.starts_with("# "), "got: {negative}");
    let absurd = p
        .prosemirror_to_markdown(&heading_doc(json!(4_000_000_000u64)))
        .unwrap();
    assert!(absurd.starts_with("###### "), "got: {absurd}");
}

#[test]
fn ordered_list_near_max_order_does_not_overflow() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "orderedList",
            "attrs": { "order": i64::MAX },
            "content": [
                { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "a" }] }] },
                { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "b" }] }] }
            ]
        }]
    });
    // Must not panic (debug overflow) or wrap; saturates instead.
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    assert!(md.contains(&format!("{}. ", i64::MAX)));
}

#[test]
fn heavily_marked_text_renders_without_quadratic_blowup() {
    let p = pm();
    // Stay under the MAX_DEPTH mark-nesting bound while using many nodes: the
    // old code cloned every node at every mark layer (O(nodes * marks^2)).
    let marks: Vec<Value> = (0..50).map(|_| json!({ "type": "bold" })).collect();
    let nodes: Vec<Value> = (0..4000)
        .map(|_| json!({ "type": "text", "text": "x", "marks": marks.clone() }))
        .collect();
    let doc = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": nodes }] });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    // The shared mark stack collapses all nodes into one run of x's.
    assert!(md.contains(&"x".repeat(4000)));
}

#[test]
fn emphasis_whitespace_is_expelled_so_marks_survive_roundtrip() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{
                "type": "text",
                "text": " bold ",
                "marks": [{ "type": "bold" }]
            }]
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();
    let marked = reparsed["content"][0]["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["text"] == "bold")
        .expect("bold text node");

    assert_eq!(marked["marks"][0]["type"], "bold");
}

// ---------------------------------------------------------------------------
// Markdown correctness regressions
// ---------------------------------------------------------------------------

#[test]
fn thematic_break_text_is_escaped_and_preserved() {
    let p = pm();
    for text in ["---", "----", "* * *", "___"] {
        let doc = json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }]
        });
        let md = p.prosemirror_to_markdown(&doc).unwrap();
        let reparsed = p.markdown_to_prosemirror(&md).unwrap();
        assert_eq!(
            extract_plain_text(&reparsed),
            text,
            "thematic-break text lost for {text:?} (md={md:?})"
        );
    }
}

#[test]
fn leading_issue_reference_is_not_escaped_as_heading() {
    assert_markdown_roundtrips("#583 is an issue reference.");
}

#[test]
fn heading_with_embedded_newline_stays_a_single_heading() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{ "type": "heading", "attrs": { "level": 2 }, "content": [{ "type": "text", "text": "a\nb" }] }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();
    let top = reparsed["content"].as_array().unwrap();
    assert_eq!(top.len(), 1, "heading split into multiple blocks: {md:?}");
    assert_eq!(top[0]["type"], "heading");
}

#[test]
fn code_block_language_with_newline_cannot_break_fence() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "codeBlock",
            "attrs": { "language": "rust\n```\n# evil" },
            "content": [{ "type": "text", "text": "let x = 1;" }]
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();
    let blocks = reparsed["content"].as_array().unwrap();
    assert_eq!(blocks.len(), 1, "fence broke out: {md:?}");
    assert_eq!(blocks[0]["type"], "codeBlock");
    assert_eq!(blocks[0]["attrs"]["language"], "rust");
    assert!(
        blocks[0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("let x = 1;")
    );
}

#[test]
fn code_block_language_with_backticks_sizes_fence() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "codeBlock",
            "attrs": { "language": "```js" },
            "content": [{ "type": "text", "text": "x" }]
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();
    let blocks = reparsed["content"].as_array().unwrap();
    assert_eq!(blocks.len(), 1, "fence broke out: {md:?}");
    assert_eq!(blocks[0]["type"], "codeBlock");
    assert!(
        blocks[0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with('x')
    );
}

#[test]
fn code_block_params_attr_is_supported() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "code_block",
            "attrs": { "params": "rust" },
            "content": [{ "type": "text", "text": "fn main() {}" }]
        }]
    });
    let md = p.prosemirror_to_markdown(&doc).unwrap();
    let reparsed = p.markdown_to_prosemirror(&md).unwrap();

    assert!(md.starts_with("```rust\n"), "params attr ignored: {md:?}");
    assert_eq!(reparsed["content"][0]["attrs"]["language"], "rust");
}

// ---------------------------------------------------------------------------
// Org-mode converter regressions (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "orgmode")]
#[test]
fn org_multi_row_table_is_one_table_with_a_header() {
    let p = pm();
    let doc = p
        .org_to_prosemirror("| a | b |\n|---+---|\n| 1 | 2 |")
        .unwrap();
    let table = &doc["content"][0];
    assert_eq!(table["type"], "table");
    let rows = table["content"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "expected one table with two rows: {doc}");
    assert_eq!(rows[0]["content"][0]["type"], "tableHeader");
    assert_eq!(rows[1]["content"][0]["type"], "tableCell");
    assert_eq!(
        extract_plain_text(&doc).replace('\n', " ").trim(),
        "a b 1 2"
    );
}

#[cfg(feature = "orgmode")]
#[test]
fn org_multi_digit_ordered_list_parses_with_order_attr() {
    let p = pm();
    let doc = p.org_to_prosemirror("10. item ten").unwrap();
    let list = &doc["content"][0];
    assert_eq!(list["type"], "orderedList");
    assert_eq!(list["attrs"]["order"], 10);
    assert_eq!(list["content"].as_array().unwrap().len(), 1);
}

#[cfg(feature = "orgmode")]
#[test]
fn org_consecutive_bullets_merge_into_one_list() {
    let p = pm();
    let doc = p.org_to_prosemirror("- one\n- two\n- three").unwrap();
    let top = doc["content"].as_array().unwrap();
    assert_eq!(top.len(), 1, "bullets did not merge: {doc}");
    assert_eq!(top[0]["type"], "bulletList");
    assert_eq!(top[0]["content"].as_array().unwrap().len(), 3);
}

#[cfg(feature = "orgmode")]
#[test]
fn org_consecutive_ordered_items_merge_into_one_list() {
    let p = pm();
    let doc = p.org_to_prosemirror("1. a\n2. b").unwrap();
    let top = doc["content"].as_array().unwrap();
    assert_eq!(top.len(), 1, "ordered items did not merge: {doc}");
    assert_eq!(top[0]["type"], "orderedList");
    assert_eq!(top[0]["content"].as_array().unwrap().len(), 2);
    // Start of 1 needs no explicit order attr.
    assert!(top[0].get("attrs").is_none());
}

#[cfg(feature = "orgmode")]
#[test]
fn org_table_cell_with_paragraph_child_exports_text() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "table",
            "content": [{
                "type": "tableRow",
                "content": [{
                    "type": "tableCell",
                    "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Value" }] }]
                }]
            }]
        }]
    });
    let org = p.prosemirror_to_org(&doc).unwrap();
    assert!(org.contains("Value"), "cell text dropped: {org:?}");
}

#[cfg(feature = "orgmode")]
#[test]
fn org_table_cell_pipe_round_trips() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "table",
            "content": [{
                "type": "tableRow",
                "content": [{ "type": "tableCell", "content": [{ "type": "text", "text": "a | b" }] }]
            }]
        }]
    });
    let org = p.prosemirror_to_org(&doc).unwrap();
    let back = p.org_to_prosemirror(&org).unwrap();
    let cells = back["content"][0]["content"][0]["content"]
        .as_array()
        .unwrap();
    assert_eq!(cells.len(), 1, "pipe split the cell: {org:?}");
    assert_eq!(cells[0]["content"][0]["text"], "a | b");
}

#[cfg(feature = "orgmode")]
#[test]
fn org_ordered_list_start_is_preserved_on_export() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "orderedList",
            "attrs": { "order": 5 },
            "content": [
                { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "five" }] }] },
                { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "six" }] }] }
            ]
        }]
    });
    let org = p.prosemirror_to_org(&doc).unwrap();
    assert!(org.contains("5. five"), "start lost: {org:?}");
    assert!(org.contains("6. six"), "start lost: {org:?}");
}

#[cfg(feature = "orgmode")]
#[test]
fn org_code_block_body_cannot_close_block_early() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "codeBlock",
            "attrs": { "language": "text" },
            "content": [{ "type": "text", "text": "safe\n#+END_SRC\n* injected" }]
        }]
    });
    let org = p.prosemirror_to_org(&doc).unwrap();
    let back = p.org_to_prosemirror(&org).unwrap();
    let blocks = back["content"].as_array().unwrap();

    assert_eq!(blocks.len(), 1, "source block closed early: {org:?}");
    assert_eq!(blocks[0]["type"], "codeBlock");
    assert_eq!(
        blocks[0]["content"][0]["text"],
        "safe\n#+END_SRC\n* injected"
    );
}

#[cfg(feature = "orgmode")]
#[test]
fn org_heading_with_embedded_newline_stays_a_single_heading() {
    let p = pm();
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "heading",
            "attrs": { "level": 2 },
            "content": [{ "type": "text", "text": "a\n* injected" }]
        }]
    });
    let org = p.prosemirror_to_org(&doc).unwrap();
    let back = p.org_to_prosemirror(&org).unwrap();
    let blocks = back["content"].as_array().unwrap();

    assert_eq!(blocks.len(), 1, "heading split into blocks: {org:?}");
    assert_eq!(blocks[0]["type"], "heading");
    assert_eq!(extract_plain_text(&back), "a * injected");
}

#[cfg(feature = "orgmode")]
#[test]
fn org_paragraph_line_starts_cannot_become_structure() {
    let p = pm();
    let text = "#+BEGIN_SRC\n* heading\n- item\n10. ordered\n| a | b |\n-----";
    let doc = json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": [{ "type": "text", "text": text }]
        }]
    });
    let org = p.prosemirror_to_org(&doc).unwrap();
    let back = p.org_to_prosemirror(&org).unwrap();
    let blocks = back["content"].as_array().unwrap();

    assert!(
        blocks.iter().all(|block| block["type"] == "paragraph"),
        "paragraph text became structure: {org:?}"
    );
    assert_eq!(extract_plain_text(&back), text);
}

#[cfg(feature = "orgmode")]
#[test]
fn org_inline_code_is_literal() {
    let p = pm();
    let doc = p.org_to_prosemirror("call ~a/b~ now").unwrap();
    let inlines = doc["content"][0]["content"].as_array().unwrap();
    // The code span is literal: the inner '/' does not start italic.
    let code = inlines
        .iter()
        .find(|n| n["marks"][0]["type"] == "code")
        .expect("a code run");
    assert_eq!(code["text"], "a/b");
    assert_eq!(code["marks"].as_array().unwrap().len(), 1);
    // Trailing text carries no marks.
    let last = inlines.last().unwrap();
    assert_eq!(last["text"], " now");
    assert!(last.get("marks").is_none());
}

#[cfg(feature = "orgmode")]
#[test]
fn org_marker_in_identifier_or_arithmetic_is_literal() {
    let p = pm();
    let underscores = p.org_to_prosemirror("the some_var_name thing").unwrap();
    assert_eq!(extract_plain_text(&underscores), "the some_var_name thing");
    let inlines = underscores["content"][0]["content"].as_array().unwrap();
    assert!(
        inlines.iter().all(|n| n.get("marks").is_none()),
        "spurious mark"
    );

    let arithmetic = p.org_to_prosemirror("compute 2 * 3 * 4 result").unwrap();
    let inlines = arithmetic["content"][0]["content"].as_array().unwrap();
    assert!(
        inlines.iter().all(|n| n.get("marks").is_none()),
        "spurious bold"
    );
}

#[cfg(feature = "orgmode")]
#[test]
fn org_link_description_cannot_inject_a_second_link() {
    let p = pm();
    let doc = link_doc("text", "http://x][evil]] extra");
    let org = p.prosemirror_to_org(&doc).unwrap();
    let back = p.org_to_prosemirror(&org).unwrap();
    let inlines = back["content"][0]["content"].as_array().unwrap();
    // Exactly one link survives; no second link is injected via `]]`.
    let links = inlines
        .iter()
        .filter(|n| n["marks"][0]["type"] == "link")
        .count();
    assert_eq!(links, 1, "link breakout produced extra links: {org:?}");
}
