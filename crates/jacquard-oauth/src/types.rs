mod client_metadata;
mod metadata;
mod request;
mod response;
mod token;

use crate::scopes::{ParseError, Scope, Scopes};

pub use self::client_metadata::*;
pub use self::metadata::*;
pub use self::request::*;
pub use self::response::*;
pub use self::token::*;
use jacquard_common::CowStr;
use jacquard_common::IntoStatic;
use jacquard_common::bos::{BosStr, DefaultStr};
use jacquard_common::deps::fluent_uri::Uri;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

/// The `prompt` parameter for an OAuth authorization request.
///
/// Controls whether the authorization server prompts the user for
/// re-authentication or re-consent, as defined in OpenID Connect Core §3.1.2.1.
#[derive(Debug, Deserialize, Clone, Copy)]
pub enum AuthorizeOptionPrompt {
    /// Prompt the user to re-authenticate.
    Login,
    /// Do not display any authentication or consent UI; fail if interaction is required.
    None,
    /// Prompt the user for explicit consent before issuing tokens.
    Consent,
    /// Prompt the user to select an account when multiple sessions are active.
    SelectAccount,
}

impl From<AuthorizeOptionPrompt> for CowStr<'static> {
    fn from(value: AuthorizeOptionPrompt) -> Self {
        CowStr::new_static(value.into())
    }
}

impl From<AuthorizeOptionPrompt> for SmolStr {
    fn from(value: AuthorizeOptionPrompt) -> Self {
        SmolStr::new_static(value.into())
    }
}

impl From<AuthorizeOptionPrompt> for &'static str {
    fn from(value: AuthorizeOptionPrompt) -> Self {
        match value {
            AuthorizeOptionPrompt::Login => "login",
            AuthorizeOptionPrompt::None => "none",
            AuthorizeOptionPrompt::Consent => "consent",
            AuthorizeOptionPrompt::SelectAccount => "select_account",
        }
    }
}

/// Options for initiating an OAuth authorization request.
#[derive(Debug)]
pub struct AuthorizeOptions<S: BosStr = DefaultStr>
where
    S: AsRef<str>,
{
    /// Override the redirect URI registered in the client metadata.
    pub redirect_uri: Option<Uri<String>>,
    /// Scopes to request. Defaults to an empty list (server-defined defaults apply).
    pub scopes: Scopes<S>,
    /// Optional prompt hint for the authorization server's UI.
    pub prompt: Option<AuthorizeOptionPrompt>,
    /// Opaque client-provided state value, echoed back in the callback for CSRF protection.
    pub state: Option<S>,
}

impl Default for AuthorizeOptions<DefaultStr> {
    fn default() -> Self {
        Self {
            redirect_uri: None,
            scopes: Scopes::empty(),
            prompt: None,
            state: None,
        }
    }
}

impl<S: BosStr + AsRef<str>> AuthorizeOptions<S> {
    /// Set the `prompt` parameter sent to the authorization server.
    pub fn with_prompt(mut self, prompt: AuthorizeOptionPrompt) -> Self {
        self.prompt = Some(prompt);
        self
    }

    /// Set a CSRF-protection `state` value to be echoed in the callback.
    pub fn with_state(mut self, state: S) -> Self {
        self.state = Some(state);
        self
    }

    /// Override the redirect URI for this specific authorization request.
    pub fn with_redirect_uri(mut self, redirect_uri: Uri<String>) -> Self {
        self.redirect_uri = Some(redirect_uri);
        self
    }

    /// Set the OAuth scopes to request.
    pub fn with_scopes(mut self, scopes: Scopes<S>) -> Self {
        self.scopes = scopes;
        self
    }
}

impl AuthorizeOptions<DefaultStr> {
    /// Parse and set OAuth scopes from a space-separated scope string.
    pub fn with_scope_str(mut self, scopes: impl AsRef<str>) -> Result<Self, ParseError> {
        self.scopes = Scopes::new(SmolStr::new(scopes.as_ref()))?;
        Ok(self)
    }

    /// Set OAuth scopes from one typed scope.
    pub fn with_scope(self, scope: Scope<SmolStr>) -> Result<Self, ParseError> {
        self.with_scope_iter([scope])
    }

    /// Set OAuth scopes from typed scope values.
    pub fn with_scope_iter<I>(mut self, scopes: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = Scope<SmolStr>>,
    {
        self.scopes = Scopes::from_scopes(scopes)?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_options_accept_scope_string() {
        let opts = AuthorizeOptions::default()
            .with_scope_str("rpc:* atproto")
            .unwrap();

        assert_eq!(opts.scopes.to_normalized_string(), "atproto rpc:*");
    }

    #[test]
    fn authorize_options_accept_typed_scopes() {
        let opts = AuthorizeOptions::default()
            .with_scope_iter([
                Scope::atproto(),
                Scope::rpc("app.bsky.feed.getTimeline").unwrap(),
                Scope::repo_create("app.bsky.feed.post").unwrap(),
            ])
            .unwrap();

        assert_eq!(
            opts.scopes.to_normalized_string(),
            "atproto repo:app.bsky.feed.post?action=create rpc:app.bsky.feed.getTimeline"
        );
    }

    #[test]
    fn authorize_options_accept_built_scopes() {
        let scopes = Scopes::builder()
            .atproto()
            .transition_generic()
            .rpc("app.bsky.feed.getTimeline")
            .unwrap()
            .build()
            .unwrap();
        let opts = AuthorizeOptions::default().with_scopes(scopes);

        assert_eq!(
            opts.scopes.to_normalized_string(),
            "atproto rpc:app.bsky.feed.getTimeline transition:generic"
        );
    }

    #[test]
    fn callback_params_form_round_trip() {
        let params = CallbackParams {
            code: SmolStr::new_static("authorization-code"),
            state: Some(SmolStr::new_static("state/with spaces")),
            iss: Some(SmolStr::new_static("https://issuer.example/path")),
        };

        let encoded = serde_html_form::to_string(&params).unwrap();
        assert_eq!(
            encoded,
            "code=authorization-code&state=state%2Fwith+spaces&iss=https%3A%2F%2Fissuer.example%2Fpath"
        );

        let decoded: CallbackParams = serde_html_form::from_str(&encoded).unwrap();
        assert_eq!(decoded.code, params.code);
        assert_eq!(decoded.state, params.state);
        assert_eq!(decoded.iss, params.iss);
    }
}

/// Query parameters delivered to the OAuth redirect URI after user authorization.
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S: serde::Serialize + BosStr",
    deserialize = "S: serde::Deserialize<'de> + BosStr"
))]
pub struct CallbackParams<S: BosStr = DefaultStr> {
    /// The authorization code issued by the authorization server.
    pub code: S,
    /// The `state` value originally sent in the authorization request, used to
    /// verify the response belongs to this session.
    pub state: Option<S>,
    /// The `iss` (issuer) parameter, required by RFC 9207 to prevent mix-up attacks.
    pub iss: Option<S>,
}

impl<S: BosStr + IntoStatic> IntoStatic for CallbackParams<S>
where
    S::Output: BosStr,
{
    type Output = CallbackParams<S::Output>;

    fn into_static(self) -> Self::Output {
        CallbackParams {
            code: self.code.into_static(),
            state: self.state.into_static(),
            iss: self.iss.into_static(),
        }
    }
}
