use spacetimedb::rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use spacetimedb::{
    log_stopwatch::LogStopwatch, reducer, table, Identity, ReducerContext, ScheduleAt, SpacetimeType, Table, Timestamp,
};

use crate::{
    customer, district, history, item,
    new_order::pack_order_line_key,
    new_order_row, oorder, order_line,
    remote::{clear_remote_warehouses, replace_remote_warehouses, RemoteWarehouse},
    stock, warehouse, Customer, District, History, Item, NewOrder, OOrder, OrderLine, Stock, Warehouse, WarehouseId,
    CUSTOMERS_PER_DISTRICT, DISTRICTS_PER_WAREHOUSE, ITEMS,
};

const LOAD_SINGLETON_ID: u8 = 1;
const WAREHOUSE_YTD_CENTS: i64 = 30_000_000;
const DISTRICT_YTD_CENTS: i64 = 3_000_000;
const CUSTOMER_CREDIT_LIMIT_CENTS: i64 = 5_000_000;
const CUSTOMER_INITIAL_BALANCE_CENTS: i64 = -1_000;
const CUSTOMER_INITIAL_YTD_PAYMENT_CENTS: i64 = 1_000;
const HISTORY_INITIAL_AMOUNT_CENTS: i64 = 1_000;
const NEW_ORDER_START: u32 = 2_101;

const TAG_ITEM: u64 = 0x1000;
const TAG_WAREHOUSE: u64 = 0x2000;
const TAG_DISTRICT: u64 = 0x3000;
const TAG_STOCK: u64 = 0x4000;
const TAG_CUSTOMER: u64 = 0x5000;
const TAG_HISTORY: u64 = 0x6000;
const TAG_ORDER_PERMUTATION: u64 = 0x7000;
const TAG_ORDER: u64 = 0x8000;
const TAG_ORDER_LINE: u64 = 0x9000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, SpacetimeType)]
pub enum TpccLoadStatus {
    Idle,
    Running,
    Failed,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, SpacetimeType)]
