#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdentityInfo<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub did_doc: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub handle: jacquard_common::types::string::Handle<'a>,
}
pub mod get_recommended_did_credentials;
pub mod refresh_identity;
pub mod request_plc_operation_signature;
pub mod resolve_did;
pub mod resolve_handle;
pub mod resolve_identity;
pub mod sign_plc_operation;
pub mod submit_plc_operation;
pub mod update_handle;
