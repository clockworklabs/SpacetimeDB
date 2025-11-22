use spacetimedb_physical_plan::plan::PhysicalPlan;
use spacetimedb_schema::schema::TableSchema;
use std::sync::Arc;

/// Scan strategy types for subscription queries
#[derive(Debug, Clone, Copy)]
enum ScanStrategy {
    /// Full table scan - no indexes used
    Sequential,
    /// Uses index but requires post-filtering on non-indexed columns
    IndexedWithFilter,
    /// Fully indexed - no post-filtering needed
    FullyIndexed,
    /// Mixed strategy (combination of index and table scans)
    Mixed,
    /// Unknown/other strategy
    Unknown,
}

/// Metrics data for a single subscription query execution
#[derive(Debug)]
pub struct QueryMetrics {
    pub scan_type: String,
    pub table_name: String,
    pub unindexed_columns: String,
    pub rows_scanned: u64,
    pub execution_time_micros: u64,
}

impl std::fmt::Display for ScanStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sequential => write!(f, "sequential"),
            Self::IndexedWithFilter => write!(f, "indexed_with_filter"),
            Self::FullyIndexed => write!(f, "fully_indexed"),
            Self::Mixed => write!(f, "mixed"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Recursively extracts column names from filter expressions
fn extract_columns(
    expr: &spacetimedb_physical_plan::plan::PhysicalExpr,
    schema: Option<&Arc<TableSchema>>,
    columns: &mut Vec<String>,
) {
    use spacetimedb_physical_plan::plan::PhysicalExpr;

    match expr {
        PhysicalExpr::Field(tuple_field) => {
            let col_name = schema
                .and_then(|s| s.columns.get(tuple_field.field_pos))
                .map(|col| col.col_name.to_string())
                .unwrap_or_else(|| format!("col_{}", tuple_field.field_pos));
            columns.push(col_name);
        }
        PhysicalExpr::BinOp(_, lhs, rhs) => {
            extract_columns(lhs, schema, columns);
            extract_columns(rhs, schema, columns);
        }
        PhysicalExpr::LogOp(_, exprs) => {
            for expr in exprs {
                extract_columns(expr, schema, columns);
            }
        }
        PhysicalExpr::Value(_) => {}
    }
}

/// Analyzes subscription scan strategy and creates QueryMetrics
pub fn get_query_metrics(
    table_name: &str,
    plan: &PhysicalPlan,
    rows_scanned: u64,
    execution_time_micros: u64,
) -> QueryMetrics {
    let has_table_scan = plan.any(&|p| matches!(p, PhysicalPlan::TableScan(..)));
    let has_index_scan = plan.any(&|p| matches!(p, PhysicalPlan::IxScan(..)));
    let has_post_filter = plan.any(&|p| matches!(p, PhysicalPlan::Filter(..)));

    let strategy = if has_table_scan && has_index_scan {
        ScanStrategy::Mixed
    } else if has_table_scan {
        ScanStrategy::Sequential
    } else if has_index_scan && has_post_filter {
        ScanStrategy::IndexedWithFilter
    } else if has_index_scan {
        ScanStrategy::FullyIndexed
    } else {
        ScanStrategy::Unknown
    };

    // Extract the schema from the plan
    let mut schema: Option<Arc<TableSchema>> = None;
    plan.visit(&mut |p| match p {
        PhysicalPlan::TableScan(scan, _) => {
            schema = Some(scan.schema.clone());
        }
        PhysicalPlan::IxScan(scan, _) => {
            schema = Some(scan.schema.clone());
        }
        _ => {}
    });

    let mut columns = Vec::new();
    plan.visit(&mut |p| {
        if let PhysicalPlan::Filter(_, expr) = p {
            extract_columns(expr, schema.as_ref(), &mut columns);
        }
    });

    QueryMetrics {
        scan_type: strategy.to_string(),
        table_name: table_name.to_string(),
        unindexed_columns: columns.join(","),
        rows_scanned,
        execution_time_micros,
    }
}
