use crate::eval::BenchmarkSpec;
use anyhow::{anyhow, Result};
use std::path::Path;

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_000_empty_reducers {
    include!("../benchmarks/basics/t_000_empty_reducers/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_001_basic_tables {
    include!("../benchmarks/basics/t_001_basic_tables/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_002_scheduled_table {
    include!("../benchmarks/basics/t_002_scheduled_table/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_003_struct_in_table {
    include!("../benchmarks/basics/t_003_struct_in_table/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_004_insert {
    include!("../benchmarks/basics/t_004_insert/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_005_update {
    include!("../benchmarks/basics/t_005_update/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_006_delete {
    include!("../benchmarks/basics/t_006_delete/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_007_crud {
    include!("../benchmarks/basics/t_007_crud/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_008_index_lookup {
    include!("../benchmarks/basics/t_008_index_lookup/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_009_init {
    include!("../benchmarks/basics/t_009_init/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_010_connect {
    include!("../benchmarks/basics/t_010_connect/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_011_helper_function {
    include!("../benchmarks/basics/t_011_helper_function/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_012_spacetime_product_type {
    include!("../benchmarks/schema/t_012_spacetime_product_type/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_013_spacetime_sum_type {
    include!("../benchmarks/schema/t_013_spacetime_sum_type/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_014_elementary_columns {
    include!("../benchmarks/schema/t_014_elementary_columns/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_015_product_type_columns {
    include!("../benchmarks/schema/t_015_product_type_columns/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_016_sum_type_columns {
    include!("../benchmarks/schema/t_016_sum_type_columns/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_017_scheduled_columns {
    include!("../benchmarks/schema/t_017_scheduled_columns/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_018_constraints {
    include!("../benchmarks/schema/t_018_constraints/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_019_many_to_many {
    include!("../benchmarks/schema/t_019_many_to_many/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_020_ecs {
    include!("../benchmarks/schema/t_020_ecs/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod schema_t_021_multi_column_index {
    include!("../benchmarks/schema/t_021_multi_column_index/spec.rs");
}

pub fn resolve_by_path(task_root: &Path) -> Result<fn() -> BenchmarkSpec> {
    let task = task_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("missing task name"))?;
    let category = task_root
        .parent()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()))
        .ok_or_else(|| anyhow!("missing category name"))?;

    let ctor = match (category, task) {
        ("basics", "t_000_empty_reducers") => basics_t_000_empty_reducers::spec,
        ("basics", "t_001_basic_tables") => basics_t_001_basic_tables::spec,
        ("basics", "t_002_scheduled_table") => basics_t_002_scheduled_table::spec,
        ("basics", "t_003_struct_in_table") => basics_t_003_struct_in_table::spec,
        ("basics", "t_004_insert") => basics_t_004_insert::spec,
        ("basics", "t_005_update") => basics_t_005_update::spec,
        ("basics", "t_006_delete") => basics_t_006_delete::spec,
        ("basics", "t_007_crud") => basics_t_007_crud::spec,
        ("basics", "t_008_index_lookup") => basics_t_008_index_lookup::spec,
        ("basics", "t_009_init") => basics_t_009_init::spec,
        ("basics", "t_010_connect") => basics_t_010_connect::spec,
        ("basics", "t_011_helper_function") => basics_t_011_helper_function::spec,
        ("schema", "t_012_spacetime_product_type") => schema_t_012_spacetime_product_type::spec,
        ("schema", "t_013_spacetime_sum_type") => schema_t_013_spacetime_sum_type::spec,
        ("schema", "t_014_elementary_columns") => schema_t_014_elementary_columns::spec,
        ("schema", "t_015_product_type_columns") => schema_t_015_product_type_columns::spec,
        ("schema", "t_016_sum_type_columns") => schema_t_016_sum_type_columns::spec,
        ("schema", "t_017_scheduled_columns") => schema_t_017_scheduled_columns::spec,
        ("schema", "t_018_constraints") => schema_t_018_constraints::spec,
        ("schema", "t_019_many_to_many") => schema_t_019_many_to_many::spec,
        ("schema", "t_020_ecs") => schema_t_020_ecs::spec,
        ("schema", "t_021_multi_column_index") => schema_t_021_multi_column_index::spec,
        _ => return Err(anyhow!("no spec registered for {}/{} (need spec.rs)", category, task)),
    };

    Ok(ctor)
}