pub enum TpccLoadPhase {
    Items,
    WarehousesDistricts,
    Stock,
    CustomersHistory,
    Orders,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct TpccLoadConfigRequest {
    pub database_number: u32,
    pub num_databases: u32,
    pub warehouses_per_database: u32,
    pub warehouse_id_offset: u32,
    pub skip_items: bool,
    pub batch_size: u32,
    pub seed: u64,
    pub load_c_last: u32,
    pub base_ts: Timestamp,
    pub spacetimedb_uri: String,
    pub database_identities: Vec<Identity>,
}

#[table(accessor = tpcc_load_config)]
#[derive(Clone, Debug)]
pub struct TpccLoadConfig {
    #[primary_key]
    pub singleton_id: u8,
    pub database_number: u32,
    pub num_databases: u32,
    pub warehouses_per_database: u32,
    pub warehouse_id_offset: u32,
    pub skip_items: bool,
    pub batch_size: u32,
    pub seed: u64,
    pub load_c_last: u32,
    pub base_ts: Timestamp,
    pub spacetimedb_uri: String,
    pub database_identities: Vec<Identity>,
}

#[table(accessor = tpcc_load_state, public)]
#[derive(Clone, Debug)]
pub struct TpccLoadState {
    #[primary_key]
    pub singleton_id: u8,
    pub status: TpccLoadStatus,
    pub phase: TpccLoadPhase,
    pub next_warehouse_id: u32,
    pub next_district_id: u8,
    pub next_item_id: u32,
    pub next_order_id: u32,
    pub chunks_completed: u64,
    pub rows_inserted: u64,
    pub last_error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub updated_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

#[table(accessor = tpcc_load_job, scheduled(run_tpcc_load_chunk))]
#[derive(Clone, Debug)]
pub struct TpccLoadJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub phase: TpccLoadPhase,
    pub next_warehouse_id: u32,
    pub next_district_id: u8,
    pub next_item_id: u32,
    pub next_order_id: u32,
}

#[reducer]
pub fn configure_tpcc_load(ctx: &ReducerContext, request: TpccLoadConfigRequest) -> Result<(), String> {
    if ctx.db.tpcc_load_job().iter().next().is_some() {
        return Err("tpcc load is already running".into());
    }
    configure_tpcc_load_internal(ctx, request)
}

#[reducer]
pub fn start_tpcc_load(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.db.tpcc_load_job().iter().next().is_some() {
        return Err("tpcc load is already running".into());
    }

    let mut state = current_state(ctx)?;
    if state.status == TpccLoadStatus::Complete {
        return Err("tpcc load has already completed; use restart_tpcc_load to run again".into());
    }
    state.status = TpccLoadStatus::Running;
    state.last_error = None;
    state.started_at = Some(ctx.timestamp);
    state.updated_at = ctx.timestamp;
    state.completed_at = None;
    replace_state(ctx, state.clone());
    insert_job(ctx, job_from_state(&state, ctx.timestamp));
    Ok(())
}

#[reducer]
pub fn resume_tpcc_load(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.db.tpcc_load_job().iter().next().is_some() {
        return Err("tpcc load is already running".into());
    }

    let mut state = current_state(ctx)?;
    if state.status == TpccLoadStatus::Complete {
        return Err("tpcc load has already completed".into());
    }
    state.status = TpccLoadStatus::Running;
    state.last_error = None;
    state.updated_at = ctx.timestamp;
    replace_state(ctx, state.clone());
    insert_job(ctx, job_from_state(&state, ctx.timestamp));
    Ok(())
}

#[reducer]
pub fn restart_tpcc_load(ctx: &ReducerContext) -> Result<(), String> {
    let request = config_as_request(&current_config(ctx)?);
    crate::clear_tpcc_business_tables(ctx);
    configure_tpcc_load_internal(ctx, request)?;
    start_tpcc_load(ctx)
}

#[reducer]
pub fn run_tpcc_load_chunk(ctx: &ReducerContext, job: TpccLoadJob) -> Result<(), String> {
    let config = current_config(ctx)?;
    let state = current_state(ctx)?;
    if state.status != TpccLoadStatus::Running {
        fail_load(ctx, state, "tpcc load state is not running".into());
        return Ok(());
    }

    let result = run_chunk(ctx, &config, &job);
    match result {
        Ok(advance) => {
            let mut next_state = state;
            next_state.phase = advance.phase;
            next_state.next_warehouse_id = advance.next_warehouse_id;
            next_state.next_district_id = advance.next_district_id;
            next_state.next_item_id = advance.next_item_id;
            next_state.next_order_id = advance.next_order_id;
            next_state.chunks_completed = next_state.chunks_completed.saturating_add(1);
            next_state.rows_inserted = next_state.rows_inserted.saturating_add(advance.rows_inserted);
            next_state.updated_at = ctx.timestamp;

            if advance.complete {
                next_state.status = TpccLoadStatus::Complete;
                next_state.completed_at = Some(ctx.timestamp);
                replace_state(ctx, next_state);
            } else {
                replace_state(ctx, next_state.clone());
                insert_job(ctx, job_from_state(&next_state, ctx.timestamp));
            }
        }
        Err(error) => fail_load(ctx, state, error),
    }

    Ok(())
}

pub(crate) fn clear_load_metadata(ctx: &ReducerContext) {
    for row in ctx.db.tpcc_load_job().iter() {
        ctx.db.tpcc_load_job().delete(row);
    }
    for row in ctx.db.tpcc_load_state().iter() {
        ctx.db.tpcc_load_state().delete(row);
    }
    for row in ctx.db.tpcc_load_config().iter() {
        ctx.db.tpcc_load_config().delete(row);
    }
    clear_remote_warehouses(ctx);
}

fn configure_tpcc_load_internal(ctx: &ReducerContext, request: TpccLoadConfigRequest) -> Result<(), String> {
    let _timer = LogStopwatch::new("configure_tpcc_load");
    validate_request(&request)?;
    clear_load_metadata(ctx);

    replace_remote_warehouses(ctx, build_remote_warehouses(&request))?;
    let state = initial_state(&request, ctx.timestamp);

    ctx.db.tpcc_load_config().insert(TpccLoadConfig {
        singleton_id: LOAD_SINGLETON_ID,
        database_number: request.database_number,
        num_databases: request.num_databases,
        warehouses_per_database: request.warehouses_per_database,
        warehouse_id_offset: request.warehouse_id_offset,
        skip_items: request.skip_items,
        batch_size: request.batch_size,
        seed: request.seed,
        load_c_last: request.load_c_last,
        base_ts: request.base_ts,
        spacetimedb_uri: request.spacetimedb_uri,
        database_identities: request.database_identities,
    });

    replace_state(ctx, state);
    Ok(())
}

fn validate_request(request: &TpccLoadConfigRequest) -> Result<(), String> {
    if request.num_databases == 0 {
        return Err("num_databases must be positive".into());
    }
    if request.warehouses_per_database == 0 {
        return Err("warehouses_per_database must be positive".into());
    }
    if request.batch_size == 0 {
        return Err("batch_size must be positive".into());
    }
    if usize::try_from(request.num_databases).ok() != Some(request.database_identities.len()) {
        return Err("database_identities length must match num_databases".into());
    }
    if request.database_number >= request.num_databases {
        return Err("database_number must be less than num_databases".into());
    }
    if request
        .num_databases
        .checked_mul(request.warehouses_per_database)
        .is_none()
    {
        return Err(format!(
            "total warehouses overflow u32 (num_databases={} * warehouses_per_database={})",
            request.num_databases, request.warehouses_per_database
        ));
    }
    // Validate that the warehouse ID range for this database doesn't overflow u32.
    // warehouse_start = database_number * warehouses_per_database + warehouse_id_offset + 1
    // warehouse_end   = warehouse_start + warehouses_per_database - 1
    if request
        .database_number
        .checked_mul(request.warehouses_per_database)
        .and_then(|v| v.checked_add(request.warehouse_id_offset))
        .and_then(|v| v.checked_add(request.warehouses_per_database))
        .is_none()
    {
        return Err(format!(
            "warehouse id range overflow u32 (database_number={}, warehouses_per_database={}, warehouse_id_offset={})",
            request.database_number, request.warehouses_per_database, request.warehouse_id_offset
        ));
    }
    Ok(())
}

fn initial_state(request: &TpccLoadConfigRequest, now: Timestamp) -> TpccLoadState {
    TpccLoadState {
        singleton_id: LOAD_SINGLETON_ID,
        status: TpccLoadStatus::Idle,
        phase: if request.skip_items {
            TpccLoadPhase::WarehousesDistricts
        } else {
            TpccLoadPhase::Items
        },
        next_warehouse_id: warehouse_start(
            request.database_number,
            request.warehouses_per_database,
            request.warehouse_id_offset,
        ),
        next_district_id: 1,
        next_item_id: 1,
        next_order_id: 1,
        chunks_completed: 0,
        rows_inserted: 0,
        last_error: None,
        started_at: None,
        updated_at: now,
        completed_at: None,
    }
}

fn config_as_request(config: &TpccLoadConfig) -> TpccLoadConfigRequest {
    TpccLoadConfigRequest {
        database_number: config.database_number,
        num_databases: config.num_databases,
        warehouses_per_database: config.warehouses_per_database,
        warehouse_id_offset: config.warehouse_id_offset,
        skip_items: config.skip_items,
        batch_size: config.batch_size,
        seed: config.seed,
        load_c_last: config.load_c_last,
        base_ts: config.base_ts,
        spacetimedb_uri: config.spacetimedb_uri.clone(),
        database_identities: config.database_identities.clone(),
    }
}

fn build_remote_warehouses(request: &TpccLoadConfigRequest) -> Vec<RemoteWarehouse> {
    let mut rows = Vec::new();
    for other_database_number in 0..request.num_databases {
        if other_database_number == request.database_number {
            continue;
        }
        let database_ident =
            request.database_identities[usize::try_from(other_database_number).expect("u32 fits usize")];
        for w_id in warehouse_range(other_database_number, request.warehouses_per_database, request.warehouse_id_offset) {
            rows.push(RemoteWarehouse {
                w_id,
                remote_database_home: database_ident,
            });
        }
    }
    rows
}

fn current_config(ctx: &ReducerContext) -> Result<TpccLoadConfig, String> {
    ctx.db
        .tpcc_load_config()
        .singleton_id()
        .find(LOAD_SINGLETON_ID)
        .ok_or_else(|| "tpcc load has not been configured".to_string())
}

fn current_state(ctx: &ReducerContext) -> Result<TpccLoadState, String> {
    ctx.db
        .tpcc_load_state()
        .singleton_id()
        .find(LOAD_SINGLETON_ID)
        .ok_or_else(|| "tpcc load state row is missing".to_string())
}

fn replace_state(ctx: &ReducerContext, state: TpccLoadState) {
    ctx.db.tpcc_load_state().singleton_id().delete(LOAD_SINGLETON_ID);
    ctx.db.tpcc_load_state().insert(state);
}

fn insert_job(ctx: &ReducerContext, job: TpccLoadJob) {
    ctx.db.tpcc_load_job().insert(job);
}

fn fail_load(ctx: &ReducerContext, mut state: TpccLoadState, error: String) {
    state.status = TpccLoadStatus::Failed;
    state.last_error = Some(error);
    state.updated_at = ctx.timestamp;
    replace_state(ctx, state);
}

fn job_from_state(state: &TpccLoadState, now: Timestamp) -> TpccLoadJob {
    TpccLoadJob {
        scheduled_id: 0,
        scheduled_at: now.into(),
        phase: state.phase,
        next_warehouse_id: state.next_warehouse_id,
        next_district_id: state.next_district_id,
        next_item_id: state.next_item_id,
        next_order_id: state.next_order_id,
    }
}

struct ChunkAdvance {
    phase: TpccLoadPhase,
    next_warehouse_id: WarehouseId,
    next_district_id: u8,
    next_item_id: u32,
    next_order_id: u32,
    rows_inserted: u64,
    complete: bool,
}

fn run_chunk(ctx: &ReducerContext, config: &TpccLoadConfig, job: &TpccLoadJob) -> Result<ChunkAdvance, String> {
    match job.phase {
        TpccLoadPhase::Items => load_item_chunk(ctx, config, job),
        TpccLoadPhase::WarehousesDistricts => load_warehouse_district_chunk(ctx, config, job),
        TpccLoadPhase::Stock => load_stock_chunk(ctx, config, job),
        TpccLoadPhase::CustomersHistory => load_customer_history_chunk(ctx, config, job),
        TpccLoadPhase::Orders => load_order_chunk(ctx, config, job),
    }
}

fn load_item_chunk(ctx: &ReducerContext, config: &TpccLoadConfig, job: &TpccLoadJob) -> Result<ChunkAdvance, String> {
    let _timer = LogStopwatch::new("load_item_chunk");
    if job.next_item_id == 0 || job.next_item_id > ITEMS {
        return Err(format!("invalid item cursor {}", job.next_item_id));
    }
    let chunk_end = (job.next_item_id + config.batch_size - 1).min(ITEMS);
    for item_id in job.next_item_id..=chunk_end {
        ctx.db.item().insert(generate_item(config, item_id));
    }

    let next_phase = if chunk_end == ITEMS {
        TpccLoadPhase::WarehousesDistricts
    } else {
        TpccLoadPhase::Items
    };
    Ok(ChunkAdvance {
        phase: next_phase,
        next_warehouse_id: warehouse_start(config.database_number, config.warehouses_per_database, config.warehouse_id_offset),
        next_district_id: 1,
        next_item_id: if chunk_end == ITEMS { 1 } else { chunk_end + 1 },
        next_order_id: 1,
        rows_inserted: u64::from(chunk_end - job.next_item_id + 1),
        complete: false,
    })
}

fn load_warehouse_district_chunk(
    ctx: &ReducerContext,
    config: &TpccLoadConfig,
    job: &TpccLoadJob,
) -> Result<ChunkAdvance, String> {
    let _timer = LogStopwatch::new("load_warehouses_district_chunk");
    let end_warehouse = warehouse_end(config.database_number, config.warehouses_per_database, config.warehouse_id_offset);
    if job.next_warehouse_id < warehouse_start(config.database_number, config.warehouses_per_database, config.warehouse_id_offset)
        || job.next_warehouse_id > end_warehouse
    {
        return Err(format!("invalid warehouse cursor {}", job.next_warehouse_id));
    }

    ctx.db
        .warehouse()
        .insert(generate_warehouse(config, job.next_warehouse_id));
    for d_id in 1..=DISTRICTS_PER_WAREHOUSE {
        ctx.db
            .district()
            .insert(generate_district(config, job.next_warehouse_id, d_id));
    }

    Ok(ChunkAdvance {
        phase: if job.next_warehouse_id == end_warehouse {
            TpccLoadPhase::Stock
        } else {
            TpccLoadPhase::WarehousesDistricts
        },
        next_warehouse_id: if job.next_warehouse_id == end_warehouse {
            warehouse_start(config.database_number, config.warehouses_per_database, config.warehouse_id_offset)
        } else {
            job.next_warehouse_id + 1
        },
        next_district_id: 1,
        next_item_id: 1,
        next_order_id: 1,
        rows_inserted: 1 + u64::from(DISTRICTS_PER_WAREHOUSE),
        complete: false,
    })
}

fn load_stock_chunk(ctx: &ReducerContext, config: &TpccLoadConfig, job: &TpccLoadJob) -> Result<ChunkAdvance, String> {
    let _timer = LogStopwatch::new("load_stock_chunk");
    let start_warehouse = warehouse_start(config.database_number, config.warehouses_per_database, config.warehouse_id_offset);
    let end_warehouse = warehouse_end(config.database_number, config.warehouses_per_database, config.warehouse_id_offset);
    if job.next_warehouse_id < start_warehouse || job.next_warehouse_id > end_warehouse {
        return Err(format!("invalid stock warehouse cursor {}", job.next_warehouse_id));
    }
    if job.next_item_id == 0 || job.next_item_id > ITEMS {
        return Err(format!("invalid stock item cursor {}", job.next_item_id));
    }

    let chunk_end = (job.next_item_id + config.batch_size - 1).min(ITEMS);
    for item_id in job.next_item_id..=chunk_end {
        ctx.db
            .stock()
            .insert(generate_stock(config, job.next_warehouse_id, item_id));
    }

    let (phase, next_warehouse_id, next_item_id, next_district_id) = if chunk_end < ITEMS {
        (TpccLoadPhase::Stock, job.next_warehouse_id, chunk_end + 1, 1)
    } else if job.next_warehouse_id < end_warehouse {
        (TpccLoadPhase::Stock, job.next_warehouse_id + 1, 1, 1)
    } else {
        (TpccLoadPhase::CustomersHistory, start_warehouse, 1, 1)
    };

    Ok(ChunkAdvance {
        phase,
        next_warehouse_id,
        next_district_id,
        next_item_id,
        next_order_id: 1,
        rows_inserted: u64::from(chunk_end - job.next_item_id + 1),
        complete: false,
    })
}

fn load_customer_history_chunk(
    ctx: &ReducerContext,
    config: &TpccLoadConfig,
    job: &TpccLoadJob,
) -> Result<ChunkAdvance, String> {
    let _timer = LogStopwatch::new("load_customer_history_chunk");
    let start_warehouse = warehouse_start(config.database_number, config.warehouses_per_database, config.warehouse_id_offset);
    let end_warehouse = warehouse_end(config.database_number, config.warehouses_per_database, config.warehouse_id_offset);
    if job.next_warehouse_id < start_warehouse || job.next_warehouse_id > end_warehouse {
        return Err(format!("invalid customer warehouse cursor {}", job.next_warehouse_id));
    }
    if !(1..=DISTRICTS_PER_WAREHOUSE).contains(&job.next_district_id) {
        return Err(format!("invalid customer district cursor {}", job.next_district_id));
    }

    for c_id in 1..=CUSTOMERS_PER_DISTRICT {
        ctx.db.customer().insert(generate_customer(
            config,
            job.next_warehouse_id,
            job.next_district_id,
            c_id,
        ));
        ctx.db.history().insert(generate_history(
            config,
            job.next_warehouse_id,
            job.next_district_id,
            c_id,
        ));
    }

    let (phase, next_warehouse_id, next_district_id, next_order_id) = advance_district(
        job.next_warehouse_id,
        job.next_district_id,
        start_warehouse,
        end_warehouse,
        TpccLoadPhase::CustomersHistory,
    );
    let (phase, next_warehouse_id, next_district_id, next_order_id) = if phase == TpccLoadPhase::CustomersHistory {
        (phase, next_warehouse_id, next_district_id, next_order_id)
    } else {
        (TpccLoadPhase::Orders, start_warehouse, 1, 1)
    };

    Ok(ChunkAdvance {
        phase,
        next_warehouse_id,
        next_district_id,
        next_item_id: 1,
        next_order_id,
        rows_inserted: u64::from(CUSTOMERS_PER_DISTRICT) * 2,
        complete: false,
    })
}

fn load_order_chunk(ctx: &ReducerContext, config: &TpccLoadConfig, job: &TpccLoadJob) -> Result<ChunkAdvance, String> {
    let _timer = LogStopwatch::new("load_order_chunk");
    let start_warehouse = warehouse_start(config.database_number, config.warehouses_per_database, config.warehouse_id_offset);
    let end_warehouse = warehouse_end(config.database_number, config.warehouses_per_database, config.warehouse_id_offset);
    if job.next_warehouse_id < start_warehouse || job.next_warehouse_id > end_warehouse {
        return Err(format!("invalid order warehouse cursor {}", job.next_warehouse_id));
    }
    if !(1..=DISTRICTS_PER_WAREHOUSE).contains(&job.next_district_id) {
        return Err(format!("invalid order district cursor {}", job.next_district_id));
    }
    if job.next_order_id == 0 || job.next_order_id > CUSTOMERS_PER_DISTRICT {
        return Err(format!("invalid order cursor {}", job.next_order_id));
    }

    let chunk_end = (job.next_order_id + config.batch_size - 1).min(CUSTOMERS_PER_DISTRICT);
    let permutation = customer_permutation(config, job.next_warehouse_id, job.next_district_id);
    let mut rows_inserted = 0u64;

    for o_id in job.next_order_id..=chunk_end {
        let customer_id = permutation[(o_id - 1) as usize];
        let mut order_rng = deterministic_rng(
            config.seed,
            TAG_ORDER,
            &[
                u64::from(job.next_warehouse_id),
                u64::from(job.next_district_id),
                u64::from(o_id),
            ],
        );
        let delivered = o_id < NEW_ORDER_START;
        let order_line_count = order_rng.gen_range(5..=15) as u8;
        ctx.db.oorder().insert(OOrder {
            order_key: crate::pack_order_key(job.next_warehouse_id, job.next_district_id, o_id),
            o_w_id: job.next_warehouse_id,
            o_d_id: job.next_district_id,
            o_id,
            o_c_id: customer_id,
            o_entry_d: config.base_ts,
            o_carrier_id: if delivered {
                Some(order_rng.gen_range(1..=10))
            } else {
                None
            },
            o_ol_cnt: order_line_count,
            o_all_local: true,
        });
        rows_inserted += 1;

        if !delivered {
            ctx.db.new_order_row().insert(NewOrder {
                new_order_key: crate::pack_order_key(job.next_warehouse_id, job.next_district_id, o_id),
                no_w_id: job.next_warehouse_id,
                no_d_id: job.next_district_id,
                no_o_id: o_id,
            });
            rows_inserted += 1;
        }

        for ol_number in 1..=order_line_count {
            let mut line_rng = deterministic_rng(
                config.seed,
                TAG_ORDER_LINE,
                &[
                    u64::from(job.next_warehouse_id),
                    u64::from(job.next_district_id),
                    u64::from(o_id),
                    u64::from(ol_number),
                ],
            );
            ctx.db.order_line().insert(OrderLine {
                order_line_key: pack_order_line_key(job.next_warehouse_id, job.next_district_id, o_id, ol_number),
                ol_w_id: job.next_warehouse_id,
                ol_d_id: job.next_district_id,
                ol_o_id: o_id,
                ol_number,
                ol_i_id: line_rng.gen_range(1..=ITEMS),
                ol_supply_w_id: job.next_warehouse_id,
                ol_delivery_d: if delivered { Some(config.base_ts) } else { None },
                ol_quantity: 5,
                ol_amount_cents: if delivered { 0 } else { line_rng.gen_range(1..=999_999) },
                ol_dist_info: alpha_string(&mut line_rng, 24, 24),
            });
            rows_inserted += 1;
        }
    }

    if chunk_end < CUSTOMERS_PER_DISTRICT {
        return Ok(ChunkAdvance {
            phase: TpccLoadPhase::Orders,
            next_warehouse_id: job.next_warehouse_id,
            next_district_id: job.next_district_id,
            next_item_id: 1,
            next_order_id: chunk_end + 1,
            rows_inserted,
            complete: false,
        });
    }

    let complete = is_last_order_district(job.next_warehouse_id, job.next_district_id, end_warehouse);
    if complete {
        return Ok(ChunkAdvance {
            phase: TpccLoadPhase::Orders,
            next_warehouse_id: end_warehouse,
            next_district_id: DISTRICTS_PER_WAREHOUSE,
            next_item_id: 1,
            next_order_id: CUSTOMERS_PER_DISTRICT,
            rows_inserted,
            complete: true,
        });
    }

    let (_, next_warehouse_id, next_district_id, next_order_id) = advance_district(
        job.next_warehouse_id,
        job.next_district_id,
        start_warehouse,
        end_warehouse,
        TpccLoadPhase::Orders,
    );

    Ok(ChunkAdvance {
        phase: TpccLoadPhase::Orders,
        next_warehouse_id,
        next_district_id,
        next_item_id: 1,
        next_order_id,
        rows_inserted,
        complete: false,
    })
}

fn advance_district(
    warehouse_id: WarehouseId,
    district_id: u8,
    start_warehouse: WarehouseId,
    end_warehouse: WarehouseId,
    phase: TpccLoadPhase,
) -> (TpccLoadPhase, WarehouseId, u8, u32) {
    if district_id < DISTRICTS_PER_WAREHOUSE {
        return (phase, warehouse_id, district_id + 1, 1);
    }
    if warehouse_id < end_warehouse {
        return (phase, warehouse_id + 1, 1, 1);
    }
    (TpccLoadPhase::Orders, start_warehouse, 1, 1)
}

fn is_last_order_district(warehouse_id: WarehouseId, district_id: u8, end_warehouse: WarehouseId) -> bool {
    warehouse_id == end_warehouse && district_id == DISTRICTS_PER_WAREHOUSE
}

fn generate_item(config: &TpccLoadConfig, item_id: u32) -> Item {
    let mut rng = deterministic_rng(config.seed, TAG_ITEM, &[u64::from(item_id)]);
    Item {
        i_id: item_id,
        i_im_id: rng.gen_range(1..=10_000),
        i_name: alpha_numeric_string(&mut rng, 14, 24),
        i_price_cents: rng.gen_range(100..=10_000),
        i_data: maybe_with_original(&mut rng, 26, 50),
    }
}

fn generate_warehouse(config: &TpccLoadConfig, warehouse_id: WarehouseId) -> Warehouse {
    let mut rng = deterministic_rng(config.seed, TAG_WAREHOUSE, &[u64::from(warehouse_id)]);
    Warehouse {
        w_id: warehouse_id,
        w_name: alpha_string(&mut rng, 6, 10),
        w_street_1: alpha_numeric_string(&mut rng, 10, 20),
        w_street_2: alpha_numeric_string(&mut rng, 10, 20),
        w_city: alpha_string(&mut rng, 10, 20),
        w_state: alpha_string(&mut rng, 2, 2),
        w_zip: zip_code(&mut rng),
        w_tax_bps: rng.gen_range(0..=2_000),
        w_ytd_cents: WAREHOUSE_YTD_CENTS,
    }
}

fn generate_district(config: &TpccLoadConfig, warehouse_id: WarehouseId, district_id: u8) -> District {
    let mut rng = deterministic_rng(
        config.seed,
        TAG_DISTRICT,
        &[u64::from(warehouse_id), u64::from(district_id)],
    );
    District {
        district_key: crate::pack_district_key(warehouse_id, district_id),
        d_w_id: warehouse_id,
        d_id: district_id,
        d_name: alpha_string(&mut rng, 6, 10),
        d_street_1: alpha_numeric_string(&mut rng, 10, 20),
        d_street_2: alpha_numeric_string(&mut rng, 10, 20),
        d_city: alpha_string(&mut rng, 10, 20),
        d_state: alpha_string(&mut rng, 2, 2),
        d_zip: zip_code(&mut rng),
        d_tax_bps: rng.gen_range(0..=2_000),
        d_ytd_cents: DISTRICT_YTD_CENTS,
        d_next_o_id: CUSTOMERS_PER_DISTRICT + 1,
    }
}

fn generate_stock(config: &TpccLoadConfig, warehouse_id: WarehouseId, item_id: u32) -> Stock {
    let mut rng = deterministic_rng(config.seed, TAG_STOCK, &[u64::from(warehouse_id), u64::from(item_id)]);
    Stock {
        stock_key: crate::pack_stock_key(warehouse_id, item_id),
        s_w_id: warehouse_id,
        s_i_id: item_id,
        s_quantity: rng.gen_range(10..=100),
        s_dist_01: alpha_string(&mut rng, 24, 24),
        s_dist_02: alpha_string(&mut rng, 24, 24),
        s_dist_03: alpha_string(&mut rng, 24, 24),
        s_dist_04: alpha_string(&mut rng, 24, 24),
        s_dist_05: alpha_string(&mut rng, 24, 24),
        s_dist_06: alpha_string(&mut rng, 24, 24),
        s_dist_07: alpha_string(&mut rng, 24, 24),
        s_dist_08: alpha_string(&mut rng, 24, 24),
        s_dist_09: alpha_string(&mut rng, 24, 24),
        s_dist_10: alpha_string(&mut rng, 24, 24),
        s_ytd: 0,
        s_order_cnt: 0,
        s_remote_cnt: 0,
        s_data: maybe_with_original(&mut rng, 26, 50),
    }
}

fn generate_customer(
    config: &TpccLoadConfig,
    warehouse_id: WarehouseId,
    district_id: u8,
    customer_id: u32,
) -> Customer {
    let mut rng = deterministic_rng(
        config.seed,
        TAG_CUSTOMER,
        &[u64::from(warehouse_id), u64::from(district_id), u64::from(customer_id)],
    );
    let credit = if rng.gen_bool(0.10) { "BC" } else { "GC" };
    let last_name = if customer_id <= 1_000 {
        make_last_name(customer_id - 1)
    } else {
        make_last_name(nurand(&mut rng, 255, 0, 999, config.load_c_last))
    };
    Customer {
        customer_key: crate::pack_customer_key(warehouse_id, district_id, customer_id),
        c_w_id: warehouse_id,
        c_d_id: district_id,
        c_id: customer_id,
        c_first: alpha_string(&mut rng, 8, 16),
        c_middle: "OE".to_string(),
        c_last: last_name,
        c_street_1: alpha_numeric_string(&mut rng, 10, 20),
        c_street_2: alpha_numeric_string(&mut rng, 10, 20),
        c_city: alpha_string(&mut rng, 10, 20),
        c_state: alpha_string(&mut rng, 2, 2),
        c_zip: zip_code(&mut rng),
        c_phone: numeric_string(&mut rng, 16, 16),
        c_since: config.base_ts,
        c_credit: credit.to_string(),
        c_credit_lim_cents: CUSTOMER_CREDIT_LIMIT_CENTS,
        c_discount_bps: rng.gen_range(0..=5_000),
        c_balance_cents: CUSTOMER_INITIAL_BALANCE_CENTS,
        c_ytd_payment_cents: CUSTOMER_INITIAL_YTD_PAYMENT_CENTS,
        c_payment_cnt: 1,
        c_delivery_cnt: 0,
        c_data: alpha_numeric_string(&mut rng, 300, 500),
    }
}

fn generate_history(config: &TpccLoadConfig, warehouse_id: WarehouseId, district_id: u8, customer_id: u32) -> History {
    let mut rng = deterministic_rng(
        config.seed,
        TAG_HISTORY,
        &[u64::from(warehouse_id), u64::from(district_id), u64::from(customer_id)],
    );
    History {
        history_id: 0,
        h_c_id: customer_id,
        h_c_d_id: district_id,
        h_c_w_id: warehouse_id,
        h_d_id: district_id,
        h_w_id: warehouse_id,
        h_date: config.base_ts,
        h_amount_cents: HISTORY_INITIAL_AMOUNT_CENTS,
        h_data: alpha_numeric_string(&mut rng, 12, 24),
    }
}

fn customer_permutation(config: &TpccLoadConfig, warehouse_id: WarehouseId, district_id: u8) -> Vec<u32> {
    let mut permutation: Vec<u32> = (1..=CUSTOMERS_PER_DISTRICT).collect();
    let mut rng = deterministic_rng(
        config.seed,
        TAG_ORDER_PERMUTATION,
        &[u64::from(warehouse_id), u64::from(district_id)],
    );
    permutation.shuffle(&mut rng);
    permutation
}

fn warehouse_range(
    database_number: u32,
    warehouses_per_database: u32,
    offset: u32,
) -> std::ops::Range<WarehouseId> {
    let start = warehouse_start(database_number, warehouses_per_database, offset);
    let end = start + warehouses_per_database;
    start..end
}

fn warehouse_start(database_number: u32, warehouses_per_database: u32, offset: u32) -> WarehouseId {
    database_number
        .checked_mul(warehouses_per_database)
        .and_then(|value| value.checked_add(offset))
        .and_then(|value| value.checked_add(1))
        .expect("warehouse id arithmetic validated at configure_tpcc_load time")
}

fn warehouse_end(database_number: u32, warehouses_per_database: u32, offset: u32) -> WarehouseId {
    warehouse_start(database_number, warehouses_per_database, offset) + warehouses_per_database - 1
}

fn deterministic_rng(seed: u64, tag: u64, parts: &[u64]) -> StdRng {
    StdRng::seed_from_u64(mix_seed(seed, tag, parts))
}

fn mix_seed(seed: u64, tag: u64, parts: &[u64]) -> u64 {
    let mut value = splitmix64(seed ^ tag);
    for part in parts {
        value = splitmix64(value ^ *part);
    }
    value
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = value;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn nurand(rng: &mut StdRng, a: u32, x: u32, y: u32, c: u32) -> u32 {
    (((rng.gen_range(0..=a) | rng.gen_range(x..=y)) + c) % (y - x + 1)) + x
}

fn alpha_string(rng: &mut StdRng, min_len: usize, max_len: usize) -> String {
    let len = rng.gen_range(min_len..=max_len);
    (0..len).map(|_| (b'A' + rng.gen_range(0..26)) as char).collect()
}

fn numeric_string(rng: &mut StdRng, min_len: usize, max_len: usize) -> String {
    let len = rng.gen_range(min_len..=max_len);
    (0..len).map(|_| (b'0' + rng.gen_range(0..10)) as char).collect()
}

fn alpha_numeric_string(rng: &mut StdRng, min_len: usize, max_len: usize) -> String {
    let len = rng.gen_range(min_len..=max_len);
    (0..len)
        .map(|_| {
            if rng.gen_bool(0.5) {
                (b'A' + rng.gen_range(0..26)) as char
            } else {
                (b'0' + rng.gen_range(0..10)) as char
            }
        })
        .collect()
}

fn zip_code(rng: &mut StdRng) -> String {
    format!("{}11111", numeric_string(rng, 4, 4))
}

fn maybe_with_original(rng: &mut StdRng, min_len: usize, max_len: usize) -> String {
    let mut data = alpha_numeric_string(rng, min_len, max_len);
    if rng.gen_bool(0.10) && data.len() >= 8 {
        let start = rng.gen_range(0..=(data.len() - 8));
        data.replace_range(start..start + 8, "ORIGINAL");
    }
    data
}

fn make_last_name(num: u32) -> String {
    const PARTS: [&str; 10] = [
        "BAR", "OUGHT", "ABLE", "PRI", "PRES", "ESE", "ANTI", "CALLY", "ATION", "EING",
    ];
    let hundreds = ((num / 100) % 10) as usize;
    let tens = ((num / 10) % 10) as usize;
    let ones = (num % 10) as usize;
    format!("{}{}{}", PARTS[hundreds], PARTS[tens], PARTS[ones])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_last_order_district_for_completion() {
        assert!(is_last_order_district(2, DISTRICTS_PER_WAREHOUSE, 2));
        assert!(!is_last_order_district(1, DISTRICTS_PER_WAREHOUSE, 2));
        assert!(!is_last_order_district(2, DISTRICTS_PER_WAREHOUSE - 1, 2));
    }

    #[test]
    fn advance_district_wraps_back_to_start_after_last_district() {
        assert_eq!(
            advance_district(2, DISTRICTS_PER_WAREHOUSE, 1, 2, TpccLoadPhase::Orders),
            (TpccLoadPhase::Orders, 1, 1, 1)
        );
    }
}
