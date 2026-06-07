//! XRPC client implementation for AT Protocol
//!
//! This module provides HTTP and XRPC client traits along with session management
//! for both app password and OAuth authentication.
//!
//! ## Key types
//!
//! - [`Agent<A>`] - Unified session wrapper with convenience methods
//! - [`CredentialSession`] - App-password authentication with auto-refresh
//! - [`crate::oauth::client::OAuthSession`] - OAuth/DPoP authentication
//! - [`AgentSession`] - Common trait for both session types
//!
//! ## Modules
//!
//! - [`credential_session`] - App-password session implementation
//! - [`token`] - Token storage and persistence
//! - [`vec_update`] - Trait for fetch-modify-put patterns on array endpoints
//!
//!
//! "Agent" in this context is derived from Bluesky's own library usage of the term.
//! It represents a (persistent) user session, and includes a number of helpful
//! methods which are available via the `AgentSessionExt` extension trait
//! on anything that implements `AgentSession` + `IdentityResolver`.

//pub mod bff_session;
/// App password session implementation with auto-refresh
pub mod credential_session;
/// Agent error type
pub mod error;
/// Token storage and on-disk persistence formats
pub mod token;
/// Trait for fetch-modify-put patterns on array-based endpoints
pub mod vec_update;

use crate::client::credential_session::{CredentialSession, SessionKey};
use crate::client::vec_update::VecUpdate;
use core::future::Future;
pub use error::*;
#[cfg(feature = "api")]
use jacquard_api::com_atproto::repo::get_record::GetRecordOutput;
#[cfg(feature = "api")]
use jacquard_api::com_atproto::{
    repo::{
        create_record::CreateRecordOutput, delete_record::DeleteRecordOutput,
        get_record::GetRecordResponse, put_record::PutRecordOutput,
    },
    server::{create_session::CreateSessionOutput, refresh_session::RefreshSessionOutput},
};
use jacquard_common::deps::fluent_uri::Uri;
use jacquard_common::error::XrpcResult;
pub use jacquard_common::error::{ClientError, XrpcResult as ClientResult};
use jacquard_common::http_client::HttpClient;
pub use jacquard_common::session::{MemorySessionStore, SessionStore, SessionStoreError};
use jacquard_common::types::blob::{Blob, MimeType};
use jacquard_common::types::collection::Collection;
#[cfg(feature = "api")]
use jacquard_common::types::did_doc::DidDocument;
#[cfg(feature = "api")]
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::recordkey::{RecordKey, Rkey};
use jacquard_common::types::string::AtUri;
#[cfg(feature = "api")]
use jacquard_common::types::uri::RecordUri;
use jacquard_common::xrpc::XrpcResponse;
use jacquard_common::xrpc::{
    CallOptions, Response, XrpcClient, XrpcError, XrpcExt, XrpcRequest, XrpcResp,
};
use jacquard_common::{AuthorizationToken, xrpc};
use jacquard_common::{
    BosStr, CowStr, IntoStatic,
    types::string::{Did, Handle},
};
use jacquard_identity::resolver::{
    DidDocResponse, IdentityError, IdentityResolver, ResolverOptions,
};
use jacquard_identity::{PublicResolver, slingshot_resolver_default};
use jacquard_oauth::authstore::ClientAuthStore;
use jacquard_oauth::client::{OAuthClient, OAuthSession};
use jacquard_oauth::dpop::DpopExt;
use jacquard_oauth::resolver::OAuthResolver;
use serde::Serialize;
#[cfg(feature = "api")]
use serde::de::DeserializeOwned;
use smol_str::SmolStr;
#[cfg(feature = "api")]
use std::marker::Send;
use std::option::Option;
use std::sync::Arc;
pub use token::FileAuthStore;
use tokio::sync::RwLock;

/// Identifies the active authentication mode for an agent/session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    /// App password (Bearer) session
    AppPassword,
    /// OAuth (DPoP) session
    OAuth,
}

/// Common interface for stateful sessions used by the Agent wrapper.
///
/// Implemented by `CredentialSession` (app‑password) and `OAuthSession` (DPoP).
#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]
pub trait AgentSession: XrpcClient + HttpClient + Send + Sync {
    /// Identify the kind of session.
    fn session_kind(&self) -> AgentKind;
    /// Return current DID and an optional session id (always Some for OAuth).
    fn session_info(&self) -> impl Future<Output = Option<(Did, Option<SmolStr>)>>;
    /// Current base endpoint.
    fn endpoint(&self) -> impl Future<Output = Uri<String>>;
    /// Override per-session call options.
    fn set_options(&self, opts: CallOptions) -> impl Future<Output = ()>;
    /// Refresh the session and return a fresh AuthorizationToken.
    fn refresh(&self) -> impl Future<Output = ClientResult<AuthorizationToken<SmolStr>>>;
}

/// Alias for an agent over a credential (app‑password) session.
pub type CredentialAgent<S, T> = Agent<CredentialSession<S, T>>;
/// Alias for an agent over an OAuth (DPoP) session.
pub type OAuthAgent<T, S> = Agent<OAuthSession<T, S>>;

/// BasicClient: in-memory store + public resolver over a credential session.
pub type BasicClient = Agent<
    CredentialSession<
        MemorySessionStore<SessionKey, AtpSession>,
        jacquard_identity::PublicResolver,
    >,
>;

impl BasicClient {
    /// Create an unauthenticated BasicClient for public API access.
    ///
    /// Uses an in-memory session store and public resolver. Suitable for
    /// read-only operations on public data without authentication.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use jacquard::client::BasicClient;
    /// # use jacquard::types::string::AtUri;
    /// # use jacquard_api::app_bsky::feed::post::Post;
    /// use crate::jacquard::client::AgentSessionExt;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = BasicClient::unauthenticated();
    /// let uri = AtUri::new_static("at://did:plc:xyz/app.bsky.feed.post/3l5abc").unwrap();
    /// let response = client.get_record::<Post, _>(&uri).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn unauthenticated() -> Self {
        use std::sync::Arc;
        let http = reqwest::Client::new();
        let resolver = jacquard_identity::PublicResolver::new(http, Default::default());
        let store = MemorySessionStore::default();
        let session = CredentialSession::new(Arc::new(store), Arc::new(resolver));
        Agent::new(session)
    }
}

impl Default for BasicClient {
    fn default() -> Self {
        Self::unauthenticated()
    }
}

