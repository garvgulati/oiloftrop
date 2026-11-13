//! Installation purpose is a host decision. Profile names cannot change it.
use crate::errors::{Error, Result, ValidationError};

pub const RUNTIME_BOUNDARY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePurpose {
    Standard,
    Fictional,
    Personal,
}

/// Host capabilities are separate from provider settings and API credentials.
/// A saved setting or a new route must not implicitly enable private-data egress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapability {
    LocalMetadata,
    ProfileAccess,
    InvestorPlan,
    PublicResearch,
    PublicResearchCredentials,
    ReviewedHoldings,
    AutomaticWork,
    GenericAi,
    CloudSync,
    Addons,
    ProviderConfiguration,
    PortfolioData,
    DatabaseTransfer,
}

impl RuntimePurpose {
    pub fn allows(self, capability: RuntimeCapability) -> bool {
        self != Self::Personal
            || matches!(
                capability,
                RuntimeCapability::LocalMetadata
                    | RuntimeCapability::ProfileAccess
                    | RuntimeCapability::InvestorPlan
                    | RuntimeCapability::PublicResearch
                    | RuntimeCapability::PublicResearchCredentials
                    | RuntimeCapability::ReviewedHoldings
            )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Fictional => "fictional",
            Self::Personal => "personal",
        }
    }

    pub fn from_env() -> Result<Self> {
        Self::from_settings(
            std::env::var("WF_RUNTIME_PURPOSE").ok().as_deref(),
            std::env::var("WF_RUNTIME_BOUNDARY_VERSION").ok().as_deref(),
            std::env::var("WEALTHFOLIO_E2E").ok().as_deref(),
            std::env::var("WEALTHFOLIO_FIXTURE_DIR").ok().as_deref(),
            std::env::var("WEALTHFOLIO_FIXTURE_AS_OF").ok().as_deref(),
        )
    }

    /// Pure parsing keeps configuration tests independent of process-wide env.
    pub fn from_settings(
        purpose: Option<&str>,
        version: Option<&str>,
        e2e: Option<&str>,
        fixture_dir: Option<&str>,
        fixture_as_of: Option<&str>,
    ) -> Result<Self> {
        let purpose = match purpose {
            None if e2e == Some("1") && fixture_dir.is_some() => Self::Fictional,
            None => Self::Standard,
            Some("standard") => Self::Standard,
            Some("fictional") => Self::Fictional,
            Some("personal") => Self::Personal,
            Some(_) => return Err(invalid("Unknown runtime purpose; startup was refused.")),
        };
        if version.is_some_and(|v| v != "1") || (purpose == Self::Personal && version != Some("1"))
        {
            return Err(invalid(
                "Unsupported runtime boundary version; startup was refused.",
            ));
        }
        match purpose {
            Self::Personal if e2e.is_some() || fixture_dir.is_some() || fixture_as_of.is_some() => {
                Err(invalid(
                    "Fixture settings conflict with this runtime purpose.",
                ))
            }
            Self::Standard
                if e2e == Some("1") || fixture_dir.is_some() || fixture_as_of.is_some() =>
            {
                Err(invalid(
                    "Fixture settings conflict with this runtime purpose.",
                ))
            }
            Self::Fictional
                if e2e != Some("1") || fixture_dir.is_none_or(|v| v.trim().is_empty()) =>
            {
                Err(invalid(
                    "Fictional runtime requires its isolated fixture settings.",
                ))
            }
            _ => Ok(purpose),
        }
    }

    /// Personal services are deliberately unavailable until outbound controls and
    /// provenance-aware readers are implemented. Never remove this gate merely
    /// to make a candidate start: complete the implementation acceptance gates.
    pub fn require_supported_services(self) -> Result<()> {
        if self == Self::Personal {
            return Err(invalid(
                "Personal portfolio services are not ready. Keep the existing installation and use the reviewed personal setup when available.",
            ));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> Error {
    ValidationError::InvalidInput(message.to_string()).into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "pure runtime configuration tests")]
mod tests {
    use super::*;

    #[test]
    fn personal_capabilities_are_independent_of_saved_credentials_and_settings() {
        for denied in [
            RuntimeCapability::AutomaticWork,
            RuntimeCapability::GenericAi,
            RuntimeCapability::CloudSync,
            RuntimeCapability::Addons,
            RuntimeCapability::ProviderConfiguration,
            RuntimeCapability::PortfolioData,
            RuntimeCapability::DatabaseTransfer,
        ] {
            assert!(!RuntimePurpose::Personal.allows(denied));
            assert!(RuntimePurpose::Fictional.allows(denied));
            assert!(RuntimePurpose::Standard.allows(denied));
        }
        for allowed in [
            RuntimeCapability::LocalMetadata,
            RuntimeCapability::ProfileAccess,
            RuntimeCapability::InvestorPlan,
            RuntimeCapability::PublicResearch,
            RuntimeCapability::PublicResearchCredentials,
            RuntimeCapability::ReviewedHoldings,
        ] {
            assert!(RuntimePurpose::Personal.allows(allowed));
        }
    }

    #[test]
    fn existing_trial_and_standard_configs_remain_distinct() {
        assert_eq!(
            RuntimePurpose::from_settings(None, None, None, None, None).unwrap(),
            RuntimePurpose::Standard
        );
        assert_eq!(
            RuntimePurpose::from_settings(None, None, Some("0"), None, None).unwrap(),
            RuntimePurpose::Standard
        );
        assert_eq!(
            RuntimePurpose::from_settings(None, None, Some("1"), Some("fixtures"), None).unwrap(),
            RuntimePurpose::Fictional
        );
    }

    #[test]
    fn personal_requires_version_and_rejects_all_fixture_variables() {
        assert!(RuntimePurpose::from_settings(Some("personal"), None, None, None, None).is_err());
        assert!(
            RuntimePurpose::from_settings(Some("personal"), Some("2"), None, None, None).is_err()
        );
        for (e2e, dir, date) in [
            (Some("0"), None, None),
            (None, Some(""), None),
            (None, None, Some("2026-10-28")),
        ] {
            assert!(
                RuntimePurpose::from_settings(Some("personal"), Some("1"), e2e, dir, date).is_err()
            );
        }
        assert_eq!(
            RuntimePurpose::from_settings(Some("personal"), Some("1"), None, None, None).unwrap(),
            RuntimePurpose::Personal
        );
        assert!(RuntimePurpose::Personal
            .require_supported_services()
            .is_err());
    }

    #[test]
    fn invalid_or_contradictory_settings_fail_closed() {
        assert!(
            RuntimePurpose::from_settings(Some("Personal"), Some("1"), None, None, None).is_err()
        );
        assert!(RuntimePurpose::from_settings(
            Some("fictional"),
            Some("1"),
            None,
            Some("fixtures"),
            None
        )
        .is_err());
        assert!(RuntimePurpose::from_settings(
            Some("standard"),
            None,
            Some("1"),
            Some("fixtures"),
            None
        )
        .is_err());
    }
}
