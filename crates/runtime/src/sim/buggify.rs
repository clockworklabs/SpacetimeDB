use crate::sim::Runtime;

/// Probabilistic fault-injection helpers for simulation code.
///
/// Reference: <https://transactional.blog/simulation/buggify>.
///
/// Buggify is tied to a specific simulation runtime. Callers toggle it on that
/// runtime, then ask whether a fault should be injected at a particular point.
pub fn enable(runtime: &Runtime) {
    runtime.enable_buggify();
}

/// Disable probabilistic fault injection for the given simulation runtime.
pub fn disable(runtime: &Runtime) {
    runtime.disable_buggify();
}

/// Returns whether buggify is enabled for the given simulation runtime.
pub fn is_enabled(runtime: &Runtime) -> bool {
    runtime.is_buggify_enabled()
}

/// Returns whether the runtime should inject a fault at this point using the
/// default deterministic probability.
pub fn should_inject_fault(runtime: &Runtime) -> bool {
    runtime.buggify()
}

/// Returns whether the runtime should inject a fault at this point using the
/// provided deterministic probability.
pub fn should_inject_fault_with_prob(runtime: &Runtime, probability: f64) -> bool {
    runtime.buggify_with_prob(probability)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_owned_buggify_controls_fault_injection() {
        let runtime = Runtime::new(7);

        assert!(!is_enabled(&runtime));
        enable(&runtime);
        assert!(is_enabled(&runtime));
        assert!(should_inject_fault_with_prob(&runtime, 1.0));
        disable(&runtime);
        assert!(!is_enabled(&runtime));
        assert!(!should_inject_fault_with_prob(&runtime, 1.0));
    }
}