/// Unauthenticated XRPC client session with identity resolution
#[derive(Debug, Clone)]
pub struct UnauthenticatedSession<T> {
    resolver: Arc<T>,
    endpoint: Arc<RwLock<Option<Uri<String>>>>,
    options: Arc<RwLock<CallOptions>>,
}

impl Default for UnauthenticatedSession<PublicResolver> {
    fn default() -> Self {
        Self::new_public()
    }
}

impl UnauthenticatedSession<PublicResolver> {
    /// Create a new unauthenticated session using the public bluesky appview api as a fallback resolver
    pub fn new_public() -> Self {
        let resolver = Arc::new(PublicResolver::default());
        let endpoint = Arc::new(RwLock::new(None));
        let options = Arc::new(RwLock::new(CallOptions::default()));
        Self {
            resolver,
            endpoint,
            options,
        }
    }

    /// Create a new unauthenticated session using the Slingshot service for handle resolution
    pub fn new_slingshot() -> Self {
        let resolver = Arc::new(slingshot_resolver_default());
        let endpoint = Arc::new(RwLock::new(None));
        let options = Arc::new(RwLock::new(CallOptions::default()));
        Self {
            resolver,
            endpoint,
            options,
        }
    }
}

impl<T: HttpClient + Sync> HttpClient for UnauthenticatedSession<T> {
    type Error = T::Error;

    #[cfg(not(target_arch = "wasm32"))]
    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, T::Error>> + Send {
        self.resolver.send_http(request)
    }

    #[cfg(target_arch = "wasm32")]
    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, T::Error>> {
        self.resolver.send_http(request)
    }
}

impl<T: HttpClient> XrpcClient for UnauthenticatedSession<T>
where
    T: Sync + Send,
{
    #[doc = " Get the base URI for the client."]
    fn base_uri(&self) -> impl Future<Output = Uri<String>> {
        async move {
            self.endpoint.read().await.clone().unwrap_or_else(|| {
                Uri::parse("https://public.api.bsky.app")
                    .expect("hardcoded URI is valid")
                    .to_owned()
            })
        }
    }

    #[doc = " Send an XRPC request and parse the response"]
    #[cfg(not(target_arch = "wasm32"))]
    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync,
    {
        async move {
            let opts = self.options.read().await.clone();
            self.send_with_opts(request, opts).await
        }
    }

    #[doc = " Send an XRPC request and parse the response"]
    #[cfg(not(target_arch = "wasm32"))]
    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>> + Send
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync,
    {
        async move {
            let base_uri = self.base_uri().await;
            self.resolver
                .xrpc(base_uri.borrow())
                .with_options(opts.clone())
                .send(&request)
                .await
        }
    }

    #[doc = " Send an XRPC request and parse the response"]
    #[cfg(target_arch = "wasm32")]
    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        async move {
            let opts = self.options.read().await.clone();
            self.send_with_opts(request, opts).await
        }
    }

    #[doc = " Send an XRPC request and parse the response"]
    #[cfg(target_arch = "wasm32")]
    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        async move {
            let base_uri = self.base_uri().await;
            self.resolver
                .xrpc(base_uri.borrow())
                .with_options(opts.clone())
                .send(&request)
                .await
        }
    }

    #[doc = " Set the base URI for the client."]
    fn set_base_uri(&self, uri: Uri<String>) -> impl Future<Output = ()> {
        async move {
            let normalized = crate::xrpc::normalize_base_uri(uri);
            let mut guard = self.endpoint.write().await;
            *guard = Some(normalized);
        }
    }

    #[doc = " Get the call options for the client."]
    fn opts(&self) -> impl Future<Output = CallOptions> {
        async move { self.options.read().await.clone() }
    }

    #[doc = " Set the call options for the client."]
    fn set_opts(&self, opts: CallOptions) -> impl Future<Output = ()> {
        async move {
            *self.options.write().await = opts.into_static();
        }
    }
}

impl<T: IdentityResolver + HttpClient> AgentSession for UnauthenticatedSession<T>
where
    T: Sync + Send,
{
    fn session_kind(&self) -> AgentKind {
        AgentKind::AppPassword
    }

    fn session_info(&self) -> impl Future<Output = Option<(Did, Option<SmolStr>)>> {
        async { None } // no session
    }

    fn endpoint(&self) -> impl Future<Output = Uri<String>> {
        async { self.base_uri().await }
    }

    #[doc = " Override per-session call options."]
    fn set_options(&self, opts: CallOptions) -> impl Future<Output = ()> {
        async move {
            *self.options.write().await = opts;
        }
    }

    #[doc = " Refresh the session and return a fresh AuthorizationToken."]
    fn refresh(&self) -> impl Future<Output = ClientResult<AuthorizationToken<SmolStr>>> {
        async {
            Err(ClientError::auth(
                jacquard_common::error::AuthError::NotAuthenticated,
            ))
        }
    }
}

impl<T: IdentityResolver + Sync> IdentityResolver for UnauthenticatedSession<T> {
    #[doc = " Access options for validation decisions in default methods"]
    fn options(&self) -> &ResolverOptions {
        self.resolver.options()
    }

    #[doc = " Resolve handle"]
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_handle<S: BosStr + Sync>(
        &self,
        handle: &Handle<S>,
    ) -> impl Future<Output = std::result::Result<Did, IdentityError>> + Send
    where
        Self: Sync,
    {
        self.resolver.resolve_handle(handle)
    }

    #[doc = " Resolve DID document"]
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        did: &Did<S>,
    ) -> impl Future<Output = std::result::Result<DidDocResponse, IdentityError>> + Send
    where
        Self: Sync,
    {
        self.resolver.resolve_did_doc(did)
    }

    #[doc = " Resolve handle"]
    #[cfg(target_arch = "wasm32")]
    fn resolve_handle<S: BosStr + Sync>(
        &self,
        handle: &Handle<S>,
    ) -> impl Future<Output = std::result::Result<Did, IdentityError>> {
        self.resolver.resolve_handle(handle)
    }

    #[doc = " Resolve DID document"]
    #[cfg(target_arch = "wasm32")]
    fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        did: &Did<S>,
    ) -> impl Future<Output = std::result::Result<DidDocResponse, IdentityError>> {
        self.resolver.resolve_did_doc(did)
    }
}

/// MemoryCredentialSession: credential session with in memory store and identity resolver
pub type MemoryCredentialSession = CredentialSession<
    MemorySessionStore<SessionKey, AtpSession>,
    jacquard_identity::JacquardResolver<reqwest::Client>,
>;

