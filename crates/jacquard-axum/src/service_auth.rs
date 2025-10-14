//! Service authentication extractor and middleware
//!
//! # Example
//!
//! ```no_run
//! use axum::{Router, routing::get};
//! use jacquard_axum::service_auth::{ServiceAuthConfig, ExtractServiceAuth};
//! use jacquard_identity::JacquardResolver;
//! use jacquard_identity::resolver::ResolverOptions;
//! use jacquard_common::types::string::Did;
//!
//! async fn handler(
//!     ExtractServiceAuth(auth): ExtractServiceAuth,
//! ) -> String {
//!     format!("Authenticated as {}", auth.did())
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let resolver = JacquardResolver::new(
//!         reqwest::Client::new(),
//!         ResolverOptions::default(),
//!     );
//!     let config = ServiceAuthConfig::new(
//!         Did::new_static("did:web:feedgen.example.com").unwrap(),
//!         resolver,
//!     );
//!
//!     let app = Router::new()
//!         .route("/xrpc/app.bsky.feed.getFeedSkeleton", get(handler))
//!         .with_state(config);
//!
//!     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
//!         .await
//!         .unwrap();
//!     axum::serve(listener, app).await.unwrap();
//! }
//! ```

use axum::{
    Json,
    extract::FromRequestParts,
    http::{HeaderValue, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jacquard_common::{
    CowStr, IntoStatic,
    service_auth::{self, PublicKey},
    types::{
        did_doc::VerificationMethod,
        string::{Did, Nsid},
    },
};
use jacquard_identity::resolver::IdentityResolver;
use serde_json::json;
use std::sync::Arc;
use thiserror::Error;

/// Trait for providing service authentication configuration.
///
/// This trait allows custom state types to provide service auth configuration
/// without requiring `ServiceAuthConfig<R>` directly.
pub trait ServiceAuth {
    /// The identity resolver type
    type Resolver: IdentityResolver;

    /// Get the service DID (expected audience)
    fn service_did(&self) -> &Did<'_>;

    /// Get a reference to the identity resolver
    fn resolver(&self) -> &Self::Resolver;

    /// Whether to require the `lxm` (method binding) field
    fn require_lxm(&self) -> bool;
}

/// Configuration for service auth verification.
///
/// This should be stored in your Axum app state and will be extracted
/// by the `ExtractServiceAuth` extractor.
pub struct ServiceAuthConfig<R> {
    /// The DID of your service (the expected audience)
    service_did: Did<'static>,
    /// Identity resolver for fetching DID documents
    resolver: Arc<R>,
    /// Whether to require the `lxm` (method binding) field
    require_lxm: bool,
}

impl<R> Clone for ServiceAuthConfig<R> {
    fn clone(&self) -> Self {
        Self {
            service_did: self.service_did.clone(),
            resolver: Arc::clone(&self.resolver),
            require_lxm: self.require_lxm,
        }
    }
}

impl<R: IdentityResolver> ServiceAuthConfig<R> {
    /// Create a new service auth config.
    ///
    /// This enables `lxm` (method binding). If you need backward compatibility,
    /// use `ServiceAuthConfig::new_legacy()`
    pub fn new(service_did: Did<'static>, resolver: R) -> Self {
        Self {
            service_did,
            resolver: Arc::new(resolver),
            require_lxm: true,
        }
    }

    /// Create a new service auth config.
    ///
    /// `lxm` (method binding) is disabled for backwards compatibility
    pub fn new_legacy(service_did: Did<'static>, resolver: R) -> Self {
        Self {
            service_did,
            resolver: Arc::new(resolver),
            require_lxm: false,
        }
    }

    /// Set whether to require the `lxm` field (method binding).
    ///
    /// When enabled, the JWT must contain an `lxm` field matching the requested endpoint.
    /// This prevents token reuse across different methods.
    pub fn require_lxm(mut self, require: bool) -> Self {
        self.require_lxm = require;
        self
    }

    /// Get the service DID.
    pub fn service_did(&self) -> &Did<'static> {
        &self.service_did
    }

    /// Get a reference to the identity resolver.
    pub fn resolver(&self) -> &R {
        &self.resolver
    }
}

impl<R: IdentityResolver> ServiceAuth for ServiceAuthConfig<R> {
    type Resolver = R;

    fn service_did(&self) -> &Did<'_> {
        &self.service_did
    }

    fn resolver(&self) -> &Self::Resolver {
        &self.resolver
    }

    fn require_lxm(&self) -> bool {
        self.require_lxm
    }
}

/// Verified service authentication information.
///
/// This is the result of successfully verifying a service auth JWT.
/// This type is extracted by the `ExtractServiceAuth` extractor.
#[derive(Debug, Clone, jacquard_derive::IntoStatic)]
pub struct VerifiedServiceAuth<'a> {
    /// The authenticated user's DID (from `iss` claim)
    did: Did<'a>,
    /// The audience (should match your service DID)
    aud: Did<'a>,
    /// The lexicon method NSID, if present
    lxm: Option<Nsid<'a>>,
    /// JWT ID (nonce), if present
    jti: Option<CowStr<'a>>,
}

