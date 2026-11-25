use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::assert::AssertionFailed;
use super::choices::Draw;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FailureKind {
    /// An assertion did not hold.
    Assertion,
    /// A rule or hook returned an error.
    Error,
    /// The run panicked.
    Panic,
    /// `open` or `after_step` exceeded its virtual-time budget.
    Timeout,
    /// No rule's guard held.
    NoRuleEnabled,
    /// Two runs of the same seed diverged.
    Nondeterminism,
}

impl FailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assertion => "assertion",
            Self::Error => "error",
            Self::Panic => "panic",
            Self::Timeout => "timeout",
            Self::NoRuleEnabled => "no-rule-enabled",
            Self::Nondeterminism => "nondeterminism",
        }
    }
}

/// Everything needed to understand and reproduce a failed run.
#[derive(Debug, Clone)]
pub struct Failure {
    pub world: &'static str,
    pub seed: u64,
    pub kind: FailureKind,
    /// Assertion name, when the failure came from a failed assertion.
    pub assertion: Option<&'static str>,
    /// Step index, or `None` if the run failed while opening the world.
    pub step: Option<u64>,
    /// Rule running when the failure happened.
    pub rule: Option<&'static str>,
    pub message: String,
    /// Rules of the most recent steps, as `(step, rule)`, oldest first.
    pub recent_rules: Vec<(u64, &'static str)>,
    pub recent: Vec<Draw>,
    pub recorded: Vec<u64>,
}

impl Failure {
    pub(crate) fn classify(err: &anyhow::Error) -> (FailureKind, Option<&'static str>, String) {
        match err.downcast_ref::<AssertionFailed>() {
            Some(failed) => (FailureKind::Assertion, Some(failed.name), failed.to_string()),
            None => (FailureKind::Error, None, format!("{err:#}")),
        }
    }

    /// Short identity used to group the same bug found by different seeds.
    pub fn signature(&self) -> String {
        let what = match self.assertion {
            Some(name) => name.to_owned(),
            None => self.message.lines().next().unwrap_or_default().to_owned(),
        };
        format!("{}:{}:{}", self.kind.as_str(), self.rule.unwrap_or("open"), what)
    }

    /// Replay data that reproduces this failure.
    pub fn replay(&self) -> Replay {
        Replay {
            world: self.world.to_owned(),
            seed: self.seed,
            steps: self.step.map_or(0, |step| step + 1),
            values: self.recorded.clone(),
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let step = self.step.map_or_else(|| "open".to_owned(), |step| step.to_string());
        writeln!(
            formatter,
            "DST FAILURE  world={}  seed={:#x}  step={}  rule={}  kind={}",
            self.world,
            self.seed,
            step,
            self.rule.unwrap_or("-"),
            self.kind.as_str()
        )?;
        writeln!(formatter, "  {}", self.message)?;
        if !self.recent_rules.is_empty() {
            let steps: Vec<String> = self
                .recent_rules
                .iter()
                .map(|(step, rule)| format!("{step}:{rule}"))
                .collect();
            writeln!(formatter, "  recent steps: {}", steps.join(" "))?;
        }
        if !self.recent.is_empty() {
            writeln!(formatter, "  recent draws (step label=value):")?;
            for draw in &self.recent {
                writeln!(formatter, "    {:>6} {}={}", draw.step, draw.label, draw.value)?;
            }
        }
        Ok(())
    }
}

/// A recorded run: world, seed, step budget and choice buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replay {
    pub world: String,
    pub seed: u64,
    pub steps: u64,
    pub values: Vec<u64>,
}

const REPLAY_HEADER: &str = "# spacetimedb-dst replay v1";

impl Replay {
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "{REPLAY_HEADER}\nworld {}\nseed {:#x}\nsteps {}\nvalues\n",
            self.world, self.seed, self.steps
        );
        for value in &self.values {
            out.push_str(&format!("{value:x}\n"));
        }
        out
    }

    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let mut lines = text.lines();
        anyhow::ensure!(lines.next() == Some(REPLAY_HEADER), "not a replay file");
        let mut field = |name: &str| -> anyhow::Result<String> {
            let line = lines.next().ok_or_else(|| anyhow::anyhow!("missing `{name}`"))?;
            line.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix(' '))
                .map(str::to_owned)
                .ok_or_else(|| anyhow::anyhow!("expected `{name}`, got `{line}`"))
        };
        let world = field("world")?;
        let seed = parse_u64(&field("seed")?)?;
        let steps = field("steps")?.parse()?;
        anyhow::ensure!(lines.next() == Some("values"), "expected `values`");
        let values = lines
            .filter(|line| !line.is_empty())
            .map(|line| u64::from_str_radix(line, 16).map_err(Into::into))
            .collect::<anyhow::Result<_>>()?;
        Ok(Self {
            world,
            seed,
            steps,
            values,
        })
    }

    pub fn read(path: &Path) -> anyhow::Result<Self> {
        Self::parse(&fs::read_to_string(path)?)
    }

    pub fn write(&self, dir: &Path, name: &str) -> io::Result<PathBuf> {
        fs::create_dir_all(dir)?;
        let path = dir.join(format!("{name}.dst"));
        fs::write(&path, self.to_text())?;
        Ok(path)
    }
}

/// Parse a decimal or `0x`-prefixed hex seed.
pub fn parse_u64(text: &str) -> anyhow::Result<u64> {
    let text = text.trim();
    Ok(match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16)?,
        None => text.parse()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_text_round_trips() {
        let replay = Replay {
            world: "datastore".into(),
            seed: 0xdead_beef,
            steps: 12,
            values: vec![0, 1, u64::MAX, 0x1234],
        };
        assert_eq!(Replay::parse(&replay.to_text()).unwrap(), replay);
    }
}
