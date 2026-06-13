//! Service authentication extractor and middleware.
//!
//! Service auth verifies AT Protocol inter-service JWTs. Normal
//! [`ServiceAuthConfig::new`] configurations require `lxm` method binding and,
//! when the `service-auth-replay` feature is enabled, reject missing or replayed
//! `jti` values by default. Use [`ServiceAuthConfig::disable_replay_protection`]
//! only for legacy compatibility.
//!
//! Global service-id allow-lists constrain present `aud` fragments but do not
//! require a fragment. Use [`require_service_id`] as a route layer for endpoints
//! that require a specific `did:web:example.com#service_id` audience fragment.
//!
//! [`ExtractOptionalServiceAuth`] treats only an absent Authorization header as
//! anonymous. Present malformed, invalid, or replayed credentials are rejected.
//!
//! The default replay store is in-memory and per process. Horizontally scaled
//! deployments should provide a shared [`ReplayStore`] implementation.
//! Legacy configs created with [`ServiceAuthConfig::new_legacy`] disable `lxm`
//! and replay requirements.
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
    Extension, Json,
    extract::FromRequestParts,
    http::{HeaderValue, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jacquard_common::deps::smol_str::SmolStr;
use jacquard_common::{
    CowStr, IntoStatic,
    service_auth::{self, PublicKey},
    types::{
        did_doc::VerificationMethod,
        string::{Did, DidService, Nsid},
    },
};
use jacquard_identity::resolver::IdentityResolver;
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Replay key for service auth JWT `jti` replay protection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplayKey {
    /// Issuer DID from the JWT.
    iss: Did,
    /// Full audience, including any service-id fragment.
    aud: DidService,
    /// JWT ID nonce.
    jti: SmolStr,
}

impl ReplayKey {
    /// Create a new replay key.
    pub fn new(iss: Did, aud: DidService, jti: impl Into<SmolStr>) -> Self {
        Self {
            iss,
            aud,
            jti: jti.into(),
        }
    }
}

/// Errors returned by replay stores.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReplayStoreError {
    /// The replay key has already been presented and has not expired.
    #[error("service auth JWT replay detected")]
    Replayed,

    /// The replay store failed.
    #[error("replay store failed: {0}")]
    Store(String),
}

/// Store used to reject replayed service auth JWT IDs.
pub trait ReplayStore: Send + Sync + 'static {
    /// Check whether `key` has been seen, and record it until `expires_at`.
    fn check_and_insert(
        &self,
        key: ReplayKey,
        expires_at: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ReplayStoreError>> + Send + '_>>;
}

/// Replay store that disables replay protection.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopReplayStore;

impl ReplayStore for NoopReplayStore {
    fn check_and_insert(
        &self,
        _key: ReplayKey,
        _expires_at: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ReplayStoreError>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

/// Default in-memory replay store.
#[cfg(feature = "service-auth-replay")]
#[derive(Debug, Clone)]
pub struct InMemoryReplayStore {
    cache: mini_moka::sync::Cache<ReplayKey, i64>,
    lock: Arc<Mutex<()>>,
}

#[cfg(feature = "service-auth-replay")]
impl Default for InMemoryReplayStore {
    fn default() -> Self {
        Self::new(100_000)
    }
}

#[cfg(feature = "service-auth-replay")]
impl InMemoryReplayStore {
    /// Create an in-memory replay store with a maximum key capacity.
    pub fn new(max_capacity: u64) -> Self {
        Self {
            cache: mini_moka::sync::Cache::new(max_capacity),
            lock: Arc::new(Mutex::new(())),
        }
    }
}

#[cfg(feature = "service-auth-replay")]
impl ReplayStore for InMemoryReplayStore {
    fn check_and_insert(
        &self,
        key: ReplayKey,
        expires_at: i64,
    ) -> Pin<Box<dyn Future<Output = Result<(), ReplayStoreError>> + Send + '_>> {
        Box::pin(async move {
            let _guard = self
                .lock
                .lock()
                .map_err(|_| ReplayStoreError::Store("replay store lock poisoned".to_string()))?;
            let now = chrono::Utc::now().timestamp();
            if let Some(existing_expires_at) = self.cache.get(&key) {
                if existing_expires_at > now {
                    return Err(ReplayStoreError::Replayed);
                }
                self.cache.invalidate(&key);
            }
            self.cache.insert(key, expires_at);
            Ok(())
        })
    }
}

/// Trait for providing service authentication configuration.
///
/// This trait allows custom state types to provide service auth configuration
/// without requiring `ServiceAuthConfig<R>` directly.
pub trait ServiceAuth {
    /// The identity resolver type
    type Resolver: IdentityResolver;

