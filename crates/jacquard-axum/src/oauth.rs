//! OAuth web helpers for Axum applications.
//!
//! This module adapts [`jacquard::oauth::client::OAuthClient`] to browser and
//! server-side Axum flows. The OAuth client remains the single source of truth:
//! metadata, authorization starts, callbacks, session restore, and logout all
//! use the same client exposed from application state via [`OAuthWebState`].
//!
//! Two authenticated-request styles are provided:
//!
//! - [`ExtractOAuthSession`] is strict and intended for APIs/headless routes.
//!   Missing or unusable auth rejects the request.
//! - [`BrowserOAuthSession`] is browser-oriented. Missing or unusable auth
//!   redirects to a configured login/start route and preserves a local
//!   `return_to` target through the OAuth round trip using short-lived private
//!   cookies keyed by OAuth state.
//!
//! The private browser session cookie stores only an encoded
//! [`SessionKey`], never OAuth tokens.

use std::{fmt, str::FromStr, sync::Arc};

use axum::{
    Form, Json, Router,
    extract::{FromRef, FromRequestParts, Query, State},
    http::{HeaderMap, HeaderName, StatusCode, Uri, request::Parts},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jacquard::common::deps::smol_str::{SmolStr, ToSmolStr};
use jacquard::identity::lexicon_resolver::LexiconSchemaResolver;
use jacquard::{
    BosStr,
    common::session::SessionKey,
    oauth::{
        atproto::atproto_client_metadata,
        authstore::ClientAuthStore,
        client::{OAuthClient, OAuthSession},
        dpop::DpopExt,
        resolver::OAuthResolver,
        types::{AuthorizeOptions, CallbackParams},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Application-state access to the one OAuth client used by all web helpers.
pub trait OAuthWebState<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    /// Returns the configured OAuth client.
    fn oauth_client(&self) -> &OAuthClient<T, S>;
}

impl<T, S> OAuthWebState<T, S> for OAuthClient<T, S>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    fn oauth_client(&self) -> &OAuthClient<T, S> {
        self
    }
}

impl<T, S> OAuthWebState<T, S> for Arc<OAuthClient<T, S>>
where
    T: OAuthResolver,
    S: ClientAuthStore,
{
    fn oauth_client(&self) -> &OAuthClient<T, S> {
        self.as_ref()
    }
}

/// Route and cookie behavior for OAuth browser helpers.
#[derive(Clone, Debug)]
pub struct OAuthWebConfig {
    /// Private cookie containing the encoded [`SessionKey`].
    pub cookie_name: SmolStr,
    /// Prefix used for state-keyed private return-target cookies.
    pub return_cookie_prefix: SmolStr,
    /// Route that starts OAuth from an identifier.
    pub start_auth_path: SmolStr,
    /// Optional local login page used when no identifier is known.
    pub login_page_path: Option<SmolStr>,
    /// OAuth callback route.
    pub callback_path: SmolStr,
    /// Logout route.
    pub logout_path: SmolStr,
    /// Fallback redirect after a successful callback.
    pub after_callback_redirect: SmolStr,
    /// Fallback redirect after logout.
    pub after_logout_redirect: Option<SmolStr>,
    /// Header name for passing an encoded session key in API/headless requests.
    ///
    /// Clients that do not use browser cookies can pass the session key as this
    /// header value instead. The session key is a JSON-encoded
    /// `{"did": "...", "session_id": "..."}` string. Checked by
    /// [`ExtractOAuthSession`] and [`ExtractOptionalOAuthSession`] as a fallback
    /// when no session cookie is present. Defaults to `x-jacquard-session`.
    pub session_header: HeaderName,
}

impl Default for OAuthWebConfig {
    fn default() -> Self {
        Self {
            cookie_name: SmolStr::new_static("jacquard_oauth_session"),
            return_cookie_prefix: SmolStr::new_static("jacquard_oauth_return_"),
            start_auth_path: SmolStr::new_static("/oauth/start"),
            login_page_path: Some(SmolStr::new_static("/oauth/login")),
            callback_path: SmolStr::new_static("/oauth/callback"),
            logout_path: SmolStr::new_static("/oauth/logout"),
            after_callback_redirect: SmolStr::new_static("/"),
            after_logout_redirect: Some(SmolStr::new_static("/")),
            session_header: HeaderName::from_static("x-jacquard-session"),
        }
    }
}

/// Error type returned by OAuth Axum helpers.
#[derive(Debug, thiserror::Error)]
pub enum OAuthAxumError {
    /// The request did not contain a usable session binding.
    #[error("missing OAuth session")]
    MissingSession,
    /// A session key was syntactically invalid.
    #[error("malformed OAuth session key: {0}")]
    MalformedSessionKey(String),
    /// A return target was unsafe or invalid.
    #[error("invalid return target")]
    InvalidReturnTo,
    /// OAuth protocol/client error.
    #[error(transparent)]
    OAuth(#[from] jacquard::oauth::error::OAuthError),
    /// AT Protocol OAuth metadata conversion error.
    #[error(transparent)]
    Atproto(#[from] jacquard::oauth::atproto::Error),
    /// JSON serialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Form serialization error.
    #[error(transparent)]
    Form(#[from] serde_html_form::ser::Error),
}

impl OAuthAxumError {
    fn status(&self) -> StatusCode {
        match self {
            Self::MissingSession => StatusCode::UNAUTHORIZED,
            Self::MalformedSessionKey(_) | Self::InvalidReturnTo => StatusCode::BAD_REQUEST,
            Self::OAuth(err) if is_unauthorized_oauth_error(err) => StatusCode::UNAUTHORIZED,
            Self::OAuth(jacquard::oauth::error::OAuthError::Callback(_)) => StatusCode::BAD_REQUEST,
            Self::OAuth(_) | Self::Atproto(_) | Self::Json(_) | Self::Form(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::MissingSession => "AuthenticationRequired",
            Self::MalformedSessionKey(_) | Self::InvalidReturnTo => "InvalidRequest",
            Self::OAuth(jacquard::oauth::error::OAuthError::Callback(_)) => "InvalidRequest",
            Self::OAuth(err) if is_unauthorized_oauth_error(err) => "AuthenticationRequired",
            Self::OAuth(_) | Self::Atproto(_) | Self::Json(_) | Self::Form(_) => {
                "InternalServerError"
            }
        }
    }
}

impl IntoResponse for OAuthAxumError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code();
        (
            status,
            Json(json!({ "error": code, "message": self.to_string() })),
        )
            .into_response()
    }
}

/// Rejection returned by [`BrowserOAuthSession`].
#[derive(Debug)]
pub enum BrowserOAuthRejection {
    /// Redirect the browser and return an updated cookie jar.
    Redirect(PrivateCookieJar, Redirect),
    /// Non-auth infrastructure failure.
    Error(OAuthAxumError),
}

impl IntoResponse for BrowserOAuthRejection {
    fn into_response(self) -> Response {
        match self {
            Self::Redirect(jar, redirect) => (jar, redirect).into_response(),
            Self::Error(err) => err.into_response(),
        }
    }
}

/// Strict OAuth session extractor for API/headless routes.
///
/// Looks up the session key from a private cookie (browser clients) or a custom
/// header (API/headless clients). The header name is configurable via
/// [`OAuthWebConfig::session_header`] and defaults to `x-jacquard-session`.
///
/// API clients that do not use browser cookies can pass the encoded session key
/// (a JSON `{"did": "...", "session_id": "..."}` string) in this header to
/// authenticate without cookie infrastructure. If both a cookie and a header are
/// present, the cookie takes precedence.
///
/// Use [`ExtractOptionalOAuthSession`] for endpoints that serve both
/// authenticated and anonymous requests.
/// Use [`BrowserOAuthSession`] for browser routes that should redirect to login
/// on failure.
pub struct ExtractOAuthSession<T, S>(pub OAuthSession<T, S>)
where
    T: OAuthResolver,
    S: ClientAuthStore;

impl<T, S, AppState> FromRequestParts<AppState> for ExtractOAuthSession<T, S>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S> + Send + Sync,
    OAuthWebConfig: FromRef<AppState>,
    axum_extra::extract::cookie::Key: FromRef<AppState>,
{
    type Rejection = OAuthAxumError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let config = OAuthWebConfig::from_ref(state);
        let jar = PrivateCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| OAuthAxumError::MissingSession)?;
        let key = read_session_key(&jar, &parts.headers, &config)?;
        let session = restore_session(state.oauth_client(), &key).await?;
        Ok(Self(session))
    }
}

/// Browser-oriented OAuth session extractor that redirects unauthenticated users.
pub struct BrowserOAuthSession<T, S>(pub OAuthSession<T, S>)
where
    T: OAuthResolver,
    S: ClientAuthStore;

impl<T, S, AppState> FromRequestParts<AppState> for BrowserOAuthSession<T, S>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S> + Send + Sync,
    OAuthWebConfig: FromRef<AppState>,
    axum_extra::extract::cookie::Key: FromRef<AppState>,
{
    type Rejection = BrowserOAuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let config = OAuthWebConfig::from_ref(state);
        let jar = PrivateCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| BrowserOAuthRejection::Error(OAuthAxumError::MissingSession))?;
        let return_to = return_to_from_uri(&parts.uri).unwrap_or_else(|| "/".to_smolstr());

        let Some(cookie) = jar.get(config.cookie_name.as_str()) else {
            return Err(redirect_to_login(jar, &config, &return_to));
        };

        let key = match decode_session_key(cookie.value()) {
            Ok(key) => key,
            Err(_) => {
                let jar = clear_session_cookie(jar, &config);
                return Err(redirect_to_login(jar, &config, &return_to));
            }
        };

        match restore_session(state.oauth_client(), &key).await {
            Ok(session) => Ok(Self(session)),
            Err(err) if is_unauthorized_oauth_error(&err) => {
                let jar = clear_session_cookie(jar, &config);
                Err(redirect_to_start_with_identifier(
                    jar,
                    &config,
                    key.did.as_str(),
                    &return_to,
                ))
            }
            Err(err) => Err(BrowserOAuthRejection::Error(err.into())),
        }
    }
}

