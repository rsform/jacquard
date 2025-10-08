//! XRPC client implementation for AT Protocol
//!
//! This module provides HTTP and XRPC client traits along with an authenticated
//! client implementation that manages session tokens.

pub mod credential_session;
pub mod token;

pub use jacquard_common::error::{ClientError, XrpcResult};
pub use jacquard_common::session::{MemorySessionStore, SessionStore, SessionStoreError};
use jacquard_common::{
    CowStr, IntoStatic,
    types::string::{Did, Handle},
};
pub use token::FileAuthStore;

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

#[derive(Debug, Clone)]
pub enum AuthSession {
    AppPassword(AtpSession),
    OAuth(jacquard_oauth::session::ClientSessionData<'static>),
}

impl AuthSession {
    pub fn did(&self) -> &Did<'static> {
        match self {
            AuthSession::AppPassword(session) => &session.did,
            AuthSession::OAuth(session) => &session.token_set.sub,
        }
    }

    pub fn refresh_token(&self) -> Option<&CowStr<'static>> {
        match self {
            AuthSession::AppPassword(session) => Some(&session.refresh_jwt),
            AuthSession::OAuth(session) => session.token_set.refresh_token.as_ref(),
        }
    }

    pub fn access_token(&self) -> &CowStr<'static> {
        match self {
            AuthSession::AppPassword(session) => &session.access_jwt,
            AuthSession::OAuth(session) => &session.token_set.access_token,
        }
    }

    pub fn set_refresh_token(&mut self, token: CowStr<'_>) {
        match self {
            AuthSession::AppPassword(session) => {
                session.refresh_jwt = token.into_static();
            }
            AuthSession::OAuth(session) => {
                session.token_set.refresh_token = Some(token.into_static());
            }
        }
    }

    pub fn set_access_token(&mut self, token: CowStr<'_>) {
        match self {
            AuthSession::AppPassword(session) => {
                session.access_jwt = token.into_static();
            }
            AuthSession::OAuth(session) => {
                session.token_set.access_token = token.into_static();
            }
        }
    }
}

impl From<AtpSession> for AuthSession {
    fn from(session: AtpSession) -> Self {
        AuthSession::AppPassword(session)
    }
}

impl From<jacquard_oauth::session::ClientSessionData<'static>> for AuthSession {
    fn from(session: jacquard_oauth::session::ClientSessionData<'static>) -> Self {
        AuthSession::OAuth(session)
    }
}
