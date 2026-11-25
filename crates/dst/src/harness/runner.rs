use std::collections::{BTreeMap, VecDeque};
use std::panic::{self, AssertUnwindSafe};
use std::thread;

use spacetimedb_runtime::sim::{self, DeterminismLog};
use spacetimedb_runtime::{sim_std, Handle};

use super::assert::{take_sometimes, SometimesCount};
use super::choices::{derive_seed, Choices};
use super::failure::{Failure, FailureKind, Replay};
use super::world::{RunContext, World};

const CHOICES_SEED_PURPOSE: u64 = 0;
const RUNTIME_SEED_PURPOSE: u64 = 1;

/// What to run.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Root seed. Workload and runtime seeds are both derived from it.
    pub seed: u64,
    /// Number of steps to run.
    pub steps: u64,
    /// Replay these recorded values instead of drawing fresh ones.
    pub replay: Option<Vec<u64>>,
    /// Run twice and fail if the runs diverge.
    pub check_determinism: bool,
}

impl RunOptions {
    pub fn new(seed: u64, steps: u64) -> Self {
        Self {
            seed,
            steps,
            replay: None,
            check_determinism: false,
        }
    }

    pub fn from_replay(replay: &Replay) -> Self {
        Self {
            seed: replay.seed,
            steps: replay.steps,
            replay: Some(replay.values.clone()),
            check_determinism: false,
        }
    }
}

/// Outcome of a passing run.
#[derive(Debug, Clone)]
pub struct RunReport {
    pub world: &'static str,
    pub seed: u64,
    pub steps: u64,
    /// How often each rule ran, in `World::RULES` order.
    pub rule_counts: Vec<(&'static str, u64)>,
    pub sometimes: BTreeMap<&'static str, SometimesCount>,
    pub recorded: Vec<u64>,
}

/// Run one seed of `W`.
pub fn run<W: World>(options: &RunOptions) -> Result<RunReport, Failure> {
    if !options.check_determinism {
        return run_once::<W>(options, Determinism::Off).0;
    }

    // Each pass runs on a fresh thread so thread-local state is not shared.
    let (first, log) = {
        let options = options.clone();
        thread::spawn(move || run_once::<W>(&options, Determinism::Record))
            .join()
            .expect("`run_once` catches panics from the run itself")
    };
    let first = first?;
    let log = log.expect("a recording run always returns its log");

    let (second, _) = {
        let options = options.clone();
        thread::spawn(move || run_once::<W>(&options, Determinism::Check(log)))
            .join()
            .expect("`run_once` catches panics from the run itself")
    };
    let second = second.map_err(|mut failure| {
        failure.kind = FailureKind::Nondeterminism;
        failure.message = format!("second run of the same seed failed: {}", failure.message);
        failure
    })?;

    if second.rule_counts != first.rule_counts || second.recorded != first.recorded {
        return Err(Failure {
            world: W::NAME,
            seed: options.seed,
            kind: FailureKind::Nondeterminism,
            assertion: None,
            step: None,
            rule: None,
            message: "second run of the same seed made different choices".to_owned(),
            recent_rules: Vec::new(),
            recent: Vec::new(),
            recorded: first.recorded,
        });
    }

    Ok(first)
}

enum Determinism {
    Off,
    Record,
    Check(DeterminismLog),
}

/// Where the run currently is, kept outside the driver so panics can report it.
#[derive(Default)]
struct Position {
    step: Option<u64>,
    rule: Option<&'static str>,
    /// Rules of the most recent steps, oldest first.
    recent_rules: VecDeque<(u64, &'static str)>,
}

/// How many recent steps a failure report lists.
const RECENT_RULES_MAX: usize = 16;

enum DriveError {
    Timeout(&'static str),
    NoRuleEnabled,
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for DriveError {
    fn from(err: anyhow::Error) -> Self {
        Self::Failed(err)
    }
}

fn run_once<W: World>(
    options: &RunOptions,
    determinism: Determinism,
) -> (Result<RunReport, Failure>, Option<DeterminismLog>) {
    let mut runtime = sim::Runtime::new(derive_seed(options.seed, RUNTIME_SEED_PURPOSE));
    let checking = matches!(determinism, Determinism::Check(_));
    match determinism {
        Determinism::Off => {}
        Determinism::Record => runtime.enable_determinism_log(),
        Determinism::Check(log) => runtime.enable_determinism_check(log),
    }
    let handle = Handle::simulation(runtime.handle());
    let mut choices = match &options.replay {
        Some(values) => Choices::replay(values.clone()),
        None => Choices::seeded(derive_seed(options.seed, CHOICES_SEED_PURPOSE)),
    };
    let mut position = Position::default();
    let mut rule_counts = vec![0u64; W::RULES.len()];
    take_sometimes();

    // Isolate the run so a panic inside it becomes a reportable failure.
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        sim_std::block_on(
            &mut runtime,
            drive::<W>(handle, options, &mut choices, &mut position, &mut rule_counts),
        )
    }));

