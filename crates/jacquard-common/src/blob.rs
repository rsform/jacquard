use crate::CowStr;

/// Wrapper for file type
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MimeType<'m>(pub CowStr<'m>);
