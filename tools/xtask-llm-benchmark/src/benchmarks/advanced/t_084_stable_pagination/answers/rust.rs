use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = audit_entry, public)]
pub struct AuditEntry {
    #[primary_key]
    pub id: u64,
    pub tenant: String,
    pub occurred_at: u64,
    pub visible: bool,
}

#[table(accessor = page_entry, public)]
pub struct PageEntry {
    #[primary_key]
    pub position: u64,
    pub entry_id: u64,
    pub occurred_at: u64,
}

#[reducer]
pub fn add_entry(ctx: &ReducerContext, id: u64, tenant: String, occurred_at: u64, visible: bool) -> Result<(), String> {
    ctx.db.audit_entry().insert(AuditEntry {
        id,
        tenant,
        occurred_at,
        visible,
    });
    Ok(())
}

#[reducer]
pub fn read_page(
    ctx: &ReducerContext,
    tenant: String,
    after_time: u64,
    after_id: u64,
    limit: u64,
) -> Result<(), String> {
    if limit == 0 || limit > 10 {
        return Err("invalid limit".into());
    }
    let mut rows: Vec<_> = ctx
        .db
        .audit_entry()
        .iter()
        .filter(|r| r.tenant == tenant && r.visible && (r.occurred_at, r.id) > (after_time, after_id))
        .collect();
    rows.sort_by_key(|r| (r.occurred_at, r.id));
    for old in ctx.db.page_entry().iter() {
        ctx.db.page_entry().position().delete(old.position);
    }
    for (position, row) in rows.into_iter().take(limit as usize).enumerate() {
        ctx.db.page_entry().insert(PageEntry {
            position: position as u64,
            entry_id: row.id,
            occurred_at: row.occurred_at,
        });
    }
    Ok(())
}