    let failure = |kind, assertion, message, choices: &Choices| Failure {
        world: W::NAME,
        seed: options.seed,
        kind,
        assertion,
        step: position.step,
        rule: position.rule,
        message,
        recent_rules: position.recent_rules.iter().copied().collect(),
        recent: choices.recent().cloned().collect(),
        recorded: choices.recorded().to_vec(),
    };

    let mut result = match outcome {
        Ok(Ok(())) => Ok(RunReport {
            world: W::NAME,
            seed: options.seed,
            steps: options.steps,
            rule_counts: W::RULES.iter().map(|rule| rule.name).zip(rule_counts).collect(),
            sometimes: take_sometimes(),
            recorded: choices.recorded().to_vec(),
        }),
        Ok(Err(DriveError::Timeout(hook))) => Err(failure(
            FailureKind::Timeout,
            None,
            format!("{hook} exceeded {:?} of virtual time", W::STEP_TIMEOUT),
            &choices,
        )),
        Ok(Err(DriveError::NoRuleEnabled)) => Err(failure(
            FailureKind::NoRuleEnabled,
            None,
            "no rule's guard holds".to_owned(),
            &choices,
        )),
        Ok(Err(DriveError::Failed(err))) => {
            let (kind, assertion, message) = Failure::classify(&err);
            Err(failure(kind, assertion, message, &choices))
        }
        Err(payload) => Err(failure(FailureKind::Panic, None, panic_message(&*payload), &choices)),
    };

    if checking
        && result.is_ok()
        && let Err(message) = runtime.finish_determinism_check()
    {
        result = Err(failure(FailureKind::Nondeterminism, None, message, &choices));
    }
    let log = runtime.take_determinism_log();
    (result, log)
}

async fn drive<W: World>(
    handle: Handle,
    options: &RunOptions,
    choices: &mut Choices,
    position: &mut Position,
    rule_counts: &mut [u64],
) -> Result<(), DriveError> {
    let mut context = RunContext {
        handle: handle.clone(),
        seed: options.seed,
        step: 0,
    };

    let mut world = handle
        .timeout(W::STEP_TIMEOUT, W::open(&context, choices))
        .await
        .map_err(|_| DriveError::Timeout("open"))??;

    let mut enabled: Vec<(u32, usize)> = Vec::with_capacity(W::RULES.len());
    for step in 0..options.steps {
        position.step = Some(step);
        position.rule = None;
        context.step = step;
        choices.set_step(step);

        enabled.clear();
        enabled.extend(
            W::RULES
                .iter()
                .enumerate()
                .filter(|(_, rule)| rule.weight > 0 && (rule.when)(&world))
                .map(|(index, rule)| (rule.weight, index)),
        );
        if enabled.is_empty() {
            return Err(DriveError::NoRuleEnabled);
        }

        let index = choices.weighted("rule", &enabled);
        let rule = &W::RULES[index];
        position.rule = Some(rule.name);
        if position.recent_rules.len() == RECENT_RULES_MAX {
            position.recent_rules.pop_front();
        }
        position.recent_rules.push_back((step, rule.name));
        rule_counts[index] += 1;

        (rule.run)(&mut world, choices)?;

        if world.can_yield() {
            handle
                .timeout(W::STEP_TIMEOUT, world.after_step(&context))
                .await
                .map_err(|_| DriveError::Timeout("after_step"))??;
        }
        world.check(&context)?;
    }

    Ok(())
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        format!("panic: {message}")
    } else if let Some(message) = payload.downcast_ref::<String>() {
        format!("panic: {message}")
    } else {
        "panic with non-string payload".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::harness::assert::assert_always;
    use crate::harness::world::{always_enabled, Rule};

    /// A counter whose rules exercise the runner, not a real system.
    struct Counter {
        value: u64,
        locked: bool,
    }

    fn increment(world: &mut Counter, choices: &mut Choices) -> anyhow::Result<()> {
        world.value += choices.below("amount", 3);
        assert_always(world.value < 1_000_000, "counter stays small", String::new)
    }

    fn lock(world: &mut Counter, _: &mut Choices) -> anyhow::Result<()> {
        world.locked = true;
        Ok(())
    }

    fn unlock(world: &mut Counter, _: &mut Choices) -> anyhow::Result<()> {
        world.locked = false;
        Ok(())
    }

    impl World for Counter {
        const NAME: &'static str = "counter";
        const RULES: &'static [Rule<Self>] = &[
            Rule {
                name: "increment",
                weight: 3,
                when: always_enabled,
                run: increment,
            },
            Rule {
                name: "lock",
                weight: 1,
                when: |world| !world.locked,
                run: lock,
            },
            Rule {
                name: "unlock",
                weight: 1,
                when: |world| world.locked,
                run: unlock,
            },
        ];

        async fn open(_: &RunContext, choices: &mut Choices) -> anyhow::Result<Self> {
            Ok(Self {
                value: choices.below("start", 10),
                locked: false,
            })
        }

        async fn after_step(&mut self, context: &RunContext) -> anyhow::Result<()> {
            assert!(!self.locked, "after_step ran while locked");
            context.handle.sleep(Duration::from_millis(1)).await;
            Ok(())
        }

        fn can_yield(&self) -> bool {
            !self.locked
        }
    }

