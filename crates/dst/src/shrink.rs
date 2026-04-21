//! Generic shrinking helpers for deterministic targets.

/// Generic remove-and-replay shrink loop.
pub fn shrink_by_removing<C, F>(
    case: &C,
    target_failure: &F,
    truncate: impl Fn(&C) -> C,
    len: impl Fn(&C) -> usize,
    remove: impl Fn(&C, usize) -> Option<C>,
    replay_failure: impl Fn(&C) -> anyhow::Result<F>,
    same_failure: impl Fn(&F, &F) -> bool,
) -> anyhow::Result<C>
where
    C: Clone,
{
    let mut shrunk = truncate(case);

    let mut changed = true;
    while changed {
        changed = false;
        for idx in (0..len(&shrunk)).rev() {
            let Some(candidate) = remove(&shrunk, idx) else {
                continue;
            };
            let Ok(candidate_failure) = replay_failure(&candidate) else {
                continue;
            };
            if same_failure(target_failure, &candidate_failure) {
                shrunk = candidate;
                changed = true;
            }
        }
    }

    Ok(shrunk)
}
