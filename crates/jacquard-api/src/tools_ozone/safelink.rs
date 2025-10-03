#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActionType<'a> {
    Block,
    Warn,
    Whitelist,
    Other(jacquard_common::CowStr<'a>),
}
impl<'a> ActionType<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Block => "block",
            Self::Warn => "warn",
            Self::Whitelist => "whitelist",
            Self::Other(s) => s.as_ref(),
        }
    }
}
impl<'a> From<&'a str> for ActionType<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            "block" => Self::Block,
            "warn" => Self::Warn,
            "whitelist" => Self::Whitelist,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> From<String> for ActionType<'a> {
    fn from(s: String) -> Self {
        match s.as_str() {
            "block" => Self::Block,
            "warn" => Self::Warn,
            "whitelist" => Self::Whitelist,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> AsRef<str> for ActionType<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl<'a> serde::Serialize for ActionType<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de, 'a> serde::Deserialize<'de> for ActionType<'a>
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
///An event for URL safety decisions
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Event<'a> {
    #[serde(borrow)]
    pub action: jacquard_common::types::value::Data<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub comment: std::option::Option<jacquard_common::CowStr<'a>>,
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub created_by: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub event_type: jacquard_common::types::value::Data<'a>,
    pub id: i64,
    #[serde(borrow)]
    pub pattern: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub reason: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub url: jacquard_common::CowStr<'a>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType<'a> {
    AddRule,
    UpdateRule,
    RemoveRule,
    Other(jacquard_common::CowStr<'a>),
}
impl<'a> EventType<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            Self::AddRule => "addRule",
            Self::UpdateRule => "updateRule",
            Self::RemoveRule => "removeRule",
            Self::Other(s) => s.as_ref(),
        }
    }
}
impl<'a> From<&'a str> for EventType<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            "addRule" => Self::AddRule,
            "updateRule" => Self::UpdateRule,
            "removeRule" => Self::RemoveRule,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> From<String> for EventType<'a> {
    fn from(s: String) -> Self {
        match s.as_str() {
            "addRule" => Self::AddRule,
            "updateRule" => Self::UpdateRule,
            "removeRule" => Self::RemoveRule,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> AsRef<str> for EventType<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl<'a> serde::Serialize for EventType<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de, 'a> serde::Deserialize<'de> for EventType<'a>
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PatternType<'a> {
    Domain,
    Url,
    Other(jacquard_common::CowStr<'a>),
}
impl<'a> PatternType<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Domain => "domain",
            Self::Url => "url",
            Self::Other(s) => s.as_ref(),
        }
    }
}
impl<'a> From<&'a str> for PatternType<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            "domain" => Self::Domain,
            "url" => Self::Url,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> From<String> for PatternType<'a> {
    fn from(s: String) -> Self {
        match s.as_str() {
            "domain" => Self::Domain,
            "url" => Self::Url,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> AsRef<str> for PatternType<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl<'a> serde::Serialize for PatternType<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de, 'a> serde::Deserialize<'de> for PatternType<'a>
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReasonType<'a> {
    Csam,
    Spam,
    Phishing,
    None,
    Other(jacquard_common::CowStr<'a>),
}
impl<'a> ReasonType<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Csam => "csam",
            Self::Spam => "spam",
            Self::Phishing => "phishing",
            Self::None => "none",
            Self::Other(s) => s.as_ref(),
        }
    }
}
impl<'a> From<&'a str> for ReasonType<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            "csam" => Self::Csam,
            "spam" => Self::Spam,
            "phishing" => Self::Phishing,
            "none" => Self::None,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> From<String> for ReasonType<'a> {
    fn from(s: String) -> Self {
        match s.as_str() {
            "csam" => Self::Csam,
            "spam" => Self::Spam,
            "phishing" => Self::Phishing,
            "none" => Self::None,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> AsRef<str> for ReasonType<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl<'a> serde::Serialize for ReasonType<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de, 'a> serde::Deserialize<'de> for ReasonType<'a>
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
///Input for creating a URL safety rule
#[jacquard_derive::lexicon]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UrlRule<'a> {
    #[serde(borrow)]
    pub action: jacquard_common::types::value::Data<'a>,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    #[serde(borrow)]
    pub comment: std::option::Option<jacquard_common::CowStr<'a>>,
    pub created_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub created_by: jacquard_common::types::string::Did<'a>,
    #[serde(borrow)]
    pub pattern: jacquard_common::types::value::Data<'a>,
    #[serde(borrow)]
    pub reason: jacquard_common::types::value::Data<'a>,
    pub updated_at: jacquard_common::types::string::Datetime,
    #[serde(borrow)]
    pub url: jacquard_common::CowStr<'a>,
}
pub mod add_rule;
pub mod query_events;
pub mod query_rules;
pub mod remove_rule;
pub mod update_rule;