impl MemoryCredentialSession {
    /// Create an unauthenticated MemoryCredentialSession.
    ///
    /// Uses an in memory store and a public resolver.
    /// Equivalent to a BasicClient that isn't wrapped in Agent
    pub fn unauthenticated() -> Self {
        use std::sync::Arc;
        let http = reqwest::Client::new();
        let resolver = jacquard_identity::JacquardResolver::new(http, Default::default());
        let store = MemorySessionStore::default();
        CredentialSession::new(Arc::new(store), Arc::new(resolver))
    }

    /// Create a MemoryCredentialSession and authenticate with the provided details
    ///
    /// - `identifier`: handle (preferred), DID, or `https://` PDS base URL.
    /// - `session_id`: optional session label; defaults to "session".
    /// - Persists and activates the session, and updates the base endpoint to the user's PDS.
    ///
    /// # Example
    /// ```no_run
    /// # use jacquard::client::BasicClient;
    /// # use jacquard::types::string::AtUri;
    /// # use jacquard::api::app_bsky::feed::post::Post;
    /// # use jacquard::types::string::Datetime;
    /// # use jacquard::CowStr;
    /// use jacquard::client::MemoryCredentialSession;
    /// use jacquard::client::{Agent, AgentSessionExt};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let (identifier, password, post_text): (CowStr<'_>, CowStr<'_>, CowStr<'_>)  = todo!();
    /// let (session, _) = MemoryCredentialSession::authenticated(identifier, password, None, None).await?;
    /// let agent = Agent::from(session);
    /// let post = Post::new().text(post_text).created_at(Datetime::now()).build();
    /// let output = agent.create_record(post, None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn authenticated(
        identifier: CowStr<'_>,
        password: CowStr<'_>,
        session_id: Option<CowStr<'_>>,
        pds: Option<Uri<String>>,
    ) -> ClientResult<(Self, AtpSession)> {
        let session = MemoryCredentialSession::unauthenticated();
        let auth = session
            .login(identifier, password, session_id, None, None, pds)
            .await?;
        Ok((session, auth))
    }
}

impl Default for MemoryCredentialSession {
    fn default() -> Self {
        MemoryCredentialSession::unauthenticated()
    }
}

/// App password session information from `com.atproto.server.createSession`
///
/// Contains the access and refresh tokens along with user identity information.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AtpSession {
    /// Access token (JWT) used for authenticated requests
    pub access_jwt: SmolStr,
    /// Refresh token (JWT) used to obtain new access tokens
    pub refresh_jwt: SmolStr,
    /// User's DID (Decentralized Identifier)
    pub did: Did,
    /// User's handle (e.g., "alice.bsky.social")
    pub handle: Handle,
    /// Account PDS endpoint, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pds: Option<Uri<String>>,
}

impl AtpSession {
    /// Return the known account PDS endpoint, if present.
    pub fn pds_endpoint(&self) -> Option<&Uri<String>> {
        self.pds.as_ref()
    }

    /// Merge a refresh response into this session, preserving the existing PDS unless
    /// the refresh response contains a parseable DID document PDS endpoint.
    #[cfg(feature = "api")]
    pub fn merge_refresh(&mut self, output: RefreshSessionOutput) {
        let pds = pds_from_data(output.did_doc.as_ref()).or_else(|| self.pds.clone());
        self.access_jwt = output.access_jwt;
        self.refresh_jwt = output.refresh_jwt;
        self.did = output.did;
        self.handle = output.handle;
        self.pds = pds;
    }
}

impl IntoStatic for AtpSession {
    type Output = Self;

    fn into_static(self) -> Self {
        self
    }
}

#[cfg(feature = "api")]
pub(crate) fn pds_from_data<S: BosStr>(
    data: Option<&jacquard_common::types::value::Data<S>>,
) -> Option<Uri<String>> {
    let doc: DidDocument = serde::Deserialize::deserialize(data?).ok()?;
    doc.pds_endpoint().map(|uri| uri.to_owned())
}

#[cfg(feature = "api")]
impl From<CreateSessionOutput> for AtpSession {
    fn from(output: CreateSessionOutput) -> Self {
        let pds = pds_from_data(output.did_doc.as_ref());
        Self {
            access_jwt: output.access_jwt,
            refresh_jwt: output.refresh_jwt,
            did: output.did,
            handle: output.handle,
            pds,
        }
    }
}

#[cfg(feature = "api")]
impl From<RefreshSessionOutput> for AtpSession {
    fn from(output: RefreshSessionOutput) -> Self {
        let pds = pds_from_data(output.did_doc.as_ref());
        Self {
            access_jwt: output.access_jwt,
            refresh_jwt: output.refresh_jwt,
            did: output.did,
            handle: output.handle,
            pds,
        }
    }
}

/// Thin wrapper over a stateful session providing a uniform `XrpcClient`.
pub struct Agent<A: AgentSession> {
    inner: A,
}

impl<A: AgentSession> Agent<A> {
    /// Wrap an existing session in an Agent.
    pub fn new(inner: A) -> Self {
        Self { inner }
    }

    /// Get a reference to the underlying session
    pub fn inner(&self) -> &A {
        &self.inner
    }

    /// Return the underlying session kind.
    pub fn kind(&self) -> AgentKind {
        self.inner.session_kind()
    }

    /// Return session info if available.
    pub async fn info(&self) -> Option<(Did, Option<SmolStr>)> {
        self.inner.session_info().await
    }

    /// Get current endpoint.
    pub async fn endpoint(&self) -> Uri<String> {
        self.inner.endpoint().await
    }

    /// Override call options for subsequent requests.
    pub async fn set_options(&self, opts: CallOptions) {
        self.inner.set_options(opts).await
    }

    /// Refresh the session and return a fresh token.
    pub async fn refresh(&self) -> ClientResult<AuthorizationToken<SmolStr>> {
        self.inner.refresh().await
    }
}

/// Output type for a collection record retrieval operation (SmolStr-backed, as returned by `into_output()`)
pub type CollectionOutput<R> = <<R as Collection>::Record as XrpcResp>::Output<SmolStr>;
/// Error type for a collection record retrieval operation
pub type CollectionErr<R> = <<R as Collection>::Record as XrpcResp>::Err;
/// Response type for the get request of a vec update operation
pub type VecGetResponse<U> = <<U as VecUpdate>::GetRequest as XrpcRequest>::Response;
/// Response type for the put request of a vec update operation
pub type VecPutResponse<U> = <<U as VecUpdate>::PutRequest as XrpcRequest>::Response;

type CollectionError<R> = <<R as Collection>::Record as XrpcResp>::Err;

