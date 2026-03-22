//! Hand-written XRPC endpoint types for com.atproto endpoints.
//!
//! These types are vendored in jacquard-common to break the circular dependency
//! between jacquard-lexgen/jacquard-identity and jacquard-api. They provide minimal
//! implementations sufficient for bootstrap code generation without builders or
//! validation helpers.
//!
use crate::Bos;
use crate::CowStr;
use crate::DefaultStr;
use crate::IntoStatic;
use crate::types::ident::AtIdentifier;
use crate::types::string::{AtUri, Cid, Did, Handle, Nsid};
use crate::types::value::Data;
use crate::xrpc::{GenericError, XrpcMethod, XrpcRequest, XrpcResp};

use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{self, Display};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// ============================================================================
// com.atproto.repo.listRecords
// ============================================================================

/// Request for com.atproto.repo.listRecords.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ListRecords<S = DefaultStr>
where
    S: Bos<str> + AsRef<str>,
{
    pub collection: Nsid<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    pub repo: AtIdentifier<S>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
}

impl<S> IntoStatic for ListRecords<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = ListRecords<S::Output>;

    fn into_static(self) -> Self::Output {
        ListRecords {
            collection: self.collection.into_static(),
            cursor: self.cursor.into_static(),
            limit: self.limit,
            repo: self.repo.into_static(),
            reverse: self.reverse,
        }
    }
}

/// Output for com.atproto.repo.listRecords.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ListRecordsOutput<S = DefaultStr>
where
    S: Bos<str> + AsRef<str>,
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<S>,
    pub records: Vec<ListRecordsRecord<S>>,
}

impl<S> IntoStatic for ListRecordsOutput<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = ListRecordsOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        ListRecordsOutput {
            cursor: self.cursor.into_static(),
            records: self.records.into_static(),
        }
    }
}

/// A single record in a list response.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ListRecordsRecord<S = DefaultStr>
where
    S: Bos<str> + AsRef<str>,
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<Cid<S>>,
    pub uri: AtUri<S>,
    pub value: Data<S>,
}

impl<S> IntoStatic for ListRecordsRecord<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = ListRecordsRecord<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        ListRecordsRecord {
            cid: self.cid.into_static(),
            uri: self.uri.into_static(),
            value: self.value.into_static(),
        }
    }
}

/// Response marker for com.atproto.repo.listRecords.
pub struct ListRecordsResponse;

impl XrpcResp for ListRecordsResponse {
    const NSID: &'static str = "com.atproto.repo.listRecords";
    const ENCODING: &'static str = "application/json";
    type Output<S: Bos<str> + AsRef<str>> = ListRecordsOutput<S>;
    type Err = GenericError;
}

impl<S> XrpcRequest for ListRecords<S>
where
    S: Bos<str> + AsRef<str> + Serialize,
{
    const NSID: &'static str = "com.atproto.repo.listRecords";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = ListRecordsResponse;
}

// ============================================================================
// com.atproto.repo.getRecord
// ============================================================================

/// Request for com.atproto.repo.getRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct GetRecord<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub cid: Option<Cid<CowStr<'a>>>,
    #[serde(borrow)]
    pub collection: Nsid<CowStr<'a>>,
    #[serde(borrow)]
    pub repo: AtIdentifier<CowStr<'a>>,
    #[serde(borrow)]
    pub rkey: CowStr<'a>,
}

impl IntoStatic for GetRecord<'_> {
    type Output = GetRecord<'static>;

    fn into_static(self) -> Self::Output {
        GetRecord {
            cid: self.cid.into_static(),
            collection: self.collection.into_static(),
            repo: self.repo.into_static(),
            rkey: self.rkey.into_static(),
        }
    }
}

/// Output for com.atproto.repo.getRecord.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordOutput<S = DefaultStr>
where
    S: Bos<str> + AsRef<str>,
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<Cid<S>>,
    pub uri: AtUri<S>,
    pub value: Data<S>,
}

impl<S> IntoStatic for GetRecordOutput<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = GetRecordOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        GetRecordOutput {
            cid: self.cid.into_static(),
            uri: self.uri.into_static(),
            value: self.value.into_static(),
        }
    }
}

/// Error type for com.atproto.repo.getRecord. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
#[serde(tag = "error", content = "message")]
pub enum GetRecordError {
    #[serde(rename = "RecordNotFound")]
    RecordNotFound(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(other)]
    Other,
}

impl Display for GetRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordNotFound(msg) => {
                write!(f, "RecordNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other => write!(f, "Unknown error"),
        }
    }
}

