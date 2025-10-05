use jacquard_common::{CowStr, IntoStatic};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationResponseType {
    Code,
    Token,
    // OIDC (https://openid.net/specs/oauth-v2-multiple-response-types-1_0.html)
    IdToken,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationResponseMode {
    Query,
    Fragment,
    // https://openid.net/specs/oauth-v2-form-post-response-mode-1_0.html#FormPostResponseMode
    FormPost,
}

#[derive(Serialize, Deserialize)]
pub enum AuthorizationCodeChallengeMethod {
    S256,
    #[serde(rename = "plain")]
    Plain,
}

#[derive(Serialize, Deserialize)]
pub struct PushedAuthorizationRequestParameters<'a> {
    // https://datatracker.ietf.org/doc/html/rfc6749#section-4.1.1
    pub response_type: AuthorizationResponseType,
    #[serde(borrow)]
    pub redirect_uri: CowStr<'a>,
    pub state: CowStr<'a>,
    pub scope: Option<CowStr<'a>>,
    // https://openid.net/specs/oauth-v2-multiple-response-types-1_0.html#ResponseModes
    pub response_mode: Option<AuthorizationResponseMode>,
    // https://datatracker.ietf.org/doc/html/rfc7636#section-4.3
    pub code_challenge: CowStr<'a>,
    pub code_challenge_method: AuthorizationCodeChallengeMethod,
    // https://openid.net/specs/openid-connect-core-1_0.html#AuthRequest
    pub login_hint: Option<CowStr<'a>>,
    pub prompt: Option<CowStr<'a>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenGrantType {
    AuthorizationCode,
    RefreshToken,
}

#[derive(Serialize, Deserialize)]
pub struct TokenRequestParameters<'a> {
    // https://datatracker.ietf.org/doc/html/rfc6749#section-4.1.3
    pub grant_type: TokenGrantType,
    #[serde(borrow)]
    pub code: CowStr<'a>,
    pub redirect_uri: CowStr<'a>,
    // https://datatracker.ietf.org/doc/html/rfc7636#section-4.5
    pub code_verifier: CowStr<'a>,
}

#[derive(Serialize, Deserialize)]
pub struct RefreshRequestParameters<'a> {
    // https://datatracker.ietf.org/doc/html/rfc6749#section-6
    pub grant_type: TokenGrantType,
    #[serde(borrow)]
    pub refresh_token: CowStr<'a>,
    pub scope: Option<CowStr<'a>>,
}

// https://datatracker.ietf.org/doc/html/rfc7009#section-2.1
#[derive(Serialize, Deserialize)]
pub struct RevocationRequestParameters<'a> {
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

impl IntoStatic for PushedAuthorizationRequestParameters<'_> {
    type Output = PushedAuthorizationRequestParameters<'static>;

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
