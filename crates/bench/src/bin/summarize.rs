//! Script to summarize benchmark results in a pretty markdown table / json file / push to prometheus.

use std::{
    collections::HashSet,
    fmt::Write as FmtWrite,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::{Parser, Subcommand};
use regex::{Captures, Regex};

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
    /// Use packed json files to generate a markdown report
    /// suitable for posting in a github PR.
    MarkdownReport {
        /// The name of the new baseline to compare.
        /// End with ".json" to load from a packed JSON file in `{target_dir}/criterion`.
        /// Otherwise, read from the loose criterion files in the filesystem.
        baseline_new: String,

        /// The name of the old baseline to compare against.
        /// End with ".json" to load from a packed JSON file in `{target_dir}/criterion`.
        /// Otherwise, read from the loose criterion files in the filesystem.
        baseline_old: Option<String>,

        /// Report will be written to this file. If not specified, will be written to stdout.
        #[arg(long = "report-name", required = false)]
        report_name: Option<String>,
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

    let crit_dir = target_dir.clone().join("criterion");
    assert!(
        crit_dir.exists(),
        "criterion directory {} inside target directory {} does not exist, \
        set a different target directory with --target-dir or the CARGO_TARGET_DIR env var",
        crit_dir.display(),
        target_dir.display()
    );

    let benchmarks = data::Benchmarks::gather(&crit_dir).expect("failed to read benchmarks");

    match args.command {
        Command::Pack {
            baseline: baseline_name,
        } => {
            assert!(
                !baseline_name.ends_with(".json"),
                "it's pointless to re-pack an already packed baseline..."
            );
            let baseline = benchmarks.by_baseline.get(&baseline_name).expect("baseline not found");

            let path = packed_baseline_json_path(&crit_dir, &baseline_name);
            let mut file = std::fs::File::create(&path).expect("failed to create file");
            serde_json::to_writer_pretty(&mut file, baseline).expect("failed to write json");
            println!("Wrote {}", path.display());
        }
        Command::MarkdownReport {
            baseline_old: baseline_old_name,
            baseline_new: baseline_new_name,
            report_name,
        } => {
            let old = baseline_old_name
                .map(|name| load_baseline(&benchmarks, &crit_dir, &name))
                .transpose()
                .expect("failed to load old baseline")
                .unwrap_or_else(|| data::BaseBenchmarks {
                    name: "n/a".to_string(),
                    benchmarks: Default::default(),
                });

            let new = load_baseline(&benchmarks, &crit_dir, &baseline_new_name).expect("failed to load new baseline");

            let report = generate_markdown_report(old, new).expect("failed to generate markdown report");

            if let Some(report_name) = report_name {
                let path = crit_dir.join(format!("{}.md", report_name));
                let mut file = std::fs::File::create(&path).expect("failed to create file");
                file.write_all(report.as_bytes()).expect("failed to write report");
                println!("Wrote {}", path.display());
            } else {
                println!("{}", report);
            }
        }
    }
}

/// `"{crit_dir}/"{name}.json"`
fn packed_baseline_json_path(crit_dir: &Path, name: &str) -> PathBuf {
    crit_dir.join(format!("{}.json", name))
}

/// If name ends with ".json", load from a packed json file. Otherwise, load from the benchmarks read from the filesystem.
fn load_baseline(benchmarks: &data::Benchmarks, crit_dir: &Path, name: &str) -> Result<data::BaseBenchmarks> {
    if name.ends_with(".json") {
        load_packed_baseline(crit_dir, name)
    } else {
        benchmarks
            .by_baseline
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("baseline {} not found", name))
    }
}

fn load_packed_baseline(crit_dir: &Path, name: &str) -> Result<data::BaseBenchmarks> {
    assert!(name.ends_with(".json"));
    let name = name.trim_end_matches(".json");
    let path = packed_baseline_json_path(crit_dir, name);
    let file = std::fs::File::open(path)?;
    let baseline = serde_json::from_reader(file)?;
    Ok(baseline)
}

