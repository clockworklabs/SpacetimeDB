//! Shared run-budget configuration for DST targets.

use std::{
    fmt,
    time::{Duration, Instant},
};

/// Coarse disk-fault profile for commitlog-backed DST targets.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum CommitlogFaultProfile {
    Off,
    Light,
    #[default]
    Default,
    Aggressive,
}

impl fmt::Display for CommitlogFaultProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("off"),
            Self::Light => f.write_str("light"),
            Self::Default => f.write_str("default"),
            Self::Aggressive => f.write_str("aggressive"),
        }
    }
}

/// Common stop conditions for generated DST runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunConfig {
    /// Hard cap on generated interactions. `None` means no interaction budget.
    ///
    /// This is the preferred budget for exact seed replay: the same target,
    /// scenario, seed, max-interactions value, and fault profile should produce
    /// the same generated interaction stream.
    pub max_interactions: Option<usize>,
    /// Wall-clock duration budget in milliseconds. `None` means no time budget.
    ///
    /// Duration runs are useful as local soaks, but the exact stop step can vary
    /// with host speed and runtime behavior. Use `max_interactions` when a
    /// failure needs precise replay.
    pub max_duration_ms: Option<u64>,
    /// Disk-fault profile for commitlog-backed targets.
    pub commitlog_fault_profile: CommitlogFaultProfile,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            max_interactions: None,
            max_duration_ms: None,
            commitlog_fault_profile: CommitlogFaultProfile::Default,
        }
    }
}

impl RunConfig {
    pub fn with_max_interactions(max_interactions: usize) -> Self {
        Self {
            max_interactions: Some(max_interactions),
            max_duration_ms: None,
            ..Default::default()
        }
    }

    pub fn with_duration_spec(duration: &str) -> anyhow::Result<Self> {
        Ok(Self {
            max_interactions: None,
            max_duration_ms: Some(parse_duration_spec(duration)?.as_millis() as u64),
            ..Default::default()
        })
    }

    pub fn with_commitlog_fault_profile(mut self, profile: CommitlogFaultProfile) -> Self {
        self.commitlog_fault_profile = profile;
        self
    }

    /// Return the wall-clock deadline for duration-budgeted runs.
    ///
    /// This intentionally uses `std::time::Instant`, not simulated time. DST
    /// duration budgets are a harness stop condition rather than part of the
    /// simulated system under test.
    pub fn deadline(&self) -> Option<Instant> {
        self.max_duration_ms
            .map(Duration::from_millis)
            .map(|duration| Instant::now() + duration)
    }

    pub fn max_interactions_or_default(&self, default: usize) -> usize {
        self.max_interactions.unwrap_or(default)
    }
}

pub fn parse_duration_spec(spec: &str) -> anyhow::Result<Duration> {
    let spec = spec.trim();
    if spec.is_empty() {
        anyhow::bail!("duration spec cannot be empty");
    }

    let split_at = spec
        .find(|ch: char| !ch.is_ascii_digit())
        .ok_or_else(|| anyhow::anyhow!("duration spec missing unit: {spec}"))?;
    let (digits, unit) = spec.split_at(split_at);
    let value: u64 = digits.parse()?;

    match unit {
        "ms" => Ok(Duration::from_millis(value)),
        "s" => Ok(Duration::from_secs(value)),
        "m" => Ok(Duration::from_secs(value.saturating_mul(60))),
        "h" => Ok(Duration::from_secs(value.saturating_mul(60 * 60))),
        _ => anyhow::bail!("unsupported duration unit: {unit}"),
    }
}
