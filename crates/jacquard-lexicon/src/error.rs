use miette::{Diagnostic, SourceSpan};
use std::io;
use std::path::PathBuf;
use thiserror::Error;

fn format_parse_error(path: &PathBuf, json_path: Option<&str>, message: &str) -> String {
    match json_path {
        Some(jp) if !jp.is_empty() => {
            format!(
                "failed to parse lexicon {}: at {}: {}",
                path.display(),
                jp,
                message
            )
        }
        _ => format!("failed to parse lexicon {}: {}", path.display(), message),
    }
}

/// Errors that can occur during lexicon code generation
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum CodegenError {
    /// IO error when reading lexicon files
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Failed to parse lexicon JSON
    #[error("{}", format_parse_error(path, json_path.as_deref(), message))]
    #[diagnostic(
        code(lexicon::parse_error),
        help("Check that the lexicon file is valid JSON and follows the lexicon schema")
    )]
    ParseError {
        /// Path to the file that failed to parse
        path: PathBuf,
        /// JSON path where the error occurred (from serde_path_to_error)
        json_path: Option<String>,
        /// The underlying error message
        message: String,
        /// Source text that failed to parse
        #[source_code]
        src: Option<String>,
        /// Location of the error in the source
        #[label("parse error here")]
        span: Option<SourceSpan>,
    },

    /// Name collision
    #[error("Name collision: {name}")]
    #[diagnostic(
        code(lexicon::name_collision),
        help(
            "Multiple types would generate the same Rust identifier. Module paths will disambiguate."
        )
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

    /// Failed to parse generated tokens back into syn AST
    #[error("Failed to parse generated code for {path:?}")]
    #[diagnostic(code(lexicon::token_parse_error))]
    TokenParseError {
        path: PathBuf,
        #[source]
        source: syn::Error,
        tokens: String,
    },

    /// Unsupported lexicon feature
    #[error("Unsupported: {message}")]
    #[diagnostic(
        code(lexicon::unsupported),
        help("This lexicon feature is not yet supported by code generation")
    )]
    Unsupported {
        /// Description of the unsupported feature
        message: String,
        /// NSID of the lexicon containing the unsupported feature
        nsid: Option<String>,
        /// Definition name if applicable
        def_name: Option<String>,
    },

    /// Failed to parse generated path string
    #[error("Failed to parse path '{path_str}'")]
    #[diagnostic(code(lexicon::path_parse_error))]
    PathParseError {
        path_str: String,
        #[source]
        source: syn::Error,
    },
}

impl CodegenError {
    /// Create a parse error with context
    pub fn parse_error(message: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self::ParseError {
            path: path.into(),
            json_path: None,
            message: message.into(),
            src: None,
            span: None,
        }
    }

    /// Create a parse error with source text and JSON path
    pub fn parse_error_with_context(
        message: impl Into<String>,
        path: impl Into<PathBuf>,
        json_path: Option<String>,
        src: String,
    ) -> Self {
        Self::ParseError {
            path: path.into(),
            json_path,
            message: message.into(),
            src: Some(src),
            span: None,
        }
    }

    /// Create an unsupported feature error
    pub fn unsupported(
        message: impl Into<String>,
        nsid: impl Into<String>,
        def_name: Option<impl Into<String>>,
    ) -> Self {
        Self::Unsupported {
            message: message.into(),
            nsid: Some(nsid.into()),
            def_name: def_name.map(|s| s.into()),
        }
    }
}

/// Result type for codegen operations
pub type Result<T> = std::result::Result<T, CodegenError>;
