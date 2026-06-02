# Seren ProseMirror

Seren ProseMirror is a small Rust library for working with the ProseMirror JSON document shape used by Seren projects. It converts between GitHub Flavored Markdown and ProseMirror JSON, validates supported ProseMirror nodes and marks, and extracts plain text for search and previews.

Org-mode conversion is available behind the optional `orgmode` feature.

## Usage

```rust
use seren_prosemirror::ProseMirror;

fn main() -> Result<(), seren_prosemirror::ProseMirrorError> {
    let pm = ProseMirror::new("seren-notes://attachment/");
    let doc = pm.markdown_to_prosemirror("Hello **world**")?;
    let markdown = pm.prosemirror_to_markdown(&doc)?;

    println!("{markdown}");
    Ok(())
}
```

## Attachment URIs

Attachment links are project-scoped with a caller-provided URI prefix, such as `seren-notes://attachment/` or `seren-bounty://attachment/`. Standalone Markdown links with a matching attachment URI and UUID are upgraded to ProseMirror `attachment` nodes.

```markdown
[report.pdf](seren-notes://attachment/123e4567-e89b-12d3-a456-426614174000)
```

## Features

Markdown, ProseMirror validation, and plain-text extraction are available without enabling any Cargo features.

- `orgmode`: Enables `prosemirror_to_org` and `org_to_prosemirror`.

## Supported Scope

The Markdown converter targets the shared schema used by Seren applications. It supports paragraphs, headings, block quotes, code blocks, lists, task lists, tables, images, attachments, horizontal rules, hard breaks, and common inline marks.

The Org-mode converter is intentionally basic and meant for import/export workflows rather than full Org syntax preservation. On import it does not reconstruct nested or indented list hierarchy: each list line becomes an item of a single top-level list, so ProseMirror -> Org -> ProseMirror round-trips are not hierarchy-preserving for nested lists.

## Validation and limits

Documents are handled as untyped `serde_json::Value`. `validate_document` performs structural checks only: it confirms the root is a `doc`, that every node and mark is one of the supported types, that attribute containers have the right shape, that headings are levels 1 to 6, that ordered-list orders are positive, and that attachments and links carry their required ids/hrefs. It does not sanitize or scheme-check URLs, and it is not required before conversion. Validation and conversion are depth-bounded: documents nested beyond the internal limit are rejected with an error (plain-text extraction instead stops descending) rather than recursing without bound. Out-of-range integer attributes such as heading `level` or list `order` are clamped or saturated during conversion.

## Security

The converters faithfully represent whatever the input document contains. Link `href` and image `src` values are structurally escaped for Markdown/Org link syntax, but their URL **scheme is passed through unchanged**. This crate does not strip `javascript:`, `data:`, or other potentially dangerous schemes, and `validate_document` does not reject them. The Markdown and Org output is therefore not safe to render as trusted HTML on its own: callers that render the output must source URLs from trusted input and/or run the rendered HTML through a sanitizer with a scheme allowlist (for example `ammonia`).
