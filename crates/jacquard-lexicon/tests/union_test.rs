// Minimal test for union attribute

use jacquard_common::CowStr;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, jacquard_derive::LexiconSchema)]
#[lexicon(nsid = "test.union")]
#[jacquard_derive::lexicon_union]
#[serde(tag = "$type")]
pub enum TestUnion<'a> {
    #[serde(borrow, rename = "test.one")]
    One(CowStr<'a>),
    #[serde(borrow, rename = "test.two")]
    Two(CowStr<'a>),
}

#[derive(Serialize, Deserialize, Clone, jacquard_derive::LexiconSchema)]
#[lexicon(nsid = "test.record", record, key = "tid")]
pub struct TestRecord<'a> {
    #[serde(borrow)]
    #[lexicon(union)]
    pub data: Option<TestUnion<'a>>,
}

#[test]
fn test_union_refs() {
    // Just check that LEXICON_UNION_REFS was generated
    assert_eq!(TestUnion::LEXICON_UNION_REFS.len(), 2);
    assert_eq!(TestUnion::LEXICON_UNION_REFS[0], "test.one");
    assert_eq!(TestUnion::LEXICON_UNION_REFS[1], "test.two");
}

#[test]
fn test_record_with_union() {
    use jacquard_lexicon::schema::LexiconSchema;

    let doc = TestRecord::lexicon_doc();
    assert_eq!(doc.id.as_ref(), "test.record");
}
