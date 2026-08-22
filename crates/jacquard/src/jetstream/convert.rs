//! Normalization of decoded archive rows into the generated
//! `network.bsky.jetstream.subscribeEvents` message shapes.
//!
//! Identity/account/sync payloads stored in the archive are CBOR of
//! `com.atproto.sync.subscribeRepos` events, so they decode
//! directly into the referenced types. Commit payloads are the raw
//! DAG-CBOR record bytes and decode straight into [`Data`].

use jacquard_api::com_atproto::sync::subscribe_repos;
use jacquard_api::network_bsky::jetstream::subscribe_events::{
    self, CommitOperation, SubscribeEventsMessage,
};

use jacquard_common::bos::Bos;
use jacquard_common::deps::codegen::serde_ipld_dagcbor;
use jacquard_common::types::datetime::Datetime;
use jacquard_common::types::did::Did;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::recordkey::{RecordKey, Rkey};
use jacquard_common::types::tid::Tid;
use jacquard_common::types::value::Data;
use jacquard_common::{BorrowOrShare, BosStr, IntoStatic};

use jacquard_common::jss::{Kind, Row};

/// Errors possible when normalizing an archive row into a wire message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    /// Row payload bytes are present but not valid DAG-CBOR.
    PayloadDecode,
    /// A commit-kind row lacks a field the wire shape and every folding
    /// consumer require.
    MissingCommitField(&'static str),
    /// A row's seq exceeds the i64 wire range.
    SeqOverflow(u64),
    /// A row timestamp falls outside the representable datetime range.
    InvalidTimestamp(i64),
}

impl core::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PayloadDecode => write!(f, "row payload is not valid DAG-CBOR"),
            Self::MissingCommitField(name) => {
                write!(f, "commit row is missing required field `{name}`")
            }
            Self::SeqOverflow(seq) => write!(f, "row seq {seq} exceeds the i64 wire range"),
            Self::InvalidTimestamp(us) => {
                write!(f, "row timestamp {us}µs is out of range")
            }
        }
    }
}

impl std::error::Error for ConvertError {}

/// Convert one decoded archive row into the Jetstream v2 API subscription message type.
pub fn row_to_message<'de, S, B>(
    row: &'de Row<S, B>,
) -> Result<SubscribeEventsMessage<S>, ConvertError>
where
    S: BosStr + Clone + IntoStatic + serde::Deserialize<'de>,
    B: Bos<[u8]> + BorrowOrShare<'de, 'de, [u8]>,
{
    let payload_bytes: &'de [u8] = row.payload.borrow_or_share();

    let display_us = row.display_time_us();
    let time = Datetime::from_timestamp_micros(display_us)
        .ok_or(ConvertError::InvalidTimestamp(display_us))?;
    let seq = i64::try_from(row.seq).map_err(|_| ConvertError::SeqOverflow(row.seq))?;

    match row.kind {
        Kind::Create | Kind::Update | Kind::Delete | Kind::CreateResync => {
            let rev =
                Tid::new(row.rev.as_ref()).map_err(|_| ConvertError::MissingCommitField("rev"))?;
            if row.rkey.as_ref().is_empty() {
                return Err(ConvertError::MissingCommitField("rkey"));
            }

            // Delete rows carry no payload; a non-empty payload on another
            // commit kind is decoded as a record.
            let record = if payload_bytes.is_empty() {
                None
            } else {
                Some(
                    serde_ipld_dagcbor::from_slice::<Data<S>>(payload_bytes)
                        .map_err(|_| ConvertError::PayloadDecode)?,
                )
            };

            let operation = match row.kind {
                Kind::Create | Kind::CreateResync => CommitOperation::Create,
                Kind::Update => CommitOperation::Update,
                Kind::Delete => CommitOperation::Delete,
                _ => unreachable!("arm only entered for commit kinds"),
            };

            let collection = Nsid::new(row.collection.clone())
                .map_err(|_| ConvertError::MissingCommitField("collection"))?;
            let did =
                Did::new(row.did.clone()).map_err(|_| ConvertError::MissingCommitField("did"))?;
            let rkey = RecordKey::<Rkey<S>>::any(row.rkey.clone())
                .map_err(|_| ConvertError::MissingCommitField("rkey"))?;

            Ok(SubscribeEventsMessage::Commit(Box::new(
                subscribe_events::Commit {
                    cid: None,
                    collection,
                    did,
                    operation,
                    record,
                    rev,
                    rkey,
                    seq,
                    time,
                    extra_data: None,
                },
            )))
        }
        Kind::Identity => {
            let identity: subscribe_repos::Identity<S> =
                serde_ipld_dagcbor::from_slice(payload_bytes)
                    .map_err(|_| ConvertError::PayloadDecode)?;
            let did =
                Did::new(row.did.clone()).map_err(|_| ConvertError::MissingCommitField("did"))?;
            Ok(SubscribeEventsMessage::Identity(Box::new(
                subscribe_events::Identity {
                    did,
                    identity,
                    seq,
                    time,
                    extra_data: None,
                },
            )))
        }
        Kind::Account => {
            let account: subscribe_repos::Account<S> =
                serde_ipld_dagcbor::from_slice(payload_bytes)
                    .map_err(|_| ConvertError::PayloadDecode)?;
            let did =
                Did::new(row.did.clone()).map_err(|_| ConvertError::MissingCommitField("did"))?;
            Ok(SubscribeEventsMessage::Account(Box::new(
                subscribe_events::Account {
                    account,
                    did,
                    seq,
                    time,
                    extra_data: None,
                },
            )))
        }
        Kind::Sync => {
            let sync: subscribe_repos::Sync<S> = serde_ipld_dagcbor::from_slice(payload_bytes)
                .map_err(|_| ConvertError::PayloadDecode)?;
            let did =
                Did::new(row.did.clone()).map_err(|_| ConvertError::MissingCommitField("did"))?;
            Ok(SubscribeEventsMessage::Sync(Box::new(
                subscribe_events::Sync {
                    did,
                    sync,
                    seq,
                    time,
                    extra_data: None,
                },
            )))
        }
    }
}