/// Optional OAuth session extractor for API/headless routes.
///
/// Like [`ExtractOAuthSession`], but returns `None` when no session key is
/// present (no cookie and no header). If a session key IS present but invalid,
/// returns an error.
///
/// Use this for endpoints that serve both authenticated and anonymous requests.
pub struct ExtractOptionalOAuthSession<T, S>(pub Option<OAuthSession<T, S>>)
where
    T: OAuthResolver,
    S: ClientAuthStore;

impl<T, S, AppState> FromRequestParts<AppState> for ExtractOptionalOAuthSession<T, S>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S> + Send + Sync,
    OAuthWebConfig: FromRef<AppState>,
    axum_extra::extract::cookie::Key: FromRef<AppState>,
{
    type Rejection = OAuthAxumError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let config = OAuthWebConfig::from_ref(state);
        let jar = PrivateCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| OAuthAxumError::MissingSession)?;
        match read_session_key(&jar, &parts.headers, &config) {
            Ok(key) => {
                let session = restore_session(state.oauth_client(), &key).await?;
                Ok(Self(Some(session)))
            }
            Err(OAuthAxumError::MissingSession) => Ok(Self(None)),
            Err(e) => Err(e),
        }
    }
}

/// Optional browser-oriented OAuth session extractor.
///
/// Like [`BrowserOAuthSession`], but returns `None` when no session cookie is
/// present instead of redirecting to login. If a cookie IS present but the
/// session is invalid or expired, the cookie is cleared and the extractor
/// still returns `None` rather than erroring or redirecting.
///
/// Use this for browser pages that show different content for authenticated
/// vs. anonymous users, without forcing a login redirect.
pub struct OptionalBrowserOAuthSession<T, S>(pub Option<OAuthSession<T, S>>)
where
    T: OAuthResolver,
    S: ClientAuthStore;

