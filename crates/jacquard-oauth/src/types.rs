mod client_metadata;
mod metadata;
mod request;
mod response;
mod token;

use crate::scopes::Scope;

pub use self::client_metadata::*;
pub use self::metadata::*;
pub use self::request::*;
pub use self::response::*;
pub use self::token::*;
use jacquard_common::CowStr;
use jacquard_common::IntoStatic;
use jacquard_common::deps::fluent_uri::Uri;
use serde::Deserialize;

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
        match value {
            AuthorizeOptionPrompt::Login => CowStr::new_static("login"),
            AuthorizeOptionPrompt::None => CowStr::new_static("none"),
            AuthorizeOptionPrompt::Consent => CowStr::new_static("consent"),
            AuthorizeOptionPrompt::SelectAccount => CowStr::new_static("select_account"),
        }
    }
}

/// Options for initiating an OAuth authorization request.
#[derive(Debug)]
pub struct AuthorizeOptions<'s> {
    /// Override the redirect URI registered in the client metadata.
    pub redirect_uri: Option<Uri<String>>,
    /// Scopes to request. Defaults to an empty list (server-defined defaults apply).
    pub scopes: Vec<Scope<'s>>,
    /// Optional prompt hint for the authorization server's UI.
    pub prompt: Option<AuthorizeOptionPrompt>,
    /// Opaque client-provided state value, echoed back in the callback for CSRF protection.
    pub state: Option<CowStr<'s>>,
}

impl Default for AuthorizeOptions<'_> {
    fn default() -> Self {
        Self {
            redirect_uri: None,
            scopes: vec![],
            prompt: None,
            state: None,
        }
    }
}

impl<'s> AuthorizeOptions<'s> {
    /// Set the `prompt` parameter sent to the authorization server.
    pub fn with_prompt(mut self, prompt: AuthorizeOptionPrompt) -> Self {
        self.prompt = Some(prompt);
        self
    }

    /// Set a CSRF-protection `state` value to be echoed in the callback.
    pub fn with_state(mut self, state: CowStr<'s>) -> Self {
        self.state = Some(state);
        self
    }

    /// Override the redirect URI for this specific authorization request.
    pub fn with_redirect_uri(mut self, redirect_uri: Uri<String>) -> Self {
        self.redirect_uri = Some(redirect_uri);
        self
    }

    /// Set the OAuth scopes to request.
    pub fn with_scopes(mut self, scopes: Vec<Scope<'s>>) -> Self {
        self.scopes = scopes;
        self
    }
}

/// Query parameters delivered to the OAuth redirect URI after user authorization.
#[derive(Debug, Deserialize)]
pub struct CallbackParams<'s> {
    /// The authorization code issued by the authorization server.
    #[serde(borrow)]
    pub code: CowStr<'s>,
    /// The `state` value originally sent in the authorization request, used to
    /// verify the response belongs to this session.
    pub state: Option<CowStr<'s>>,
    /// The `iss` (issuer) parameter, required by RFC 9207 to prevent mix-up attacks.
    pub iss: Option<CowStr<'s>>,
}

impl IntoStatic for CallbackParams<'_> {
    type Output = CallbackParams<'static>;

    fn into_static(self) -> Self::Output {
        CallbackParams {
            code: self.code.into_static(),
            state: self.state.map(|s| s.into_static()),
            iss: self.iss.map(|s| s.into_static()),
        }
    }
}
