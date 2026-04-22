//! Shared run-budget configuration for DST targets.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Common stop conditions for generated DST runs.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunConfig {
    /// Hard cap on generated interactions. `None` means no interaction budget.
    pub max_interactions: Option<usize>,
    /// Wall-clock duration budget in milliseconds. `None` means no time budget.
    pub max_duration_ms: Option<u64>,
}

impl RunConfig {
    pub fn with_max_interactions(max_interactions: usize) -> Self {
        Self {
            max_interactions: Some(max_interactions),
            max_duration_ms: None,
        }
    }

    pub fn with_duration_spec(duration: &str) -> anyhow::Result<Self> {
        Ok(Self {
            max_interactions: None,
            max_duration_ms: Some(parse_duration_spec(duration)?.as_millis() as u64),
        })
    }

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
