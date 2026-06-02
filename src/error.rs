use thiserror::Error;

/// Errors produced while converting or validating ProseMirror documents.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProseMirrorError {
    /// A node, mark, or document field has an invalid shape.
    #[error("Invalid document structure: {0}")]
    InvalidStructure(String),
    /// The document contains a node outside the supported shared schema.
    #[error("Unsupported node type: {0}")]
    UnsupportedNode(String),
    /// The document contains a mark outside the supported shared schema.
    #[error("Unsupported mark type: {0}")]
    UnsupportedMark(String),
    /// Parsing failed before a ProseMirror document could be produced.
    #[error("Parse error: {0}")]
    ParseError(String),
}
