//! How many items a CPS chain can visit under the default `recursion_limit` of 128. See `build.rs`.
//! Measured with rustc 1.96.1: `CPS_MODE=rules` passes at 125 items and fails at 126; `CPS_MODE=proc` passes at 63
//! and fails at 64. `include!` is itself a macro expansion, so the chain starts one level deep. Invoked at the top
//! level of a source file, the `rules` chain passes at 126 and fails at 127. In general the chain's depth adds to
//! the depth of its invocation.
include!(concat!(env!("OUT_DIR"), "/chain.rs"));

#[test]
fn collected_every_item() {
    assert_eq!(COLLECTED.len(), EXPECTED);
}