    /// Get the service DID (expected audience)
    fn service_did(&self) -> Did<&str>;

    /// Get a reference to the identity resolver
    fn resolver(&self) -> &Self::Resolver;

    /// Whether to require the `lxm` (method binding) field.
    fn require_lxm(&self) -> bool;

    /// Service-id fragments allowed by global validation.
    fn allowed_services(&self) -> &[SmolStr];

    /// Whether replay protection is enabled.
    fn replay_protection_enabled(&self) -> bool;

    /// Replay store used by replay protection.
    fn replay_store(&self) -> &dyn ReplayStore;
}

/// Configuration for service auth verification.
///
/// This should be stored in your Axum app state and will be extracted
/// by the `ExtractServiceAuth` extractor.
pub struct ServiceAuthConfig<R> {
    /// The DID of your service (the expected audience).
    service_did: Did,
    /// Identity resolver for fetching DID documents.
    resolver: Arc<R>,
    /// Whether to require the `lxm` (method binding) field.
    require_lxm: bool,
    /// Globally allowed service-id fragments.
    allowed_services: Vec<SmolStr>,
    /// Replay store used when replay protection is enabled.
    replay_store: Arc<dyn ReplayStore>,
    /// Whether replay protection is enabled.
    replay_protection_enabled: bool,
}

impl<R> Clone for ServiceAuthConfig<R> {
    fn clone(&self) -> Self {
        Self {
            service_did: self.service_did.clone(),
            resolver: Arc::clone(&self.resolver),
            require_lxm: self.require_lxm,
            allowed_services: self.allowed_services.clone(),
            replay_store: Arc::clone(&self.replay_store),
            replay_protection_enabled: self.replay_protection_enabled,
        }
    }
}

fn default_replay_store() -> (Arc<dyn ReplayStore>, bool) {
    #[cfg(feature = "service-auth-replay")]
    {
        (Arc::new(InMemoryReplayStore::default()), true)
    }
    #[cfg(not(feature = "service-auth-replay"))]
    {
        (Arc::new(NoopReplayStore), false)
    }
}

impl<R: IdentityResolver> ServiceAuthConfig<R> {
    /// Create a new service auth config.
    ///
    /// This enables `lxm` (method binding). If you need backward compatibility,
    /// use `ServiceAuthConfig::new_legacy()`
    pub fn new(service_did: Did, resolver: R) -> Self {
        let (replay_store, replay_protection_enabled) = default_replay_store();
        Self {
            service_did,
            resolver: Arc::new(resolver),
            require_lxm: true,
            allowed_services: Vec::new(),
            replay_store,
            replay_protection_enabled,
        }
    }

