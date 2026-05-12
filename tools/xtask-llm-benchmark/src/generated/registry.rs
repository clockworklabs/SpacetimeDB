use crate::eval::BenchmarkSpec;
use anyhow::{anyhow, Result};
use std::path::Path;

#[allow(dead_code)]
#[allow(clippy::all)]
mod auth_t_026_auth_identity_check {
    include!("../benchmarks/auth/t_026_auth_identity_check/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod auth_t_027_private_vs_public_table {
    include!("../benchmarks/auth/t_027_private_vs_public_table/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod auth_t_041_registered_user_gate {
    include!("../benchmarks/auth/t_041_registered_user_gate/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod auth_t_042_admin_bootstrap {
    include!("../benchmarks/auth/t_042_admin_bootstrap/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod auth_t_043_role_based_access {
    include!("../benchmarks/auth/t_043_role_based_access/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod auth_t_044_ban_list {
    include!("../benchmarks/auth/t_044_ban_list/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod auth_t_045_rate_limit {
    include!("../benchmarks/auth/t_045_rate_limit/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod auth_t_046_shared_document {
    include!("../benchmarks/auth/t_046_shared_document/spec.rs");
}

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
mod basics_t_038_schedule_at_time {
    include!("../benchmarks/basics/t_038_schedule_at_time/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_039_cancel_schedule {
    include!("../benchmarks/basics/t_039_cancel_schedule/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod basics_t_040_lifecycle_player {
    include!("../benchmarks/basics/t_040_lifecycle_player/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod data_modeling_t_024_event_table {
    include!("../benchmarks/data_modeling/t_024_event_table/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod data_modeling_t_025_optional_fields {
    include!("../benchmarks/data_modeling/t_025_optional_fields/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod data_modeling_t_028_cascade_delete {
    include!("../benchmarks/data_modeling/t_028_cascade_delete/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod data_modeling_t_029_filter_and_aggregate {
    include!("../benchmarks/data_modeling/t_029_filter_and_aggregate/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod data_modeling_t_030_two_table_join {
    include!("../benchmarks/data_modeling/t_030_two_table_join/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod data_modeling_t_031_unique_constraint {
    include!("../benchmarks/data_modeling/t_031_unique_constraint/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod queries_t_022_view_basic {
    include!("../benchmarks/queries/t_022_view_basic/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod queries_t_023_view_per_user {
    include!("../benchmarks/queries/t_023_view_per_user/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod queries_t_032_range_query {
    include!("../benchmarks/queries/t_032_range_query/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod queries_t_033_sort_and_limit {
    include!("../benchmarks/queries/t_033_sort_and_limit/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod queries_t_034_find_first {
    include!("../benchmarks/queries/t_034_find_first/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod queries_t_035_select_distinct {
    include!("../benchmarks/queries/t_035_select_distinct/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod queries_t_036_count_without_collect {
    include!("../benchmarks/queries/t_036_count_without_collect/spec.rs");
}

#[allow(dead_code)]
#[allow(clippy::all)]
mod queries_t_037_multi_column_filter {
    include!("../benchmarks/queries/t_037_multi_column_filter/spec.rs");
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
        ("auth", "t_026_auth_identity_check") => auth_t_026_auth_identity_check::spec,
        ("auth", "t_027_private_vs_public_table") => auth_t_027_private_vs_public_table::spec,
        ("auth", "t_041_registered_user_gate") => auth_t_041_registered_user_gate::spec,
        ("auth", "t_042_admin_bootstrap") => auth_t_042_admin_bootstrap::spec,
        ("auth", "t_043_role_based_access") => auth_t_043_role_based_access::spec,
        ("auth", "t_044_ban_list") => auth_t_044_ban_list::spec,
        ("auth", "t_045_rate_limit") => auth_t_045_rate_limit::spec,
        ("auth", "t_046_shared_document") => auth_t_046_shared_document::spec,
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
        ("basics", "t_038_schedule_at_time") => basics_t_038_schedule_at_time::spec,
        ("basics", "t_039_cancel_schedule") => basics_t_039_cancel_schedule::spec,
        ("basics", "t_040_lifecycle_player") => basics_t_040_lifecycle_player::spec,
        ("data_modeling", "t_024_event_table") => data_modeling_t_024_event_table::spec,
        ("data_modeling", "t_025_optional_fields") => data_modeling_t_025_optional_fields::spec,
        ("data_modeling", "t_028_cascade_delete") => data_modeling_t_028_cascade_delete::spec,
        ("data_modeling", "t_029_filter_and_aggregate") => data_modeling_t_029_filter_and_aggregate::spec,
        ("data_modeling", "t_030_two_table_join") => data_modeling_t_030_two_table_join::spec,
        ("data_modeling", "t_031_unique_constraint") => data_modeling_t_031_unique_constraint::spec,
        ("queries", "t_022_view_basic") => queries_t_022_view_basic::spec,
        ("queries", "t_023_view_per_user") => queries_t_023_view_per_user::spec,
        ("queries", "t_032_range_query") => queries_t_032_range_query::spec,
        ("queries", "t_033_sort_and_limit") => queries_t_033_sort_and_limit::spec,
        ("queries", "t_034_find_first") => queries_t_034_find_first::spec,
        ("queries", "t_035_select_distinct") => queries_t_035_select_distinct::spec,
        ("queries", "t_036_count_without_collect") => queries_t_036_count_without_collect::spec,
        ("queries", "t_037_multi_column_filter") => queries_t_037_multi_column_filter::spec,
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
