//! Script to pack benchmark results into JSON files.
//! These are read by the benchmarks-viewer application: https://github.com/clockworklabs/benchmarks-viewer,
//! which is used to generate reports on the benchmarks.
//! See also: the github actions scripts that invoke this command, `SpacetimeDB/.github/workflows/benchmarks.yml` and `SpacetimeDB/.github/workflows/callgrind_benchmarks.yml`.
use clap::{Parser, Subcommand};

/// Helper script to pack / summarize Criterion benchmark results.
#[derive(Parser)]
struct Cli {
    /// The path to the target directory where Criterion's benchmark data is stored.
    /// Uses the location of the `summarize` executable by default.
    #[arg(long = "target-dir")]
    target_dir: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Clone)]
enum Command {
    /// Pack a criterion baseline to a single json file.
    /// This is used to store baselines in CI.
    Pack {
        /// The name of the baseline to pack.
        #[arg(default_value = "base")]
        baseline: String,
    },
    /// Pack the most recent iai-callgrind data to a single json file.
    /// (iai-callgrind has no concept of a "baseline").
    /// This is used to store data in CI.
    PackCallgrind {
        /// The name of the output data file.
        /// Placed in `{target_dir}/iai/{name}.json`.
        name: String,
    },
}

fn main() {
    let args = Cli::parse();

    let target_dir = if let Some(target_dir) = args.target_dir {
        let target_dir = std::path::PathBuf::from(target_dir);
        assert!(
            target_dir.exists(),
            "target directory {} does not exist, set a different target directory with \
            --target-dir or the CARGO_TARGET_DIR env var",
            target_dir.display()
        );
        target_dir
    } else {
        let mut target_dir = std::env::current_exe().expect("no executable path?");
        target_dir.pop();
        target_dir.pop();
        target_dir
    };

    match args.command {
        Command::Pack {
            baseline: baseline_name,
        } => {
            criterion::pack(baseline_name, &target_dir);
        }
        Command::PackCallgrind { name } => callgrind::pack(name, &target_dir),
    }
}

mod criterion {
    use std::path::{Path, PathBuf};

    use regex::Regex;

    pub fn pack(baseline_name: String, target_dir: &Path) {
        assert!(
            !baseline_name.ends_with(".json"),
            "it's pointless to re-pack an already packed baseline..."
        );
        let crit_dir = get_crit_dir(target_dir);
        let benchmarks = data::Benchmarks::gather(&crit_dir).expect("failed to read benchmarks");

        let baseline = benchmarks.by_baseline.get(&baseline_name).expect("baseline not found");

        let path = packed_baseline_json_path(&crit_dir, &baseline_name);
        let mut file = std::fs::File::create(&path).expect("failed to create file");
        serde_json::to_writer_pretty(&mut file, baseline).expect("failed to write json");
        println!("Wrote {}", path.display());
    }

    fn get_crit_dir(target_dir: &Path) -> PathBuf {
        let crit_dir = target_dir.join("criterion");
        assert!(
            crit_dir.exists(),
            "criterion directory {} inside target directory {} does not exist, \
        set a different target directory with --target-dir or the CARGO_TARGET_DIR env var",
            crit_dir.display(),
            target_dir.display()
        );
        crit_dir
    }

    /// `"{crit_dir}/"{name}.json"`
    fn packed_baseline_json_path(crit_dir: &Path, name: &str) -> PathBuf {
        crit_dir.join(format!("{}.json", name))
    }

    lazy_static::lazy_static! {
        static ref EMOJI: Regex = Regex::new(r"(on_disk|mem)").unwrap();
    }

