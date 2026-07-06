#![allow(clippy::disallowed_macros)]

use std::time::Duration;

use anyhow::{bail, Context, Result};
use spacetimedb_sats::product;
use spacetimedb_testing::modules::{start_runtime, CompilationMode, CompiledModule, IN_MEMORY_CONFIG};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

const WARMUP_RUNS: usize = 5;
const MEASURED_RUNS: usize = 31;
const MEDIAN_THRESHOLD: Duration = Duration::from_micros(100);

const REDUCERS: &[&str] = &[
    "test_index_scan_on_id",
    "test_index_scan_on_chunk",
    "test_index_scan_on_x_z_dimension",
    "test_index_scan_on_x_z",
];

fn main() -> Result<()> {
    let module = CompiledModule::compile("perf-test", CompilationMode::Release);
    let runtime = start_runtime();

    runtime.block_on(async {
        let module = module.load_module(IN_MEMORY_CONFIG, None).await;
        let no_args = product![];

        println!("loading perf-test location table...");
        module
            .call_reducer_binary("load_location_table", &no_args)
            .await
            .context("failed to load perf-test location table")?;

        let mut failures = Vec::new();
        for &reducer in REDUCERS {
            for _ in 0..WARMUP_RUNS {
                module
                    .call_reducer_binary_result(reducer, &no_args)
                    .await
                    .with_context(|| format!("failed during warmup for {reducer}"))?;
            }

            let mut samples = Vec::with_capacity(MEASURED_RUNS);
            for _ in 0..MEASURED_RUNS {
                let result = module
                    .call_reducer_binary_result(reducer, &no_args)
                    .await
                    .with_context(|| format!("failed during measured run for {reducer}"))?;
                samples.push(result.execution_duration);
            }

            samples.sort_unstable();
            let median = samples[samples.len() / 2];

            println!("{reducer:<36} median={median:?}");
            if median >= MEDIAN_THRESHOLD {
                failures.push(format!("{reducer} median {median:?}"));
            }
        }

        if !failures.is_empty() {
            bail!(
                "index scan benchmark failed; median threshold is {:?}; failures: {}",
                MEDIAN_THRESHOLD,
                failures.join(", ")
            );
        }

        println!(
            "index scan benchmark passed; all medians are below {:?}",
            MEDIAN_THRESHOLD
        );
        Ok(())
    })
}
