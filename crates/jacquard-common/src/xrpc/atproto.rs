//! Hand-written XRPC endpoint types for com.atproto endpoints.
//!
//! These types are vendored in jacquard-common to break the circular dependency
//! between jacquard-lexgen/jacquard-identity and jacquard-api. They provide minimal
//! implementations sufficient for bootstrap code generation without builders or
//! validation helpers.

use alloc::vec::Vec;
use crate::CowStr;
use crate::IntoStatic;
use crate::types::string::{AtUri, Cid, Did, Handle, Nsid};
use crate::types::ident::AtIdentifier;
use crate::types::value::Data;
use crate::xrpc::{GenericError, XrpcMethod, XrpcRequest, XrpcResp};
use core::error::Error;
use core::fmt::{self, Display};
use serde::{Deserialize, Serialize};

// ============================================================================
// com.atproto.repo.listRecords
// ============================================================================

/// Request for com.atproto.repo.listRecords.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct ListRecords<'a> {
    #[serde(borrow)]
    pub collection: Nsid<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub cursor: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(borrow)]
    pub repo: AtIdentifier<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
}

impl IntoStatic for ListRecords<'_> {
    type Output = ListRecords<'static>;

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
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct ListRecordsOutput<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub cursor: Option<CowStr<'a>>,
    #[serde(borrow)]
    pub records: Vec<ListRecordsRecord<'a>>,
}

impl IntoStatic for ListRecordsOutput<'_> {
    type Output = ListRecordsOutput<'static>;

    fn into_static(self) -> Self::Output {
        ListRecordsOutput {
            cursor: self.cursor.into_static(),
            records: self.records.into_static(),
        }
    }
}

/// A single record in a list response.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct ListRecordsRecord<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub cid: Option<Cid<'a>>,
    #[serde(borrow)]
    pub uri: AtUri<'a>,
    #[serde(borrow)]
    pub value: Data<'a>,
}

impl IntoStatic for ListRecordsRecord<'_> {
    type Output = ListRecordsRecord<'static>;

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
    type Output<'de> = ListRecordsOutput<'de>;
    type Err<'de> = GenericError<'de>;
}

impl<'a> XrpcRequest for ListRecords<'a> {
    const NSID: &'static str = "com.atproto.repo.listRecords";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = ListRecordsResponse;
}

// ============================================================================
// com.atproto.repo.getRecord
// ============================================================================

/// Request for com.atproto.repo.getRecord.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct GetRecord<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub cid: Option<Cid<'a>>,
    #[serde(borrow)]
    pub collection: Nsid<'a>,
    #[serde(borrow)]
    pub repo: AtIdentifier<'a>,
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
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct GetRecordOutput<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub cid: Option<Cid<'a>>,
    #[serde(borrow)]
    pub uri: AtUri<'a>,
    #[serde(borrow)]
    pub value: Data<'a>,
}

impl IntoStatic for GetRecordOutput<'_> {
    type Output = GetRecordOutput<'static>;

    fn into_static(self) -> Self::Output {
        GetRecordOutput {
            cid: self.cid.into_static(),
            uri: self.uri.into_static(),
            value: self.value.into_static(),
        }
    }
}

/// Error type for com.atproto.repo.getRecord.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(tag = "error", content = "message")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum GetRecordError<'a> {
    #[serde(rename = "RecordNotFound")]
    RecordNotFound(Option<CowStr<'a>>),
}

impl<'a> Display for GetRecordError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordNotFound(msg) => {
                write!(f, "RecordNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
        }
    }
}

impl Error for GetRecordError<'_> {}

impl IntoStatic for GetRecordError<'_> {
    type Output = GetRecordError<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            Self::RecordNotFound(msg) => GetRecordError::RecordNotFound(msg.into_static()),
        }
    }
}

/// Response marker for com.atproto.repo.getRecord.
pub struct GetRecordResponse;

impl XrpcResp for GetRecordResponse {
    const NSID: &'static str = "com.atproto.repo.getRecord";
    const ENCODING: &'static str = "application/json";
    type Output<'de> = GetRecordOutput<'de>;
    type Err<'de> = GetRecordError<'de>;
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
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct ResolveHandle<'a> {
    #[serde(borrow)]
    pub handle: Handle<'a>,
}

impl IntoStatic for ResolveHandle<'_> {
    type Output = ResolveHandle<'static>;

    fn into_static(self) -> Self::Output {
        ResolveHandle {
            handle: self.handle.into_static(),
        }
    }
}

/// Output for com.atproto.identity.resolveHandle.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct ResolveHandleOutput<'a> {
    #[serde(borrow)]
    pub did: Did<'a>,
}