    /// Create a new service auth config.
    ///
    /// `lxm` (method binding) is disabled for backwards compatibility
    pub fn new_legacy(service_did: Did, resolver: R) -> Self {
        Self {
            service_did,
            resolver: Arc::new(resolver),
            require_lxm: false,
            allowed_services: Vec::new(),
            replay_store: Arc::new(NoopReplayStore),
            replay_protection_enabled: false,
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

    /// Replace the global allowed service-id fragments.
    pub fn with_allowed_services<I, Svc>(mut self, services: I) -> Self
    where
        I: IntoIterator<Item = Svc>,
        Svc: Into<SmolStr>,
    {
        self.allowed_services = services.into_iter().map(Into::into).collect();
        self
    }

    /// Add a single global allowed service-id fragment.
    pub fn allow_service(mut self, service: impl Into<SmolStr>) -> Self {
        self.allowed_services.push(service.into());
        self
    }

    /// Replace the replay store and enable replay protection.
    pub fn with_replay_store(mut self, store: impl ReplayStore) -> Self {
        self.replay_store = Arc::new(store);
        self.replay_protection_enabled = true;
        self
    }

    /// Disable replay protection for legacy compatibility.
    pub fn disable_replay_protection(mut self) -> Self {
        self.replay_store = Arc::new(NoopReplayStore);
        self.replay_protection_enabled = false;
        self
    }

    /// Get the globally allowed service-id fragments.
    pub fn allowed_services(&self) -> &[SmolStr] {
        &self.allowed_services
    }

    /// Get the service DID.
    pub fn service_did(&self) -> Did<&str> {
        self.service_did.borrow()
    }

    /// Get a reference to the identity resolver.
    pub fn resolver(&self) -> &R {
        &self.resolver
    }
}

impl<R: IdentityResolver> ServiceAuth for ServiceAuthConfig<R> {
    type Resolver = R;

    fn service_did(&self) -> Did<&str> {
        self.service_did.borrow()
    }

    fn resolver(&self) -> &Self::Resolver {
        &self.resolver
    }

    fn require_lxm(&self) -> bool {
        self.require_lxm
    }

    fn allowed_services(&self) -> &[SmolStr] {
        &self.allowed_services
    }

    fn replay_protection_enabled(&self) -> bool {
        self.replay_protection_enabled
    }

    fn replay_store(&self) -> &dyn ReplayStore {
        self.replay_store.as_ref()
    }
}

/// Route-scoped service auth policy.
#[derive(Debug, Clone, Default)]
pub struct ServiceAuthRoutePolicy {
    /// Required service-id fragment for this route.
    required_service_id: Option<SmolStr>,
}

impl ServiceAuthRoutePolicy {
    /// Require a specific service-id fragment for this route.
    pub fn require_service_id(service_id: impl Into<SmolStr>) -> Self {
        Self {
            required_service_id: Some(service_id.into()),
        }
    }

    /// Get the required service-id fragment.
    pub fn required_service_id(&self) -> Option<&str> {
        self.required_service_id.as_deref()
    }
}

/// Create an axum route layer that requires a specific service-id fragment.
pub fn require_service_id(service_id: impl Into<SmolStr>) -> Extension<ServiceAuthRoutePolicy> {
    Extension(ServiceAuthRoutePolicy::require_service_id(service_id))
}

/// Verified service authentication information.
///
/// This is the result of successfully verifying a service auth JWT.
/// This type is extracted by the `ExtractServiceAuth` extractor.
#[derive(Debug, Clone, jacquard_derive::IntoStatic)]
pub struct VerifiedServiceAuth<'a> {
    /// The authenticated user's DID (from `iss` claim)
    did: Did,
    /// The audience (should match your service DID, with optional service fragment).
    aud: DidService,
    /// The lexicon method NSID, if present
    lxm: Option<Nsid>,
    /// JWT ID (nonce), if present
    jti: Option<CowStr<'a>>,
}

impl<'a> VerifiedServiceAuth<'a> {
    /// Get the authenticated user's DID.
    pub fn did(&self) -> Did<&str> {
        self.did.borrow()
    }

    /// Get the full audience, including any service-id fragment.
    pub fn aud(&self) -> DidService<&str> {
        self.aud.borrow()
    }

    /// Get the fragmentless service DID audience.
    pub fn audience(&self) -> Did<&str> {
        self.aud.audience()
    }

    /// Get the optional service-id fragment.
    pub fn service(&self) -> Option<&str> {
        self.aud.service()
    }

