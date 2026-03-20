use jacquard_common::{CowStr, IntoStatic};
use serde::{Deserialize, Serialize};

/// The `response_type` parameter for an OAuth 2.0 authorization request.
///
/// Determines what the authorization server returns in the redirect response.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationResponseType {
    /// Authorization code flow — server returns a short-lived code for token exchange.
    Code,
    /// Implicit flow — server returns an access token directly (not recommended for new clients).
    Token,
    /// OpenID Connect ID token response (see the
    /// [multiple response types spec](https://openid.net/specs/oauth-v2-multiple-response-types-1_0.html)).
    IdToken,
}

/// The `response_mode` parameter controlling how the authorization response is returned.
///
/// Defaults to `query` for `code` response type and `fragment` for `token`.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationResponseMode {
    /// Parameters are appended as query string components to the redirect URI.
    Query,
    /// Parameters are appended as URI fragment components to the redirect URI.
    Fragment,
    /// Parameters are encoded in an HTML form POSTed to the redirect URI.
    ///
    /// <https://openid.net/specs/oauth-v2-form-post-response-mode-1_0.html#FormPostResponseMode>
    FormPost,
}

/// PKCE code challenge method, as defined in RFC 7636.
///
/// `S256` is strongly preferred; `Plain` should only be used when the client
/// cannot perform SHA-256.
#[derive(Serialize, Deserialize, Debug)]
pub enum AuthorizationCodeChallengeMethod {
    /// SHA-256 hash of the code verifier, base64url-encoded (recommended).
    S256,
    /// Raw code verifier used as the challenge (not recommended).
    #[serde(rename = "plain")]
    Plain,
}

/// Parameters for a Pushed Authorization Request (PAR), as defined in RFC 9126.
///
/// PAR allows clients to push their authorization parameters directly to the
/// authorization server before redirecting the user, improving security by keeping
/// parameters out of the browser URL.
#[derive(Serialize, Deserialize, Debug)]
pub struct ParParameters<'a> {
    /// The response type to request (e.g. `code`).
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc6749#section-4.1.1>
    pub response_type: AuthorizationResponseType,
    /// The redirect URI where the authorization response will be sent.
    #[serde(borrow)]
    pub redirect_uri: CowStr<'a>,
    /// An opaque CSRF state value to be echoed back in the callback.
    pub state: CowStr<'a>,
    /// Space-separated list of requested scopes.
    pub scope: Option<CowStr<'a>>,
    /// How the authorization response parameters are delivered to the client.
    ///
    /// <https://openid.net/specs/oauth-v2-multiple-response-types-1_0.html#ResponseModes>
    pub response_mode: Option<AuthorizationResponseMode>,
    /// The PKCE code challenge derived from the code verifier.
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc7636#section-4.3>
    pub code_challenge: CowStr<'a>,
    /// The method used to derive the code challenge.
    pub code_challenge_method: AuthorizationCodeChallengeMethod,
    /// Hint to pre-fill the login form with a handle or email.
    ///
    /// <https://openid.net/specs/openid-connect-core-1_0.html#AuthRequest>
    pub login_hint: Option<CowStr<'a>>,
    /// Prompt hint controlling authorization server UI behavior.
    pub prompt: Option<CowStr<'a>>,
}

/// The `grant_type` parameter for a token endpoint request.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenGrantType {
    /// Exchange an authorization code for tokens.
    AuthorizationCode,
    /// Use a refresh token to obtain a new access token.
    RefreshToken,
}

/// Parameters for exchanging an authorization code for tokens (RFC 6749 §4.1.3).
#[derive(Serialize, Deserialize)]
pub struct TokenRequestParameters<'a> {
    /// Must be `authorization_code` for the authorization code grant.
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc6749#section-4.1.3>
    pub grant_type: TokenGrantType,
    /// The authorization code received from the authorization server.
    #[serde(borrow)]
    pub code: CowStr<'a>,
    /// The redirect URI used in the original authorization request.
    pub redirect_uri: CowStr<'a>,
    /// The PKCE code verifier that was used to generate the code challenge (RFC 7636 §4.5).
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc7636#section-4.5>
    pub code_verifier: CowStr<'a>,
}

/// Parameters for refreshing an access token using a refresh token (RFC 6749 §6).
#[derive(Serialize, Deserialize)]
pub struct RefreshRequestParameters<'a> {
    /// Must be `refresh_token` for the refresh grant.
    ///
    /// <https://datatracker.ietf.org/doc/html/rfc6749#section-6>
    pub grant_type: TokenGrantType,
    /// The refresh token previously issued to the client.
    #[serde(borrow)]
    pub refresh_token: CowStr<'a>,
    /// Optional scope to request; must not exceed the originally granted scope.
    pub scope: Option<CowStr<'a>>,
}

/// Parameters for a token revocation request (RFC 7009 §2.1).
///
/// Sent to the revocation endpoint to invalidate an access or refresh token,
/// for example on logout.
///
/// <https://datatracker.ietf.org/doc/html/rfc7009#section-2.1>
#[derive(Serialize, Deserialize)]
pub struct RevocationRequestParameters<'a> {
    /// The token to be revoked.
    #[serde(borrow)]
    pub token: CowStr<'a>,
    // ?
    // pub token_type_hint: Option<String>,
}

impl IntoStatic for RevocationRequestParameters<'_> {
    type Output = RevocationRequestParameters<'static>;

    fn into_static(self) -> Self::Output {
        Self::Output {
            token: self.token.into_static(),
        }
    }
}

impl IntoStatic for TokenRequestParameters<'_> {
    type Output = TokenRequestParameters<'static>;

    fn into_static(self) -> Self::Output {
        Self::Output {
            grant_type: self.grant_type,
            code: self.code.into_static(),
            redirect_uri: self.redirect_uri.into_static(),
            code_verifier: self.code_verifier.into_static(),
        }
    }
}

impl IntoStatic for RefreshRequestParameters<'_> {
    type Output = RefreshRequestParameters<'static>;

    fn into_static(self) -> Self::Output {
        Self::Output {
            grant_type: self.grant_type,
            refresh_token: self.refresh_token.into_static(),
            scope: self.scope.map(CowStr::into_static),
        }
    }
}

impl IntoStatic for ParParameters<'_> {
    type Output = ParParameters<'static>;

    fn into_static(self) -> Self::Output {
        Self::Output {
            redirect_uri: self.redirect_uri.into_static(),
            response_type: self.response_type,
            scope: self.scope.into_static(),
            code_challenge: self.code_challenge.into_static(),
            code_challenge_method: self.code_challenge_method,
            state: self.state.into_static(),
            response_mode: self.response_mode,
            login_hint: self.login_hint.into_static(),
            prompt: self.prompt.into_static(),
        }
    }
}
