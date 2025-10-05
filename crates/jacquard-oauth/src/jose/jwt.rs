use jacquard_common::{CowStr, IntoStatic};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Claims<'a> {
    #[serde(flatten)]
    pub registered: RegisteredClaims<'a>,
    #[serde(flatten)]
    #[serde(borrow)]
    pub public: PublicClaims<'a>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]

pub struct RegisteredClaims<'a> {
    #[serde(borrow)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<RegisteredClaimsAud<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<CowStr<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]

pub struct PublicClaims<'a> {
    #[serde(borrow)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub htm: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub htu: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ath: Option<CowStr<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<CowStr<'a>>,
}

impl<'a> From<RegisteredClaims<'a>> for Claims<'a> {
    fn from(registered: RegisteredClaims<'a>) -> Self {
        Self {
            registered,
            public: PublicClaims::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RegisteredClaimsAud<'a> {
    #[serde(borrow)]
    Single(CowStr<'a>),
    Multiple(Vec<CowStr<'a>>),
}

impl IntoStatic for RegisteredClaims<'_> {
    type Output = RegisteredClaims<'static>;
    fn into_static(self) -> Self::Output {
        RegisteredClaims {
            iss: self.iss.map(IntoStatic::into_static),
            sub: self.sub.map(IntoStatic::into_static),
            aud: self.aud.map(IntoStatic::into_static),
            exp: self.exp,
            nbf: self.nbf,
            iat: self.iat,
            jti: self.jti.map(IntoStatic::into_static),
        }
    }
}

impl IntoStatic for PublicClaims<'_> {
    type Output = PublicClaims<'static>;
    fn into_static(self) -> Self::Output {
        PublicClaims {
            htm: self.htm.map(IntoStatic::into_static),
            htu: self.htu.map(IntoStatic::into_static),
            ath: self.ath.map(IntoStatic::into_static),
            nonce: self.nonce.map(IntoStatic::into_static),
        }
    }
}

impl IntoStatic for RegisteredClaimsAud<'_> {
    type Output = RegisteredClaimsAud<'static>;
    fn into_static(self) -> Self::Output {
        match self {
            RegisteredClaimsAud::Single(s) => RegisteredClaimsAud::Single(s.into_static()),
            RegisteredClaimsAud::Multiple(v) => {
                RegisteredClaimsAud::Multiple(v.into_iter().map(IntoStatic::into_static).collect())
            }
        }
    }
}
