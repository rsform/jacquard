// Test schemas for verifying extraction works
// These are only compiled in tests/dev builds

use jacquard_common::CowStr;
use jacquard_derive::LexiconSchema;

#[derive(LexiconSchema)]
#[lexicon(nsid = "com.example.testRecord", record, key = "tid")]
pub struct TestRecord<'a> {
    #[lexicon(max_length = 100)]
    pub text: CowStr<'a>,
    pub count: i64,
}

#[derive(LexiconSchema)]
#[lexicon(nsid = "com.example.testRecord#fragment")]
pub struct TestFragment {
    pub field: i64,
}

#[derive(LexiconSchema)]
#[lexicon(nsid = "com.example.testDefs.defs#defOne")]
pub struct DefOne {
    pub value: String,
}

#[derive(LexiconSchema)]
#[lexicon(nsid = "com.example.testDefs.defs#defTwo")]
pub struct DefTwo {
    pub number: i64,
}
