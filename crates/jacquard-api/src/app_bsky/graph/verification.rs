///Record declaring a verification relationship between two accounts. Verifications are only considered valid by an app if issued by an account the app considers trusted.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Verification<'a> {
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub display_name: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    #[serde(borrow)]
    pub subject: jacquard_common::types::string::Did<'a>,
}