impl<'a> VerifiedServiceAuth<'a> {
    /// Get the authenticated user's DID.
    pub fn did(&self) -> &Did<'a> {
        &self.did
    }

    /// Get the audience (your service DID).
    pub fn aud(&self) -> &Did<'a> {
        &self.aud
    }

    /// Get the lexicon method NSID, if present.
    pub fn lxm(&self) -> Option<&Nsid<'a>> {
        self.lxm.as_ref()
    }

    /// Get the JWT ID (nonce), if present.
    ///
    /// You can use this for replay protection by tracking seen JTIs
    /// until their expiration time.
    pub fn jti(&self) -> Option<&str> {
        self.jti.as_ref().map(|j| j.as_ref())
    }
}

/// Axum extractor for service authentication.
///
/// This extracts and verifies a service auth JWT from the Authorization header,
/// resolving the issuer's DID to verify the signature.
///
/// # Example
///
/// ```no_run
/// use axum::{Router, routing::get};
/// use jacquard_axum::service_auth::{ServiceAuthConfig, ExtractServiceAuth};
/// use jacquard_identity::JacquardResolver;
/// use jacquard_identity::resolver::ResolverOptions;
/// use jacquard_common::types::string::Did;
///
/// async fn handler(
///     ExtractServiceAuth(auth): ExtractServiceAuth,
/// ) -> String {
///     format!("Authenticated as {}", auth.did())
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let resolver = JacquardResolver::new(
///         reqwest::Client::new(),
///         ResolverOptions::default(),
///     );
///     let config = ServiceAuthConfig::new(
///         Did::new_static("did:web:feedgen.example.com").unwrap(),
///         resolver,
///     );
///
///     let app = Router::new()
///         .route("/xrpc/app.bsky.feed.getFeedSkeleton", get(handler))
///         .with_state(config);
///
///     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
///         .await
///         .unwrap();
///     axum::serve(listener, app).await.unwrap();
/// }
/// ```
pub struct ExtractServiceAuth(pub VerifiedServiceAuth<'static>);

/// Errors that can occur during service auth verification.
#[derive(Debug, Error, miette::Diagnostic)]
pub enum ServiceAuthError {
    /// Authorization header is missing
    #[error("missing Authorization header")]
    MissingAuthHeader,

    /// Authorization header is malformed (not "Bearer `token`")
    #[error("invalid Authorization header format")]
    InvalidAuthHeader,

    /// JWT parsing or verification failed
    #[error("JWT verification failed: {0}")]
    JwtError(#[from] service_auth::ServiceAuthError),

    /// DID resolution failed
    #[error("failed to resolve DID {did}: {source}")]
    DidResolutionFailed {
        did: Did<'static>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// No valid signing key found in DID document
    #[error("no valid signing key found in DID document for {0}")]
    NoSigningKey(Did<'static>),

    /// Method binding required but missing
    #[error("lxm (method binding) is required but missing from token")]
    MethodBindingRequired,

    /// Invalid key format
    #[error("invalid key format: {0}")]
    InvalidKey(String),
}

impl IntoResponse for ServiceAuthError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            ServiceAuthError::MissingAuthHeader => {
                (StatusCode::UNAUTHORIZED, "AuthMissing", self.to_string())
            }
            ServiceAuthError::InvalidAuthHeader => {
                (StatusCode::UNAUTHORIZED, "AuthMissing", self.to_string())
            }
            ServiceAuthError::JwtError(_) => (
                StatusCode::UNAUTHORIZED,
                "AuthenticationRequired",
                self.to_string(),
            ),
            ServiceAuthError::DidResolutionFailed { .. } => (
                StatusCode::UNAUTHORIZED,
                "AuthenticationRequired",
                self.to_string(),
            ),
            ServiceAuthError::NoSigningKey(_) => (
                StatusCode::UNAUTHORIZED,
                "AuthenticationRequired",
                self.to_string(),
            ),
            ServiceAuthError::MethodBindingRequired => (
                StatusCode::UNAUTHORIZED,
                "AuthenticationRequired",
                self.to_string(),
            ),
            ServiceAuthError::InvalidKey(_) => (
                StatusCode::UNAUTHORIZED,
                "AuthenticationRequired",
                self.to_string(),
            ),
        };

        tracing::warn!("Service auth failed: {}", message);

        (
            status,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            Json(json!({
                "error": error_code,
                "message": message,
            })),
        )
            .into_response()
    }
}

impl<S> FromRequestParts<S> for ExtractServiceAuth
where
    S: ServiceAuth + Send + Sync,
    S::Resolver: Send + Sync,
{
    type Rejection = ServiceAuthError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            // Extract Authorization header
            let auth_header = parts
                .headers
                .get(header::AUTHORIZATION)
                .ok_or(ServiceAuthError::MissingAuthHeader)?;

            // Parse Bearer token
            let auth_str = auth_header
                .to_str()
                .map_err(|_| ServiceAuthError::InvalidAuthHeader)?;

            let token = auth_str
                .strip_prefix("Bearer ")
                .ok_or(ServiceAuthError::InvalidAuthHeader)?;

            // Parse JWT
            let parsed = service_auth::parse_jwt(token)?;

            // Get claims for DID resolution
            let claims = parsed.claims();

            // Resolve DID to get signing key (do this before checking claims)
            let did_doc = state
                .resolver()
                .resolve_did_doc(&claims.iss)
                .await
                .map_err(|e| ServiceAuthError::DidResolutionFailed {
                    did: claims.iss.clone().into_static(),
                    source: Box::new(e),
                })?;

            // Parse the DID document response to get verification methods
            let doc = did_doc
                .parse()
                .map_err(|e| ServiceAuthError::DidResolutionFailed {
                    did: claims.iss.clone().into_static(),
                    source: Box::new(e),
                })?;

            // Extract signing key from DID document
            let verification_methods = doc
                .verification_method
                .as_deref()
                .ok_or_else(|| ServiceAuthError::NoSigningKey(claims.iss.clone().into_static()))?;

            let signing_key = extract_signing_key(verification_methods)
                .ok_or_else(|| ServiceAuthError::NoSigningKey(claims.iss.clone().into_static()))?;

            // Verify signature FIRST - if this fails, nothing else matters
            service_auth::verify_signature(&parsed, &signing_key)?;

            // Now validate claims (audience, expiration, etc.)
            claims.validate(state.service_did())?;

            // Check method binding if required
            if state.require_lxm() && claims.lxm.is_none() {
                return Err(ServiceAuthError::MethodBindingRequired);
            }

            // All checks passed - return verified auth
            Ok(ExtractServiceAuth(VerifiedServiceAuth {
                did: claims.iss.clone().into_static(),
                aud: claims.aud.clone().into_static(),
                lxm: claims.lxm.as_ref().map(|l| l.clone().into_static()),
                jti: claims.jti.as_ref().map(|j| j.clone().into_static()),
            }))
        }
    }
}