type VecUpdateGetError<U> =
    <<<U as VecUpdate>::GetRequest as XrpcRequest>::Response as XrpcResp>::Err;

type VecUpdatePutError<U> =
    <<<U as VecUpdate>::PutRequest as XrpcRequest>::Response as XrpcResp>::Err;

/// Extension trait providing convenience methods for common repository operations.
///
/// This trait is automatically implemented for any type that implements both
/// [`AgentSession`] and [`IdentityResolver`]. It provides higher-level methods
/// that handle common patterns like fetch-modify-put, with automatic repo resolution
/// for at:// uris, and typed record operations.
///
/// # Available Operations
///
/// - **Basic CRUD**: [`create_record`](Self::create_record), [`get_record`](Self::get_record),
///   [`put_record`](Self::put_record), [`delete_record`](Self::delete_record)
/// - **Update patterns**: [`update_record`](Self::update_record) (fetch-modify-put for records),
///   [`update_vec`](Self::update_vec) and [`update_vec_item`](Self::update_vec_item) (for array endpoints)
/// - **Blob operations**: [`upload_blob`](Self::upload_blob)
///
/// # Example
///
/// ```no_run
/// # use jacquard::client::BasicClient;
/// # use jacquard_api::app_bsky::feed::post::Post;
/// # use jacquard_common::types::string::{AtUri, Datetime};
/// # use jacquard_common::CowStr;
/// use jacquard::client::AgentSessionExt;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let agent: BasicClient = todo!();
/// // Create a post
/// let post = Post {
///     text: CowStr::from("Hello from Jacquard!"),
///     created_at: Datetime::now(),
///     # embed: None, entities: None, facets: None, labels: None,
///     # langs: None, reply: None, tags: None, extra_data: Default::default(),
/// };
/// let output = agent.create_record(post, None).await?;
///
/// // Read it back
/// let response = agent.get_record::<Post, _>(&output.uri).await?;
/// let record = response.parse()?;
/// println!("Post: {}", record.value.text);
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "api")]
pub trait AgentSessionExt: AgentSession + IdentityResolver {
    /// Create a new record in the repository.
    ///
    /// The collection is inferred from the record type's `Collection::NSID`.
    /// The repo is automatically filled from the session info.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use jacquard::client::BasicClient;
    /// # use jacquard_api::app_bsky::feed::post::Post;
    /// # use jacquard_common::types::string::Datetime;
    /// # use jacquard_common::CowStr;
    /// use jacquard::client::AgentSessionExt;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let agent: BasicClient = todo!();
    /// let post = Post {
    ///     text: CowStr::from("Hello world!"),
    ///     created_at: Datetime::now(),
    ///     embed: None,
    ///     entities: None,
    ///     facets: None,
    ///     labels: None,
    ///     langs: None,
    ///     reply: None,
    ///     tags: None,
    ///     extra_data: Default::default(),
    /// };
    /// let output = agent.create_record(post, None).await?;
    /// println!("Created record: {}", output.uri);
    /// # Ok(())
    /// # }
    /// ```
    fn create_record<R>(
        &self,
        record: R,
        rkey: Option<RecordKey<Rkey>>,
    ) -> impl Future<Output = Result<CreateRecordOutput>>
    where
        R: Collection + serde::Serialize,
    {
        async move {
            use jacquard_api::com_atproto::repo::create_record::CreateRecord;
            use jacquard_common::types::ident::AtIdentifier;
            use jacquard_common::types::value::to_data;

            let (did, _) = self
                .session_info()
                .await
                .ok_or_else(|| AgentError::no_session().for_collection("create record", R::NSID))?;

            #[cfg(feature = "tracing")]
            let _span = tracing::debug_span!("create_record", collection = %R::nsid()).entered();

            let data =
                to_data(&record).map_err(|e| AgentError::sub_operation("serialize record", e))?;

            let request = CreateRecord::new()
                .repo(AtIdentifier::Did(did))
                .collection(R::nsid().into_static())
                .record(data)
                .rkey(rkey.map(|k| k.clone()))
                .build();

            #[cfg(feature = "tracing")]
            _span.exit();

            let response = self
                .send(request)
                .await
                .map_err(|e| e.for_collection("create record", R::NSID))?;
            response.into_output().map_err(|e| match e {
                XrpcError::Auth(auth) => AgentError::from(auth),
                XrpcError::Xrpc(typed) => AgentError::sub_operation("create record", typed),
                e => AgentError::xrpc(e),
            })
        }
    }

    /// Get a record from the repository using an at:// URI.
    ///
    /// Returns a typed `Response` that deserializes directly to the record type.
    /// Use `.parse()` to borrow from the response buffer, or `.into_output()` for owned data.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use jacquard::client::BasicClient;
    /// # use jacquard_api::app_bsky::feed::post::Post;
    /// # use jacquard_common::types::string::AtUri;
    /// # use jacquard_common::IntoStatic;
    /// use jacquard::client::AgentSessionExt;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let agent: BasicClient = todo!();
    /// let uri = AtUri::new_static("at://did:plc:xyz/app.bsky.feed.post/3l5bqm7lepk2c").unwrap();
    /// let response = agent.get_record::<Post, _>(&uri).await?;
    /// let output = response.parse()?;  // PostGetRecordOutput<'_> borrowing from buffer
    /// println!("Post text: {}", output.value.text);
    ///
    /// // Or get owned data
    /// let output_owned = response.into_output()?;
    /// # Ok(())
    /// # }
    /// ```
    fn get_record<R, S>(
        &self,
        uri: &AtUri<S>,
    ) -> impl Future<Output = ClientResult<Response<R::Record>>>
    where
        R: Collection,
        S: BosStr + Sync,
    {
        async move {
            #[cfg(feature = "tracing")]
            let _span =
                tracing::debug_span!("get_record", collection = %R::nsid(), uri = %uri).entered();

            // Validate that URI's collection matches the expected type
            if let Some(uri_collection) = uri.collection() {
                if uri_collection.as_str() != R::nsid().as_str() {
                    return Err(ClientError::invalid_request(format!(
                        "Collection mismatch: URI contains '{}' but type parameter expects '{}'",
                        uri_collection,
                        R::nsid()
                    ))
                    .with_help("ensure the URI collection matches the record type"));
                }
            }

            let rkey = uri.rkey().ok_or_else(|| {
                ClientError::invalid_request("AtUri missing rkey")
                    .with_help("ensure the URI includes a record key after the collection")
            })?;

            #[cfg(feature = "tracing")]
            _span.exit();

            // Resolve authority (DID or handle) to get DID and PDS.
            let (repo_did, pds_url) = match uri.authority() {
                AtIdentifier::Did(did) => {
                    let pds = self.pds_for_did(&did).await.map_err(|e| {
                        ClientError::from(e)
                            .with_context("DID document resolution failed during record retrieval")
                    })?;
                    (did.into_static(), pds)
                }
                AtIdentifier::Handle(handle) => {
                    self.pds_for_handle(&handle).await.map_err(|e| {
                        ClientError::from(e)
                            .with_context("handle resolution failed during record retrieval")
                    })?
                }
            };

            // Make stateless XRPC call to that PDS (no auth required for public records).
            // All fields use SmolStr backing to satisfy the builder's single S type parameter.
            use jacquard_api::com_atproto::repo::get_record::GetRecord;
            let request = GetRecord::new()
                .repo(AtIdentifier::Did(repo_did.clone()))
                .collection(R::nsid().into_static())
                .rkey(rkey.into_static())
                .build();

            let response: Response<GetRecordResponse> = {
                let http_request =
                    xrpc::build_http_request(&pds_url.borrow(), &request, &self.opts().await)?;

                let http_response = self
                    .send_http(http_request)
                    .await
                    .map_err(|e| ClientError::transport(e).for_collection("get record", R::NSID))?;

                xrpc::process_response(http_response)
                    .map_err(|e| e.for_collection("get record", R::NSID))?
            };
            Ok(response.transmute())
        }
    }