impl IntoStatic for ResolveHandleOutput<'_> {
    type Output = ResolveHandleOutput<'static>;

    fn into_static(self) -> Self::Output {
        ResolveHandleOutput {
            did: self.did.into_static(),
        }
    }
}

/// Error type for com.atproto.identity.resolveHandle.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(tag = "error", content = "message")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ResolveHandleError<'a> {
    #[serde(rename = "HandleNotFound")]
    HandleNotFound(Option<CowStr<'a>>),
}

impl<'a> Display for ResolveHandleError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandleNotFound(msg) => {
                write!(f, "HandleNotFound")?;
                if let Some(msg) = msg {
                    write!(f, ": {}", msg)?;
                }
                Ok(())
            }
        }
    }
}

impl Error for ResolveHandleError<'_> {}

impl IntoStatic for ResolveHandleError<'_> {
    type Output = ResolveHandleError<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            Self::HandleNotFound(msg) => ResolveHandleError::HandleNotFound(msg.into_static()),
        }
    }
}

/// Response marker for com.atproto.identity.resolveHandle.
pub struct ResolveHandleResponse;

impl XrpcResp for ResolveHandleResponse {
    const NSID: &'static str = "com.atproto.identity.resolveHandle";
    const ENCODING: &'static str = "application/json";
    type Output<'de> = ResolveHandleOutput<'de>;
    type Err<'de> = ResolveHandleError<'de>;
}

impl<'a> XrpcRequest for ResolveHandle<'a> {
    const NSID: &'static str = "com.atproto.identity.resolveHandle";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    type Response = ResolveHandleResponse;
}

// ============================================================================
// com.atproto.identity.resolveDid
// ============================================================================

/// Request for com.atproto.identity.resolveDid.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct ResolveDid<'a> {
    #[serde(borrow)]
    pub did: Did<'a>,
}

impl IntoStatic for ResolveDid<'_> {
    type Output = ResolveDid<'static>;

    fn into_static(self) -> Self::Output {
        ResolveDid {
            did: self.did.into_static(),
        }
    }
}

/// Output for com.atproto.identity.resolveDid.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct ResolveDidOutput<'a> {
    #[serde(borrow)]
    pub did_doc: Data<'a>,
}

impl IntoStatic for ResolveDidOutput<'_> {
    type Output = ResolveDidOutput<'static>;

    fn into_static(self) -> Self::Output {
        ResolveDidOutput {
            did_doc: self.did_doc.into_static(),
        }
    }
}

/// Error type for com.atproto.identity.resolveDid.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
#[serde(tag = "error", content = "message")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum ResolveDidError<'a> {
    #[serde(rename = "DidNotFound")]
    DidNotFound(Option<CowStr<'a>>),
    #[serde(rename = "DidDeactivated")]
    DidDeactivated(Option<CowStr<'a>>),
}

impl<'a> Display for ResolveDidError<'a> {
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
        }
    }
}

impl Error for ResolveDidError<'_> {}

impl IntoStatic for ResolveDidError<'_> {
    type Output = ResolveDidError<'static>;

    fn into_static(self) -> Self::Output {
        match self {
            Self::DidNotFound(msg) => ResolveDidError::DidNotFound(msg.into_static()),
            Self::DidDeactivated(msg) => ResolveDidError::DidDeactivated(msg.into_static()),
        }
    }
}

/// Response marker for com.atproto.identity.resolveDid.
pub struct ResolveDidResponse;

impl XrpcResp for ResolveDidResponse {
    const NSID: &'static str = "com.atproto.identity.resolveDid";
    const ENCODING: &'static str = "application/json";
    type Output<'de> = ResolveDidOutput<'de>;
    type Err<'de> = ResolveDidError<'de>;
}

impl<'a> XrpcRequest for ResolveDid<'a> {
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
    use crate::IntoStatic;

    #[test]
    fn test_list_records_serializes() {
        let req = ListRecords {
            repo: AtIdentifier::new("test.bsky.social").unwrap().into_static().into(),
            collection: Nsid::new("app.bsky.feed.post").unwrap().into_static(),
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
        // Just verify it parses without error
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
            repo: AtIdentifier::new("test.bsky.social").unwrap().into_static().into(),
            collection: Nsid::new("app.bsky.feed.post").unwrap().into_static(),
            cursor: None,
            limit: Some(50),
            reverse: None,
        };
        let _static = list_records.into_static();

        let get_record = GetRecord {
            repo: AtIdentifier::new("test.bsky.social").unwrap().into_static().into(),
            collection: Nsid::new("app.bsky.feed.post").unwrap().into_static(),
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