impl Error for GetRecordError {}

/// Response marker for com.atproto.repo.getRecord.
pub struct GetRecordResponse;

impl XrpcResp for GetRecordResponse {
    const NSID: &'static str = "com.atproto.repo.getRecord";
    const ENCODING: &'static str = "application/json";
    type Output<S: Bos<str> + AsRef<str>> = GetRecordOutput<S>;
    type Err = GetRecordError;
}

impl<'a> XrpcRequest for GetRecord<'a> {
    const NSID: &'static str = "com.atproto.repo.getRecord";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = GetRecordResponse;
}

// ============================================================================
// com.atproto.identity.resolveHandle
// ============================================================================

/// Request for com.atproto.identity.resolveHandle.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ResolveHandle<S: Bos<str> + AsRef<str> = DefaultStr> {
    pub handle: Handle<S>,
}

impl<S> IntoStatic for ResolveHandle<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = ResolveHandle<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        ResolveHandle {
            handle: self.handle.into_static(),
        }
    }
}

/// Output for com.atproto.identity.resolveHandle.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ResolveHandleOutput<S: Bos<str> + AsRef<str> = DefaultStr> {
    pub did: Did<S>,
}

impl<S> IntoStatic for ResolveHandleOutput<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = ResolveHandleOutput<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        ResolveHandleOutput {
            did: self.did.into_static(),
        }
    }
}

/// Error type for com.atproto.identity.resolveHandle. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "error", content = "message")]
#[allow(missing_docs)]
pub enum ResolveHandleError {
    #[serde(rename = "HandleNotFound")]
    HandleNotFound(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(other)]
    Other,
}

impl Display for ResolveHandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandleNotFound(msg) => {
                write!(f, "HandleNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other => write!(f, "Unknown error"),
        }
    }
}

impl Error for ResolveHandleError {}

/// Response marker for com.atproto.identity.resolveHandle.
pub struct ResolveHandleResponse;

impl XrpcResp for ResolveHandleResponse {
    const NSID: &'static str = "com.atproto.identity.resolveHandle";
    const ENCODING: &'static str = "application/json";
    type Output<S: Bos<str> + AsRef<str>> = ResolveHandleOutput<S>;
    type Err = ResolveHandleError;
}

impl<S: Bos<str> + AsRef<str> + Serialize> XrpcRequest for ResolveHandle<S> {
    const NSID: &'static str = "com.atproto.identity.resolveHandle";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = ResolveHandleResponse;
}

// ============================================================================
// com.atproto.identity.resolveDid
// ============================================================================

/// Request for com.atproto.identity.resolveDid.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ResolveDid<S: Bos<str> + AsRef<str> = DefaultStr> {
    pub did: Did<S>,
}

impl<S> IntoStatic for ResolveDid<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    <S as IntoStatic>::Output: Bos<str>,
    <S as IntoStatic>::Output: AsRef<str>,
{
    type Output = ResolveDid<<S as IntoStatic>::Output>;

    fn into_static(self) -> Self::Output {
        ResolveDid {
            did: self.did.into_static(),
        }
    }
}

/// Output for com.atproto.identity.resolveDid.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub struct ResolveDidOutput<S = DefaultStr>
where
    S: Bos<str> + AsRef<str>,
{
    pub did_doc: Data<S>,
}

impl<S> IntoStatic for ResolveDidOutput<S>
where
    S: Bos<str> + AsRef<str> + IntoStatic,
    S::Output: Bos<str> + AsRef<str>,
{
    type Output = ResolveDidOutput<S::Output>;

    fn into_static(self) -> Self::Output {
        ResolveDidOutput {
            did_doc: self.did_doc.into_static(),
        }
    }
}

/// Error type for com.atproto.identity.resolveDid. Always SmolStr-backed.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "error", content = "message")]
#[allow(missing_docs)]
pub enum ResolveDidError {
    #[serde(rename = "DidNotFound")]
    DidNotFound(Option<SmolStr>),
    #[serde(rename = "DidDeactivated")]
    DidDeactivated(Option<SmolStr>),
    /// Catch-all for unknown error codes.
    #[serde(other)]
    Other,
}

impl Display for ResolveDidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DidNotFound(msg) => {
                write!(f, "DidNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::DidDeactivated(msg) => {
                write!(f, "DidDeactivated")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
            Self::Other => write!(f, "Unknown error"),
        }
    }
}

impl Error for ResolveDidError {}

/// Response marker for com.atproto.identity.resolveDid.
pub struct ResolveDidResponse;