    /// Untyped, freeform record fetcher.
    /// Hits <https://slingshot.microcosm.blue>
    fn fetch_record_slingshot<S>(
        &self,
        uri: &AtUri<S>,
    ) -> impl Future<Output = Result<GetRecordOutput>>
    where
        S: BosStr + Sync,
    {
        async move {
            #[cfg(feature = "tracing")]
            let _span = tracing::debug_span!("fetch_record_slingshot", uri = %uri).entered();

            // Make stateless XRPC call to that PDS (no auth required for public records)
            use jacquard_api::com_atproto::repo::get_record::GetRecord;
            let collection = uri.collection().clone().ok_or(AgentError::sub_operation(
                "no collection",
                ClientError::invalid_request("no collection"),
            ))?;
            let rkey = uri.rkey().ok_or(AgentError::sub_operation(
                "no rkey",
                ClientError::invalid_request("no rkey"),
            ))?;
            let request = GetRecord::builder()
                .repo(uri.authority().clone())
                .collection(collection.clone())
                .rkey(RecordKey(rkey.clone()))
                .build();

            #[cfg(feature = "tracing")]
            _span.exit();

            let response: Response<GetRecordResponse> = {
                let http_request = xrpc::build_http_request(
                    &Uri::parse("https://slingshot.microcosm.blue")
                        .expect("slingshot url is valid"),
                    &request,
                    &self.opts().await,
                )?;

                let http_response = self.send_http(http_request).await.map_err(|e| {
                    ClientError::transport(e).for_collection("fetch record", collection.as_str())
                })?;

                xrpc::process_response(http_response)
                    .map_err(|e| e.for_collection("fetch record", collection.as_str()))?
            };
            let output = response.into_output().map_err(|e| match e {
                XrpcError::Auth(auth) => AgentError::from(auth),
                XrpcError::Xrpc(typed) => AgentError::sub_operation("parse record", typed),
                e => AgentError::xrpc(e),
            })?;
            Ok(output)
        }
    }

    /// Fetches a record from the PDS. Returns an owned, parsed response.
    ///
    /// Takes an at:// URI annotated with the collection type, which be constructed with `R::uri(uri)`
    /// where `R` is the type of record you want (e.g. `app_bsky::feed::post::Post::uri(uri)` for Bluesky posts).
    fn fetch_record<R, S>(
        &self,
        uri: &RecordUri<S, R>,
    ) -> impl Future<Output = Result<CollectionOutput<R>>>
    where
        R: Collection,
        S: BosStr + Sync,
        CollectionOutput<R>: serde::de::DeserializeOwned,
        CollectionError<R>: Send + Sync + 'static,
    {
        let uri = uri.as_uri();
        async move {
            use smol_str::format_smolstr;

            let response = self.get_record::<R, S>(uri).await?;
            let response: Response<R::Record> = response.transmute();
            let output = response.into_output().map_err(|e| match e {
                XrpcError::Auth(auth) => AgentError::from(auth),
                XrpcError::Xrpc(typed) => AgentError::new(
                    AgentErrorKind::SubOperation {
                        step: "parse record",
                    },
                    None,
                )
                .with_details(format_smolstr!("{:?}", typed)),
                // Note: typed error formatted as Debug since CollectionErr<R> is not Display.
                e => AgentError::xrpc(e),
            })?;
            Ok(output)
        }
    }

    /// Update a record in-place with a fetch-modify-put pattern.
    ///
    /// This fetches the record using an at:// URI, converts it to owned data, applies
    /// the modification function, and puts it back. The modification function receives
    /// a mutable reference to the record data.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use jacquard::client::BasicClient;
    /// # use jacquard_api::app_bsky::actor::profile::Profile;
    /// # use jacquard_common::CowStr;
    /// # use jacquard_common::types::string::AtUri;
    /// use jacquard::client::AgentSessionExt;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let agent: BasicClient = todo!();
    /// let uri = AtUri::new_static("at://did:plc:xyz/app.bsky.actor.profile/self").unwrap();
    /// // Update profile record in-place
    /// agent.update_record::<Profile, _>(&uri, |profile| {
    ///     profile.display_name = Some(CowStr::from("New Name"));
    ///     profile.description = Some(CowStr::from("Updated bio"));
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    fn update_record<R, S>(
        &self,
        uri: &AtUri<S>,
        f: impl FnOnce(&mut R),
    ) -> impl Future<Output = Result<PutRecordOutput>>
    where
        R: Collection + Serialize,
        R: From<CollectionOutput<R>>,
        CollectionOutput<R>: serde::de::DeserializeOwned,
        CollectionError<R>: Send + Sync + std::error::Error + 'static,
        S: BosStr + Sync,
    {
        async move {
            // Fetch the record - Response<R::Record> where R::Record::Output<SmolStr> = R
            let response = self.get_record::<R, S>(uri).await?;

            #[cfg(feature = "tracing")]
            let _span = tracing::debug_span!("update_record", collection = %R::nsid(), uri = %uri)
                .entered();

            // Parse to get the record, borrowing from the response buffer.
            // Err is now a plain owned type; no into_static() needed.
            let record = response.parse().map_err(|e| match e {
                XrpcError::Auth(auth) => AgentError::from(auth),
                XrpcError::Xrpc(typed) => AgentError::sub_operation("parse record", typed),
                e => AgentError::xrpc(e),
            })?;

            // Convert to owned
            let mut owned = R::from(record);

            // Apply modification
            f(&mut owned);

            // Put it back
            // Convert the borrowed Rkey<&str> to an owned Rkey<SmolStr>, then wrap in RecordKey.
            // The Rkey<SmolStr> is already validated (extracted from a valid AtUri), so direct
            // construction is safe.
            let rkey = RecordKey(
                uri.rkey()
                    .ok_or_else(|| {
                        use jacquard_common::types::string::AtStrError;
                        AgentError::sub_operation(
                            "extract rkey",
                            AtStrError::missing("at-uri-scheme", &uri, "rkey"),
                        )
                    })?
                    .convert::<SmolStr>(),
            );

            #[cfg(feature = "tracing")]
            _span.exit();
            self.put_record::<R>(rkey, owned).await
        }
    }

