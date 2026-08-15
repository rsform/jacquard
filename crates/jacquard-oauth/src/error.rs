use jacquard_common::session::SessionStoreError;
use miette::Diagnostic;
#[cfg(feature = "scope-check")]
use smol_str::SmolStr;

use crate::request::RequestError;
use crate::resolver::ResolverError;

/// High-level errors emitted by OAuth helpers.
#[derive(Debug, thiserror::Error, Diagnostic)]
#[non_exhaustive]
pub enum OAuthError {
    /// An error occurred during identity or metadata resolution.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::resolver))]
    Resolver(#[from] ResolverError),

    /// An error occurred while making an OAuth HTTP request.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::request))]
    Request(#[from] RequestError),

    /// An error occurred reading or writing session state.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::storage))]
    Storage(#[from] SessionStoreError),

    /// An error occurred during DPoP proof generation or validation.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::dpop))]
    Dpop(#[from] crate::dpop::DpopError),

    /// An error occurred with the client's key set.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::keyset))]
    Keyset(#[from] crate::keyset::Error),

    /// An ATProto-specific OAuth error (e.g. scope validation, client ID).
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::atproto))]
    Atproto(#[from] crate::atproto::Error),

    /// An error occurred managing or refreshing an OAuth session.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::session))]
    Session(#[from] crate::session::Error),

    /// A JSON serialization or deserialization error.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::serde_json))]
    SerdeJson(#[from] serde_json::Error),

    /// A URI parse error.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::url))]
    Url(#[from] jacquard_common::deps::fluent_uri::ParseError),

    /// A form (URL-encoded) serialization error.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::form))]
    Form(#[from] serde_html_form::ser::Error),

    /// Invalid OAuth helper input.
    #[error("invalid OAuth request: {0}")]
    #[diagnostic(code(jacquard_oauth::invalid_request))]
    InvalidRequest(String),

    /// An XRPC request made as part of an OAuth flow failed.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::xrpc))]
    Xrpc(#[from] jacquard_common::error::ClientError),

    /// The space authority rejected a credential-mint request.
    #[error("space credential request failed: {0}")]
    #[diagnostic(code(jacquard_oauth::space_mint))]
    SpaceMint(String),

    /// An error validating an authorization callback.
    #[error(transparent)]
    #[diagnostic(code(jacquard_oauth::callback))]
    Callback(#[from] CallbackError),

    /// An error occurred checking request scope permissions.
    #[cfg(feature = "scope-check")]
    #[error(transparent)]
    #[diagnostic(transparent)]
    ScopeCheck(#[from] ScopeError),
}

/// Typed callback validation errors (redirect handling).
#[derive(Debug, thiserror::Error, Diagnostic)]
#[non_exhaustive]
pub enum CallbackError {
    /// The `state` parameter was absent from the authorization callback.
    ///
    /// State is required to prevent CSRF attacks per RFC 6749 §10.12.
    #[error("missing state parameter in callback")]
    #[diagnostic(code(jacquard_oauth::callback::missing_state))]
    MissingState,
    /// The `iss` (issuer) parameter was absent from the authorization callback.
    ///
    /// RFC 9207 requires `iss` to be present so that clients can reject
    /// mix-up attacks from malicious authorization servers.
    #[error("missing `iss` parameter")]
    #[diagnostic(code(jacquard_oauth::callback::missing_iss))]
    MissingIssuer,
    /// The issuer in the callback did not match the expected authorization server.
    #[error("issuer mismatch: expected {expected}, got {got}")]
    #[diagnostic(code(jacquard_oauth::callback::issuer_mismatch))]
    IssuerMismatch {
        /// The issuer that was expected.
        expected: String,
        /// The issuer that was actually present in the callback.
        got: String,
    },
    /// The authorization request timed out before a callback was received.
    #[error("timeout")]
    #[diagnostic(code(jacquard_oauth::callback::timeout))]
    Timeout,
    /// The local loopback callback server could not start or accept callbacks.
    #[error("loopback callback server error: {0}")]
    #[diagnostic(code(jacquard_oauth::callback::loopback_server))]
    LoopbackServer(String),
    /// An error occurred resolving permission sets during session creation.
    #[cfg(feature = "scope-check")]
    #[error("scope resolution failed: {detail}")]
    #[diagnostic(code(jacquard_oauth::callback::scope_resolution))]
    ScopeResolution {
        /// Description of the resolution failure.
        detail: String,
    },
}

/// Error returned when a request's required scope is not covered by the session's granted scopes.
#[cfg(feature = "scope-check")]
#[derive(Debug, thiserror::Error, Diagnostic)]
#[error("request to `{nsid}` not permitted: no granted scope covers this endpoint")]
#[diagnostic(
    code(jacquard_oauth::scope_check),
    help("granted scopes: {granted}. The endpoint requires an `rpc:` scope covering `{nsid}`.")
)]
pub struct ScopeError {
    /// The NSID of the XRPC method that was denied.
    pub nsid: SmolStr,
    /// Human-readable summary of the granted scopes for diagnostic output.
    pub granted: SmolStr,
}

/// Convenience alias for `Result<T, OAuthError>`.
pub type Result<T> = core::result::Result<T, OAuthError>;