impl<T, S, AppState> FromRequestParts<AppState> for OptionalBrowserOAuthSession<T, S>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S> + Send + Sync,
    OAuthWebConfig: FromRef<AppState>,
    axum_extra::extract::cookie::Key: FromRef<AppState>,
{
    type Rejection = OAuthAxumError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let config = OAuthWebConfig::from_ref(state);
        let jar = PrivateCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| OAuthAxumError::MissingSession)?;

        let Some(cookie) = jar.get(config.cookie_name.as_str()) else {
            return Ok(Self(None));
        };

        let key = match decode_session_key(cookie.value()) {
            Ok(key) => key,
            Err(_) => {
                // Stale or malformed cookie — clear it and treat as anonymous.
                let jar = clear_session_cookie(jar, &config);
                // The jar is consumed by clear; we can't write it back here, so
                // we just return None. The cookie will expire naturally.
                drop(jar);
                return Ok(Self(None));
            }
        };

        match restore_session(state.oauth_client(), &key).await {
            Ok(session) => Ok(Self(Some(session))),
            Err(err) if is_unauthorized_oauth_error(&err) => {
                // Expired/revoked session — treat as anonymous rather than erroring.
                Ok(Self(None))
            }
            Err(err) => Err(err.into()),
        }
    }
}

/// Query/form fields accepted by the default start-auth route adapters.
#[derive(Debug, Clone, Deserialize)]
pub struct StartAuthRequest {
    /// Handle, DID, PDS URL, or entryway identifier to authenticate.
    pub identifier: SmolStr,
    /// Optional local path to return to after callback.
    #[serde(default)]
    pub return_to: Option<SmolStr>,
}

