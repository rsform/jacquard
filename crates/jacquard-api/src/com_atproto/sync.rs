#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostStatus<'a> {
    Active,
    Idle,
    Offline,
    Throttled,
    Banned,
    Other(jacquard_common::CowStr<'a>),
}
impl<'a> HostStatus<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Offline => "offline",
            Self::Throttled => "throttled",
            Self::Banned => "banned",
            Self::Other(s) => s.as_ref(),
        }
    }
}
impl<'a> From<&'a str> for HostStatus<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            "active" => Self::Active,
            "idle" => Self::Idle,
            "offline" => Self::Offline,
            "throttled" => Self::Throttled,
            "banned" => Self::Banned,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> From<String> for HostStatus<'a> {
    fn from(s: String) -> Self {
        match s.as_str() {
            "active" => Self::Active,
            "idle" => Self::Idle,
            "offline" => Self::Offline,
            "throttled" => Self::Throttled,
            "banned" => Self::Banned,
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> AsRef<str> for HostStatus<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl<'a> serde::Serialize for HostStatus<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de, 'a> serde::Deserialize<'de> for HostStatus<'a>
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
pub mod get_blob;
pub mod get_blocks;
pub mod get_checkout;
pub mod get_head;
pub mod get_host_status;
pub mod get_latest_commit;
pub mod get_record;
pub mod get_repo;
pub mod get_repo_status;
pub mod list_blobs;
pub mod list_hosts;
pub mod list_repos;
pub mod list_repos_by_collection;
pub mod notify_of_update;
pub mod request_crawl;
pub mod subscribe_repos;