    /// Get the lexicon method NSID, if present.
    pub fn lxm(&self) -> Option<Nsid<&str>> {
        self.lxm.as_ref().map(|l| l.borrow())
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

/// Axum extractor for optional service authentication.
///
/// Like `ExtractServiceAuth`, but returns `None` if no Authorization header
/// is present. If a header IS present but invalid, returns an error.
///
/// Use this for endpoints that work for both authenticated and anonymous users,
/// but show different content based on auth status.
///
/// # Example
///
/// ```no_run
/// use axum::{Router, routing::get};
/// use jacquard_axum::service_auth::{ServiceAuthConfig, ExtractOptionalServiceAuth};
/// use jacquard_identity::JacquardResolver;
/// use jacquard_identity::resolver::ResolverOptions;
/// use jacquard_common::types::string::Did;
///
/// async fn handler(
///     ExtractOptionalServiceAuth(auth): ExtractOptionalServiceAuth,
/// ) -> String {
///     match auth {
///         Some(a) => format!("Authenticated as {}", a.did()),
///         None => "Anonymous request".to_string(),
///     }
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let resolver = JacquardResolver::new(
///         reqwest::Client::new(),
///         ResolverOptions::default(),
///     );
///     let config = ServiceAuthConfig::new(
///         Did::new_static("did:web:example.com").unwrap(),
///         resolver,
///     );
///
///     let app = Router::new()
///         .route("/xrpc/com.example.getData", get(handler))
///         .with_state(config);
///
///     let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
///         .await
///         .unwrap();
///     axum::serve(listener, app).await.unwrap();
/// }
/// ```
pub struct ExtractOptionalServiceAuth(pub Option<VerifiedServiceAuth<'static>>);

/// Errors that can occur during service auth verification.
#[derive(Debug, Error, miette::Diagnostic)]
#[non_exhaustive]
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
        did: Did,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// No valid signing key found in DID document
    #[error("no valid signing key found in DID document for {0}")]
    NoSigningKey(Did),

    /// Method binding required but missing
    #[error("lxm (method binding) is required but missing from token")]
    MethodBindingRequired,

    /// Invalid key format
    #[error("invalid key format: {0}")]
    InvalidKey(String),

    /// Service-id fragment is required for this route.
    #[error("service id {required} is required but missing from token audience")]
    ServiceIdRequired {
        /// Required service-id fragment.
        required: SmolStr,
    },

    /// Service-id fragment does not match this route.
    #[error("service id mismatch: required {required}, got {actual}")]
    RouteServiceIdMismatch {
        /// Required service-id fragment.
        required: SmolStr,
        /// Actual service-id fragment.
        actual: SmolStr,
    },

    /// Replay protection is enabled but the token has no `jti`.
    #[error("service auth JWT is missing required jti")]
    MissingJti,

    /// Replay protection rejected the token.
    #[error("replay protection failed: {0}")]
    Replay(#[from] ReplayStoreError),
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
            ServiceAuthError::InvalidKey(_)
            | ServiceAuthError::ServiceIdRequired { .. }
            | ServiceAuthError::RouteServiceIdMismatch { .. }
            | ServiceAuthError::MissingJti
            | ServiceAuthError::Replay(_) => (
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

fn owned_did<S: jacquard_common::BosStr>(did: &Did<S>) -> Did {
    Did::new_owned(did.as_str()).unwrap()
}

fn bearer_token_from_parts(parts: &Parts) -> Result<Option<&str>, ServiceAuthError> {
    let Some(auth_header) = parts.headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };

    let auth_str = auth_header
        .to_str()
        .map_err(|_| ServiceAuthError::InvalidAuthHeader)?;
    let token = auth_str
        .strip_prefix("Bearer ")
        .ok_or(ServiceAuthError::InvalidAuthHeader)?;
    Ok(Some(token))
}

async fn verify_service_auth<S>(
    parts: &Parts,
    state: &S,
    token: &str,
) -> Result<VerifiedServiceAuth<'static>, ServiceAuthError>
where
    S: ServiceAuth + Send + Sync,
    S::Resolver: Send + Sync,
{
    let parsed = service_auth::parse_jwt(token)?;
    let claims = parsed.claims();

    let did_doc = state
        .resolver()
        .resolve_did_doc(&claims.iss)
        .await
        .map_err(|e| ServiceAuthError::DidResolutionFailed {
            did: owned_did(&claims.iss),
            source: Box::new(e),
        })?;

    let doc = did_doc
        .parse()
        .map_err(|e| ServiceAuthError::DidResolutionFailed {
            did: owned_did(&claims.iss),
            source: Box::new(e),
        })?;

    let verification_methods = doc
        .verification_method
        .as_deref()
        .ok_or_else(|| ServiceAuthError::NoSigningKey(owned_did(&claims.iss)))?;

    let signing_key = extract_signing_key(verification_methods)
        .ok_or_else(|| ServiceAuthError::NoSigningKey(claims.iss.clone().into_static()))?;

    service_auth::verify_signature(&parsed, &signing_key)?;
    claims.validate(&state.service_did(), state.allowed_services())?;

    if state.require_lxm() && claims.lxm.is_none() {
        return Err(ServiceAuthError::MethodBindingRequired);
    }

    if let Some(policy) = parts.extensions.get::<ServiceAuthRoutePolicy>() {
        if let Some(required) = policy.required_service_id() {
            match claims.aud.service() {
                Some(actual) if actual == required => {}
                Some(actual) => {
                    return Err(ServiceAuthError::RouteServiceIdMismatch {
                        required: SmolStr::new(required),
                        actual: SmolStr::new(actual),
                    });
                }
                None => {
                    return Err(ServiceAuthError::ServiceIdRequired {
                        required: SmolStr::new(required),
                    });
                }
            }
        }
    }

    if state.replay_protection_enabled() {
        let jti = claims.jti.as_ref().ok_or(ServiceAuthError::MissingJti)?;
        let key = ReplayKey::new(
            claims.iss.clone().into_static(),
            claims.aud.clone().into_static(),
            jti.clone(),
        );
        state
            .replay_store()
            .check_and_insert(key, claims.exp)
            .await?;
    }

    Ok(VerifiedServiceAuth {
        did: claims.iss.clone().into_static(),
        aud: claims.aud.clone().into_static(),
        lxm: claims.lxm.as_ref().map(|l| l.clone().into_static()),
        jti: claims.jti.as_ref().map(|j| CowStr::from(j.clone())),
    })
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
            let token =
                bearer_token_from_parts(parts)?.ok_or(ServiceAuthError::MissingAuthHeader)?;
            verify_service_auth(parts, state, token).await.map(Self)
        }
    }
}

impl<S> FromRequestParts<S> for ExtractOptionalServiceAuth
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
            let Some(token) = bearer_token_from_parts(parts)? else {
                return Ok(Self(None));
            };
            verify_service_auth(parts, state, token)
                .await
                .map(|auth| Self(Some(auth)))
        }
    }
}

/// Extract the signing key from a DID document's verification methods.
///
/// This looks for a key with type "atproto" or the first available key
/// if no atproto-specific key is found.
fn extract_signing_key(methods: &[VerificationMethod<CowStr<'_>>]) -> Option<PublicKey> {
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
        [0x80, 0x24] => PublicKey::from_p256_bytes(key_material)
            .inspect_err(|_e| {
                #[cfg(feature = "tracing")]
                tracing::error!("Failed to parse p256 public key: {}", _e);
            })
            .ok(),
        // secp256k1-pub (0xe7)
        [0xe7, 0x01] => PublicKey::from_k256_bytes(key_material)
            .inspect_err(|_e| {
                #[cfg(feature = "tracing")]
                tracing::error!("Failed to parse secp256k1 public key: {}", _e);
            })
            .ok(),
        _ => {
            #[cfg(feature = "tracing")]
            tracing::error!("Unsupported public key multicodec: {:?}", codec);
            None
        }
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
/// use jacquard_identity::{PublicResolver, JacquardResolver};
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
///             service_auth_middleware::<ServiceAuthConfig<PublicResolver>>,
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