    /// Delete a record from the repository.
    ///
    /// The collection is inferred from the type parameter.
    /// The repo is automatically filled from the session info.
    fn delete_record<R>(
        &self,
        rkey: RecordKey<Rkey>,
    ) -> impl Future<Output = Result<DeleteRecordOutput>>
    where
        R: Collection + Serialize,
    {
        async move {
            let (did, _) = self
                .session_info()
                .await
                .ok_or_else(|| AgentError::no_session().for_collection("delete record", R::NSID))?;
            #[cfg(feature = "tracing")]
            let _span = tracing::debug_span!("delete_record", collection = %R::nsid()).entered();

            use jacquard_api::com_atproto::repo::delete_record::DeleteRecord;
            use jacquard_common::types::ident::AtIdentifier;

            let request = DeleteRecord::new()
                .repo(AtIdentifier::Did(did.clone()))
                .collection(R::nsid().into_static())
                .rkey(rkey.into_static())
                .build();

            #[cfg(feature = "tracing")]
            _span.exit();

            let response = self
                .send(request)
                .await
                .map_err(|e| e.for_collection("delete record", R::NSID))?;
            response.into_output().map_err(|e| match e {
                XrpcError::Auth(auth) => AgentError::from(auth),
                XrpcError::Xrpc(typed) => AgentError::sub_operation("delete record", typed),
                e => AgentError::xrpc(e),
            })
        }
    }

    /// Put (upsert) a record in the repository.
    ///
    /// The collection is inferred from the record type's `Collection::NSID`.
    /// The repo is automatically filled from the session info.
    fn put_record<R>(
        &self,
        rkey: RecordKey<Rkey>,
        record: R,
    ) -> impl Future<Output = Result<PutRecordOutput>>
    where
        R: Collection + serde::Serialize,
    {
        async move {
            #[cfg(feature = "tracing")]
            let _span = tracing::debug_span!("put_record", collection = %R::nsid()).entered();

            use jacquard_api::com_atproto::repo::put_record::PutRecord;
            use jacquard_common::types::ident::AtIdentifier;
            use jacquard_common::types::value::to_data;

            let (did, _) = self
                .session_info()
                .await
                .ok_or_else(|| AgentError::no_session().for_collection("put record", R::NSID))?;

            let data =
                to_data(&record).map_err(|e| AgentError::sub_operation("serialize record", e))?;

            let request = PutRecord::new()
                .repo(AtIdentifier::Did(did.clone()))
                .collection(R::nsid().into_static())
                .rkey(rkey.into_static())
                .record(data)
                .build();

            #[cfg(feature = "tracing")]
            _span.exit();

            let response = self
                .send(request)
                .await
                .map_err(|e| e.for_collection("put record", R::NSID))?;
            response.into_output().map_err(|e| match e {
                XrpcError::Auth(auth) => AgentError::from(auth),
                XrpcError::Xrpc(typed) => AgentError::sub_operation("put record", typed),
                e => AgentError::xrpc(e),
            })
        }
    }

    /// Upload a blob to the repository.
    ///
    /// The mime type is sent as a Content-Type header hint, though the server also performs
    /// its own inference.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use jacquard::client::BasicClient;
    /// # use jacquard_common::types::blob::MimeType;
    /// use jacquard::client::AgentSessionExt;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let agent: BasicClient = todo!();
    /// let data = std::fs::read("image.png")?;
    /// let mime_type = MimeType::new_static("image/png");
    /// let blob_ref = agent.upload_blob(data, mime_type).await?;
    /// # Ok(())
    /// # }
    /// ```
    fn upload_blob(
        &self,
        data: impl Into<bytes::Bytes>,
        mime_type: MimeType<&str>,
    ) -> impl Future<Output = Result<Blob>> {
        async move {
            #[cfg(feature = "tracing")]
            let _span = tracing::debug_span!("upload_blob", mime_type = %mime_type).entered();

            use http::header::CONTENT_TYPE;
            use jacquard_api::com_atproto::repo::upload_blob::UploadBlob;

            let bytes = data.into();
            let request = UploadBlob { body: bytes };

            // Override Content-Type header with actual mime type instead of */*
            let mut opts = self.opts().await;

            opts.extra_headers.push((
                CONTENT_TYPE,
                http::HeaderValue::from_str(mime_type.as_str())
                    .map_err(|e| AgentError::sub_operation("set Content-Type header", e))?,
            ));

            #[cfg(feature = "tracing")]
            _span.exit();

            let response = self.send_with_opts(request, opts).await?;
            let output = response.into_output().map_err(|e| match e {
                XrpcError::Auth(auth) => AgentError::from(auth),
                XrpcError::Xrpc(typed) => AgentError::sub_operation("upload blob", typed),
                e => AgentError::xrpc(e),
            })?;
            // Blob is now SmolStr-backed (owned), so no into_static() needed.
            Ok(output.blob.blob().clone())
        }
    }

