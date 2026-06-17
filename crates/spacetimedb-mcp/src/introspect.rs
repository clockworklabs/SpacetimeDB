//! Pure transformations from a SpacetimeDB module definition into the shapes
//! the MCP tools return. Kept free of I/O so they can be unit-tested directly.

use serde::Serialize;
use spacetimedb_lib::db::raw_def::v9::RawModuleDefV9;
use spacetimedb_lib::sats;

/// Names of every table in the module, in declaration order.
pub fn table_names(def: &RawModuleDefV9) -> Vec<String> {
    def.tables.iter().map(|t| t.name.to_string()).collect()
}

/// A reducer, reduced to the bits an agent cares about when browsing a module.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ReducerSummary {
    pub name: String,
    /// The reducer's lifecycle role (`Init`, `OnConnect`, `OnDisconnect`),
    /// or `None` for an ordinary reducer.
    pub lifecycle: Option<String>,
}

/// Summarize every reducer in the module, in declaration order.
pub fn reducer_summaries(def: &RawModuleDefV9) -> Vec<ReducerSummary> {
    def.reducers
        .iter()
        .map(|r| ReducerSummary {
            name: r.name.to_string(),
            lifecycle: r.lifecycle.map(|l| format!("{l:?}")),
        })
        .collect()
}

/// Serialize the full module definition to pretty JSON, using SpacetimeDB's
/// own SATS serialization so the output matches `spacetime describe --json`.
pub fn schema_json(def: &RawModuleDefV9) -> serde_json::Result<String> {
    serde_json::to_string_pretty(sats::serde::SerdeWrapper::from_ref(def))
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_lib::db::raw_def::v9::{Lifecycle, RawModuleDefV9Builder};
    use spacetimedb_lib::sats::{AlgebraicType, ProductType};

    /// A small synthetic module: two tables, two reducers (one lifecycle).
    fn sample() -> RawModuleDefV9 {
        let mut b = RawModuleDefV9Builder::new();
        b.build_table_with_new_type_for_tests("widget", ProductType::from([("id", AlgebraicType::U64)]), false)
            .finish();
        b.build_table_with_new_type_for_tests("gadget", ProductType::from([("name", AlgebraicType::String)]), false)
            .finish();
        let no_params: [(&str, AlgebraicType); 0] = [];
        b.add_reducer("init", ProductType::from(no_params), Some(Lifecycle::Init));
        b.add_reducer("do_thing", ProductType::from([("x", AlgebraicType::U64)]), None);
        b.finish()
    }

    #[test]
    fn table_names_lists_every_table() {
        let mut names = table_names(&sample());
        names.sort();
        assert_eq!(names, vec!["gadget".to_string(), "widget".to_string()]);
    }

    #[test]
    fn reducer_summaries_capture_name_and_lifecycle() {
        let summaries = reducer_summaries(&sample());
        let init = summaries.iter().find(|s| s.name == "init").unwrap();
        assert_eq!(init.lifecycle.as_deref(), Some("Init"));
        let ordinary = summaries.iter().find(|s| s.name == "do_thing").unwrap();
        assert_eq!(ordinary.lifecycle, None);
    }

    #[test]
    fn schema_json_round_trips_through_deserialize_wrapper() {
        // The MCP client decodes the schema endpoint with `DeserializeWrapper`,
        // while `schema_json` serializes with `SerdeWrapper`. They must be duals;
        // this guards that contract so the live decode path can't silently break.
        use spacetimedb_lib::de::serde::DeserializeWrapper;
        let json = schema_json(&sample()).unwrap();
        let DeserializeWrapper(decoded): DeserializeWrapper<RawModuleDefV9> = serde_json::from_str(&json).unwrap();
        assert_eq!(table_names(&decoded).len(), 2);
        assert_eq!(decoded.reducers.len(), 2);
    }
}
