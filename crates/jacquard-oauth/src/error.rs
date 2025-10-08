use jacquard_common::session::SessionStoreError;
use miette::Diagnostic;

use crate::request::RequestError;
use crate::resolver::ResolverError;

/// High-level errors emitted by OAuth helpers.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum OAuthError {
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::resolver))]
    Resolver(#[from] ResolverError),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::request))]
    Request(#[from] RequestError),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::storage))]
    Storage(#[from] SessionStoreError),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::dpop))]
    Dpop(#[from] crate::dpop::Error),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::keyset))]
    Keyset(#[from] crate::keyset::Error),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::atproto))]
    Atproto(#[from] crate::atproto::Error),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::session))]
    Session(#[from] crate::session::Error),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::serde_json))]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::url))]
    Url(#[from] url::ParseError),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::form))]
    Form(#[from] serde_html_form::ser::Error),

    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::callback))]
    Callback(#[from] CallbackError),
}

/// Typed callback validation errors (redirect handling).
#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum CallbackError {
    #[error("missing state parameter in callback")] 
    #[diagnostic(code(jacquard_oauth::callback::missing_state))]
    MissingState,
    #[error("missing `iss` parameter")] 
    #[diagnostic(code(jacquard_oauth::callback::missing_iss))]
    MissingIssuer,
    #[error("issuer mismatch: expected {expected}, got {got}")]
    #[diagnostic(code(jacquard_oauth::callback::issuer_mismatch))]
    IssuerMismatch { expected: String, got: String },
}

pub type Result<T> = core::result::Result<T, OAuthError>;