/// Extract the signing key from a DID document's verification methods.
///
/// This looks for a key with type "atproto" or the first available key
/// if no atproto-specific key is found.
fn extract_signing_key(methods: &[VerificationMethod]) -> Option<PublicKey> {
    // First try to find an atproto-specific key
    let atproto_method = methods
        .iter()
        .find(|m| m.r#type.as_ref() == "Multikey" || m.r#type.as_ref() == "atproto");

    let method = atproto_method.or_else(|| methods.first())?;

    // Parse the multikey
    let public_key_multibase = method.public_key_multibase.as_ref()?;

    // Decode multibase
    let (_, key_bytes) = multibase::decode(public_key_multibase.as_ref()).ok()?;

    // First two bytes are the multicodec prefix
    if key_bytes.len() < 2 {
        return None;
    }

    let codec = &key_bytes[..2];
    let key_material = &key_bytes[2..];

    match codec {
        // p256-pub (0x1200)
        [0x80, 0x24] => PublicKey::from_p256_bytes(key_material).ok(),
        // secp256k1-pub (0xe7)
        [0xe7, 0x01] => PublicKey::from_k256_bytes(key_material).ok(),
        _ => None,
    }
}

/// Middleware for verifying service authentication on all requests.
///
/// This middleware extracts and verifies the service auth JWT, then adds the
/// `VerifiedServiceAuth` to request extensions for downstream handlers to access.
///
/// # Example
///
/// ```no_run
/// use axum::{Router, routing::get, middleware, Extension};
/// use jacquard_axum::service_auth::{ServiceAuthConfig, service_auth_middleware};
/// use jacquard_identity::JacquardResolver;
/// use jacquard_identity::resolver::ResolverOptions;
/// use jacquard_common::types::string::Did;
///
/// async fn handler(
///     Extension(auth): Extension<jacquard_axum::service_auth::VerifiedServiceAuth<'static>>,
/// ) -> String {
///     format!("Authenticated as {}", auth.did())
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let resolver = JacquardResolver::new(
///         reqwest::Client::new(),
///         ResolverOptions::default(),
///     );
///     let config = ServiceAuthConfig::new(
///         Did::new_static("did:web:feedgen.example.com").unwrap(),
///         resolver,
///     );
///
///     let app = Router::new()
///         .route("/xrpc/app.bsky.feed.getFeedSkeleton", get(handler))
///         .layer(middleware::from_fn_with_state(
///             config.clone(),
///             service_auth_middleware::<ServiceAuthConfig<JacquardResolver>>,
///         ))
///         .with_state(config);
///
///     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
///         .await
///         .unwrap();
///     axum::serve(listener, app).await.unwrap();
/// }
/// ```
pub async fn service_auth_middleware<S>(
    state: axum::extract::State<S>,
    mut req: axum::extract::Request,
    next: Next,
) -> Result<Response, ServiceAuthError>
where
    S: ServiceAuth + Send + Sync + Clone,
    S::Resolver: Send + Sync,
{
    // Extract auth from request parts
    let (mut parts, body) = req.into_parts();
    let ExtractServiceAuth(auth) =
        ExtractServiceAuth::from_request_parts(&mut parts, &state.0).await?;

    // Add auth to extensions
    parts.extensions.insert(auth);

    // Reconstruct request and continue
    req = axum::extract::Request::from_parts(parts, body);
    Ok(next.run(req).await)
}
