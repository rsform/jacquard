#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DescribeRepoParams<'a> {
    #[serde(borrow)]
    pub repo: jacquard_common::types::ident::AtIdentifier<'a>,
}
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DescribeRepoOutput<'a> {
    #[serde(borrow)]
    pub collections: Vec<jacquard_common::types::string::Nsid<'a>>,
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub did_doc: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
    pub handle_is_correct: bool,
}