/// Conventional OAuth web routes using the state-provided OAuth client.
///
/// The route paths are taken from `config` so that custom
/// [`OAuthWebConfig`] paths are honored consistently with the redirects and
/// cookie scopes produced by the browser helpers. Passing the same config
/// instance that you expose via [`FromRef`] keeps the mounted routes and the
/// helpers in agreement.
pub fn routes<T, S, AppState>(config: &OAuthWebConfig) -> Router<AppState>
where
    T: OAuthResolver + DpopExt + LexiconSchemaResolver + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S> + Clone + Send + Sync + 'static,
    OAuthWebConfig: FromRef<AppState>,
    axum_extra::extract::cookie::Key: FromRef<AppState>,
{
    Router::new()
        .route(
            "/oauth-client-metadata.json",
            get(client_metadata_handler::<T, S, AppState>),
        )
        .route(
            config.start_auth_path.as_str(),
            get(start_auth_query::<T, S, AppState>),
        )
        .route(
            config.start_auth_path.as_str(),
            post(start_auth_form::<T, S, AppState>),
        )
        .route(
            config.callback_path.as_str(),
            get(callback_handler::<T, S, AppState>),
        )
        .route(
            config.logout_path.as_str(),
            post(logout_handler::<T, S, AppState>),
        )
}

/// Serve OAuth client metadata derived from the state OAuth client.
pub async fn client_metadata_handler<T, S, AppState>(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, OAuthAxumError>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S>,
{
    let oauth = state.oauth_client();
    let metadata = atproto_client_metadata(
        &oauth.registry.client_data.config,
        &oauth.registry.client_data.keyset,
    )?;
    Ok(Json(metadata))
}

/// Start OAuth from query parameters and return an authorization redirect.
pub async fn start_auth_query<T, S, AppState>(
    State(state): State<AppState>,
    State(config): State<OAuthWebConfig>,
    jar: PrivateCookieJar,
    Query(input): Query<StartAuthRequest>,
) -> Result<(PrivateCookieJar, Redirect), OAuthAxumError>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S>,
{
    start_auth_with_return_cookie(state.oauth_client(), &config, jar, input).await
}

/// Start OAuth from form fields and return an authorization redirect.
pub async fn start_auth_form<T, S, AppState>(
    State(state): State<AppState>,
    State(config): State<OAuthWebConfig>,
    jar: PrivateCookieJar,
    Form(input): Form<StartAuthRequest>,
) -> Result<(PrivateCookieJar, Redirect), OAuthAxumError>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S>,
{
    start_auth_with_return_cookie(state.oauth_client(), &config, jar, input).await
}

/// Complete OAuth callback, set the private session cookie, and redirect.
pub async fn callback_handler<T, S, AppState>(
    State(state): State<AppState>,
    State(config): State<OAuthWebConfig>,
    jar: PrivateCookieJar,
    Query(params): Query<CallbackParams>,
) -> Result<(PrivateCookieJar, Redirect), OAuthAxumError>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static + LexiconSchemaResolver,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S>,
{
    let state_value = params.state.clone();
    let session = state.oauth_client().callback(params).await?;
    let (did, session_id) = session.session_info().await;
    let key = SessionKey::new(did, session_id);
    let mut jar = set_session_cookie(jar, &config, &key)?;
    let redirect_to = if let Some(state_value) = state_value {
        let (new_jar, return_to) = take_return_to_cookie(jar, &config, state_value.as_ref());
        jar = new_jar;
        return_to.unwrap_or_else(|| config.after_callback_redirect.clone())
    } else {
        config.after_callback_redirect.clone()
    };
    Ok((jar, Redirect::to(redirect_to.as_str())))
}

/// Logout the current OAuth session, clear the private session cookie, and redirect or return 204.
pub async fn logout_handler<T, S, AppState>(
    State(state): State<AppState>,
    State(config): State<OAuthWebConfig>,
    jar: PrivateCookieJar,
    headers: HeaderMap,
) -> Result<Response, OAuthAxumError>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    AppState: OAuthWebState<T, S>,
{
    let key = read_session_key(&jar, &headers, &config)?;
    let session = restore_session(state.oauth_client(), &key).await?;
    session.logout().await?;
    let jar = clear_session_cookie(jar, &config);
    Ok(match &config.after_logout_redirect {
        Some(path) => (jar, Redirect::to(path.as_str())).into_response(),
        None => (jar, StatusCode::NO_CONTENT).into_response(),
    })
}

/// Start OAuth and return a redirect to the authorization server.
pub async fn start_auth_redirect<T, S, I, Str>(
    oauth: &OAuthClient<T, S>,
    identifier: I,
    options: AuthorizeOptions<Str>,
) -> Result<Redirect, OAuthAxumError>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
    I: AsRef<str>,
    Str: BosStr + FromStr + Ord + Clone + fmt::Debug,
    <Str as FromStr>::Err: fmt::Debug,
{
    let url = oauth.start_auth(identifier.as_ref(), options).await?;
    Ok(Redirect::temporary(&url))
}

/// Encode a session key for cookies or headers.
pub fn encode_session_key(key: &SessionKey) -> Result<String, OAuthAxumError> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(key)?))
}

