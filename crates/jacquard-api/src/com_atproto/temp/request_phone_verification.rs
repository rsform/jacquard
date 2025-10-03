#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestPhoneVerificationInput<'a> {
    #[serde(borrow)]
    pub phone_number: jacquard_common::CowStr<'a>,
}
