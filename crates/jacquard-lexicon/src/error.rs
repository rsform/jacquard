use miette::{Diagnostic, SourceSpan};
use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during lexicon code generation
#[derive(Debug, Error, Diagnostic)]
pub enum CodegenError {
    /// IO error when reading lexicon files
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Failed to parse lexicon JSON
    #[error("Failed to parse lexicon JSON in {}", path.display())]
    #[diagnostic(
        code(lexicon::parse_error),
        help("Check that the lexicon file is valid JSON and follows the lexicon schema")
    )]
    ParseError {
        #[source]
        source: serde_json::Error,
        /// Path to the file that failed to parse
        path: PathBuf,
        /// Source text that failed to parse
        #[source_code]
        src: Option<String>,
        /// Location of the error in the source
        #[label("parse error here")]
        span: Option<SourceSpan>,
    },

    /// Reference to non-existent lexicon or def
    #[error("Reference to unknown type: {ref_string}")]
    #[diagnostic(
        code(lexicon::unknown_ref),
        help("Add the referenced lexicon to your corpus or use Data<'a> as a fallback type")
    )]
    UnknownRef {
        /// The ref string that couldn't be resolved
        ref_string: String,
        /// NSID of lexicon containing the ref
        lexicon_nsid: String,
        /// Def name containing the ref
        def_name: String,
        /// Field path containing the ref
        field_path: String,
    },

    /// Circular reference detected in type definitions
    #[error("Circular reference detected")]
    #[diagnostic(
        code(lexicon::circular_ref),
        help("The code generator uses Box<T> for union variants to handle circular references")
    )]
    CircularRef {
        /// The ref string that forms a cycle
        ref_string: String,
        /// The cycle path
        cycle: Vec<String>,
    },

    /// Invalid lexicon structure
    #[error("Invalid lexicon: {message}")]
    #[diagnostic(code(lexicon::invalid))]
    InvalidLexicon {
        message: String,
        /// NSID of the invalid lexicon
        lexicon_nsid: String,
    },

    /// Unsupported lexicon feature
    #[error("Unsupported feature: {feature}")]
    #[diagnostic(
        code(lexicon::unsupported),
        help("This lexicon feature is not yet supported by the code generator")
    )]
    Unsupported {
        /// Description of the unsupported feature
        feature: String,
        /// NSID of lexicon containing the feature
        lexicon_nsid: String,
        /// Optional suggestion for workaround
        suggestion: Option<String>,
    },

    /// Name collision
    #[error("Name collision: {name}")]
    #[diagnostic(
        code(lexicon::name_collision),
        help("Multiple types would generate the same Rust identifier. Module paths will disambiguate.")
    )]
    NameCollision {
        /// The colliding name
        name: String,
        /// NSIDs that would generate this name
        nsids: Vec<String>,
    },

    /// Code formatting error
    #[error("Failed to format generated code")]
    #[diagnostic(code(lexicon::format_error))]
    FormatError {
        #[source]
        source: syn::Error,
    },

    /// Generic error with context
    #[error("{message}")]
    #[diagnostic(code(lexicon::error))]
    Other {
        message: String,
        /// Optional source error
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl CodegenError {
    /// Create a parse error with context
    pub fn parse_error(source: serde_json::Error, path: impl Into<PathBuf>) -> Self {
        Self::ParseError {
            source,
            path: path.into(),
            src: None,
            span: None,
        }
    }

    /// Create a parse error with source text
    pub fn parse_error_with_source(
        source: serde_json::Error,
        path: impl Into<PathBuf>,
        src: String,
    ) -> Self {
        // Try to extract error location from serde_json error
        let span = if let Some(line) = source.line().checked_sub(1) {
            let col = source.column().saturating_sub(1);
            // Approximate byte offset (not perfect but good enough for display)
            Some((line * 80 + col, 1).into())
        } else {
            None
        };

        Self::ParseError {
            source,
            path: path.into(),
            src: Some(src),
            span,
        }
    }

    /// Create an unknown ref error
    pub fn unknown_ref(
        ref_string: impl Into<String>,
        lexicon_nsid: impl Into<String>,
        def_name: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self::UnknownRef {
            ref_string: ref_string.into(),
            lexicon_nsid: lexicon_nsid.into(),
            def_name: def_name.into(),
            field_path: field_path.into(),
        }
    }

    /// Create an invalid lexicon error
    pub fn invalid_lexicon(message: impl Into<String>, lexicon_nsid: impl Into<String>) -> Self {
        Self::InvalidLexicon {
            message: message.into(),
            lexicon_nsid: lexicon_nsid.into(),
        }
    }

    /// Create an unsupported feature error
    pub fn unsupported(
        feature: impl Into<String>,
        lexicon_nsid: impl Into<String>,
        suggestion: Option<impl Into<String>>,
    ) -> Self {
        Self::Unsupported {
            feature: feature.into(),
            lexicon_nsid: lexicon_nsid.into(),
            suggestion: suggestion.map(|s| s.into()),
        }
    }
}

/// Result type for codegen operations
pub type Result<T> = std::result::Result<T, CodegenError>;