    /// Data types for deserializing stored Criterion benchmark results.
    ///
    /// Unfortunately, there is no published library for this, so we use the schema
    /// from `critcmp` under the MIT license:
    /// https://github.com/BurntSushi/critcmp/blob/daaf0383c3981c98a6eaaef47142755e5bddb3c4/src/data.rs
    ///
    /// TODO(jgilles): update this if we update our Criterion version past 0.4.
    #[allow(unused)]
    #[allow(clippy::all)]
    #[allow(rust_2018_idioms)]
    mod data {
        /*
        The MIT License (MIT)

        Copyright (c) 2015 Andrew Gallant

        Permission is hereby granted, free of charge, to any person obtaining a copy
        of this software and associated documentation files (the "Software"), to deal
        in the Software without restriction, including without limitation the rights
        to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
        copies of the Software, and to permit persons to whom the Software is
        furnished to do so, subject to the following conditions:

        The above copyright notice and this permission notice shall be included in
        all copies or substantial portions of the Software.

        THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
        IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
        FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
        AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
        LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
        OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
        THE SOFTWARE.
        */

        use std::collections::BTreeMap;
        use std::fs::File;
        use std::io;
        use std::path::Path;

        use serde::de::DeserializeOwned;
        use serde::{Deserialize, Serialize};
        use serde_json as json;
        use walkdir::WalkDir;

        // NOTE(jgilles): added this to make this compile
        use anyhow::{anyhow as err, bail as fail, Result};

        #[derive(Clone, Debug, Default)]
        pub struct Benchmarks {
            pub by_baseline: BTreeMap<String, BaseBenchmarks>,
        }

        #[derive(Clone, Debug, Deserialize, Serialize)]
        pub struct BaseBenchmarks {
            pub name: String,
            pub benchmarks: BTreeMap<String, Benchmark>,
        }

        #[derive(Clone, Debug, Deserialize, Serialize)]
        pub struct Benchmark {
            pub baseline: String,
            pub fullname: String,
            #[serde(rename = "criterion_benchmark_v1")]
            pub info: CBenchmark,
            #[serde(rename = "criterion_estimates_v1")]
            pub estimates: CEstimates,
        }

        #[derive(Clone, Debug, Deserialize, Serialize)]
        pub struct CBenchmark {
            pub group_id: String,
            pub function_id: Option<String>,
            pub value_str: Option<String>,
            pub throughput: Option<CThroughput>,
            pub full_id: String,
            pub directory_name: String,
        }

        #[derive(Clone, Debug, Deserialize, Serialize)]
        #[serde(rename_all = "PascalCase")]
        pub struct CThroughput {
            pub bytes: Option<u64>,
            pub elements: Option<u64>,
        }

        #[derive(Clone, Debug, Deserialize, Serialize)]
        pub struct CEstimates {
            pub mean: CStats,
            pub median: CStats,
            pub median_abs_dev: CStats,
            pub slope: Option<CStats>,
            pub std_dev: CStats,
        }

        #[derive(Clone, Debug, Deserialize, Serialize)]
        pub struct CStats {
            pub confidence_interval: CConfidenceInterval,
            pub point_estimate: f64,
            pub standard_error: f64,
        }

        #[derive(Clone, Debug, Deserialize, Serialize)]
        pub struct CConfidenceInterval {
            pub confidence_level: f64,
            pub lower_bound: f64,
            pub upper_bound: f64,
        }

        impl Benchmarks {
            pub fn gather<P: AsRef<Path>>(criterion_dir: P) -> Result<Benchmarks> {
                let mut benchmarks = Benchmarks::default();
                for result in WalkDir::new(criterion_dir) {
                    let dent = result?;
                    let b = match Benchmark::from_path(dent.path())? {
                        None => continue,
                        Some(b) => b,
                    };
                    benchmarks
                        .by_baseline
                        .entry(b.baseline.clone())
                        .or_insert_with(|| BaseBenchmarks {
                            name: b.baseline.clone(),
                            benchmarks: BTreeMap::new(),
                        })
                        .benchmarks
                        .insert(b.benchmark_name().to_string(), b);
                }
                Ok(benchmarks)
            }
        }

        impl Benchmark {
            fn from_path<P: AsRef<Path>>(path: P) -> Result<Option<Benchmark>> {
                let path = path.as_ref();
                Benchmark::from_path_imp(path).map_err(|err| {
                    if let Some(parent) = path.parent() {
                        err!("{}: {}", parent.display(), err)
                    } else {
                        err!("unknown path: {}", err)
                    }
                })
            }

