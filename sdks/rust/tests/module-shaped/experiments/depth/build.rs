//! Generates a CPS chain over `CPS_ITEMS` items (default 125).
//! `CPS_MODE=rules` (default): each descriptor calls the next descriptor directly, one expansion per item.
//! `CPS_MODE=proc`: each descriptor calls the `pm::collect!` proc macro, which calls the next descriptor,
//! two expansions per item.
use std::fmt::Write;

fn main() {
    println!("cargo:rerun-if-env-changed=CPS_ITEMS");
    println!("cargo:rerun-if-env-changed=CPS_MODE");
    let n: usize = std::env::var("CPS_ITEMS").map(|s| s.parse().unwrap()).unwrap_or(125);
    let mode = std::env::var("CPS_MODE").unwrap_or_else(|_| "rules".into());
    let mut out = String::new();
    if mode == "proc" {
        out.push_str("mod items {\n");
        for i in 0..n {
            writeln!(out, "    #[pm::describe(item{i})] pub struct Item{i};").unwrap();
        }
        out.push_str("}\n");
        let paths: Vec<String> = (0..n).map(|i| format!("items::__desc_item{i}")).collect();
        writeln!(out, "pm::collect! {{ [{}] }}", paths.join(", ")).unwrap();
    } else {
        out.push_str(
            "macro_rules! finish { ($($acc:ident)*) => { pub const COLLECTED: &[&str] = &[$(stringify!($acc)),*]; } }\n",
        );
        out.push_str("mod items {\n");
        for i in 0..n {
            writeln!(
                out,
                "    macro_rules! __desc_item{i} {{
        ( [ $next:ident $(, $rest:ident)* ] $($acc:ident)* ) => {{ $next! {{ [ $($rest),* ] $($acc)* item{i} }} }};
        ( [ ] $($acc:ident)* ) => {{ finish! {{ $($acc)* item{i} }} }};
    }}
    pub(crate) use __desc_item{i};"
            )
            .unwrap();
        }
        out.push_str("}\nuse items::*;\n");
        let rest: Vec<String> = (1..n).map(|i| format!("__desc_item{i}")).collect();
        writeln!(out, "__desc_item0! {{ [{}] }}", rest.join(", ")).unwrap();
    }
    writeln!(out, "pub const EXPECTED: usize = {n};").unwrap();
    std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("chain.rs"), out).unwrap();
}
