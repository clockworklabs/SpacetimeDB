use super::io::load_run;
use crate::context::constants::ALL_MODES;
use crate::llm::model_routes::default_model_routes;
use crate::results::BenchmarkRun;
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

pub fn cmd_llm_benchmark_diff(base_file: &str, head_file: &str) -> Result<String> {
    let base: BenchmarkRun = load_run(base_file).unwrap_or_default();
    let head: BenchmarkRun = load_run(head_file).unwrap_or_default();

    let mut out = String::new();
    out.push_str("# LLM Benchmark Diff\n\n");

    let canonical: Vec<String> = default_model_routes()
        .iter()
        .map(|r| r.display_name.to_string())
        .collect();

    for mode in ALL_MODES {
        out.push_str(&format!("## Mode: `{mode}`\n\n"));

        for lang in ["rust", "csharp"] {
            let b = base.modes.iter().find(|m| m.mode == mode.to_string() && m.lang == lang);
            let h = head.modes.iter().find(|m| m.mode == mode.to_string() && m.lang == lang);

            out.push_str(&format!("### Lang: `{lang}`\n"));

            let (bhash, hhash) = (
                b.map(|x| x.hash.as_str()).unwrap_or("—"),
                h.map(|x| x.hash.as_str()).unwrap_or("—"),
            );
            if bhash != hhash {
                out.push_str(&format!(
                    "Docs hash changed:\n- base: `{}`\n- head: `{}`\n\n",
                    bhash, hhash
                ));
            }

            let mut bm: BTreeMap<String, f32> = BTreeMap::new();
            let mut hm: BTreeMap<String, f32> = BTreeMap::new();
            let mut seen: BTreeSet<String> = BTreeSet::new();

            if let Some(b) = b {
                for s in &b.models {
                    bm.insert(s.name.clone(), s.score.unwrap_or(f32::NAN));
                    seen.insert(s.name.clone());
                }
            }
            if let Some(h) = h {
                for s in &h.models {
                    hm.insert(s.name.clone(), s.score.unwrap_or(f32::NAN));
                    seen.insert(s.name.clone());
                }
            }

            let mut ordered: Vec<String> = Vec::new();
            for name in &canonical {
                if seen.contains(name.as_str()) {
                    ordered.push(name.clone());
                }
            }
            for extra in seen.iter() {
                if !ordered.iter().any(|n| n == extra) {
                    ordered.push(extra.clone());
                }
            }

            out.push_str("| Model | Base | Head | Δ |\n|---|---:|---:|---:|\n");
            for name in ordered {
                let bb = bm.get(&name).copied().unwrap_or(f32::NAN);
                let hh = hm.get(&name).copied().unwrap_or(f32::NAN);
                let dd = if bb.is_nan() || hh.is_nan() { f32::NAN } else { hh - bb };
                out.push_str(&format!("| {} | {} | {} | {} |\n", name, fmtf(bb), fmtf(hh), fmtf(dd)));
            }
            out.push('\n');
        }
    }

    Ok(out)
}

fn fmtf(v: f32) -> String {
    if v.is_nan() {
        "—".into()
    } else {
        format!("{:.3}", v)
    }
}