/// Decode a session key from cookies or headers.
pub fn decode_session_key(value: &str) -> Result<SessionKey, OAuthAxumError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|err| OAuthAxumError::MalformedSessionKey(err.to_string()))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| OAuthAxumError::MalformedSessionKey(err.to_string()))
}

/// Validate a local browser return target.
pub fn validate_return_to(value: &str) -> Result<SmolStr, OAuthAxumError> {
    if !value.starts_with('/')
        || value.starts_with("//")
        || value.contains('\\')
        || value.chars().any(|ch| ch.is_control())
    {
        return Err(OAuthAxumError::InvalidReturnTo);
    }
    Ok(SmolStr::from(value))
}

/// Set the private browser session cookie.
pub fn set_session_cookie(
    jar: PrivateCookieJar,
    config: &OAuthWebConfig,
    key: &SessionKey,
) -> Result<PrivateCookieJar, OAuthAxumError> {
    let mut cookie = Cookie::new(config.cookie_name.to_string(), encode_session_key(key)?);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    Ok(jar.add(cookie))
}

/// Clear the private browser session cookie.
pub fn clear_session_cookie(jar: PrivateCookieJar, config: &OAuthWebConfig) -> PrivateCookieJar {
    let mut cookie = Cookie::from(config.cookie_name.to_string());
    cookie.set_path("/");
    jar.remove(cookie)
}

async fn restore_session<T, S>(
    oauth: &OAuthClient<T, S>,
    key: &SessionKey,
) -> Result<OAuthSession<T, S>, jacquard::oauth::error::OAuthError>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    oauth.restore(&key.did, key.session_id.as_str()).await
}

fn read_session_key(
    jar: &PrivateCookieJar,
    headers: &HeaderMap,
    config: &OAuthWebConfig,
) -> Result<SessionKey, OAuthAxumError> {
    if let Some(cookie) = jar.get(config.cookie_name.as_str()) {
        return decode_session_key(cookie.value());
    }
    if let Some(header) = headers.get(&config.session_header) {
        let value = header
            .to_str()
            .map_err(|err| OAuthAxumError::MalformedSessionKey(err.to_string()))?;
        return decode_session_key(value);
    }
    Err(OAuthAxumError::MissingSession)
}

async fn start_auth_with_return_cookie<T, S>(
    oauth: &OAuthClient<T, S>,
    config: &OAuthWebConfig,
    jar: PrivateCookieJar,
    input: StartAuthRequest,
) -> Result<(PrivateCookieJar, Redirect), OAuthAxumError>
where
    T: OAuthResolver + DpopExt + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    let mut options = AuthorizeOptions::<SmolStr>::default();
    let jar = if let Some(return_to) = input.return_to.as_deref() {
        let return_to = validate_return_to(return_to)?;
        let state = jacquard::oauth::utils::generate_nonce();
        options.state = Some(state.clone());
        set_return_to_cookie(jar, config, state.as_str(), return_to.as_str())
    } else {
        jar
    };
    let redirect = start_auth_redirect(oauth, input.identifier, options).await?;
    Ok((jar, redirect))
}

