//! Composition gate for proposal-0016 public permissioned primitives.

use bytes::Bytes;
use ed25519_dalek::SigningKey;
use jacquard_common::SmolStr;
use jacquard_common::types::aturi::AtSpaceUri;
use jacquard_common::types::cid::Cid;
use jacquard_common::types::did::Did;
use jacquard_common::types::tid::Tid;
use jacquard_lexicon::lexicon::LexiconDoc;
use jacquard_repo::permissioned::{
    CommitContext, LtHash, SpaceTypeDeclaration, WriteOperation, WriteState, apply_writes,
    format_cursor, list_repo_ops, sign_commit_with_ikm, verify_commit,
};
use std::str::FromStr;

#[test]
fn permissioned_data_conformance() {
    let path: AtSpaceUri<SmolStr> = AtSpaceUri::new_owned(
        "at://did:plc:space/space/com.example.type/demo/did:plc:author/com.example.record/rkey",
    )
    .unwrap();
    assert_eq!(
        path.as_str(),
        "at://did:plc:space/space/com.example.type/demo/did:plc:author/com.example.record/rkey"
    );

    let declaration: LexiconDoc<'_> = serde_json::from_str(
        r#"{
            "lexicon": 1,
            "id": "com.example.type",
            "defs": { "main": {
                "type": "space", "key": "demo", "name": "Example",
                "collections": ["com.example.record"]
            }}
        }"#,
    )
    .unwrap();
    let declaration = SpaceTypeDeclaration::from_lexicon("com.example.type", &declaration).unwrap();
    assert_eq!(declaration.key.as_str(), "demo");

    let mut hash = LtHash::default();
    hash.add("one");
    hash.add("two");
    assert_eq!(
        hex::encode(hash.digest()),
        "ae05cb6d224379d9710c290c8529945c5b0e0fde9ead30b9699057ce701c63e7"
    );

    let context = CommitContext {
        space: AtSpaceUri::new_owned("at://did:plc:space/space/com.example.type/demo").unwrap(),
        author: Did::new_owned("did:plc:author").unwrap(),
        rev: Tid::new("3jzfcijpj2m2a").unwrap(),
    };
    let key = SigningKey::from_bytes(&[11; 32]);
    let commit = sign_commit_with_ikm([9; 32], &context, &key, [0x20; 32]).unwrap();
    verify_commit(&commit, &context, &key.verifying_key()).unwrap();

    let mut state = WriteState::default();
    let operation = WriteOperation::Create {
        uri: path.clone(),
        cid: Cid::from_str("bafybeigdyrzt5o5p4s5x6f7g8h9j0k1l2m3n4o5p6q7r8s9t0u").unwrap(),
        value: Bytes::from_static(b"{}"),
    };
    let applied = apply_writes(&mut state, &context.space, &context.rev, &[operation]).unwrap();
    assert_eq!(applied.oplog[0].idx, 0);
    let page = list_repo_ops(&applied.oplog, None, None, Some(100), Some(commit)).unwrap();
    assert_eq!(page.ops.len(), 1);
    assert!(page.commit.is_some());
    assert_eq!(format_cursor(&context.rev, 0), "3jzfcijpj2m2a/0");
}
