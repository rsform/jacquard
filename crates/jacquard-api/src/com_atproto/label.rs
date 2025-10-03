///Metadata tag on an atproto resource (eg, repo or record).
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Label<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub cid: std::option::Option<jacquard_common::types::string::Cid<'a>>,
    pub cts: jacquard_common::types::string::Datetime,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub exp: std::option::Option<jacquard_common::types::string::Datetime>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub neg: std::option::Option<bool>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub sig: std::option::Option<bytes::Bytes>,
    #[serde(borrow)]
    pub src: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub uri: jacquard_common::types::string::Uri<'a>,
    #[serde(borrow)]
    pub val: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub ver: std::option::Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LabelValue<'a> {
    Hide,
    NoPromote,
    Warn,
    NoUnauthenticated,
    DmcaViolation,
    Doxxing,
    Porn,
    Sexual,
    Nudity,
    Nsfl,
    Gore,
    Other(jacquard_common::CowStr<'a>),
}
impl<'a> LabelValue<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Hide => "!hide",
            Self::NoPromote => "!no-promote",
            Self::Warn => "!warn",
            Self::NoUnauthenticated => "!no-unauthenticated",
            Self::DmcaViolation => "dmca-violation",
            Self::Doxxing => "doxxing",
            Self::Porn => "porn",
            Self::Sexual => "sexual",
            Self::Nudity => "nudity",
            Self::Nsfl => "nsfl",
            Self::Gore => "gore",
            Self::Other(s) => s.as_ref(),
        }
    }
}
impl<'a> From<&'a str> for LabelValue<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            "!hide" => Self::Hide,
            "!no-promote" => Self::NoPromote,
            "!warn" => Self::Warn,
            "!no-unauthenticated" => Self::NoUnauthenticated,
            "dmca-violation" => Self::DmcaViolation,
            "doxxing" => Self::Doxxing,
            "porn" => Self::Porn,
            "sexual" => Self::Sexual,
            "nudity" => Self::Nudity,
            "nsfl" => Self::Nsfl,
            "gore" => Self::Gore,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> From<String> for LabelValue<'a> {
    fn from(s: String) -> Self {
        match s.as_str() {
            "!hide" => Self::Hide,
            "!no-promote" => Self::NoPromote,
            "!warn" => Self::Warn,
            "!no-unauthenticated" => Self::NoUnauthenticated,
            "dmca-violation" => Self::DmcaViolation,
            "doxxing" => Self::Doxxing,
            "porn" => Self::Porn,
            "sexual" => Self::Sexual,
            "nudity" => Self::Nudity,
            "nsfl" => Self::Nsfl,
            "gore" => Self::Gore,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> AsRef<str> for LabelValue<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl<'a> serde::Serialize for LabelValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de, 'a> serde::Deserialize<'de> for LabelValue<'a>
where
    'de: 'a,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <&'de str>::deserialize(deserializer)?;
        Ok(Self::from(s))
    }
}
///Declares a label value and its expected interpretations and behaviors.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LabelValueDefinition<'a> {
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub adult_only: std::option::Option<bool>,
    #[serde(borrow)]
    pub blurs: jacquard_common::CowStr<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub default_setting: std::option::Option<jacquard_common::CowStr<'a>>,
    #[serde(borrow)]
    pub identifier: jacquard_common::CowStr<'a>,
    #[serde(borrow)]
    pub locales: Vec<jacquard_common::types::value::Data<'a>>,
    #[serde(borrow)]
    pub severity: jacquard_common::CowStr<'a>,
}
///Strings which describe the label in the UI, localized into a specific language.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LabelValueDefinitionStrings<'a> {
    #[serde(borrow)]
    pub description: jacquard_common::CowStr<'a>,
    pub lang: jacquard_common::types::string::Language,
    #[serde(borrow)]
    pub name: jacquard_common::CowStr<'a>,
}
///Metadata tag on an atproto record, published by the author within the record. Note that schemas should use #selfLabels, not #selfLabel.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SelfLabel<'a> {
    #[serde(borrow)]
    pub val: jacquard_common::CowStr<'a>,
}
///Metadata tags on an atproto record, published by the author within the record.
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SelfLabels<'a> {
    #[serde(borrow)]
    pub values: Vec<jacquard_common::types::value::Data<'a>>,
}
pub mod query_labels;
pub mod subscribe_labels;
