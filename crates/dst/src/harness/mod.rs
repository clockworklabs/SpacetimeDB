//! Target-agnostic DST framework.
//!
//! A test is a [`World`] (the system under test plus what the test tracks
//! about it) and a list of [`Rule`]s, one per operation. The runner picks
//! enabled rules by weight, records every random choice in [`Choices`], and
//! turns errors, panics and timeouts into a [`Failure`] that can be replayed.

mod assert;
mod campaign;
mod choices;
mod failure;
mod runner;
mod world;

pub use self::assert::{assert_always, assert_sometimes, assertion_failed, AssertionFailed, SometimesCount};
pub use self::campaign::{run_campaign, CampaignEvent, CampaignOptions, CampaignSummary, FailureGroup};
pub use self::choices::{derive_seed, Choices, Draw};
pub use self::failure::{parse_u64, Failure, FailureKind, Replay};
pub use self::runner::{run, RunOptions, RunReport};
pub use self::world::{always_enabled, Rule, RunContext, World};