    /// Update a vec-based data structure with a fetch-modify-put pattern.
    ///
    /// This is useful for endpoints like preferences that return arrays requiring
    /// fetch-modify-put operations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// agent.update_vec::<PreferencesUpdate>(|prefs| {
    ///     prefs.push(AdultContentPref::new().enabled(true).build().into());
    ///     prefs.retain(|p| !matches!(p, Preference::Hidden(_)));
    /// }).await?;
    /// ```
    fn update_vec<'a, U>(
        &self,
        modify: impl FnOnce(&mut Vec<<U as VecUpdate>::Item>),
    ) -> impl Future<Output = Result<xrpc::Response<VecPutResponse<U>>>>
    where
        U: VecUpdate,
        <U as VecUpdate>::PutRequest: Send + Sync + Serialize,
        <U as VecUpdate>::GetRequest: Send + Sync + Serialize,
        VecGetResponse<U>: Send + Sync,
        VecPutResponse<U>: Send + Sync,
        <VecGetResponse<U> as XrpcResp>::Output<SmolStr>: DeserializeOwned,
        <VecPutResponse<U> as XrpcResp>::Output<SmolStr>: DeserializeOwned,
        VecUpdateGetError<U>: Send + Sync + std::error::Error + 'static,
        VecUpdatePutError<U>: Send + Sync + std::error::Error + 'static,
    {
        async {
            // Fetch current data
            let get_request = U::build_get();
            let response = self.send(get_request).await?;
            let output = response.into_output().map_err(|e| match e {
                XrpcError::Auth(auth) => AgentError::from(auth),
                XrpcError::Xrpc(typed) => AgentError::sub_operation("update vec", typed),
                e => AgentError::xrpc(e),
            })?;

            // Extract vec
            let mut items = U::extract_vec(output);

            // Apply modification
            modify(&mut items);

            // Build put request
            let put_request = U::build_put(items);

            // Send it
            Ok(self.send(put_request).await?)
        }
    }

    /// Update a single item in a vec-based data structure.
    ///
    /// This is a convenience wrapper around `update_vec` that finds and replaces
    /// a single matching item, or appends it if not found.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let pref = AdultContentPref::new().enabled(true).build();
    /// agent.update_vec_item::<PreferencesUpdate>(pref.into()).await?;
    /// ```
    fn update_vec_item<U>(
        &self,
        item: <U as VecUpdate>::Item,
    ) -> impl Future<Output = Result<xrpc::Response<VecPutResponse<U>>>>
    where
        U: VecUpdate,
        <U as VecUpdate>::PutRequest: Send + Sync + Serialize,
        <U as VecUpdate>::GetRequest: Send + Sync + Serialize,
        VecGetResponse<U>: Send + Sync,
        VecPutResponse<U>: Send + Sync,
        <VecGetResponse<U> as XrpcResp>::Output<SmolStr>: DeserializeOwned,
        <VecPutResponse<U> as XrpcResp>::Output<SmolStr>: DeserializeOwned,
        VecUpdateGetError<U>: Send + Sync + std::error::Error + 'static,
        VecUpdatePutError<U>: Send + Sync + std::error::Error + 'static,
    {
        async {
            self.update_vec::<U>(|vec| {
                if let Some(pos) = vec.iter().position(|i| U::matches(i, &item)) {
                    vec[pos] = item;
                } else {
                    vec.push(item);
                }
            })
            .await
        }
    }
}

#[cfg(feature = "api")]
impl<T: AgentSession + IdentityResolver> AgentSessionExt for T {}

impl<S, T, W> AgentSession for CredentialSession<S, T, W>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: IdentityResolver + HttpClient + XrpcExt + Send + Sync + 'static,
    W: Send + Sync,
{
    fn session_kind(&self) -> AgentKind {
        AgentKind::AppPassword
    }
    fn session_info(&self) -> impl Future<Output = Option<(Did, Option<SmolStr>)>> {
        async move {
            CredentialSession::<S, T, W>::session_info(self)
                .await
                // Convert the SmolStr session id to CowStr<'static>.
                .map(|key| (key.did, Some(key.session_id)))
        }
    }
    fn endpoint(&self) -> impl Future<Output = Uri<String>> {
        CredentialSession::<S, T, W>::endpoint(self)
    }
    fn set_options(&self, opts: CallOptions) -> impl Future<Output = ()> {
        CredentialSession::<S, T, W>::set_options(self, opts)
    }
    fn refresh(&self) -> impl Future<Output = ClientResult<AuthorizationToken<SmolStr>>> {
        async move {
            Ok(CredentialSession::<S, T, W>::refresh(self)
                .await?
                .into_static())
        }
    }
}

impl<T, S, W> AgentSession for OAuthSession<T, S, W>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + XrpcExt + Send + Sync + 'static,
    W: Send + Sync,
{
    fn session_kind(&self) -> AgentKind {
        AgentKind::OAuth
    }
    fn session_info(&self) -> impl Future<Output = Option<(Did, Option<SmolStr>)>> {
        async {
            let (did, sid) = OAuthSession::<T, S, W>::session_info(self).await;
            // did is already Did<SmolStr>; convert SmolStr sid to CowStr<'static>.
            Some((did, Some(sid)))
        }
    }
    fn endpoint(&self) -> impl Future<Output = Uri<String>> {
        self.endpoint()
    }
    fn set_options(&self, opts: CallOptions) -> impl Future<Output = ()> {
        self.set_options(opts)
    }
    fn refresh(&self) -> impl Future<Output = ClientResult<AuthorizationToken<SmolStr>>> {
        async {
            self.refresh()
                .await
                .map(|t| t.into_static())
                .map_err(|e| ClientError::transport(e).with_context("OAuth token refresh failed"))
        }
    }
}

impl<T, S> AgentSession for OAuthClient<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
{
    fn session_kind(&self) -> AgentKind {
        AgentKind::OAuth
    }
    fn session_info(&self) -> impl Future<Output = Option<(Did, Option<SmolStr>)>> {
        async { None }
    }
    fn endpoint(&self) -> impl Future<Output = Uri<String>> {
        async { self.base_uri().await }
    }
    fn set_options(&self, opts: CallOptions) -> impl Future<Output = ()> {
        async { self.set_opts(opts).await }
    }
    fn refresh(&self) -> impl Future<Output = ClientResult<AuthorizationToken<SmolStr>>> {
        async {
            Err(ClientError::auth(
                jacquard_common::error::AuthError::NotAuthenticated,
            ))
        }
    }
}

impl<A: AgentSession> HttpClient for Agent<A> {
    type Error = <A as HttpClient>::Error;

    #[cfg(not(target_arch = "wasm32"))]
    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> + Send
    {
        self.inner.send_http(request)
    }

    #[cfg(target_arch = "wasm32")]
    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> {
        self.inner.send_http(request)
    }
}

