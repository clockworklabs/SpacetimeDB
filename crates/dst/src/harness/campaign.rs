//! Long-running loops over many seeds.
//!
//! A campaign derives every run's seed from one campaign seed, so the whole
//! campaign is reproducible and can resume from any run index. Failures are
//! grouped by signature; the first few seeds of each group get replay files.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::assert::SometimesCount;
use super::choices::derive_seed;
use super::failure::Failure;
use super::runner::{run, RunOptions, RunReport};
use super::world::World;

/// Replay files kept per failure signature.
const REPLAYS_PER_SIGNATURE: usize = 3;

#[derive(Debug, Clone)]
pub struct CampaignOptions {
    pub campaign_seed: u64,
    /// Steps per run.
    pub steps: u64,
    /// Stop after this many runs.
    pub runs: Option<u64>,
    /// Stop after this much wall-clock time.
    pub duration: Option<Duration>,
    /// First run index, to resume a campaign.
    pub start: u64,
    /// Check determinism on the first run and on every n-th run after it. 0 disables.
    pub determinism_every: u64,
    pub stop_on_failure: bool,
    /// Replay files go to `<out_dir>/failures/`.
    pub out_dir: PathBuf,
}

impl CampaignOptions {
    /// Seed of the run at `index`.
    pub fn run_seed(&self, index: u64) -> u64 {
        derive_seed(self.campaign_seed, index)
    }
}

/// One group of failures with the same signature.
#[derive(Debug, Clone)]
pub struct FailureGroup {
    pub first: Failure,
    pub count: u64,
    pub replays: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct CampaignSummary {
    pub runs: u64,
    pub passed: u64,
    pub steps: u64,
    pub elapsed: Duration,
    pub next_index: u64,
    pub rule_counts: BTreeMap<&'static str, u64>,
    pub sometimes: BTreeMap<&'static str, SometimesCount>,
    pub failures: BTreeMap<String, FailureGroup>,
}

impl CampaignSummary {
    pub fn failed(&self) -> u64 {
        self.runs - self.passed
    }

    fn record_pass(&mut self, report: &RunReport) {
        self.passed += 1;
        self.steps += report.steps;
        for (rule, count) in &report.rule_counts {
            *self.rule_counts.entry(rule).or_default() += count;
        }
        for (name, count) in &report.sometimes {
            let total = self.sometimes.entry(name).or_default();
            total.evaluated += count.evaluated;
            total.held += count.held;
        }
    }
}

/// Something that happened during a campaign, for progress output.
pub enum CampaignEvent<'a> {
    Passed {
        index: u64,
        report: &'a RunReport,
    },
    Failed {
        index: u64,
        failure: &'a Failure,
        replay: Option<&'a PathBuf>,
    },
}

/// Run seeds of `W` until the run count or duration is reached.
pub fn run_campaign<W: World>(
    options: &CampaignOptions,
    mut on_event: impl FnMut(CampaignEvent<'_>, &CampaignSummary),
) -> CampaignSummary {
    // The loop below ends only through one of these bounds.
    assert!(
        options.runs.is_some() || options.duration.is_some(),
        "a campaign needs a run count or a duration"
    );
    let started = Instant::now();
    let mut summary = CampaignSummary::default();
    let failures_dir = options.out_dir.join("failures");

    let mut index = options.start;
    loop {
        if options.runs.is_some_and(|runs| summary.runs >= runs)
            || options.duration.is_some_and(|duration| started.elapsed() >= duration)
        {
            break;
        }

        let nth = index - options.start;
        let run_options = RunOptions {
            check_determinism: options.determinism_every > 0 && nth.is_multiple_of(options.determinism_every),
            ..RunOptions::new(options.run_seed(index), options.steps)
        };
        let result = run::<W>(&run_options);
        summary.runs += 1;
        summary.elapsed = started.elapsed();
        summary.next_index = index + 1;

        match result {
            Ok(report) => {
                summary.record_pass(&report);
                on_event(CampaignEvent::Passed { index, report: &report }, &summary);
            }
            Err(failure) => {
                let group = summary
                    .failures
                    .entry(failure.signature())
                    .or_insert_with(|| FailureGroup {
                        first: failure.clone(),
                        count: 0,
                        replays: Vec::new(),
                    });
                group.count += 1;
                let replay = (group.replays.len() < REPLAYS_PER_SIGNATURE)
                    .then(|| {
                        let name = format!("{}-{:016x}", W::NAME, failure.seed);
                        failure.replay().write(&failures_dir, &name).ok()
                    })
                    .flatten();
                if let Some(path) = &replay {
                    group.replays.push(path.clone());
                }
                on_event(
                    CampaignEvent::Failed {
                        index,
                        failure: &failure,
                        replay: replay.as_ref(),
                    },
                    &summary,
                );
                if options.stop_on_failure {
                    break;
                }
            }
        }

        index += 1;
    }

    summary.elapsed = started.elapsed();
    summary
}
