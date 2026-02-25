use spacetimedb::{reducer, table, ReducerContext, Table};

/// Table where the canonical wire name ("report") differs from the Rust accessor name ("reports").
#[table(name = "report", accessor = reports, public)]
pub struct Report {
    #[primary_key]
    #[auto_inc]
    pub id: u32,
    pub title: String,
    pub body: String,
}

/// Table where the canonical wire name ("report_category") differs from the accessor ("categories").
#[table(name = "report_category", accessor = categories, public)]
pub struct ReportCategory {
    #[primary_key]
    #[auto_inc]
    pub id: u32,
    pub name: String,
    pub report_id: u32,
}

#[reducer]
pub fn create_report(ctx: &ReducerContext, title: String, body: String) {
    ctx.db.reports().insert(Report {
        id: 0,
        title,
        body,
    });
}
