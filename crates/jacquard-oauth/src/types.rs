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
            scopes: vec![Scope::Atproto],
            prompt: None,
            state: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams<'s> {
    #[serde(borrow)]
    pub code: CowStr<'s>,
    pub state: Option<CowStr<'s>>,
    pub iss: Option<CowStr<'s>>,
}