fn generate_markdown_report(old: data::BaseBenchmarks, new: data::BaseBenchmarks) -> Result<String> {
    let mut result = String::new();

    writeln!(&mut result, "# Benchmark Report")?;
    writeln!(&mut result)?;

    writeln!(
        &mut result,
        "Legend:

- `load`: number of rows pre-loaded into the database
- `count`: number of rows touched by the transaction
- index types:
    - `unique`: a single index on the `id` column
    - `non_unique`: no indexes
    - `multi_index`: non-unique index on every column
- schemas:
    - `person(id: u32, name: String, age: u64)`
    - `location(id: u32, name: String, age: u64)`

All throughputs are single-threaded.

    "
    )?;

    let remaining = old
        .benchmarks
        .keys()
        .chain(new.benchmarks.keys())
        .collect::<HashSet<_>>();
    let mut remaining = remaining.into_iter().collect::<Vec<_>>();
    remaining.sort();

    writeln!(&mut result, "## Empty transaction")?;
    let table = extract_benchmarks_to_table(
        r"(?x) (?P<db>[^/]+) / (?P<on_disk>[^/]+) / empty",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    writeln!(&mut result, "## Single-row insertions")?;
    let table = extract_benchmarks_to_table(
        r"(?x) (?P<db>[^/]+) / (?P<on_disk>[^/]+) / insert_1 /
                (?P<schema>[^/]+) / (?P<index_type>[^/]+) /
                load = (?P<load>[^/]+)",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    writeln!(&mut result, "## Multi-row insertions")?;
    let table = extract_benchmarks_to_table(
        r"(?x) (?P<db>[^/]+) / (?P<on_disk>[^/]+) / insert_bulk /
                        (?P<schema>[^/]+) / (?P<index_type>[^/]+) /
                        load = (?P<load>[^/]+) / count = (?P<count>[^/]+)",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    writeln!(&mut result, "## Full table iterate")?;
    let table = extract_benchmarks_to_table(
        r"(?x) (?P<db>[^/]+) / (?P<on_disk>[^/]+) / iterate / 
                        (?P<schema>[^/]+) / (?P<index_type>[^/]+) ",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    writeln!(&mut result, "## Find unique key")?;
    let table = extract_benchmarks_to_table(
        r"(?x) (?P<db>[^/]+) / (?P<on_disk>[^/]+) / find_unique /
                (?P<key_type>[^/]+) /
                load = (?P<load>[^/]+) ",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    writeln!(&mut result, "## Filter")?;
    let table = extract_benchmarks_to_table(
        r"(?x) (?P<db>[^/]+) / (?P<on_disk>[^/]+) / filter /
                (?P<key_type>[^/]+) / (?P<index_strategy>[^/]+) / 
                load = (?P<load>[^/]+) / count = (?P<count>[^/]+)",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    writeln!(&mut result, "## Serialize")?;
    let table = extract_benchmarks_to_table(
        r"(?x) serialize / (?P<schema>[^/]+) / (?P<format>[^/]+) /
                count = (?P<count>[^/]+)",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    writeln!(&mut result, "## Module: invoke with large arguments")?;
    let table = extract_benchmarks_to_table(
        r"(?x) stdb_module / large_arguments / (?P<arg_size>[^/]+)",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    writeln!(&mut result, "## Module: print bulk")?;
    let table = extract_benchmarks_to_table(
        r"(?x) stdb_module / print_bulk / lines = (?P<line_count>[^/]+)",
        &old,
        &new,
        &mut remaining,
    )?;
    writeln!(&mut result, "{table}")?;

    // catch-all for remaining benchmarks
    writeln!(&mut result, "## Remaining benchmarks")?;
    let table = extract_benchmarks_to_table(r"(?x) (?P<name>.+)", &old, &new, &mut remaining)?;
    writeln!(&mut result, "{table}")?;

    assert_eq!(remaining.len(), 0);

    Ok(result)
}

/// A given benchmark group fits a pattern such as
/// `[db]/[disk]/insert_1/[schema]/[index_type]/load=[load]`
///
/// Pass a regex using named capture groups to extract all such benchmarks to a table.
/// We use insignificant whitespace to make these easier to read.
/// For example:
///
/// `r"(?x) (?P<db>[^/]+) / (?P<on_disk>[^/]+) / insert_1 / (?P<schema>[^/]+) / (?P<index_type>[^/]+) / load = (?P<load>[^/]+)"`
///
/// Some strings are treated specially:
/// - `on_disk -> 💿`, `mem -> 🧠`
fn extract_benchmarks_to_table(
    pattern: &str,
    old: &data::BaseBenchmarks,
    new: &data::BaseBenchmarks,
    remaining: &mut Vec<&String>,
) -> Result<String> {
    let regex = regex::Regex::new(pattern)?;

    let mut capture_names: Vec<_> = regex.capture_names().map(|name| name.unwrap_or("")).collect();
    capture_names.remove(0); // thi

    let mut headers = capture_names
        .clone()
        .iter()
        .map(|s| s.replace('_', " "))
        .collect::<Vec<_>>();
    headers.push("new latency".to_string());
    headers.push("old latency".to_string());
    headers.push("new throughput".to_string());
    headers.push("old throughput".to_string());

    let mut rows = Vec::new();

    let mut extracted = HashSet::new();

    for (i, bench_name) in remaining.iter().enumerate() {
        let captures = if let Some(captures) = regex.captures(bench_name) {
            extracted.insert(i);
            captures
        } else {
            continue;
        };

        let mut row = Vec::new();
        for capture in &capture_names {
            let cell = captures.name(capture).unwrap().as_str();

            row.push(emojify(cell));
        }

        if let Some(new) = new.benchmarks.get(&**bench_name) {
            row.push(time(new.nanoseconds(), new.stddev()))
        } else {
            row.push("-".to_string());
        }

        if let Some(old) = old.benchmarks.get(&**bench_name) {
            row.push(time(old.nanoseconds(), old.stddev()))
        } else {
            row.push("-".to_string());
        }

        if let Some(new) = new.benchmarks.get(&**bench_name) {
            if let Some(data::Throughput::Elements(throughput)) = new.throughput() {
                row.push(throughput_per(throughput, "tx"))
            } else {
                row.push("-".to_string());
            }
        } else {
            row.push("-".to_string());
        }

        if let Some(old) = old.benchmarks.get(&**bench_name) {
            if let Some(data::Throughput::Elements(throughput)) = old.throughput() {
                row.push(throughput_per(throughput, "tx"))
            } else {
                row.push("-".to_string());
            }
        } else {
            row.push("-".to_string());
        }

        rows.push(row)
    }

    rows.sort();

    *remaining = remaining
        .iter()
        .enumerate()
        .filter_map(|(i, s)| if extracted.contains(&i) { None } else { Some(*s) })
        .collect();

    Ok(format_markdown_table(headers, rows))
}

fn format_markdown_table(headers: Vec<String>, rows: Vec<Vec<String>>) -> String {
    for row in &rows {
        assert_eq!(row.len(), headers.len(), "internal error: mismatched row lengths");
    }

    let mut result = "\n".to_string();

    let mut max_widths = headers.iter().map(|s| s.len()).collect::<Vec<_>>();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            max_widths[i] = max_widths[i].max(cell.len());
        }
    }

    result.push_str("| ");
    for (i, header) in headers.iter().enumerate() {
        result.push_str(&format!("{:width$} | ", header, width = max_widths[i]));
    }
    result.push('\n');

    result.push('|');
    for max_width in &max_widths {
        result.push_str(&format!("-{:-<width$}-|", "", width = *max_width));
    }
    result.push('\n');

    for row in &rows {
        result.push_str("| ");
        for (i, cell) in row.iter().enumerate() {
            result.push_str(&format!("{:width$} | ", cell, width = max_widths[i]));
        }
        result.push('\n');
    }

    result
}

fn time(nanos: f64, stddev: f64) -> String {
    const MIN_MICRO: f64 = 2_000.0;
    const MIN_MILLI: f64 = 2_000_000.0;
    const MIN_SEC: f64 = 2_000_000_000.0;

    let (div, label) = if nanos < MIN_MICRO {
        (1.0, "ns")
    } else if nanos < MIN_MILLI {
        (1_000.0, "µs")
    } else if nanos < MIN_SEC {
        (1_000_000.0, "ms")
    } else {
        (1_000_000_000.0, "s")
    };
    format!("{:.1}±{:.2}{}", nanos / div, stddev / div, label)
}

fn throughput_per(per: f64, unit: &str) -> String {
    const MIN_K: f64 = (2 * (1 << 10) as u64) as f64;
    const MIN_M: f64 = (2 * (1 << 20) as u64) as f64;
    const MIN_G: f64 = (2 * (1 << 30) as u64) as f64;

    if per < MIN_K {
        format!("{} {}/sec", per as u64, unit)
    } else if per < MIN_M {
        format!("{:.1} K{}/sec", (per / (1 << 10) as f64), unit)
    } else if per < MIN_G {
        format!("{:.1} M{}/sec", (per / (1 << 20) as f64), unit)
    } else {
        format!("{:.1} G{}/sec", (per / (1 << 30) as f64), unit)
    }
}

lazy_static::lazy_static! {
    static ref EMOJI: Regex = Regex::new(r"(on_disk|mem)").unwrap();
}

fn emojify(text: &str) -> String {
    EMOJI
        .replace_all(text, |cap: &Captures| match &cap[0] {
            "on_disk" => "💿",
            "mem" => "🧠",
            _ => unimplemented!(),
        })
        .to_string()
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
