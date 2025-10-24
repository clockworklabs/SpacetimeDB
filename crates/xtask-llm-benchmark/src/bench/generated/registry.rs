pub mod basics {
    pub mod t_000_empty_reducers {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_000_empty_reducers/spec.rs"#
        );
    }
    pub mod t_001_basic_tables {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_001_basic_tables/spec.rs"#
        );
    }
    pub mod t_002_scheduled_table {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_002_scheduled_table/spec.rs"#
        );
    }
    pub mod t_003_struct_in_table {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_003_struct_in_table/spec.rs"#
        );
    }
    pub mod t_004_insert {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_004_insert/spec.rs"#
        );
    }
    pub mod t_005_update {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_005_update/spec.rs"#
        );
    }
    pub mod t_006_delete {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_006_delete/spec.rs"#
        );
    }
    pub mod t_007_crud {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_007_crud/spec.rs"#
        );
    }
    pub mod t_008_index_lookup {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_008_index_lookup/spec.rs"#
        );
    }
    pub mod t_009_init {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_009_init/spec.rs"#
        );
    }
    pub mod t_010_connect {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_010_connect/spec.rs"#
        );
    }
    pub mod t_011_helper_function {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/basics/t_011_helper_function/spec.rs"#
        );
    }
}
pub mod schema {
    pub mod t_012_spacetime_product_type {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_012_spacetime_product_type/spec.rs"#
        );
    }
    pub mod t_013_spacetime_sum_type {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_013_spacetime_sum_type/spec.rs"#
        );
    }
    pub mod t_014_elementary_columns {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_014_elementary_columns/spec.rs"#
        );
    }
    pub mod t_015_product_type_columns {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_015_product_type_columns/spec.rs"#
        );
    }
    pub mod t_016_sum_type_columns {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_016_sum_type_columns/spec.rs"#
        );
    }
    pub mod t_017_scheduled_columns {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_017_scheduled_columns/spec.rs"#
        );
    }
    pub mod t_018_constraints {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_018_constraints/spec.rs"#
        );
    }
    pub mod t_019_many_to_many {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_019_many_to_many/spec.rs"#
        );
    }
    pub mod t_020_ecs {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_020_ecs/spec.rs"#
        );
    }
    pub mod t_021_multi_column_index {
        include!(
            r#"E:/Development/ClockworkLabs/SpacetimeDB/SpacetimeDBPrivate/private/crates/xtask-llm-benchmark/src/benchmarks/schema/t_021_multi_column_index/spec.rs"#
        );
    }
}
use crate::eval::BenchmarkSpec;
use anyhow::{anyhow, Result};
use std::path::Path;

pub fn resolve_by_path(task_root: &Path) -> Result<fn() -> BenchmarkSpec> {
    let task = task_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("missing task name"))?;
    let category = task_root
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("missing category name"))?;
    let ctor = match (category, task) {
        ("basics", "t_000_empty_reducers") => self::basics::t_000_empty_reducers::spec,
        ("basics", "t_001_basic_tables") => self::basics::t_001_basic_tables::spec,
        ("basics", "t_002_scheduled_table") => self::basics::t_002_scheduled_table::spec,
        ("basics", "t_003_struct_in_table") => self::basics::t_003_struct_in_table::spec,
        ("basics", "t_004_insert") => self::basics::t_004_insert::spec,
        ("basics", "t_005_update") => self::basics::t_005_update::spec,
        ("basics", "t_006_delete") => self::basics::t_006_delete::spec,
        ("basics", "t_007_crud") => self::basics::t_007_crud::spec,
        ("basics", "t_008_index_lookup") => self::basics::t_008_index_lookup::spec,
        ("basics", "t_009_init") => self::basics::t_009_init::spec,
        ("basics", "t_010_connect") => self::basics::t_010_connect::spec,
        ("basics", "t_011_helper_function") => self::basics::t_011_helper_function::spec,
        ("schema", "t_012_spacetime_product_type") => self::schema::t_012_spacetime_product_type::spec,
        ("schema", "t_013_spacetime_sum_type") => self::schema::t_013_spacetime_sum_type::spec,
        ("schema", "t_014_elementary_columns") => self::schema::t_014_elementary_columns::spec,
        ("schema", "t_015_product_type_columns") => self::schema::t_015_product_type_columns::spec,
        ("schema", "t_016_sum_type_columns") => self::schema::t_016_sum_type_columns::spec,
        ("schema", "t_017_scheduled_columns") => self::schema::t_017_scheduled_columns::spec,
        ("schema", "t_018_constraints") => self::schema::t_018_constraints::spec,
        ("schema", "t_019_many_to_many") => self::schema::t_019_many_to_many::spec,
        ("schema", "t_020_ecs") => self::schema::t_020_ecs::spec,
        ("schema", "t_021_multi_column_index") => self::schema::t_021_multi_column_index::spec,
        _ => return Err(anyhow!("no spec registered for {}/{} (need spec.rs)", category, task)),
    };
    Ok(ctor)
}