#[cfg(feature = "streaming")]
impl<A> jacquard_common::http_client::HttpClientExt for Agent<A>
where
    A: AgentSession + jacquard_common::http_client::HttpClientExt,
{
    #[cfg(not(target_arch = "wasm32"))]
    fn send_http_streaming(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<
        Output = core::result::Result<
            http::Response<jacquard_common::stream::ByteStream>,
            Self::Error,
        >,
    > + Send {
        self.inner.send_http_streaming(request)
    }

    #[cfg(target_arch = "wasm32")]
    fn send_http_streaming(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<
        Output = core::result::Result<
            http::Response<jacquard_common::stream::ByteStream>,
            Self::Error,
        >,
    > {
        self.inner.send_http_streaming(request)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn send_http_bidirectional<Str>(
        &self,
        parts: http::request::Parts,
        body: Str,
    ) -> impl Future<
        Output = core::result::Result<
            http::Response<jacquard_common::stream::ByteStream>,
            Self::Error,
        >,
    > + Send
    where
        Str: n0_future::Stream<
                Item = core::result::Result<bytes::Bytes, jacquard_common::StreamError>,
            > + Send
            + 'static,
    {
        self.inner.send_http_bidirectional(parts, body)
    }

    #[cfg(target_arch = "wasm32")]
    fn send_http_bidirectional<Str>(
        &self,
        parts: http::request::Parts,
        body: Str,
    ) -> impl Future<
        Output = core::result::Result<
            http::Response<jacquard_common::stream::ByteStream>,
            Self::Error,
        >,
    >
    where
        Str: n0_future::Stream<
                Item = core::result::Result<bytes::Bytes, jacquard_common::StreamError>,
            > + 'static,
    {
        self.inner.send_http_bidirectional(parts, body)
    }
}

impl<A: AgentSession> XrpcClient for Agent<A> {
    async fn base_uri(&self) -> Uri<String> {
        self.inner.base_uri().await
    }
    fn opts(&self) -> impl Future<Output = CallOptions> {
        self.inner.opts()
    }

    async fn set_opts(&self, opts: CallOptions) {
        self.inner.set_opts(opts).await
    }

    async fn set_base_uri(&self, uri: Uri<String>) {
        self.inner.set_base_uri(uri).await
    }
    fn send<R>(
        &self,
        request: R,
    ) -> impl Future<Output = XrpcResult<Response<<R as XrpcRequest>::Response>>>
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        async move { self.inner.send(request).await }
    }

    async fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions,
    ) -> XrpcResult<Response<<R as XrpcRequest>::Response>>
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        self.inner.send_with_opts(request, opts).await
    }
}

#[cfg(feature = "streaming")]
impl<A> jacquard_common::xrpc::XrpcStreamingClient for Agent<A>
where
    A: AgentSession + jacquard_common::xrpc::XrpcStreamingClient,
{
    #[cfg(not(target_arch = "wasm32"))]
    fn download<R>(
        &self,
        request: R,
    ) -> impl Future<
        Output = core::result::Result<
            jacquard_common::xrpc::StreamingResponse,
            jacquard_common::StreamError,
        >,
    > + Send
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync,
    {
        self.inner.download(request)
    }

    #[cfg(target_arch = "wasm32")]
    fn download<R>(
        &self,
        request: R,
    ) -> impl Future<
        Output = core::result::Result<
            jacquard_common::xrpc::StreamingResponse,
            jacquard_common::StreamError,
        >,
    >
    where
        R: XrpcRequest + Send + Sync + Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
    {
        self.inner.download(request)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn stream<S, B>(
        &self,
        stream: jacquard_common::xrpc::XrpcProcedureSend<S::Frame<B>>,
    ) -> impl Future<
        Output = core::result::Result<
            jacquard_common::xrpc::XrpcResponseStream<<<S as jacquard_common::xrpc::XrpcProcedureStream>::Response as jacquard_common::xrpc::XrpcStreamResp>::Frame<B>>,
            jacquard_common::StreamError,
        >,
    >
    where
        B: BosStr + 'static,
        S: jacquard_common::xrpc::XrpcProcedureStream + 'static,
        <<S as jacquard_common::xrpc::XrpcProcedureStream>::Response as jacquard_common::xrpc::XrpcStreamResp>::Frame<B>: jacquard_common::xrpc::XrpcStreamResp,
        Self: Sync,
    {
        self.inner.stream::<S, B>(stream)
    }

    #[cfg(target_arch = "wasm32")]
    fn stream<S, B>(
        &self,
        stream: jacquard_common::xrpc::XrpcProcedureSend<S::Frame<B>>,
    ) -> impl Future<
        Output = core::result::Result<
            jacquard_common::xrpc::XrpcResponseStream<<<S as jacquard_common::xrpc::XrpcProcedureStream>::Response as jacquard_common::xrpc::XrpcStreamResp>::Frame<B>>,
            jacquard_common::StreamError,
        >,
    >
    where
        B: BosStr + 'static,
        S: jacquard_common::xrpc::XrpcProcedureStream + 'static,
        <<S as jacquard_common::xrpc::XrpcProcedureStream>::Response as jacquard_common::xrpc::XrpcStreamResp>::Frame<B>: jacquard_common::xrpc::XrpcStreamResp,
    {
        self.inner.stream::<S, B>(stream)
    }
}

impl<A: AgentSession + IdentityResolver> IdentityResolver for Agent<A> {
    fn options(&self) -> &ResolverOptions {
        self.inner.options()
    }

    fn resolve_handle<S: BosStr + Sync>(
        &self,
        handle: &Handle<S>,
    ) -> impl Future<Output = core::result::Result<Did, IdentityError>> {
        async { self.inner.resolve_handle(handle).await }
    }

    fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        did: &Did<S>,
    ) -> impl Future<Output = core::result::Result<DidDocResponse, IdentityError>> {
        async { self.inner.resolve_did_doc(did).await }
    }
}

impl<A: AgentSession> AgentSession for Agent<A> {
    fn session_kind(&self) -> AgentKind {
        self.kind()
    }

    fn session_info(&self) -> impl Future<Output = Option<(Did, Option<SmolStr>)>> {
        async { self.info().await }
    }

    fn endpoint(&self) -> impl Future<Output = Uri<String>> {
        async { self.endpoint().await }
    }

    fn set_options(&self, opts: CallOptions) -> impl Future<Output = ()> {
        async { self.set_options(opts).await }
    }

    fn refresh(&self) -> impl Future<Output = ClientResult<AuthorizationToken<SmolStr>>> {
        async { self.refresh().await }
    }
}

impl<A: AgentSession> From<A> for Agent<A> {
    fn from(inner: A) -> Self {
        Self::new(inner)
    }
}