            fn from_path_imp(path: &Path) -> Result<Option<Benchmark>> {
                match path.file_name() {
                    None => return Ok(None),
                    Some(filename) => {
                        if filename != "estimates.json" {
                            return Ok(None);
                        }
                    }
                }
                // Criterion's directory structure looks like this:
                //
                //     criterion/{group}/{name}/{baseline}/estimates.json
                //
                // In the same directory as `estimates.json`, there is also a
                // `benchmark.json` which contains most of the info we need about
                // a benchmark, including its name. From the path, we only extract the
                // baseline name.
                let parent = path
                    .parent()
                    .ok_or_else(|| err!("{}: could not find parent dir", path.display()))?;
                let baseline = parent
                    .file_name()
                    .map(|p| p.to_string_lossy().into_owned())
                    .ok_or_else(|| err!("{}: could not find baseline name", path.display()))?;
                if baseline == "change" {
                    // This isn't really a baseline, but special state emitted by
                    // Criterion to reflect its own comparison between baselines. We
                    // don't use it.
                    return Ok(None);
                }

                let info = CBenchmark::from_path(parent.join("benchmark.json"))?;
                let estimates = CEstimates::from_path(path)?;
                let fullname = format!("{}/{}", baseline, info.full_id);
                Ok(Some(Benchmark {
                    baseline,
                    fullname,
                    info,
                    estimates,
                }))
            }

            pub fn nanoseconds(&self) -> f64 {
                self.estimates.mean.point_estimate
            }

            pub fn stddev(&self) -> f64 {
                self.estimates.std_dev.point_estimate
            }

            pub fn fullname(&self) -> &str {
                &self.fullname
            }

            pub fn baseline(&self) -> &str {
                &self.baseline
            }

            pub fn benchmark_name(&self) -> &str {
                &self.info.full_id
            }

            pub fn throughput(&self) -> Option<Throughput> {
                const NANOS_PER_SECOND: f64 = 1_000_000_000.0;

                let scale = NANOS_PER_SECOND / self.nanoseconds();

                self.info.throughput.as_ref().and_then(|t| {
                    if let Some(num) = t.bytes {
                        Some(Throughput::Bytes(num as f64 * scale))
                    } else if let Some(num) = t.elements {
                        Some(Throughput::Elements(num as f64 * scale))
                    } else {
                        None
                    }
                })
            }
        }

        #[derive(Clone, Copy, Debug)]
        pub enum Throughput {
            Bytes(f64),
            Elements(f64),
        }

        impl BaseBenchmarks {
            pub fn from_path<P: AsRef<Path>>(path: P) -> Result<BaseBenchmarks> {
                deserialize_json_path(path.as_ref())
            }
        }

        impl CBenchmark {
            fn from_path<P: AsRef<Path>>(path: P) -> Result<CBenchmark> {
                deserialize_json_path(path.as_ref())
            }
        }

        impl CEstimates {
            fn from_path<P: AsRef<Path>>(path: P) -> Result<CEstimates> {
                deserialize_json_path(path.as_ref())
            }
        }

        fn deserialize_json_path<D: DeserializeOwned>(path: &Path) -> Result<D> {
            let file = File::open(path).map_err(|err| {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    err!("{}: {}", name, err)
                } else {
                    err!("{}: {}", path.display(), err)
                }
            })?;
            let buf = io::BufReader::new(file);
            let b = json::from_reader(buf).map_err(|err| {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    err!("{}: {}", name, err)
                } else {
                    err!("{}: {}", path.display(), err)
                }
            })?;
            Ok(b)
        }
    }
}

mod callgrind {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use serde::{Deserialize, Serialize};

    pub fn pack(name: String, target_dir: &Path) {
        let iai_callgrind_dir = get_iai_callgrind_dir(target_dir);

        let benchmarks = gather_benchmarks(&iai_callgrind_dir);

        let path = packed_json_path(&iai_callgrind_dir, &name);
        let mut file = std::fs::File::create(&path).expect("failed to create file");
        serde_json::to_writer_pretty(&mut file, &benchmarks).expect("failed to write json");
        println!("Wrote {}", path.display());
    }

