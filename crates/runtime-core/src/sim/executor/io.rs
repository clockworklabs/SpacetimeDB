use crate::sim::{io::SimulatorIO, Rng};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// The max number of submissions to run per [Driver::tick].
    pub max_submissions_per_tick: usize,
    /// The max number of completions to finish per [Driver::tick].
    pub max_completions_per_tick: usize,
    /// Submission reordering probability.
    ///
    /// Describes the probability by which to select the next submission queue
    /// entry randomly, as opposed to the oldest entry in the queue.
    pub prob_reorder_submissions: f64,
    /// Completion reordering probability.
    ///
    /// Describes the probability by which to select the next completion queue
    /// entry randomly, as opposed to the oldest entry in the queue.
    pub prob_reorder_completions: f64,
    /// Probability by which to skip one submission queue entry.
    ///
    /// If skipped, the entry still counts towards `max_submissions_per_tick`.
    pub prob_skip: f64,
    /// Probability by which to cancel a submission queue entry.
    ///
    /// [crate::sim::io::op::Submission::cancel()] is called on the entry, which
    /// may generate a completion.
    pub prob_cancel: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_submissions_per_tick: 1,
            max_completions_per_tick: 1,
            prob_reorder_submissions: 0.0,
            prob_reorder_completions: 0.0,
            prob_skip: 0.0,
            prob_cancel: 0.0,
        }
    }
}

pub struct Driver {
    io: SimulatorIO,
    config: Config,
}

impl Driver {
    pub fn new(config: Config) -> Self {
        Self {
            io: <_>::default(),
            config,
        }
    }

    /// Advance the I/O simulator according the [Config].
    ///
    /// Returns `true` if progress has been made, or there are pending entries
    /// in either the submission or completion queue.
    pub fn tick(&self, rng: &Rng) -> bool {
        let mut progress = false;
        for _ in 0..self.config.max_submissions_per_tick {
            if !rng.buggify_with_prob(self.config.prob_skip) {
                let sqe = if rng.buggify_with_prob(self.config.prob_reorder_submissions) {
                    self.io.random_submission(rng)
                } else {
                    self.io.next_submission()
                };

                if let Some(sqe) = sqe {
                    if rng.buggify_with_prob(self.config.prob_cancel) {
                        sqe.cancel();
                    } else {
                        self.io.execute(sqe);
                    }
                    progress = true;
                }
            }
        }

        for _ in 0..self.config.max_completions_per_tick {
            let cqe = if rng.buggify_with_prob(self.config.prob_reorder_completions) {
                self.io.random_completion(rng)
            } else {
                self.io.next_completion()
            };

            if let Some(cqe) = cqe {
                cqe.complete();
                progress = true
            }
        }

        progress |= self.io.pending();
        progress
    }

    pub fn io(&self) -> &SimulatorIO {
        &self.io
    }
}
