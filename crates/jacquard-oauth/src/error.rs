use miette::Diagnostic;
use thiserror::Error;

/// Errors emitted by OAuth helpers.
#[derive(Debug, Error, Diagnostic)]
pub enum OAuthError {
    /// Invalid or unsupported JWK
    #[error("invalid JWK: {0}")]
    #[diagnostic(
        code(jacquard_oauth::jwk),
        help("Ensure EC P-256 JWK with base64url x,y,d values")
    )]
    Jwk(String),
    /// Signing error
    #[error("signing error: {0}")]
    #[diagnostic(
        code(jacquard_oauth::signing),
        help("Check ES256 key material and input payloads")
    )]
    Signing(String),
    /// Serialization error
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::serde))]
    Serde(#[from] serde_json::Error),
    /// URL error
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::url))]
    Url(#[from] url::ParseError),
    /// URL error
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::url))]
    UrlEncoding(#[from] serde_html_form::ser::Error),
    /// PKCE error
    #[error("pkce error: {0}")]
    #[diagnostic(
        code(jacquard_oauth::pkce),
        help("PKCE must use S256; ensure verifier/challenge generated")
    )]
    Pkce(String),
}

pub type Result<T> = core::result::Result<T, OAuthError>;
