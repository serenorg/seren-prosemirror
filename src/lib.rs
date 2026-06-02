//! ProseMirror JSON format handling and conversion to/from Markdown and Org-mode.
//!
//! Shared library used by Seren projects.
//!
//! # URI Scheme
//!
//! Attachment references use a configurable URI scheme. Create a [`ProseMirror`]
//! instance with your project's scheme:
//!
//! ```
//! use seren_prosemirror::ProseMirror;
//!
//! let pm = ProseMirror::new("seren-bounty://attachment/");
//! let doc = pm.markdown_to_prosemirror("Hello **world**").unwrap();
//! ```
//!
//! # Org-mode support
//!
//! Enable the `orgmode` feature for `prosemirror_to_org` and `org_to_prosemirror`.

mod attachment;
mod error;
mod markdown;
#[cfg(feature = "orgmode")]
mod orgmode;
mod schema;
mod text;

use serde_json::{Value, json};

pub use error::ProseMirrorError;
pub use schema::validate_document;
pub use text::extract_plain_text;

/// Maximum node nesting depth the converters and validator descend into,
/// bounding recursion on deeply nested documents.
pub(crate) const MAX_DEPTH: usize = 64;

/// ProseMirror converter parameterized by attachment URI scheme.
pub struct ProseMirror {
    attachment_uri_scheme: String,
}

impl ProseMirror {
    /// Create a new converter with the given attachment URI scheme.
    ///
    /// The scheme should include the trailing slash, e.g. `"seren-bounty://attachment/"`.
    pub fn new(attachment_uri_scheme: &str) -> Self {
        Self {
            attachment_uri_scheme: attachment_uri_scheme.to_string(),
        }
    }

    /// Convert GitHub Flavored Markdown to ProseMirror JSON.
    pub fn markdown_to_prosemirror(&self, markdown: &str) -> Result<Value, ProseMirrorError> {
        markdown::markdown_to_prosemirror(markdown, &self.attachment_uri_scheme)
    }

    /// Convert ProseMirror JSON to GitHub Flavored Markdown.
    ///
    /// # Security
    ///
    /// Link `href` and image `src` values are escaped so they cannot break out
    /// of Markdown link syntax, but their URL scheme is passed through
    /// unchanged: `javascript:` and `data:` URLs are **not** removed. The
    /// produced Markdown is not safe to render as trusted HTML on its own;
    /// sanitize URLs (e.g. with a scheme allowlist) downstream before display.
    pub fn prosemirror_to_markdown(&self, doc: &Value) -> Result<String, ProseMirrorError> {
        markdown::prosemirror_to_markdown(doc, &self.attachment_uri_scheme)
    }

    /// Convert ProseMirror JSON to Org-mode format.
    ///
    /// # Security
    ///
    /// As with [`Self::prosemirror_to_markdown`], link targets are structurally
    /// escaped but their URL scheme is not filtered; sanitize URLs downstream
    /// before rendering the output as trusted HTML.
    #[cfg(feature = "orgmode")]
    pub fn prosemirror_to_org(&self, doc: &Value) -> Result<String, ProseMirrorError> {
        orgmode::prosemirror_to_org(doc, &self.attachment_uri_scheme)
    }

    /// Convert Org-mode to ProseMirror JSON (basic support).
    #[cfg(feature = "orgmode")]
    pub fn org_to_prosemirror(&self, org: &str) -> Result<Value, ProseMirrorError> {
        orgmode::org_to_prosemirror(org, &self.attachment_uri_scheme)
    }
}

/// Create an empty ProseMirror document.
pub fn empty_document() -> Value {
    json!({
        "type": "doc",
        "content": [{
            "type": "paragraph",
            "content": []
        }]
    })
}

#[cfg(test)]
mod tests;