    fn packed_json_path(iai_callgrind_dir: &Path, name: &str) -> PathBuf {
        iai_callgrind_dir.join(format!("{}.json", name))
    }

    fn get_iai_callgrind_dir(target_dir: &Path) -> PathBuf {
        let iai_callgrind_dir = target_dir.join("iai");
        assert!(
            iai_callgrind_dir.exists(),
            "iai-callgrind directory {} inside target directory {} does not exist",
            iai_callgrind_dir.display(),
            target_dir.display()
        );
        iai_callgrind_dir
    }

    fn gather_benchmarks(iai_callgrind_dir: &Path) -> Vec<Benchmark> {
        let root = iai_callgrind_dir.join("spacetimedb-bench").join("callgrind");
        let summary_name: std::ffi::OsString = "summary.json".into();

        let mut results = vec![];

        for entry in walkdir::WalkDir::new(root) {
            let entry = entry.unwrap();
            if entry.path().file_name() != Some(&summary_name) {
                continue;
            }
            match extract_benchmark_data(entry.path()) {
                Ok(benchmark) => results.push(benchmark),
                Err(e) => eprintln!(
                    "Failed to extract benchmark data from {}: {}",
                    entry.path().display(),
                    e
                ),
            }
        }

        results
    }

    fn extract_benchmark_data(path: &Path) -> Result<Benchmark> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let contents: serde_json::Map<String, serde_json::Value> = serde_json::from_reader(&mut reader)?;

        #[derive(Debug)]
        struct Err;
        impl std::fmt::Display for Err {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unexpected summary layout")
            }
        }
        impl std::error::Error for Err {}

        // get the measurements field out of the summary file
        let measurements = contents
            .get("callgrind_summary")
            .ok_or(Err)?
            .as_object()
            .ok_or(Err)?
            .get("summaries")
            .ok_or(Err)?
            .as_array()
            .ok_or(Err)?
            .get(0)
            .ok_or(Err)?
            .as_object()
            .ok_or(Err)?
            .get("events")
            .ok_or(Err)?
            .as_object()
            .ok_or(Err)?;
        let measurements = itertools::process_results(
            measurements
                .iter()
                .map(|(k, v)| -> Result<(String, serde_json::Value)> {
                    Ok((
                        relabel_callgrind_metric(k).to_string(),
                        v.as_object().ok_or(Err)?.get("new").ok_or(Err)?.clone(),
                    ))
                }),
            |iter| iter.collect(),
        )?;

        let details = contents.get("details").unwrap().as_str().unwrap();
        let basic_format_correct = details.starts_with("r#\"{") && details.ends_with("}\"#");
        assert!(
            basic_format_correct,
            "`details` field of of {} should be a json blob wrapped in a raw Rust string",
            path.display()
        );

        let details = details.trim_start_matches("r#\"").trim_end_matches("\"#");
        let details = serde_json::from_str(details).unwrap();

        Ok(Benchmark {
            metadata: details,
            measurements,
        })
    }

    fn relabel_callgrind_metric(name: &str) -> &'static str {
        match name {
            "Ir" => "instructions",
            "Dr" => "data reads",
            "Dw" => "data writes",
            "I1mr" => "L1 instruction read misses",
            "D1mr" => "L1 data read misses",
            "D1mw" => "L1 data write misses",
            "ILmr" => "LL instruction read misses",
            "DLmr" => "LL data read misses",
            "DLmw" => "LL data write misses",
            "L1hits" => "L1 cache hits",
            "LLhits" => "LL cache hits",
            "RamHits" => "RAM hits",
            "TotalRW" => "total reads + writes",
            "EstimatedCycles" => "estimated cycles",
            "Bc" => "conditional branches",
            "Bi" => "indirect branches",
            _ => unimplemented!("unknown callgrind metric {}", name),
        }
    }

    #[derive(Serialize, Deserialize)]
    struct Benchmark {
        metadata: serde_json::Map<String, serde_json::Value>,
        measurements: serde_json::Map<String, serde_json::Value>,
    }
}
