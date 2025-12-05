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
use serde::Deserialize;
use url::Url;

#[derive(Debug, Deserialize, Clone, Copy)]
pub enum AuthorizeOptionPrompt {
    Login,
    None,
    Consent,
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

#[derive(Debug)]
pub struct AuthorizeOptions<'s> {
    pub redirect_uri: Option<Url>,
    pub scopes: Vec<Scope<'s>>,
    pub prompt: Option<AuthorizeOptionPrompt>,
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
    pub fn with_prompt(mut self, prompt: AuthorizeOptionPrompt) -> Self {
        self.prompt = Some(prompt);
        self
    }

    pub fn with_state(mut self, state: CowStr<'s>) -> Self {
        self.state = Some(state);
        self
    }

    pub fn with_redirect_uri(mut self, redirect_uri: Url) -> Self {
        self.redirect_uri = Some(redirect_uri);
        self
    }

    pub fn with_scopes(mut self, scopes: Vec<Scope<'s>>) -> Self {
        self.scopes = scopes;
        self
    }
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams<'s> {
    #[serde(borrow)]
    pub code: CowStr<'s>,
    pub state: Option<CowStr<'s>>,
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
