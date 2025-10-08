//! XRPC client implementation for AT Protocol
//!
//! This module provides HTTP and XRPC client traits along with an authenticated
//! client implementation that manages session tokens.

pub mod credential_session;
pub mod token;

use core::future::Future;

use jacquard_common::AuthorizationToken;
use jacquard_common::error::TransportError;
pub use jacquard_common::error::{ClientError, XrpcResult};
pub use jacquard_common::session::{MemorySessionStore, SessionStore, SessionStoreError};
use jacquard_common::types::xrpc::{CallOptions, Response, XrpcClient, XrpcRequest};
use jacquard_common::{
    CowStr, IntoStatic,
    types::string::{Did, Handle},
};
use jacquard_common::{http_client::HttpClient, types::xrpc::XrpcExt};
use jacquard_identity::resolver::IdentityResolver;
use jacquard_oauth::authstore::ClientAuthStore;
use jacquard_oauth::client::OAuthSession;
use jacquard_oauth::dpop::DpopExt;
use jacquard_oauth::resolver::OAuthResolver;
pub use token::FileAuthStore;

use crate::client::credential_session::{CredentialSession, SessionKey};

pub(crate) const NSID_REFRESH_SESSION: &str = "com.atproto.server.refreshSession";

/// Basic client wrapper: reqwest transport + in-memory session store.
pub struct BasicClient(); //AtClient<reqwest::Client, MemorySessionStore<Did<'static>, AuthSession>>);

/// App password session information from `com.atproto.server.createSession`
///
/// Contains the access and refresh tokens along with user identity information.
#[derive(Debug, Clone)]
pub struct AtpSession {
    /// Access token (JWT) used for authenticated requests
    pub access_jwt: CowStr<'static>,
    /// Refresh token (JWT) used to obtain new access tokens
    pub refresh_jwt: CowStr<'static>,
    /// User's DID (Decentralized Identifier)
    pub did: Did<'static>,
    /// User's handle (e.g., "alice.bsky.social")
    pub handle: Handle<'static>,
}

impl From<jacquard_api::com_atproto::server::create_session::CreateSessionOutput<'_>>
    for AtpSession
{
    fn from(
        output: jacquard_api::com_atproto::server::create_session::CreateSessionOutput<'_>,
    ) -> Self {
        Self {
            access_jwt: output.access_jwt.into_static(),
            refresh_jwt: output.refresh_jwt.into_static(),
            did: output.did.into_static(),
            handle: output.handle.into_static(),
        }
    }
}

impl From<jacquard_api::com_atproto::server::refresh_session::RefreshSessionOutput<'_>>
    for AtpSession
{
    fn from(
        output: jacquard_api::com_atproto::server::refresh_session::RefreshSessionOutput<'_>,
    ) -> Self {
        Self {
            access_jwt: output.access_jwt.into_static(),
            refresh_jwt: output.refresh_jwt.into_static(),
            did: output.did.into_static(),
            handle: output.handle.into_static(),
        }
    }
}

/// A unified indicator for the type of authenticated session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    /// App password (Bearer) session
    AppPassword,
    /// OAuth (DPoP) session
    OAuth,
}

/// Common interface for stateful sessions used by the Agent wrapper.
pub trait AgentSession: XrpcClient + HttpClient + Send + Sync {
    /// Identify the kind of session.
    fn session_kind(&self) -> AgentKind;
    /// Return current DID and an optional session id (always Some for OAuth).
    fn session_info(
        &self,
    ) -> core::pin::Pin<
        Box<dyn Future<Output = Option<(Did<'static>, Option<CowStr<'static>>)>> + Send + '_>,
    >;
    /// Current base endpoint.
    fn endpoint(&self) -> core::pin::Pin<Box<dyn Future<Output = url::Url> + Send + '_>>;
    /// Override per-session call options.
    fn set_options<'a>(
        &'a self,
        opts: CallOptions<'a>,
    ) -> core::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
    /// Refresh the session and return a fresh AuthorizationToken.
    fn refresh(
        &self,
    ) -> core::pin::Pin<
        Box<dyn Future<Output = Result<AuthorizationToken<'static>, ClientError>> + Send + '_>,
    >;
}