impl XrpcResp for ResolveDidResponse {
    const NSID: &'static str = "com.atproto.identity.resolveDid";
    const ENCODING: &'static str = "application/json";
    type Output<S: Bos<str> + AsRef<str>> = ResolveDidOutput<S>;
    type Err = ResolveDidError;
}

impl<S> XrpcRequest for ResolveDid<S>
where
    S: Bos<str> + AsRef<str> + Serialize,
{
    const NSID: &'static str = "com.atproto.identity.resolveDid";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = ResolveDidResponse;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IntoStatic, cowstr::ToCowStr};

    #[test]
    fn test_list_records_serializes() {
        let req = ListRecords {
            repo: AtIdentifier::new("test.bsky.social".to_cowstr()).unwrap(),
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            cursor: None,
            limit: Some(50),
            reverse: None,
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["repo"], "test.bsky.social");
        assert_eq!(json["collection"], "app.bsky.feed.post");
        assert_eq!(json["limit"], 50);
        assert!(!json.as_object().unwrap().contains_key("cursor"));
    }

    #[test]
    fn test_list_records_output_deserializes() {
        let json_str = r#"{
            "records": [
                {
                    "uri": "at://did:plc:test/app.bsky.feed.post/123",
                    "cid": "bafy123",
                    "value": {}
                }
            ],
            "cursor": "page2"
        }"#;

        let output: ListRecordsOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.records.len(), 1);
        assert!(output.cursor.is_some());
        assert_eq!(output.cursor.as_ref().unwrap().as_str(), "page2");
    }

    #[test]
    fn test_get_record_output_deserializes() {
        let json_str = r#"{
            "uri": "at://did:plc:test/app.bsky.feed.post/123",
            "cid": "bafy123",
            "value": {}
        }"#;

        let output: GetRecordOutput = serde_json::from_str(json_str).unwrap();
        assert!(output.cid.is_some());
        assert_eq!(output.cid.as_ref().unwrap().as_str(), "bafy123");
    }

    #[test]
    fn test_get_record_error_deserializes() {
        let json_str = r#"{"error": "RecordNotFound", "message": "not found"}"#;
        let error: GetRecordError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, GetRecordError::RecordNotFound(Some(_))));
    }

    #[test]
    fn test_resolve_handle_output_deserializes() {
        let json_str = r#"{"did": "did:plc:abc123"}"#;
        let output: ResolveHandleOutput = serde_json::from_str(json_str).unwrap();
        assert_eq!(output.did.as_str(), "did:plc:abc123");
    }

    #[test]
    fn test_resolve_handle_error_deserializes() {
        let json_str = r#"{"error": "HandleNotFound", "message": "handle not found"}"#;
        let error: ResolveHandleError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, ResolveHandleError::HandleNotFound(Some(_))));
    }

    #[test]
    fn test_resolve_did_output_deserializes() {
        let json_str = r#"{"didDoc": {}}"#;
        let output: ResolveDidOutput = serde_json::from_str(json_str).unwrap();
        let _ = output;
    }

    #[test]
    fn test_resolve_did_error_deserializes_not_found() {
        let json_str = r#"{"error": "DidNotFound", "message": "did not found"}"#;
        let error: ResolveDidError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, ResolveDidError::DidNotFound(Some(_))));
    }

    #[test]
    fn test_resolve_did_error_deserializes_deactivated() {
        let json_str = r#"{"error": "DidDeactivated", "message": "did is deactivated"}"#;
        let error: ResolveDidError = serde_json::from_str(json_str).unwrap();
        assert!(matches!(error, ResolveDidError::DidDeactivated(Some(_))));
    }

    #[test]
    fn test_types_implement_into_static() {
        let list_records = ListRecords {
            repo: AtIdentifier::new("test.bsky.social".to_cowstr()).unwrap(),
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            cursor: None,
            limit: Some(50),
            reverse: None,
        };
        let _static = list_records.into_static();

        let get_record = GetRecord {
            repo: AtIdentifier::new("test.bsky.social".to_cowstr()).unwrap(),
            collection: Nsid::new("app.bsky.feed.post".to_cowstr())
                .unwrap()
                .into_static(),
            rkey: CowStr::from("abc123").into_static(),
            cid: None,
        };
        let _static = get_record.into_static();

        let resolve_handle = ResolveHandle {
            handle: Handle::new("test.bsky.social").unwrap().into_static(),
        };
        let _static = resolve_handle.into_static();

        let resolve_did = ResolveDid {
            did: Did::new("did:plc:abc123").unwrap().into_static(),
        };
        let _static = resolve_did.into_static();
    }
}
