///Specifies the sub-string range a facet feature applies to. Start index is inclusive, end index is exclusive. Indices are zero-indexed, counting bytes of the UTF-8 encoded text. NOTE: some languages, like Javascript, use UTF-16 or Unicode codepoints for string slice indexing; in these languages, convert to byte arrays before working with facets.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ByteSlice<'a> {
    pub byte_end: i64,
    pub byte_start: i64,
}
///Facet feature for a URL. The text URL may have been simplified or truncated, but the facet reference should be a complete URL.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Link<'a> {
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::Uri<'a>,
}
///Annotation of a sub-string within rich text.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Facet<'a> {
    #[serde(borrow)]
    pub features: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub index: jacquard_common::types::value::Data<'a>,
}
///Facet feature for mention of another account. The text is usually a handle, including a '@' prefix, but the facet reference is a DID.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mention<'a> {
    #[serde(borrow)]
    pub did: jacquard_common::types::string::Did<'a>,
}
///Facet feature for a hashtag. The text usually includes a '#' prefix, but the facet reference should not (except in the case of 'double hash tags').
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tag<'a> {
    #[serde(borrow)]
    pub tag: jacquard_common::CowStr<'a>,
}