impl<S, T> AgentSession for CredentialSession<S, T>
where
    S: SessionStore<SessionKey, AtpSession> + Send + Sync + 'static,
    T: IdentityResolver + HttpClient + XrpcExt + Send + Sync + 'static,
{
    fn session_kind(&self) -> AgentKind {
        AgentKind::AppPassword
    }
    fn session_info(
        &self,
    ) -> core::pin::Pin<
        Box<dyn Future<Output = Option<(Did<'static>, Option<CowStr<'static>>)>> + Send + '_>,
    > {
        Box::pin(async move {
            CredentialSession::<S, T>::session_info(self)
                .await
                .map(|(did, sid)| (did, Some(sid)))
        })
    }
    fn endpoint(&self) -> core::pin::Pin<Box<dyn Future<Output = url::Url> + Send + '_>> {
        Box::pin(async move { CredentialSession::<S, T>::endpoint(self).await })
    }
    fn set_options<'a>(
        &'a self,
        opts: CallOptions<'a>,
    ) -> core::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move { CredentialSession::<S, T>::set_options(self, opts).await })
    }
    fn refresh(
        &self,
    ) -> core::pin::Pin<
        Box<dyn Future<Output = Result<AuthorizationToken<'static>, ClientError>> + Send + '_>,
    > {
        Box::pin(async move {
            Ok(CredentialSession::<S, T>::refresh(self)
                .await?
                .into_static())
        })
    }
}

impl<T, S> AgentSession for OAuthSession<T, S>
where
    S: ClientAuthStore + Send + Sync + 'static,
    T: OAuthResolver + DpopExt + XrpcExt + Send + Sync + 'static,
{
    fn session_kind(&self) -> AgentKind {
        AgentKind::OAuth
    }
    fn session_info(
        &self,
    ) -> core::pin::Pin<
        Box<dyn Future<Output = Option<(Did<'static>, Option<CowStr<'static>>)>> + Send + '_>,
    > {
        Box::pin(async move {
            let (did, sid) = OAuthSession::<T, S>::session_info(self).await;
            Some((did.into_static(), Some(sid.into_static())))
        })
    }
    fn endpoint(&self) -> core::pin::Pin<Box<dyn Future<Output = url::Url> + Send + '_>> {
        Box::pin(async move { self.endpoint().await })
    }
    fn set_options<'a>(
        &'a self,
        opts: CallOptions<'a>,
    ) -> core::pin::Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move { self.set_options(opts).await })
    }
    fn refresh(
        &self,
    ) -> core::pin::Pin<
        Box<dyn Future<Output = Result<AuthorizationToken<'static>, ClientError>> + Send + '_>,
    > {
        Box::pin(async move {
            self.refresh()
                .await
                .map(|t| t.into_static())
                .map_err(|e| ClientError::Transport(TransportError::Other(Box::new(e))))
        })
    }
}

/// Thin wrapper that erases the concrete session type while preserving type-safety.
pub struct Agent<A: AgentSession> {
    inner: A,
}

impl<A: AgentSession> Agent<A> {
    /// Wrap an existing session in an Agent.
    pub fn new(inner: A) -> Self {
        Self { inner }
    }

    /// Return the underlying session kind.
    pub fn kind(&self) -> AgentKind {
        self.inner.session_kind()
    }

    /// Return session info if available.
    pub async fn info(&self) -> Option<(Did<'static>, Option<CowStr<'static>>)> {
        self.inner.session_info().await
    }

    /// Get current endpoint.
    pub async fn endpoint(&self) -> url::Url {
        self.inner.endpoint().await
    }

    /// Override call options.
    pub async fn set_options(&self, opts: CallOptions<'_>) {
        self.inner.set_options(opts).await
    }

    /// Refresh the session and return a fresh token.
    pub async fn refresh(&self) -> Result<AuthorizationToken<'static>, ClientError> {
        self.inner.refresh().await
    }
}

impl<A: AgentSession> HttpClient for Agent<A> {
    type Error = <A as HttpClient>::Error;

    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = core::result::Result<http::Response<Vec<u8>>, Self::Error>> + Send
    {
        self.inner.send_http(request)
    }
}

impl<A: AgentSession> XrpcClient for Agent<A> {
    fn base_uri(&self) -> url::Url {
        self.inner.base_uri()
    }
    fn opts(&self) -> impl Future<Output = CallOptions<'_>> {
        self.inner.opts()
    }
    fn send<R: XrpcRequest + Send>(
        self,
        request: &R,
    ) -> impl Future<Output = XrpcResult<Response<R>>> {
        async move { self.inner.send(request).await }
    }
}
