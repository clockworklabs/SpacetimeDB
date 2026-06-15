//! Shared run-budget configuration for DST targets.

use std::{
    fmt,
    time::{Duration, Instant},
};

/// Storage fault-injection profile for commitlog and snapshot wrappers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CommitlogFaultProfile {
    /// No faults injected regardless of buggify state.
    Off,
    /// Low probability latency and short I/O only.
    Light,
    /// Moderate-latency and short I/O only.
    #[default]
    Default,
    /// Heavy-latency and short I/O only.
    Aggressive,
}

impl CommitlogFaultProfile {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "off" => Ok(Self::Off),
            "light" => Ok(Self::Light),
            "default" => Ok(Self::Default),
            "aggressive" => Ok(Self::Aggressive),
            _ => anyhow::bail!(
                "unsupported commitlog fault profile: {value}; expected one of: off, light, default, aggressive"
            ),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Light => "light",
            Self::Default => "default",
            Self::Aggressive => "aggressive",
        }
    }
}

impl fmt::Display for CommitlogFaultProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StorageFaultSummary {
    pub profile: CommitlogFaultProfile,
    pub latency: usize,
    pub short_read: usize,
    pub short_write: usize,
    pub read_error: usize,
    pub write_error: usize,
    pub flush_error: usize,
    pub fsync_error: usize,
    pub open_error: usize,
    pub metadata_error: usize,
    pub no_space: usize,
    pub partial_failure: usize,
}

/// Common stop conditions for generated DST runs.
pub const DEFAULT_HARNESS_PHASE_TIMEOUT_MS: u64 = 30_000;

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
    /// Virtual-time watchdog for each target execution and collection phase.
    /// `None` disables the watchdog.
    pub harness_phase_timeout_ms: Option<u64>,
    pub commitlog_fault_profile: CommitlogFaultProfile,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            max_interactions: None,
            max_duration_ms: None,
            harness_phase_timeout_ms: Some(DEFAULT_HARNESS_PHASE_TIMEOUT_MS),
            commitlog_fault_profile: CommitlogFaultProfile::Default,
        }
    }
}

impl RunConfig {
    pub fn with_max_interactions(max_interactions: usize) -> Self {
        Self {
            max_interactions: Some(max_interactions),
            max_duration_ms: None,
            harness_phase_timeout_ms: Some(DEFAULT_HARNESS_PHASE_TIMEOUT_MS),
            commitlog_fault_profile: CommitlogFaultProfile::Default,
        }
    }

    pub fn with_duration_spec(duration: &str) -> anyhow::Result<Self> {
        Ok(Self {
            max_interactions: None,
            max_duration_ms: Some(parse_duration_spec(duration)?.as_millis() as u64),
            harness_phase_timeout_ms: Some(DEFAULT_HARNESS_PHASE_TIMEOUT_MS),
            commitlog_fault_profile: CommitlogFaultProfile::Default,
        })
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