    #[test]
    fn same_seed_same_run_and_never_yields_while_locked() {
        let first = run::<Counter>(&RunOptions::new(9, 500)).unwrap();
        let second = run::<Counter>(&RunOptions::new(9, 500)).unwrap();
        assert_eq!(first.recorded, second.recorded);
        assert_eq!(first.rule_counts, second.rule_counts);
        assert!(first.rule_counts.iter().all(|(_, count)| *count > 0));
    }

    #[test]
    fn determinism_check_passes_for_deterministic_world() {
        let options = RunOptions {
            check_determinism: true,
            ..RunOptions::new(3, 200)
        };
        run::<Counter>(&options).unwrap();
    }

    struct Failing;

    fn draw_and_check(_: &mut Failing, choices: &mut Choices) -> anyhow::Result<()> {
        let value = choices.below("value", 100);
        assert_always(value != 7, "value is never seven", || format!("drew {value}"))
    }

    impl World for Failing {
        const NAME: &'static str = "failing";
        const RULES: &'static [Rule<Self>] = &[Rule {
            name: "draw",
            weight: 1,
            when: always_enabled,
            run: draw_and_check,
        }];

        async fn open(_: &RunContext, _: &mut Choices) -> anyhow::Result<Self> {
            Ok(Self)
        }
    }

    #[test]
    fn failure_reports_assertion_and_replays() {
        let failure = run::<Failing>(&RunOptions::new(1, 10_000)).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Assertion);
        assert_eq!(failure.assertion, Some("value is never seven"));
        assert_eq!(failure.rule, Some("draw"));

        let replay = Replay::parse(&failure.replay().to_text()).unwrap();
        let again = run::<Failing>(&RunOptions::from_replay(&replay)).unwrap_err();
        assert_eq!(again.step, failure.step);
        assert_eq!(again.signature(), failure.signature());
    }

    struct Panicking;

    impl World for Panicking {
        const NAME: &'static str = "panicking";
        const RULES: &'static [Rule<Self>] = &[Rule {
            name: "panic",
            weight: 1,
            when: always_enabled,
            run: |_, _| panic!("kaboom"),
        }];

        async fn open(_: &RunContext, _: &mut Choices) -> anyhow::Result<Self> {
            Ok(Self)
        }
    }

    #[test]
    fn panics_become_failures_with_position() {
        let failure = run::<Panicking>(&RunOptions::new(1, 5)).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Panic);
        assert_eq!(failure.step, Some(0));
        assert_eq!(failure.rule, Some("panic"));
        assert!(failure.message.contains("kaboom"));
    }

    struct Stalling;

    impl World for Stalling {
        const NAME: &'static str = "stalling";
        const RULES: &'static [Rule<Self>] = &[Rule {
            name: "noop",
            weight: 1,
            when: always_enabled,
            run: |_, _| Ok(()),
        }];
        const STEP_TIMEOUT: Duration = Duration::from_secs(1);

        async fn open(_: &RunContext, _: &mut Choices) -> anyhow::Result<Self> {
            Ok(Self)
        }

        async fn after_step(&mut self, context: &RunContext) -> anyhow::Result<()> {
            context.handle.sleep(Duration::from_secs(3600)).await;
            Ok(())
        }
    }

    #[test]
    fn stalled_after_step_times_out_in_virtual_time() {
        let failure = run::<Stalling>(&RunOptions::new(1, 5)).unwrap_err();
        assert_eq!(failure.kind, FailureKind::Timeout);
        assert_eq!(failure.step, Some(0));
    }
}
