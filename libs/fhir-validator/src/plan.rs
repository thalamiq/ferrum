use crate::{
    BundleConfig, ConstraintsConfig, ProfilesConfig, ReferencesConfig, SchemaConfig,
    TerminologyConfig,
};

/// Compiled validation plan - list of steps to execute
#[derive(Debug, Clone)]
pub struct ValidationPlan {
    pub steps: Vec<Step>,
    pub fail_fast: bool,
    pub max_issues: usize,
}

#[derive(Debug, Clone)]
pub enum Step {
    Schema(SchemaPlan),
    Profiles(ProfilesPlan),
    Constraints(ConstraintsPlan),
    Terminology(TerminologyPlan),
    References(ReferencesPlan),
    Bundles(BundlePlan),
}

// ============================================================================
// Step Plans
// ============================================================================

#[derive(Debug, Clone)]
pub struct SchemaPlan {
    pub allow_unknown_elements: bool,
    pub allow_modifier_extensions: bool,
}

impl From<&SchemaConfig> for SchemaPlan {
    fn from(cfg: &SchemaConfig) -> Self {
        Self {
            allow_unknown_elements: cfg.allow_unknown_elements,
            allow_modifier_extensions: cfg.allow_modifier_extensions,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfilesPlan {
    /// Explicit list of profile URLs to validate against.
    /// If Some, validates against these profiles instead of meta.profile.
    /// If None, validates against profiles declared in resource.meta.profile.
    pub explicit_profiles: Option<Vec<String>>,
}

impl From<&ProfilesConfig> for ProfilesPlan {
    fn from(cfg: &ProfilesConfig) -> Self {
        Self {
            explicit_profiles: cfg.explicit_profiles.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConstraintsPlan {
    pub mode: crate::ConstraintsMode,
    pub best_practice: crate::BestPracticeMode,
    pub suppress: Vec<crate::ConstraintId>,
    pub level_overrides: Vec<crate::ConstraintLevelOverride>,
}

impl From<&ConstraintsConfig> for ConstraintsPlan {
    fn from(cfg: &ConstraintsConfig) -> Self {
        Self {
            mode: cfg.mode,
            best_practice: cfg.best_practice,
            suppress: cfg.suppress.clone(),
            level_overrides: cfg.level_overrides.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerminologyPlan {
    pub mode: crate::TerminologyMode,
    pub extensible_handling: crate::ExtensibleHandling,
    pub timeout: std::time::Duration,
    pub on_timeout: crate::TimeoutPolicy,
    pub cache: crate::CachePolicy,
}

impl From<&TerminologyConfig> for TerminologyPlan {
    fn from(cfg: &TerminologyConfig) -> Self {
        Self {
            mode: cfg.mode,
            extensible_handling: cfg.extensible_handling,
            timeout: cfg.timeout,
            on_timeout: cfg.on_timeout,
            cache: cfg.cache,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReferencesPlan {
    pub mode: crate::ReferenceMode,
    pub allow_external: bool,
}

impl From<&ReferencesConfig> for ReferencesPlan {
    fn from(cfg: &ReferencesConfig) -> Self {
        Self {
            mode: cfg.mode,
            allow_external: cfg.allow_external,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BundlePlan {
    // Future: bundle type validations, entry checks, etc.
}

impl From<&BundleConfig> for BundlePlan {
    fn from(_cfg: &BundleConfig) -> Self {
        Self {}
    }
}