fn set_return_to_cookie(
    jar: PrivateCookieJar,
    config: &OAuthWebConfig,
    state: &str,
    return_to: &str,
) -> PrivateCookieJar {
    let mut cookie = Cookie::new(return_cookie_name(config, state), return_to.to_owned());
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path(config.callback_path.to_string());
    jar.add(cookie)
}

fn take_return_to_cookie(
    jar: PrivateCookieJar,
    config: &OAuthWebConfig,
    state: &str,
) -> (PrivateCookieJar, Option<SmolStr>) {
    let name = return_cookie_name(config, state);
    let return_to = jar
        .get(&name)
        .and_then(|cookie| validate_return_to(cookie.value()).ok());
    let mut removal = Cookie::from(name);
    removal.set_path(config.callback_path.to_string());
    (jar.remove(removal), return_to)
}

fn return_cookie_name(config: &OAuthWebConfig, state: &str) -> String {
    format!(
        "{}{}",
        config.return_cookie_prefix,
        URL_SAFE_NO_PAD.encode(state.as_bytes())
    )
}

fn return_to_from_uri(uri: &Uri) -> Option<SmolStr> {
    let mut path = uri.path().to_owned();
    if let Some(query) = uri.query() {
        path.push('?');
        path.push_str(query);
    }
    validate_return_to(&path).ok()
}

fn redirect_to_login(
    jar: PrivateCookieJar,
    config: &OAuthWebConfig,
    return_to: &str,
) -> BrowserOAuthRejection {
    let path = config
        .login_page_path
        .as_ref()
        .unwrap_or(&config.start_auth_path);
    BrowserOAuthRejection::Redirect(
        jar,
        Redirect::temporary(&append_query(path, &[("return_to", return_to)])),
    )
}

fn redirect_to_start_with_identifier(
    jar: PrivateCookieJar,
    config: &OAuthWebConfig,
    identifier: &str,
    return_to: &str,
) -> BrowserOAuthRejection {
    BrowserOAuthRejection::Redirect(
        jar,
        Redirect::temporary(&append_query(
            &config.start_auth_path,
            &[("identifier", identifier), ("return_to", return_to)],
        )),
    )
}

fn append_query(path: &str, params: &[(&str, &str)]) -> String {
    #[derive(Serialize)]
    struct Pair<'a> {
        #[serde(flatten)]
        values: std::collections::BTreeMap<&'a str, &'a str>,
    }
    let values = params.iter().map(|(k, v)| (*k, *v)).collect();
    let query = serde_html_form::to_string(Pair { values }).unwrap_or_default();
    format!("{path}?{query}")
}

fn is_unauthorized_oauth_error(err: &jacquard::oauth::error::OAuthError) -> bool {
    match err {
        jacquard::oauth::error::OAuthError::Session(session_err) => {
            matches!(
                session_err,
                jacquard::oauth::session::Error::SessionNotFound
            )
        }
        jacquard::oauth::error::OAuthError::Request(request_err) => request_err.is_permanent(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jacquard::types::string::Did;

    #[test]
    fn session_key_codec_round_trips() {
        let key = SessionKey::new(Did::new_static("did:plc:alice").unwrap(), "state");
        let encoded = encode_session_key(&key).unwrap();
        assert_eq!(decode_session_key(&encoded).unwrap(), key);
    }

    #[test]
    fn session_key_codec_rejects_malformed_payload() {
        assert!(matches!(
            decode_session_key("not base64"),
            Err(OAuthAxumError::MalformedSessionKey(_))
        ));
    }

    #[test]
    fn return_to_validation_rejects_unsafe_targets() {
        assert_eq!(
            validate_return_to("/protected?x=1").unwrap(),
            "/protected?x=1"
        );
        assert!(validate_return_to("https://evil.example/").is_err());
        assert!(validate_return_to("//evil.example/").is_err());
        assert!(validate_return_to("/evil\\path").is_err());
    }

    #[test]
    fn return_cookie_names_are_keyed_by_state() {
        let config = OAuthWebConfig::default();
        assert_ne!(
            return_cookie_name(&config, "state-a"),
            return_cookie_name(&config, "state-b")
        );
    }
}
