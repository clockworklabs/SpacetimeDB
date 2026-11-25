use std::time::Duration;

use spacetimedb_runtime::Handle;

use super::Choices;

/// One operation a test can perform.
///
/// Rules are plain data: the runner picks among the rules whose `when` guard
/// holds, by weight, and calls `run`. Rules are synchronous on purpose. They
/// cannot await, so they cannot hold a lock across a yield or stall the
/// driver; async work belongs in [`World::after_step`].
pub struct Rule<W: 'static> {
    pub name: &'static str,
    pub weight: u32,
    pub when: fn(&W) -> bool,
    pub run: fn(&mut W, &mut Choices) -> anyhow::Result<()>,
}

/// Guard for rules that are enabled in every state.
pub fn always_enabled<W>(_: &W) -> bool {
    true
}

/// Per-run context the runner passes to world hooks.
pub struct RunContext {
    /// Handle of the simulation runtime that drives this run.
    pub handle: Handle,
    /// Root seed of this run.
    pub seed: u64,
    /// Index of the current step.
    pub step: u64,
}

/// The system under test plus everything the test tracks about it.
#[allow(async_fn_in_trait)]
pub trait World: Sized + 'static {
    /// Name used in reports and replay files.
    const NAME: &'static str;

    /// Every rule this world supports.
    const RULES: &'static [Rule<Self>];

    /// Virtual-time budget for each `open` and `after_step` call.
    const STEP_TIMEOUT: Duration = Duration::from_secs(600);

    /// Build the system under test.
    async fn open(context: &RunContext, choices: &mut Choices) -> anyhow::Result<Self>;

    /// Runs between steps, whenever [`World::can_yield`] holds. The only place
    /// the harness awaits: yields, sleeps, and async work requested by rules.
    async fn after_step(&mut self, _context: &RunContext) -> anyhow::Result<()> {
        Ok(())
    }

    /// Invariants checked after every step.
    fn check(&mut self, _context: &RunContext) -> anyhow::Result<()> {
        Ok(())
    }

    /// False while the world holds a lock that production never holds across
    /// an await. The runner skips `after_step` until this is true again.
    fn can_yield(&self) -> bool {
        true
    }
}
