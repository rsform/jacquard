#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReasonType<'a> {
    ToolsOzoneReportDefsReasonAppeal,
    ToolsOzoneReportDefsReasonViolenceAnimalWelfare,
    ToolsOzoneReportDefsReasonViolenceThreats,
    ToolsOzoneReportDefsReasonViolenceGraphicContent,
    ToolsOzoneReportDefsReasonViolenceSelfHarm,
    ToolsOzoneReportDefsReasonViolenceGlorification,
    ToolsOzoneReportDefsReasonViolenceExtremistContent,
    ToolsOzoneReportDefsReasonViolenceTrafficking,
    ToolsOzoneReportDefsReasonViolenceOther,
    ToolsOzoneReportDefsReasonSexualAbuseContent,
    ToolsOzoneReportDefsReasonSexualNcii,
    ToolsOzoneReportDefsReasonSexualSextortion,
    ToolsOzoneReportDefsReasonSexualDeepfake,
    ToolsOzoneReportDefsReasonSexualAnimal,
    ToolsOzoneReportDefsReasonSexualUnlabeled,
    ToolsOzoneReportDefsReasonSexualOther,
    ToolsOzoneReportDefsReasonChildSafetyCsam,
    ToolsOzoneReportDefsReasonChildSafetyGroom,
    ToolsOzoneReportDefsReasonChildSafetyMinorPrivacy,
    ToolsOzoneReportDefsReasonChildSafetyEndangerment,
    ToolsOzoneReportDefsReasonChildSafetyHarassment,
    ToolsOzoneReportDefsReasonChildSafetyPromotion,
    ToolsOzoneReportDefsReasonChildSafetyOther,
    ToolsOzoneReportDefsReasonHarassmentTroll,
    ToolsOzoneReportDefsReasonHarassmentTargeted,
    ToolsOzoneReportDefsReasonHarassmentHateSpeech,
    ToolsOzoneReportDefsReasonHarassmentDoxxing,
    ToolsOzoneReportDefsReasonHarassmentOther,
    ToolsOzoneReportDefsReasonMisleadingBot,
    ToolsOzoneReportDefsReasonMisleadingImpersonation,
    ToolsOzoneReportDefsReasonMisleadingSpam,
    ToolsOzoneReportDefsReasonMisleadingScam,
    ToolsOzoneReportDefsReasonMisleadingSyntheticContent,
    ToolsOzoneReportDefsReasonMisleadingMisinformation,
    ToolsOzoneReportDefsReasonMisleadingOther,
    ToolsOzoneReportDefsReasonRuleSiteSecurity,
    ToolsOzoneReportDefsReasonRuleStolenContent,
    ToolsOzoneReportDefsReasonRuleProhibitedSales,
    ToolsOzoneReportDefsReasonRuleBanEvasion,
    ToolsOzoneReportDefsReasonRuleOther,
    ToolsOzoneReportDefsReasonCivicElectoralProcess,
    ToolsOzoneReportDefsReasonCivicDisclosure,
    ToolsOzoneReportDefsReasonCivicInterference,
    ToolsOzoneReportDefsReasonCivicMisinformation,
    ToolsOzoneReportDefsReasonCivicImpersonation,
    Other(jacquard_common::CowStr<'a>),
}
impl<'a> ReasonType<'a> {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ToolsOzoneReportDefsReasonAppeal => {
                "tools.ozone.report.defs#reasonAppeal"
            }
            Self::ToolsOzoneReportDefsReasonViolenceAnimalWelfare => {
                "tools.ozone.report.defs#reasonViolenceAnimalWelfare"
            }
            Self::ToolsOzoneReportDefsReasonViolenceThreats => {
                "tools.ozone.report.defs#reasonViolenceThreats"
            }
            Self::ToolsOzoneReportDefsReasonViolenceGraphicContent => {
                "tools.ozone.report.defs#reasonViolenceGraphicContent"
            }
            Self::ToolsOzoneReportDefsReasonViolenceSelfHarm => {
                "tools.ozone.report.defs#reasonViolenceSelfHarm"
            }
            Self::ToolsOzoneReportDefsReasonViolenceGlorification => {
                "tools.ozone.report.defs#reasonViolenceGlorification"
            }
            Self::ToolsOzoneReportDefsReasonViolenceExtremistContent => {
                "tools.ozone.report.defs#reasonViolenceExtremistContent"
            }
            Self::ToolsOzoneReportDefsReasonViolenceTrafficking => {
                "tools.ozone.report.defs#reasonViolenceTrafficking"
            }
            Self::ToolsOzoneReportDefsReasonViolenceOther => {
                "tools.ozone.report.defs#reasonViolenceOther"
            }
            Self::ToolsOzoneReportDefsReasonSexualAbuseContent => {
                "tools.ozone.report.defs#reasonSexualAbuseContent"
            }
            Self::ToolsOzoneReportDefsReasonSexualNcii => {
                "tools.ozone.report.defs#reasonSexualNCII"
            }
            Self::ToolsOzoneReportDefsReasonSexualSextortion => {
                "tools.ozone.report.defs#reasonSexualSextortion"
            }
            Self::ToolsOzoneReportDefsReasonSexualDeepfake => {
                "tools.ozone.report.defs#reasonSexualDeepfake"
            }
            Self::ToolsOzoneReportDefsReasonSexualAnimal => {
                "tools.ozone.report.defs#reasonSexualAnimal"
            }
            Self::ToolsOzoneReportDefsReasonSexualUnlabeled => {
                "tools.ozone.report.defs#reasonSexualUnlabeled"
            }
            Self::ToolsOzoneReportDefsReasonSexualOther => {
                "tools.ozone.report.defs#reasonSexualOther"
            }
            Self::ToolsOzoneReportDefsReasonChildSafetyCsam => {
                "tools.ozone.report.defs#reasonChildSafetyCSAM"
            }
            Self::ToolsOzoneReportDefsReasonChildSafetyGroom => {
                "tools.ozone.report.defs#reasonChildSafetyGroom"
            }
            Self::ToolsOzoneReportDefsReasonChildSafetyMinorPrivacy => {
                "tools.ozone.report.defs#reasonChildSafetyMinorPrivacy"
            }
            Self::ToolsOzoneReportDefsReasonChildSafetyEndangerment => {
                "tools.ozone.report.defs#reasonChildSafetyEndangerment"
            }
            Self::ToolsOzoneReportDefsReasonChildSafetyHarassment => {
                "tools.ozone.report.defs#reasonChildSafetyHarassment"
            }
            Self::ToolsOzoneReportDefsReasonChildSafetyPromotion => {
                "tools.ozone.report.defs#reasonChildSafetyPromotion"
            }
            Self::ToolsOzoneReportDefsReasonChildSafetyOther => {
                "tools.ozone.report.defs#reasonChildSafetyOther"
            }
            Self::ToolsOzoneReportDefsReasonHarassmentTroll => {
                "tools.ozone.report.defs#reasonHarassmentTroll"
            }
            Self::ToolsOzoneReportDefsReasonHarassmentTargeted => {
                "tools.ozone.report.defs#reasonHarassmentTargeted"
            }
            Self::ToolsOzoneReportDefsReasonHarassmentHateSpeech => {
                "tools.ozone.report.defs#reasonHarassmentHateSpeech"
            }
            Self::ToolsOzoneReportDefsReasonHarassmentDoxxing => {
                "tools.ozone.report.defs#reasonHarassmentDoxxing"
            }
            Self::ToolsOzoneReportDefsReasonHarassmentOther => {
                "tools.ozone.report.defs#reasonHarassmentOther"
            }
            Self::ToolsOzoneReportDefsReasonMisleadingBot => {
                "tools.ozone.report.defs#reasonMisleadingBot"
            }
            Self::ToolsOzoneReportDefsReasonMisleadingImpersonation => {
                "tools.ozone.report.defs#reasonMisleadingImpersonation"
            }
            Self::ToolsOzoneReportDefsReasonMisleadingSpam => {
                "tools.ozone.report.defs#reasonMisleadingSpam"
            }
            Self::ToolsOzoneReportDefsReasonMisleadingScam => {
                "tools.ozone.report.defs#reasonMisleadingScam"
            }
            Self::ToolsOzoneReportDefsReasonMisleadingSyntheticContent => {
                "tools.ozone.report.defs#reasonMisleadingSyntheticContent"
            }
            Self::ToolsOzoneReportDefsReasonMisleadingMisinformation => {
                "tools.ozone.report.defs#reasonMisleadingMisinformation"
            }
            Self::ToolsOzoneReportDefsReasonMisleadingOther => {
                "tools.ozone.report.defs#reasonMisleadingOther"
            }
            Self::ToolsOzoneReportDefsReasonRuleSiteSecurity => {
                "tools.ozone.report.defs#reasonRuleSiteSecurity"
            }
            Self::ToolsOzoneReportDefsReasonRuleStolenContent => {
                "tools.ozone.report.defs#reasonRuleStolenContent"
            }
            Self::ToolsOzoneReportDefsReasonRuleProhibitedSales => {
                "tools.ozone.report.defs#reasonRuleProhibitedSales"
            }
            Self::ToolsOzoneReportDefsReasonRuleBanEvasion => {
                "tools.ozone.report.defs#reasonRuleBanEvasion"
            }
            Self::ToolsOzoneReportDefsReasonRuleOther => {
                "tools.ozone.report.defs#reasonRuleOther"
            }
            Self::ToolsOzoneReportDefsReasonCivicElectoralProcess => {
                "tools.ozone.report.defs#reasonCivicElectoralProcess"
            }
            Self::ToolsOzoneReportDefsReasonCivicDisclosure => {
                "tools.ozone.report.defs#reasonCivicDisclosure"
            }
            Self::ToolsOzoneReportDefsReasonCivicInterference => {
                "tools.ozone.report.defs#reasonCivicInterference"
            }
            Self::ToolsOzoneReportDefsReasonCivicMisinformation => {
                "tools.ozone.report.defs#reasonCivicMisinformation"
            }
            Self::ToolsOzoneReportDefsReasonCivicImpersonation => {
                "tools.ozone.report.defs#reasonCivicImpersonation"
            }
            Self::Other(s) => s.as_ref(),
        }
    }
}
impl<'a> From<&'a str> for ReasonType<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            "tools.ozone.report.defs#reasonAppeal" => {
                Self::ToolsOzoneReportDefsReasonAppeal
            }
            "tools.ozone.report.defs#reasonViolenceAnimalWelfare" => {
                Self::ToolsOzoneReportDefsReasonViolenceAnimalWelfare
            }
            "tools.ozone.report.defs#reasonViolenceThreats" => {
                Self::ToolsOzoneReportDefsReasonViolenceThreats
            }
            "tools.ozone.report.defs#reasonViolenceGraphicContent" => {
                Self::ToolsOzoneReportDefsReasonViolenceGraphicContent
            }
            "tools.ozone.report.defs#reasonViolenceSelfHarm" => {
                Self::ToolsOzoneReportDefsReasonViolenceSelfHarm
            }
            "tools.ozone.report.defs#reasonViolenceGlorification" => {
                Self::ToolsOzoneReportDefsReasonViolenceGlorification
            }
            "tools.ozone.report.defs#reasonViolenceExtremistContent" => {
                Self::ToolsOzoneReportDefsReasonViolenceExtremistContent
            }
            "tools.ozone.report.defs#reasonViolenceTrafficking" => {
                Self::ToolsOzoneReportDefsReasonViolenceTrafficking
            }
            "tools.ozone.report.defs#reasonViolenceOther" => {
                Self::ToolsOzoneReportDefsReasonViolenceOther
            }
            "tools.ozone.report.defs#reasonSexualAbuseContent" => {
                Self::ToolsOzoneReportDefsReasonSexualAbuseContent
            }
            "tools.ozone.report.defs#reasonSexualNCII" => {
                Self::ToolsOzoneReportDefsReasonSexualNcii
            }
            "tools.ozone.report.defs#reasonSexualSextortion" => {
                Self::ToolsOzoneReportDefsReasonSexualSextortion
            }
            "tools.ozone.report.defs#reasonSexualDeepfake" => {
                Self::ToolsOzoneReportDefsReasonSexualDeepfake
            }
            "tools.ozone.report.defs#reasonSexualAnimal" => {
                Self::ToolsOzoneReportDefsReasonSexualAnimal
            }
            "tools.ozone.report.defs#reasonSexualUnlabeled" => {
                Self::ToolsOzoneReportDefsReasonSexualUnlabeled
            }
            "tools.ozone.report.defs#reasonSexualOther" => {
                Self::ToolsOzoneReportDefsReasonSexualOther
            }
            "tools.ozone.report.defs#reasonChildSafetyCSAM" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyCsam
            }
            "tools.ozone.report.defs#reasonChildSafetyGroom" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyGroom
            }
            "tools.ozone.report.defs#reasonChildSafetyMinorPrivacy" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyMinorPrivacy
            }
            "tools.ozone.report.defs#reasonChildSafetyEndangerment" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyEndangerment
            }
            "tools.ozone.report.defs#reasonChildSafetyHarassment" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyHarassment
            }
            "tools.ozone.report.defs#reasonChildSafetyPromotion" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyPromotion
            }
            "tools.ozone.report.defs#reasonChildSafetyOther" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyOther
            }
            "tools.ozone.report.defs#reasonHarassmentTroll" => {
                Self::ToolsOzoneReportDefsReasonHarassmentTroll
            }
            "tools.ozone.report.defs#reasonHarassmentTargeted" => {
                Self::ToolsOzoneReportDefsReasonHarassmentTargeted
            }
            "tools.ozone.report.defs#reasonHarassmentHateSpeech" => {
                Self::ToolsOzoneReportDefsReasonHarassmentHateSpeech
            }
            "tools.ozone.report.defs#reasonHarassmentDoxxing" => {
                Self::ToolsOzoneReportDefsReasonHarassmentDoxxing
            }
            "tools.ozone.report.defs#reasonHarassmentOther" => {
                Self::ToolsOzoneReportDefsReasonHarassmentOther
            }
            "tools.ozone.report.defs#reasonMisleadingBot" => {
                Self::ToolsOzoneReportDefsReasonMisleadingBot
            }
            "tools.ozone.report.defs#reasonMisleadingImpersonation" => {
                Self::ToolsOzoneReportDefsReasonMisleadingImpersonation
            }
            "tools.ozone.report.defs#reasonMisleadingSpam" => {
                Self::ToolsOzoneReportDefsReasonMisleadingSpam
            }
            "tools.ozone.report.defs#reasonMisleadingScam" => {
                Self::ToolsOzoneReportDefsReasonMisleadingScam
            }
            "tools.ozone.report.defs#reasonMisleadingSyntheticContent" => {
                Self::ToolsOzoneReportDefsReasonMisleadingSyntheticContent
            }
            "tools.ozone.report.defs#reasonMisleadingMisinformation" => {
                Self::ToolsOzoneReportDefsReasonMisleadingMisinformation
            }
            "tools.ozone.report.defs#reasonMisleadingOther" => {
                Self::ToolsOzoneReportDefsReasonMisleadingOther
            }
            "tools.ozone.report.defs#reasonRuleSiteSecurity" => {
                Self::ToolsOzoneReportDefsReasonRuleSiteSecurity
            }
            "tools.ozone.report.defs#reasonRuleStolenContent" => {
                Self::ToolsOzoneReportDefsReasonRuleStolenContent
            }
            "tools.ozone.report.defs#reasonRuleProhibitedSales" => {
                Self::ToolsOzoneReportDefsReasonRuleProhibitedSales
            }
            "tools.ozone.report.defs#reasonRuleBanEvasion" => {
                Self::ToolsOzoneReportDefsReasonRuleBanEvasion
            }
            "tools.ozone.report.defs#reasonRuleOther" => {
                Self::ToolsOzoneReportDefsReasonRuleOther
            }
            "tools.ozone.report.defs#reasonCivicElectoralProcess" => {
                Self::ToolsOzoneReportDefsReasonCivicElectoralProcess
            }
            "tools.ozone.report.defs#reasonCivicDisclosure" => {
                Self::ToolsOzoneReportDefsReasonCivicDisclosure
            }
            "tools.ozone.report.defs#reasonCivicInterference" => {
                Self::ToolsOzoneReportDefsReasonCivicInterference
            }
            "tools.ozone.report.defs#reasonCivicMisinformation" => {
                Self::ToolsOzoneReportDefsReasonCivicMisinformation
            }
            "tools.ozone.report.defs#reasonCivicImpersonation" => {
                Self::ToolsOzoneReportDefsReasonCivicImpersonation
            }
            _ => Self::Other(jacquard_common::CowStr::from(s)),
        }
    }
}
impl<'a> From<String> for ReasonType<'a> {
    fn from(s: String) -> Self {
        match s.as_str() {
            "tools.ozone.report.defs#reasonAppeal" => {
                Self::ToolsOzoneReportDefsReasonAppeal
            }
            "tools.ozone.report.defs#reasonViolenceAnimalWelfare" => {
                Self::ToolsOzoneReportDefsReasonViolenceAnimalWelfare
            }
            "tools.ozone.report.defs#reasonViolenceThreats" => {
                Self::ToolsOzoneReportDefsReasonViolenceThreats
            }
            "tools.ozone.report.defs#reasonViolenceGraphicContent" => {
                Self::ToolsOzoneReportDefsReasonViolenceGraphicContent
            }
            "tools.ozone.report.defs#reasonViolenceSelfHarm" => {
                Self::ToolsOzoneReportDefsReasonViolenceSelfHarm
            }
            "tools.ozone.report.defs#reasonViolenceGlorification" => {
                Self::ToolsOzoneReportDefsReasonViolenceGlorification
            }
            "tools.ozone.report.defs#reasonViolenceExtremistContent" => {
                Self::ToolsOzoneReportDefsReasonViolenceExtremistContent
            }
            "tools.ozone.report.defs#reasonViolenceTrafficking" => {
                Self::ToolsOzoneReportDefsReasonViolenceTrafficking
            }
            "tools.ozone.report.defs#reasonViolenceOther" => {
                Self::ToolsOzoneReportDefsReasonViolenceOther
            }
            "tools.ozone.report.defs#reasonSexualAbuseContent" => {
                Self::ToolsOzoneReportDefsReasonSexualAbuseContent
            }
            "tools.ozone.report.defs#reasonSexualNCII" => {
                Self::ToolsOzoneReportDefsReasonSexualNcii
            }
            "tools.ozone.report.defs#reasonSexualSextortion" => {
                Self::ToolsOzoneReportDefsReasonSexualSextortion
            }
            "tools.ozone.report.defs#reasonSexualDeepfake" => {
                Self::ToolsOzoneReportDefsReasonSexualDeepfake
            }
            "tools.ozone.report.defs#reasonSexualAnimal" => {
                Self::ToolsOzoneReportDefsReasonSexualAnimal
            }
            "tools.ozone.report.defs#reasonSexualUnlabeled" => {
                Self::ToolsOzoneReportDefsReasonSexualUnlabeled
            }
            "tools.ozone.report.defs#reasonSexualOther" => {
                Self::ToolsOzoneReportDefsReasonSexualOther
            }
            "tools.ozone.report.defs#reasonChildSafetyCSAM" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyCsam
            }
            "tools.ozone.report.defs#reasonChildSafetyGroom" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyGroom
            }
            "tools.ozone.report.defs#reasonChildSafetyMinorPrivacy" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyMinorPrivacy
            }
            "tools.ozone.report.defs#reasonChildSafetyEndangerment" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyEndangerment
            }
            "tools.ozone.report.defs#reasonChildSafetyHarassment" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyHarassment
            }
            "tools.ozone.report.defs#reasonChildSafetyPromotion" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyPromotion
            }
            "tools.ozone.report.defs#reasonChildSafetyOther" => {
                Self::ToolsOzoneReportDefsReasonChildSafetyOther
            }
            "tools.ozone.report.defs#reasonHarassmentTroll" => {
                Self::ToolsOzoneReportDefsReasonHarassmentTroll
            }
            "tools.ozone.report.defs#reasonHarassmentTargeted" => {
                Self::ToolsOzoneReportDefsReasonHarassmentTargeted
            }
            "tools.ozone.report.defs#reasonHarassmentHateSpeech" => {
                Self::ToolsOzoneReportDefsReasonHarassmentHateSpeech
            }
            "tools.ozone.report.defs#reasonHarassmentDoxxing" => {
                Self::ToolsOzoneReportDefsReasonHarassmentDoxxing
            }
            "tools.ozone.report.defs#reasonHarassmentOther" => {
                Self::ToolsOzoneReportDefsReasonHarassmentOther
            }
            "tools.ozone.report.defs#reasonMisleadingBot" => {
                Self::ToolsOzoneReportDefsReasonMisleadingBot
            }
            "tools.ozone.report.defs#reasonMisleadingImpersonation" => {
                Self::ToolsOzoneReportDefsReasonMisleadingImpersonation
            }
            "tools.ozone.report.defs#reasonMisleadingSpam" => {
                Self::ToolsOzoneReportDefsReasonMisleadingSpam
            }
            "tools.ozone.report.defs#reasonMisleadingScam" => {
                Self::ToolsOzoneReportDefsReasonMisleadingScam
            }
            "tools.ozone.report.defs#reasonMisleadingSyntheticContent" => {
                Self::ToolsOzoneReportDefsReasonMisleadingSyntheticContent
            }
            "tools.ozone.report.defs#reasonMisleadingMisinformation" => {
                Self::ToolsOzoneReportDefsReasonMisleadingMisinformation
            }
            "tools.ozone.report.defs#reasonMisleadingOther" => {
                Self::ToolsOzoneReportDefsReasonMisleadingOther
            }
            "tools.ozone.report.defs#reasonRuleSiteSecurity" => {
                Self::ToolsOzoneReportDefsReasonRuleSiteSecurity
            }
            "tools.ozone.report.defs#reasonRuleStolenContent" => {
                Self::ToolsOzoneReportDefsReasonRuleStolenContent
            }
            "tools.ozone.report.defs#reasonRuleProhibitedSales" => {
                Self::ToolsOzoneReportDefsReasonRuleProhibitedSales
            }
            "tools.ozone.report.defs#reasonRuleBanEvasion" => {
                Self::ToolsOzoneReportDefsReasonRuleBanEvasion
            }
            "tools.ozone.report.defs#reasonRuleOther" => {
                Self::ToolsOzoneReportDefsReasonRuleOther
            }
            "tools.ozone.report.defs#reasonCivicElectoralProcess" => {
                Self::ToolsOzoneReportDefsReasonCivicElectoralProcess
            }
            "tools.ozone.report.defs#reasonCivicDisclosure" => {
                Self::ToolsOzoneReportDefsReasonCivicDisclosure
            }
            "tools.ozone.report.defs#reasonCivicInterference" => {
                Self::ToolsOzoneReportDefsReasonCivicInterference
            }
            "tools.ozone.report.defs#reasonCivicMisinformation" => {
                Self::ToolsOzoneReportDefsReasonCivicMisinformation
            }
            "tools.ozone.report.defs#reasonCivicImpersonation" => {
                Self::ToolsOzoneReportDefsReasonCivicImpersonation
            }
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
