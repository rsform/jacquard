#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MuteActorInput<'a> {
    #[serde(borrow)]
    pub actor: jacquard_common::types::ident::AtIdentifier<'a>,
}
