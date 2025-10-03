#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StrongRef<'a> {
    pub cid: jacquard_common::types::string::Cid<'a>,
    pub uri: jacquard_common::types::string::AtUri<'a>,
}