#[cfg(all(test, feature = "zstd"))]
mod tests {
    use super::*;
    use jacquard_api::network_bsky::jetstream::subscribe_events::SubscribeEventsMessage;
    use jacquard_common::{
        deps::{bytes, codegen::serde_ipld_dagcbor},
        jss::{Kind, Row, decode_block_body, read_block_frame},
    };
    use smol_str::SmolStr;

    fn fixture() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../jacquard-common/src/jss/testdata/segments.jss"
        ))
        .expect("fixture present")
    }

    #[test]
    fn archive_identity_maps() {
        let file = fixture();
        let (body, _) = read_block_frame(&file, 256, false)
            .expect("frame")
            .expect("present");
        let rows: Vec<Row<&str, &[u8]>> = decode_block_body(&body).expect("rows");

        let mut checked = 0;
        for row in rows {
            let did = row.did.to_string();
            let message = row_to_message(&row).expect("convert");
            match message {
                SubscribeEventsMessage::Identity(inner) => {
                    assert_eq!(inner.identity.did.as_ref(), did);
                    assert!(inner.seq >= 1);
                    checked += 1;
                }
                other => panic!("expected identity, got {other:?}"),
            }
        }
        assert_eq!(checked, 40);
    }

    #[test]
    fn archive_account_maps() {
        let payload = serde_ipld_dagcbor::to_vec(&serde_json::json!({
            "active": false,
            "did": "did:plc:aundg2bbjkngd7rfn2p64wkp",
            "seq": 17,
            "status": "deleted",
            "time": "2026-01-01T00:00:00.000Z"
        }))
        .expect("encode account payload");
        let row = Row::<SmolStr, &[u8]> {
            seq: 18,
            witnessed_at: 1_767_225_601_000_000,
            indexed_at: 0,
            kind: Kind::Account,
            collection: SmolStr::default(),
            did: SmolStr::from("did:plc:aundg2bbjkngd7rfn2p64wkp"),
            rkey: SmolStr::default(),
            rev: SmolStr::default(),
            payload: &payload,
        };

        let SubscribeEventsMessage::Account(account) = row_to_message(&row).expect("convert")
        else {
            panic!("expected account");
        };
        assert_eq!(account.seq, 18);
        assert_eq!(account.account.seq, 17);
        assert!(!account.account.active);
        assert_eq!(
            account.account.status.as_ref().map(|s| s.as_str()),
            Some("deleted")
        );
    }

    #[test]
    fn archive_sync_maps() {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct SyncWire<'a> {
            #[serde(with = "jacquard_common::serde_bytes_helper")]
            blocks: bytes::Bytes,
            did: &'a str,
            rev: &'a str,
            seq: i64,
            time: &'a str,
        }

        let payload = serde_ipld_dagcbor::to_vec(&SyncWire {
            blocks: bytes::Bytes::from_static(&[1, 2, 3]),
            did: "did:plc:aundg2bbjkngd7rfn2p64wkp",
            rev: "3l2h5k7q4g2x7",
            seq: 20,
            time: "2026-01-01T00:00:00.000Z",
        })
        .expect("encode sync payload");
        let row = Row::<SmolStr, &[u8]> {
            seq: 21,
            witnessed_at: 1_767_225_601_000_000,
            indexed_at: 0,
            kind: Kind::Sync,
            collection: SmolStr::default(),
            did: SmolStr::from("did:plc:aundg2bbjkngd7rfn2p64wkp"),
            rkey: SmolStr::default(),
            rev: SmolStr::default(),
            payload: &payload,
        };

        let SubscribeEventsMessage::Sync(sync) = row_to_message(&row).expect("convert") else {
            panic!("expected sync");
        };
        assert_eq!(sync.seq, 21);
        assert_eq!(sync.sync.seq, 20);
        assert_eq!(sync.sync.rev.as_str(), "3l2h5k7q4g2x7");
        assert_eq!(sync.sync.blocks.as_ref(), &[1, 2, 3]);
    }

    #[test]
    fn archive_commit_maps() {
        let row = Row::<SmolStr, &[u8]> {
            seq: 7,
            witnessed_at: 1_700_000_000_000_000,
            indexed_at: 0,
            kind: Kind::Create,
            collection: SmolStr::from("app.bsky.feed.post"),
            did: SmolStr::from("did:plc:aundg2bbjkngd7rfn2p64wkp"),
            rkey: SmolStr::from("3l2h5k7q4g2"),
            rev: SmolStr::from("3l2h5k7q4g2x7"),
            payload: b"\xa1dtextehello",
        };

        let message = row_to_message(&row).expect("convert");
        let SubscribeEventsMessage::Commit(commit) = message else {
            panic!("expected commit");
        };
        assert_eq!(commit.operation.as_str(), "create");
        assert_eq!(commit.collection.as_ref(), "app.bsky.feed.post");
        assert_eq!(commit.seq, 7);
        assert!(commit.record.is_some());
    }

    #[test]
    fn archive_create_resync_maps_to_commit_create() {
        let row = Row::<SmolStr, &[u8]> {
            seq: 9,
            witnessed_at: 1_700_000_000_000_000,
            indexed_at: 0,
            kind: Kind::CreateResync,
            collection: SmolStr::from("app.bsky.feed.post"),
            did: SmolStr::from("did:plc:aundg2bbjkngd7rfn2p64wkp"),
            rkey: SmolStr::from("3l2h5k7q4g2"),
            rev: SmolStr::from("3l2h5k7q4g2x7"),
            payload: b"\xa1dtextehello",
        };

        let message = row_to_message(&row).expect("convert");
        let SubscribeEventsMessage::Commit(commit) = message else {
            panic!("expected commit");
        };
        assert_eq!(commit.operation.as_str(), "create");
    }

    #[test]
    fn archive_delete_maps_without_record() {
        let row = Row::<SmolStr, &[u8]> {
            seq: 11,
            witnessed_at: 1_700_000_000_000_000,
            indexed_at: 0,
            kind: Kind::Delete,
            collection: SmolStr::from("app.bsky.feed.post"),
            did: SmolStr::from("did:plc:aundg2bbjkngd7rfn2p64wkp"),
            rkey: SmolStr::from("3l2h5k7q4g2"),
            rev: SmolStr::from("3l2h5k7q4g2x7"),
            payload: b"",
        };

        let message = row_to_message(&row).expect("convert");
        let SubscribeEventsMessage::Commit(commit) = message else {
            panic!("expected commit");
        };
        assert_eq!(commit.operation.as_str(), "delete");
        assert!(commit.record.is_none());
    }

    #[test]
    fn convert_rejects_out_of_range_timestamp() {
        let row = Row {
            seq: 1,
            witnessed_at: i64::MAX,
            indexed_at: 0,
            kind: Kind::Identity,
            collection: SmolStr::default(),
            did: SmolStr::from("did:plc:aundg2bbjkngd7rfn2p64wkp"),
            rkey: SmolStr::default(),
            rev: SmolStr::default(),
            payload: &b""[..],
        };
        assert_eq!(
            row_to_message(&row).unwrap_err(),
            ConvertError::InvalidTimestamp(i64::MAX)
        );
    }
}
