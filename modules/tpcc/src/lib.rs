use spacetimedb::{
    procedure, reducer, table, ProcedureContext, ReducerContext, ScheduleAt, SpacetimeType, Table, Timestamp,
};
use std::collections::BTreeSet;

const DISTRICTS_PER_WAREHOUSE: u8 = 10;
const CUSTOMERS_PER_DISTRICT: u32 = 3_000;
const ITEMS: u32 = 100_000;
const MAX_C_DATA_LEN: usize = 500;
const TAX_SCALE: i64 = 10_000;

macro_rules! ensure {
    ($cond:expr, $($arg:tt)+) => {
        if !($cond) {
            return Err(format!($($arg)+));
        }
    };
}

#[derive(Clone, Debug, SpacetimeType)]
pub enum CustomerSelector {
    ById(u32),
    ByLastName(String),
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct NewOrderLineInput {
    pub item_id: u32,
    pub supply_w_id: u16,
    pub quantity: u32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct NewOrderLineResult {
    pub item_id: u32,
    pub item_name: String,
    pub supply_w_id: u16,
    pub quantity: u32,
    pub stock_quantity: i32,
    pub item_price_cents: i64,
    pub amount_cents: i64,
    pub brand_generic: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct NewOrderResult {
    pub warehouse_tax_bps: i32,
    pub district_tax_bps: i32,
    pub customer_discount_bps: i32,
    pub customer_last: String,
    pub customer_credit: String,
    pub order_id: u32,
    pub entry_d: Timestamp,
    pub total_amount_cents: i64,
    pub all_local: bool,
    pub lines: Vec<NewOrderLineResult>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct PaymentResult {
    pub warehouse_name: String,
    pub district_name: String,
    pub customer_id: u32,
    pub customer_first: String,
    pub customer_middle: String,
    pub customer_last: String,
    pub customer_balance_cents: i64,
    pub customer_credit: String,
    pub customer_discount_bps: i32,
    pub payment_amount_cents: i64,
    pub customer_data: Option<String>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct OrderStatusLineResult {
    pub item_id: u32,
    pub supply_w_id: u16,
    pub quantity: u32,
    pub amount_cents: i64,
    pub delivery_d: Option<Timestamp>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct OrderStatusResult {
    pub customer_id: u32,
    pub customer_first: String,
    pub customer_middle: String,
    pub customer_last: String,
    pub customer_balance_cents: i64,
    pub order_id: Option<u32>,
    pub order_entry_d: Option<Timestamp>,
    pub carrier_id: Option<u8>,
    pub lines: Vec<OrderStatusLineResult>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct StockLevelResult {
    pub warehouse_id: u16,
    pub district_id: u8,
    pub threshold: i32,
    pub low_stock_count: u32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct DeliveryQueueAck {
    pub scheduled_id: u64,
    pub queued_at: Timestamp,
    pub warehouse_id: u16,
    pub carrier_id: u8,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct DeliveryProgress {
    pub run_id: String,
    pub pending_jobs: u64,
    pub completed_jobs: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct DeliveryCompletionView {
    pub completion_id: u64,
    pub run_id: String,
    pub driver_id: String,
    pub terminal_id: u32,
    pub request_id: u64,
    pub warehouse_id: u16,
    pub carrier_id: u8,
    pub queued_at: Timestamp,
    pub completed_at: Timestamp,
    pub skipped_districts: u8,
    pub processed_districts: u8,
}

#[table(accessor = warehouse)]
#[derive(Clone, Debug)]
pub struct Warehouse {
    #[primary_key]
    pub w_id: u16,
    pub w_name: String,
    pub w_street_1: String,
    pub w_street_2: String,
    pub w_city: String,
    pub w_state: String,
    pub w_zip: String,
    pub w_tax_bps: i32,
    pub w_ytd_cents: i64,
}

#[table(
    accessor = district,
    index(accessor = by_w_d, btree(columns = [d_w_id, d_id]))
)]
#[derive(Clone, Debug)]
pub struct District {
    pub d_w_id: u16,
    pub d_id: u8,
    pub d_name: String,
    pub d_street_1: String,
    pub d_street_2: String,
    pub d_city: String,
    pub d_state: String,
    pub d_zip: String,
    pub d_tax_bps: i32,
    pub d_ytd_cents: i64,
    pub d_next_o_id: u32,
}

#[table(
    accessor = customer,
    index(accessor = by_w_d_c_id, btree(columns = [c_w_id, c_d_id, c_id])),
    index(accessor = by_w_d_last_first_id, btree(columns = [c_w_id, c_d_id, c_last, c_first, c_id]))
)]
#[derive(Clone, Debug)]
pub struct Customer {
    pub c_w_id: u16,
    pub c_d_id: u8,
    pub c_id: u32,
    pub c_first: String,
    pub c_middle: String,
    pub c_last: String,
    pub c_street_1: String,
    pub c_street_2: String,
    pub c_city: String,
    pub c_state: String,
    pub c_zip: String,
    pub c_phone: String,
    pub c_since: Timestamp,
    pub c_credit: String,
    pub c_credit_lim_cents: i64,
    pub c_discount_bps: i32,
    pub c_balance_cents: i64,
    pub c_ytd_payment_cents: i64,
    pub c_payment_cnt: u32,
    pub c_delivery_cnt: u32,
    pub c_data: String,
}

#[table(accessor = history)]
#[derive(Clone, Debug)]
pub struct History {
    #[primary_key]
    #[auto_inc]
    pub history_id: u64,
    pub h_c_id: u32,
    pub h_c_d_id: u8,
    pub h_c_w_id: u16,
    pub h_d_id: u8,
    pub h_w_id: u16,
    pub h_date: Timestamp,
    pub h_amount_cents: i64,
    pub h_data: String,
}

#[table(accessor = item)]
#[derive(Clone, Debug)]
pub struct Item {
    #[primary_key]
    pub i_id: u32,
    pub i_im_id: u32,
    pub i_name: String,
    pub i_price_cents: i64,
    pub i_data: String,
}

#[table(
    accessor = stock,
    index(accessor = by_w_i, btree(columns = [s_w_id, s_i_id]))
)]
#[derive(Clone, Debug)]
pub struct Stock {
    pub s_w_id: u16,
    pub s_i_id: u32,
    pub s_quantity: i32,
    pub s_dist_01: String,
    pub s_dist_02: String,
    pub s_dist_03: String,
    pub s_dist_04: String,
    pub s_dist_05: String,
    pub s_dist_06: String,
    pub s_dist_07: String,
    pub s_dist_08: String,
    pub s_dist_09: String,
    pub s_dist_10: String,
    pub s_ytd: u64,
    pub s_order_cnt: u32,
    pub s_remote_cnt: u32,
    pub s_data: String,
}

#[table(
    accessor = oorder,
    index(accessor = by_w_d_o_id, btree(columns = [o_w_id, o_d_id, o_id])),
    index(accessor = by_w_d_c_o_id, btree(columns = [o_w_id, o_d_id, o_c_id, o_id]))
)]
#[derive(Clone, Debug)]
pub struct OOrder {
    pub o_w_id: u16,
    pub o_d_id: u8,
    pub o_id: u32,
    pub o_c_id: u32,
    pub o_entry_d: Timestamp,
    pub o_carrier_id: Option<u8>,
    pub o_ol_cnt: u8,
    pub o_all_local: bool,
}

#[table(
    accessor = new_order_row,
    index(accessor = by_w_d_o_id, btree(columns = [no_w_id, no_d_id, no_o_id]))
)]
#[derive(Clone, Debug)]
pub struct NewOrder {
    pub no_w_id: u16,
    pub no_d_id: u8,
    pub no_o_id: u32,
}

#[table(
    accessor = order_line,
    index(accessor = by_w_d_o_number, btree(columns = [ol_w_id, ol_d_id, ol_o_id, ol_number]))
)]
#[derive(Clone, Debug)]
pub struct OrderLine {
    pub ol_w_id: u16,
    pub ol_d_id: u8,
    pub ol_o_id: u32,
    pub ol_number: u8,
    pub ol_i_id: u32,
    pub ol_supply_w_id: u16,
    pub ol_delivery_d: Option<Timestamp>,
    pub ol_quantity: u32,
    pub ol_amount_cents: i64,
    pub ol_dist_info: String,
}

#[table(
    accessor = delivery_job,
    scheduled(run_delivery_job),
    index(accessor = by_run_id, btree(columns = [run_id]))
)]
#[derive(Clone, Debug)]
pub struct DeliveryJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub run_id: String,
    pub driver_id: String,
    pub terminal_id: u32,
    pub request_id: u64,
    pub queued_at: Timestamp,
    pub w_id: u16,
    pub carrier_id: u8,
    pub next_d_id: u8,
    pub skipped_districts: u8,
    pub processed_districts: u8,
}

#[table(
    accessor = delivery_completion,
    index(accessor = by_run_completion, btree(columns = [run_id, completion_id]))
)]
#[derive(Clone, Debug)]
pub struct DeliveryCompletion {
    #[primary_key]
    #[auto_inc]
    pub completion_id: u64,
    pub run_id: String,
    pub driver_id: String,
    pub terminal_id: u32,
    pub request_id: u64,
    pub warehouse_id: u16,
    pub carrier_id: u8,
    pub queued_at: Timestamp,
    pub completed_at: Timestamp,
    pub skipped_districts: u8,
    pub processed_districts: u8,
}

struct PaymentRequest<'a> {
    w_id: u16,
    d_id: u8,
    c_w_id: u16,
    c_d_id: u8,
    customer_selector: &'a CustomerSelector,
    payment_amount_cents: i64,
    now: Timestamp,
}

#[reducer]
pub fn reset_tpcc(ctx: &ReducerContext) -> Result<(), String> {
    for row in ctx.db.delivery_job().iter() {
        ctx.db.delivery_job().delete(row);
    }
    for row in ctx.db.delivery_completion().iter() {
        ctx.db.delivery_completion().delete(row);
    }
    for row in ctx.db.order_line().iter() {
        ctx.db.order_line().delete(row);
    }
    for row in ctx.db.new_order_row().iter() {
        ctx.db.new_order_row().delete(row);
    }
    for row in ctx.db.oorder().iter() {
        ctx.db.oorder().delete(row);
    }
    for row in ctx.db.history().iter() {
        ctx.db.history().delete(row);
    }
    for row in ctx.db.customer().iter() {
        ctx.db.customer().delete(row);
    }
    for row in ctx.db.district().iter() {
        ctx.db.district().delete(row);
    }
    for row in ctx.db.stock().iter() {
        ctx.db.stock().delete(row);
    }
    for row in ctx.db.item().iter() {
        ctx.db.item().delete(row);
    }
    for row in ctx.db.warehouse().iter() {
        ctx.db.warehouse().delete(row);
    }
    Ok(())
}

#[reducer]
pub fn load_warehouses(ctx: &ReducerContext, rows: Vec<Warehouse>) -> Result<(), String> {
    for row in rows {
        validate_warehouse_row(&row)?;
        ctx.db.warehouse().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_districts(ctx: &ReducerContext, rows: Vec<District>) -> Result<(), String> {
    for row in rows {
        insert_district_checked(ctx, row)?;
    }
    Ok(())
}

#[reducer]
pub fn load_customers(ctx: &ReducerContext, rows: Vec<Customer>) -> Result<(), String> {
    for row in rows {
        insert_customer_checked(ctx, row)?;
    }
    Ok(())
}

#[reducer]
pub fn load_history(ctx: &ReducerContext, rows: Vec<History>) -> Result<(), String> {
    for mut row in rows {
        row.history_id = 0;
        ctx.db.history().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_items(ctx: &ReducerContext, rows: Vec<Item>) -> Result<(), String> {
    for row in rows {
        validate_item_row(&row)?;
        ctx.db.item().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_stocks(ctx: &ReducerContext, rows: Vec<Stock>) -> Result<(), String> {
    for row in rows {
        insert_stock_checked(ctx, row)?;
    }
    Ok(())
}

#[reducer]
pub fn load_orders(ctx: &ReducerContext, rows: Vec<OOrder>) -> Result<(), String> {
    for row in rows {
        insert_order_checked_reducer(ctx, row)?;
    }
    Ok(())
}

#[reducer]
pub fn load_new_orders(ctx: &ReducerContext, rows: Vec<NewOrder>) -> Result<(), String> {
    for row in rows {
        insert_new_order_checked_reducer(ctx, row)?;
    }
    Ok(())
}

#[reducer]
pub fn load_order_lines(ctx: &ReducerContext, rows: Vec<OrderLine>) -> Result<(), String> {
    for row in rows {
        insert_order_line_checked_reducer(ctx, row)?;
    }
    Ok(())
}

#[procedure]
pub fn new_order(
    ctx: &mut ProcedureContext,
    w_id: u16,
    d_id: u8,
    c_id: u32,
    order_lines: Vec<NewOrderLineInput>,
) -> Result<NewOrderResult, String> {
    ctx.try_with_tx(|tx| new_order_tx(tx, w_id, d_id, c_id, order_lines.clone()))
}

#[procedure]
pub fn payment(
    ctx: &mut ProcedureContext,
    w_id: u16,
    d_id: u8,
    c_w_id: u16,
    c_d_id: u8,
    customer: CustomerSelector,
    payment_amount_cents: i64,
) -> Result<PaymentResult, String> {
    let now = ctx.timestamp;
    ctx.try_with_tx(|tx| {
        payment_tx(
            tx,
            PaymentRequest {
                w_id,
                d_id,
                c_w_id,
                c_d_id,
                customer_selector: &customer,
                payment_amount_cents,
                now,
            },
        )
    })
}

#[procedure]
pub fn order_status(
    ctx: &mut ProcedureContext,
    w_id: u16,
    d_id: u8,
    customer: CustomerSelector,
) -> Result<OrderStatusResult, String> {
    ctx.try_with_tx(|tx| order_status_tx(tx, w_id, d_id, &customer))
}

#[procedure]
pub fn stock_level(
    ctx: &mut ProcedureContext,
    w_id: u16,
    d_id: u8,
    threshold: i32,
) -> Result<StockLevelResult, String> {
    ctx.try_with_tx(|tx| stock_level_tx(tx, w_id, d_id, threshold))
}

#[procedure]
pub fn queue_delivery(
    ctx: &mut ProcedureContext,
    run_id: String,
    driver_id: String,
    terminal_id: u32,
    request_id: u64,
    w_id: u16,
    carrier_id: u8,
) -> Result<DeliveryQueueAck, String> {
    let queued_at = ctx.timestamp;
    ctx.try_with_tx(|tx| {
        ensure_warehouse_exists(tx, w_id)?;
        ensure!((1..=10).contains(&carrier_id), "carrier_id must be in the range 1..=10");

        let job = tx.db.delivery_job().insert(DeliveryJob {
            scheduled_id: 0,
            scheduled_at: queued_at.into(),
            run_id: run_id.clone(),
            driver_id: driver_id.clone(),
            terminal_id,
            request_id,
            queued_at,
            w_id,
            carrier_id,
            next_d_id: 1,
            skipped_districts: 0,
            processed_districts: 0,
        });

        Ok(DeliveryQueueAck {
            scheduled_id: job.scheduled_id,
            queued_at,
            warehouse_id: w_id,
            carrier_id,
        })
    })
}

#[procedure]
pub fn delivery_progress(ctx: &mut ProcedureContext, run_id: String) -> Result<DeliveryProgress, String> {
    ctx.try_with_tx(|tx| {
        let pending_jobs = tx.db.delivery_job().by_run_id().filter(&run_id).count() as u64;
        let completed_jobs = tx
            .db
            .delivery_completion()
            .by_run_completion()
            .filter((&run_id, 0u64..))
            .count() as u64;
        Ok(DeliveryProgress {
            run_id: run_id.clone(),
            pending_jobs,
            completed_jobs,
        })
    })
}

#[procedure]
pub fn fetch_delivery_completions(
    ctx: &mut ProcedureContext,
    run_id: String,
    after_completion_id: u64,
    limit: u32,
) -> Result<Vec<DeliveryCompletionView>, String> {
    ctx.try_with_tx(|tx| {
        let limit = limit as usize;
        let rows = tx
            .db
            .delivery_completion()
            .by_run_completion()
            .filter((&run_id, after_completion_id.saturating_add(1)..))
            .take(limit)
            .map(as_delivery_completion_view)
            .collect();
        Ok(rows)
    })
}

#[reducer]
pub fn run_delivery_job(ctx: &ReducerContext, job: DeliveryJob) -> Result<(), String> {
    let mut next_job = job.clone();

    let had_order = process_delivery_district(ctx, job.w_id, job.next_d_id, job.carrier_id, ctx.timestamp)?;
    next_job.processed_districts = next_job.processed_districts.saturating_add(1);
    if !had_order {
        next_job.skipped_districts = next_job.skipped_districts.saturating_add(1);
    }

    let jobs = ctx.db.delivery_job();
    jobs.scheduled_id().delete(job.scheduled_id);

    if job.next_d_id >= DISTRICTS_PER_WAREHOUSE {
        ctx.db.delivery_completion().insert(DeliveryCompletion {
            completion_id: 0,
            run_id: job.run_id,
            driver_id: job.driver_id,
            terminal_id: job.terminal_id,
            request_id: job.request_id,
            warehouse_id: job.w_id,
            carrier_id: job.carrier_id,
            queued_at: job.queued_at,
            completed_at: ctx.timestamp,
            skipped_districts: next_job.skipped_districts,
            processed_districts: next_job.processed_districts,
        });
    } else {
        next_job.next_d_id += 1;
        next_job.scheduled_at = ctx.timestamp.into();
        ctx.db.delivery_job().insert(next_job);
    }

    Ok(())
}

fn validate_warehouse_row(row: &Warehouse) -> Result<(), String> {
    ensure!(
        (1..=i32::from(u16::MAX)).contains(&(row.w_id as i32)),
        "warehouse id must be positive"
    );
    Ok(())
}

fn validate_district_row(row: &District) -> Result<(), String> {
    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&row.d_id),
        "district id out of range"
    );
    Ok(())
}

fn validate_customer_row(row: &Customer) -> Result<(), String> {
    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&row.c_d_id),
        "customer district id out of range"
    );
    ensure!(
        (1..=CUSTOMERS_PER_DISTRICT).contains(&row.c_id),
        "customer id out of range"
    );
    Ok(())
}

fn validate_item_row(row: &Item) -> Result<(), String> {
    ensure!((1..=ITEMS).contains(&row.i_id), "item id out of range");
    Ok(())
}

fn validate_stock_row(row: &Stock) -> Result<(), String> {
    ensure!((1..=ITEMS).contains(&row.s_i_id), "stock item id out of range");
    Ok(())
}

fn validate_order_row(row: &OOrder) -> Result<(), String> {
    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&row.o_d_id),
        "order district id out of range"
    );
    ensure!((5..=15).contains(&row.o_ol_cnt), "order line count out of range");
    Ok(())
}

fn validate_new_order_row(row: &NewOrder) -> Result<(), String> {
    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&row.no_d_id),
        "new-order district id out of range"
    );
    Ok(())
}

fn validate_order_line_row(row: &OrderLine) -> Result<(), String> {
    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&row.ol_d_id),
        "order-line district id out of range"
    );
    ensure!((1..=15).contains(&row.ol_number), "order-line number out of range");
    ensure!(row.ol_quantity > 0, "order-line quantity must be positive");
    Ok(())
}

fn insert_district_checked(ctx: &ReducerContext, row: District) -> Result<(), String> {
    validate_district_row(&row)?;
    ensure!(
        ctx.db
            .district()
            .by_w_d()
            .filter((row.d_w_id, row.d_id))
            .next()
            .is_none(),
        "district ({}, {}) already exists",
        row.d_w_id,
        row.d_id
    );
    ctx.db.district().insert(row);
    Ok(())
}

fn insert_customer_checked(ctx: &ReducerContext, row: Customer) -> Result<(), String> {
    validate_customer_row(&row)?;
    ensure!(
        ctx.db
            .customer()
            .by_w_d_c_id()
            .filter((row.c_w_id, row.c_d_id, row.c_id))
            .next()
            .is_none(),
        "customer ({}, {}, {}) already exists",
        row.c_w_id,
        row.c_d_id,
        row.c_id
    );
    ctx.db.customer().insert(row);
    Ok(())
}

fn insert_stock_checked(ctx: &ReducerContext, row: Stock) -> Result<(), String> {
    validate_stock_row(&row)?;
    ensure!(
        ctx.db
            .stock()
            .by_w_i()
            .filter((row.s_w_id, row.s_i_id))
            .next()
            .is_none(),
        "stock ({}, {}) already exists",
        row.s_w_id,
        row.s_i_id
    );
    ctx.db.stock().insert(row);
    Ok(())
}

fn insert_order_checked_reducer(ctx: &ReducerContext, row: OOrder) -> Result<(), String> {
    validate_order_row(&row)?;
    ensure!(
        ctx.db
            .oorder()
            .by_w_d_o_id()
            .filter((row.o_w_id, row.o_d_id, row.o_id))
            .next()
            .is_none(),
        "order ({}, {}, {}) already exists",
        row.o_w_id,
        row.o_d_id,
        row.o_id
    );
    ctx.db.oorder().insert(row);
    Ok(())
}

fn insert_new_order_checked_reducer(ctx: &ReducerContext, row: NewOrder) -> Result<(), String> {
    validate_new_order_row(&row)?;
    ensure!(
        ctx.db
            .new_order_row()
            .by_w_d_o_id()
            .filter((row.no_w_id, row.no_d_id, row.no_o_id))
            .next()
            .is_none(),
        "new-order ({}, {}, {}) already exists",
        row.no_w_id,
        row.no_d_id,
        row.no_o_id
    );
    ctx.db.new_order_row().insert(row);
    Ok(())
}

fn insert_order_line_checked_reducer(ctx: &ReducerContext, row: OrderLine) -> Result<(), String> {
    validate_order_line_row(&row)?;
    ensure!(
        ctx.db
            .order_line()
            .by_w_d_o_number()
            .filter((row.ol_w_id, row.ol_d_id, row.ol_o_id, row.ol_number))
            .next()
            .is_none(),
        "order-line ({}, {}, {}, {}) already exists",
        row.ol_w_id,
        row.ol_d_id,
        row.ol_o_id,
        row.ol_number
    );
    ctx.db.order_line().insert(row);
    Ok(())
}

fn insert_order_checked_tx(tx: &spacetimedb::TxContext, row: OOrder) -> Result<(), String> {
    validate_order_row(&row)?;
    ensure!(
        tx.db
            .oorder()
            .by_w_d_o_id()
            .filter((row.o_w_id, row.o_d_id, row.o_id))
            .next()
            .is_none(),
        "order ({}, {}, {}) already exists",
        row.o_w_id,
        row.o_d_id,
        row.o_id
    );
    tx.db.oorder().insert(row);
    Ok(())
}

fn insert_new_order_checked_tx(tx: &spacetimedb::TxContext, row: NewOrder) -> Result<(), String> {
    validate_new_order_row(&row)?;
    ensure!(
        tx.db
            .new_order_row()
            .by_w_d_o_id()
            .filter((row.no_w_id, row.no_d_id, row.no_o_id))
            .next()
            .is_none(),
        "new-order ({}, {}, {}) already exists",
        row.no_w_id,
        row.no_d_id,
        row.no_o_id
    );
    tx.db.new_order_row().insert(row);
    Ok(())
}

fn insert_order_line_checked_tx(tx: &spacetimedb::TxContext, row: OrderLine) -> Result<(), String> {
    validate_order_line_row(&row)?;
    ensure!(
        tx.db
            .order_line()
            .by_w_d_o_number()
            .filter((row.ol_w_id, row.ol_d_id, row.ol_o_id, row.ol_number))
            .next()
            .is_none(),
        "order-line ({}, {}, {}, {}) already exists",
        row.ol_w_id,
        row.ol_d_id,
        row.ol_o_id,
        row.ol_number
    );
    tx.db.order_line().insert(row);
    Ok(())
}

fn new_order_tx(
    tx: &spacetimedb::TxContext,
    w_id: u16,
    d_id: u8,
    c_id: u32,
    order_lines: Vec<NewOrderLineInput>,
) -> Result<NewOrderResult, String> {
    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&d_id),
        "district id out of range"
    );
    ensure!(
        (5..=15).contains(&order_lines.len()),
        "new-order requires between 5 and 15 order lines"
    );

    let warehouse = find_warehouse(tx, w_id)?;
    let district = find_district(tx, w_id, d_id)?;
    let customer = find_customer_by_id(tx, w_id, d_id, c_id)?;

    let mut touched_items = Vec::with_capacity(order_lines.len());
    let mut all_local = true;
    for line in &order_lines {
        ensure!(line.quantity > 0, "order line quantity must be positive");
        let item = find_item(tx, line.item_id)?;
        let stock = find_stock(tx, line.supply_w_id, line.item_id)?;
        if line.supply_w_id != w_id {
            all_local = false;
        }
        touched_items.push((line.clone(), item, stock));
    }

    let order_id = district.d_next_o_id;

    replace_district_tx(
        tx,
        district.clone(),
        District {
            d_next_o_id: district.d_next_o_id + 1,
            ..district.clone()
        },
    )?;

    insert_order_checked_tx(
        tx,
        OOrder {
            o_w_id: w_id,
            o_d_id: d_id,
            o_id: order_id,
            o_c_id: c_id,
            o_entry_d: tx.timestamp,
            o_carrier_id: None,
            o_ol_cnt: order_lines.len() as u8,
            o_all_local: all_local,
        },
    )?;

    insert_new_order_checked_tx(
        tx,
        NewOrder {
            no_w_id: w_id,
            no_d_id: d_id,
            no_o_id: order_id,
        },
    )?;

    let mut line_results = Vec::with_capacity(touched_items.len());
    let mut subtotal_cents = 0i64;
    for (idx, (line, item, stock)) in touched_items.into_iter().enumerate() {
        let updated_stock_quantity = adjust_stock_quantity(stock.s_quantity, line.quantity as i32);
        replace_stock_tx(
            tx,
            stock.clone(),
            Stock {
                s_quantity: updated_stock_quantity,
                s_ytd: stock.s_ytd + u64::from(line.quantity),
                s_order_cnt: stock.s_order_cnt + 1,
                s_remote_cnt: stock.s_remote_cnt + u32::from(line.supply_w_id != w_id),
                ..stock.clone()
            },
        )?;

        let line_amount_cents = item.i_price_cents * i64::from(line.quantity);
        subtotal_cents += line_amount_cents;
        let dist_info = district_stock_info(&stock, d_id);
        insert_order_line_checked_tx(
            tx,
            OrderLine {
                ol_w_id: w_id,
                ol_d_id: d_id,
                ol_o_id: order_id,
                ol_number: (idx + 1) as u8,
                ol_i_id: line.item_id,
                ol_supply_w_id: line.supply_w_id,
                ol_delivery_d: None,
                ol_quantity: line.quantity,
                ol_amount_cents: line_amount_cents,
                ol_dist_info: dist_info,
            },
        )?;

        let brand_generic = if contains_original(&item.i_data) && contains_original(&stock.s_data) {
            "B"
        } else {
            "G"
        };
        line_results.push(NewOrderLineResult {
            item_id: item.i_id,
            item_name: item.i_name,
            supply_w_id: line.supply_w_id,
            quantity: line.quantity,
            stock_quantity: updated_stock_quantity,
            item_price_cents: item.i_price_cents,
            amount_cents: line_amount_cents,
            brand_generic: brand_generic.to_string(),
        });
    }

    let taxed = apply_tax(
        subtotal_cents,
        i64::from(warehouse.w_tax_bps) + i64::from(district.d_tax_bps),
    );
    let total_amount_cents = apply_discount(taxed, i64::from(customer.c_discount_bps));

    Ok(NewOrderResult {
        warehouse_tax_bps: warehouse.w_tax_bps,
        district_tax_bps: district.d_tax_bps,
        customer_discount_bps: customer.c_discount_bps,
        customer_last: customer.c_last,
        customer_credit: customer.c_credit,
        order_id,
        entry_d: tx.timestamp,
        total_amount_cents,
        all_local,
        lines: line_results,
    })
}

fn payment_tx(tx: &spacetimedb::TxContext, req: PaymentRequest<'_>) -> Result<PaymentResult, String> {
    ensure!(req.payment_amount_cents > 0, "payment amount must be positive");

    let warehouse = find_warehouse(tx, req.w_id)?;
    let district = find_district(tx, req.w_id, req.d_id)?;
    let customer = resolve_customer(tx, req.c_w_id, req.c_d_id, req.customer_selector)?;

    tx.db.warehouse().w_id().update(Warehouse {
        w_ytd_cents: warehouse.w_ytd_cents + req.payment_amount_cents,
        ..warehouse.clone()
    });

    replace_district_tx(
        tx,
        district.clone(),
        District {
            d_ytd_cents: district.d_ytd_cents + req.payment_amount_cents,
            ..district.clone()
        },
    )?;

    let mut updated_customer = Customer {
        c_balance_cents: customer.c_balance_cents - req.payment_amount_cents,
        c_ytd_payment_cents: customer.c_ytd_payment_cents + req.payment_amount_cents,
        c_payment_cnt: customer.c_payment_cnt + 1,
        ..customer.clone()
    };

    if updated_customer.c_credit == "BC" {
        let prefix = format!(
            "{} {} {} {} {} {} {}|",
            updated_customer.c_id,
            updated_customer.c_d_id,
            updated_customer.c_w_id,
            req.d_id,
            req.w_id,
            req.payment_amount_cents,
            req.now.to_micros_since_unix_epoch()
        );
        updated_customer.c_data = format!("{prefix}{}", updated_customer.c_data);
        updated_customer.c_data.truncate(MAX_C_DATA_LEN);
    }

    replace_customer_tx(tx, customer.clone(), updated_customer.clone())?;

    tx.db.history().insert(History {
        history_id: 0,
        h_c_id: updated_customer.c_id,
        h_c_d_id: updated_customer.c_d_id,
        h_c_w_id: updated_customer.c_w_id,
        h_d_id: req.d_id,
        h_w_id: req.w_id,
        h_date: req.now,
        h_amount_cents: req.payment_amount_cents,
        h_data: format!("{}    {}", warehouse.w_name, district.d_name),
    });

    Ok(PaymentResult {
        warehouse_name: warehouse.w_name,
        district_name: district.d_name,
        customer_id: updated_customer.c_id,
        customer_first: updated_customer.c_first,
        customer_middle: updated_customer.c_middle,
        customer_last: updated_customer.c_last,
        customer_balance_cents: updated_customer.c_balance_cents,
        customer_credit: updated_customer.c_credit.clone(),
        customer_discount_bps: updated_customer.c_discount_bps,
        payment_amount_cents: req.payment_amount_cents,
        customer_data: if updated_customer.c_credit == "BC" {
            Some(updated_customer.c_data)
        } else {
            None
        },
    })
}

fn order_status_tx(
    tx: &spacetimedb::TxContext,
    w_id: u16,
    d_id: u8,
    customer_selector: &CustomerSelector,
) -> Result<OrderStatusResult, String> {
    let customer = resolve_customer(tx, w_id, d_id, customer_selector)?;

    let mut latest_order: Option<OOrder> = None;
    for row in tx
        .db
        .oorder()
        .by_w_d_c_o_id()
        .filter((w_id, d_id, customer.c_id, 0u32..))
    {
        latest_order = Some(row);
    }

    let mut lines = Vec::new();
    if let Some(order) = &latest_order {
        for line in tx
            .db
            .order_line()
            .by_w_d_o_number()
            .filter((w_id, d_id, order.o_id, 0u8..))
        {
            lines.push(OrderStatusLineResult {
                item_id: line.ol_i_id,
                supply_w_id: line.ol_supply_w_id,
                quantity: line.ol_quantity,
                amount_cents: line.ol_amount_cents,
                delivery_d: line.ol_delivery_d,
            });
        }
    }

    Ok(OrderStatusResult {
        customer_id: customer.c_id,
        customer_first: customer.c_first,
        customer_middle: customer.c_middle,
        customer_last: customer.c_last,
        customer_balance_cents: customer.c_balance_cents,
        order_id: latest_order.as_ref().map(|row| row.o_id),
        order_entry_d: latest_order.as_ref().map(|row| row.o_entry_d),
        carrier_id: latest_order.as_ref().and_then(|row| row.o_carrier_id),
        lines,
    })
}

fn stock_level_tx(
    tx: &spacetimedb::TxContext,
    w_id: u16,
    d_id: u8,
    threshold: i32,
) -> Result<StockLevelResult, String> {
    let district = find_district(tx, w_id, d_id)?;
    let start_o_id = district.d_next_o_id.saturating_sub(20);
    let end_o_id = district.d_next_o_id;

    let mut item_ids = BTreeSet::new();
    for line in tx
        .db
        .order_line()
        .by_w_d_o_number()
        .filter((w_id, d_id, start_o_id..end_o_id))
    {
        item_ids.insert(line.ol_i_id);
    }

    let mut low_stock_count = 0u32;
    for item_id in item_ids {
        let stock = find_stock(tx, w_id, item_id)?;
        if stock.s_quantity < threshold {
            low_stock_count += 1;
        }
    }

    Ok(StockLevelResult {
        warehouse_id: w_id,
        district_id: d_id,
        threshold,
        low_stock_count,
    })
}

fn process_delivery_district(
    ctx: &ReducerContext,
    w_id: u16,
    d_id: u8,
    carrier_id: u8,
    delivered_at: Timestamp,
) -> Result<bool, String> {
    let maybe_new_order = ctx.db.new_order_row().by_w_d_o_id().filter((w_id, d_id, 0u32..)).next();
    let Some(new_order) = maybe_new_order else {
        return Ok(false);
    };

    let order = find_order_by_id_reducer(ctx, w_id, d_id, new_order.no_o_id)?;

    ctx.db.new_order_row().delete(new_order);
    replace_order_reducer(
        ctx,
        order.clone(),
        OOrder {
            o_carrier_id: Some(carrier_id),
            ..order.clone()
        },
    )?;

    let mut total_amount_cents = 0i64;
    let order_lines: Vec<_> = ctx
        .db
        .order_line()
        .by_w_d_o_number()
        .filter((w_id, d_id, order.o_id, 0u8..))
        .collect();
    for line in order_lines {
        total_amount_cents += line.ol_amount_cents;
        replace_order_line_reducer(
            ctx,
            line.clone(),
            OrderLine {
                ol_delivery_d: Some(delivered_at),
                ..line
            },
        )?;
    }

    let customer = find_customer_by_id_reducer(ctx, w_id, d_id, order.o_c_id)?;
    replace_customer_reducer(
        ctx,
        customer.clone(),
        Customer {
            c_balance_cents: customer.c_balance_cents + total_amount_cents,
            c_delivery_cnt: customer.c_delivery_cnt + 1,
            ..customer
        },
    )?;

    Ok(true)
}

fn resolve_customer(
    tx: &spacetimedb::TxContext,
    w_id: u16,
    d_id: u8,
    selector: &CustomerSelector,
) -> Result<Customer, String> {
    match selector {
        CustomerSelector::ById(id) => find_customer_by_id(tx, w_id, d_id, *id),
        CustomerSelector::ByLastName(last_name) => {
            let rows: Vec<_> = tx
                .db
                .customer()
                .by_w_d_last_first_id()
                .filter((w_id, d_id, last_name.as_str(), ""..))
                .collect();
            ensure!(!rows.is_empty(), "customer not found");
            Ok(rows[(rows.len() - 1) / 2].clone())
        }
    }
}

fn find_warehouse(tx: &spacetimedb::TxContext, w_id: u16) -> Result<Warehouse, String> {
    tx.db
        .warehouse()
        .w_id()
        .find(w_id)
        .ok_or_else(|| format!("warehouse {w_id} not found"))
}

fn ensure_warehouse_exists(tx: &spacetimedb::TxContext, w_id: u16) -> Result<(), String> {
    find_warehouse(tx, w_id).map(|_| ())
}

fn find_district(tx: &spacetimedb::TxContext, w_id: u16, d_id: u8) -> Result<District, String> {
    tx.db
        .district()
        .by_w_d()
        .filter((w_id, d_id))
        .next()
        .ok_or_else(|| format!("district ({w_id}, {d_id}) not found"))
}

fn find_customer_by_id(tx: &spacetimedb::TxContext, w_id: u16, d_id: u8, c_id: u32) -> Result<Customer, String> {
    tx.db
        .customer()
        .by_w_d_c_id()
        .filter((w_id, d_id, c_id))
        .next()
        .ok_or_else(|| format!("customer ({w_id}, {d_id}, {c_id}) not found"))
}

fn find_customer_by_id_reducer(ctx: &ReducerContext, w_id: u16, d_id: u8, c_id: u32) -> Result<Customer, String> {
    ctx.db
        .customer()
        .by_w_d_c_id()
        .filter((w_id, d_id, c_id))
        .next()
        .ok_or_else(|| format!("customer ({w_id}, {d_id}, {c_id}) not found"))
}

fn find_order_by_id_reducer(ctx: &ReducerContext, w_id: u16, d_id: u8, o_id: u32) -> Result<OOrder, String> {
    ctx.db
        .oorder()
        .by_w_d_o_id()
        .filter((w_id, d_id, o_id))
        .next()
        .ok_or_else(|| format!("order ({w_id}, {d_id}, {o_id}) not found"))
}

fn find_item(tx: &spacetimedb::TxContext, item_id: u32) -> Result<Item, String> {
    tx.db
        .item()
        .i_id()
        .find(item_id)
        .ok_or_else(|| format!("item {item_id} not found"))
}

fn find_stock(tx: &spacetimedb::TxContext, w_id: u16, item_id: u32) -> Result<Stock, String> {
    tx.db
        .stock()
        .by_w_i()
        .filter((w_id, item_id))
        .next()
        .ok_or_else(|| format!("stock ({w_id}, {item_id}) not found"))
}

fn replace_district_tx(tx: &spacetimedb::TxContext, old: District, new: District) -> Result<(), String> {
    ensure!(
        old.d_w_id == new.d_w_id && old.d_id == new.d_id,
        "district identity cannot change during update"
    );
    tx.db.district().delete(old);
    tx.db.district().insert(new);
    Ok(())
}

fn replace_customer_tx(tx: &spacetimedb::TxContext, old: Customer, new: Customer) -> Result<(), String> {
    ensure!(
        old.c_w_id == new.c_w_id && old.c_d_id == new.c_d_id && old.c_id == new.c_id,
        "customer identity cannot change during update"
    );
    tx.db.customer().delete(old);
    tx.db.customer().insert(new);
    Ok(())
}

fn replace_stock_tx(tx: &spacetimedb::TxContext, old: Stock, new: Stock) -> Result<(), String> {
    ensure!(
        old.s_w_id == new.s_w_id && old.s_i_id == new.s_i_id,
        "stock identity cannot change during update"
    );
    tx.db.stock().delete(old);
    tx.db.stock().insert(new);
    Ok(())
}

fn replace_customer_reducer(ctx: &ReducerContext, old: Customer, new: Customer) -> Result<(), String> {
    ensure!(
        old.c_w_id == new.c_w_id && old.c_d_id == new.c_d_id && old.c_id == new.c_id,
        "customer identity cannot change during update"
    );
    ctx.db.customer().delete(old);
    ctx.db.customer().insert(new);
    Ok(())
}

fn replace_order_reducer(ctx: &ReducerContext, old: OOrder, new: OOrder) -> Result<(), String> {
    ensure!(
        old.o_w_id == new.o_w_id && old.o_d_id == new.o_d_id && old.o_id == new.o_id,
        "order identity cannot change during update"
    );
    ctx.db.oorder().delete(old);
    ctx.db.oorder().insert(new);
    Ok(())
}

fn replace_order_line_reducer(ctx: &ReducerContext, old: OrderLine, new: OrderLine) -> Result<(), String> {
    ensure!(
        old.ol_w_id == new.ol_w_id
            && old.ol_d_id == new.ol_d_id
            && old.ol_o_id == new.ol_o_id
            && old.ol_number == new.ol_number,
        "order-line identity cannot change during update"
    );
    ctx.db.order_line().delete(old);
    ctx.db.order_line().insert(new);
    Ok(())
}

fn district_stock_info(stock: &Stock, d_id: u8) -> String {
    match d_id {
        1 => stock.s_dist_01.clone(),
        2 => stock.s_dist_02.clone(),
        3 => stock.s_dist_03.clone(),
        4 => stock.s_dist_04.clone(),
        5 => stock.s_dist_05.clone(),
        6 => stock.s_dist_06.clone(),
        7 => stock.s_dist_07.clone(),
        8 => stock.s_dist_08.clone(),
        9 => stock.s_dist_09.clone(),
        10 => stock.s_dist_10.clone(),
        _ => String::new(),
    }
}

fn contains_original(data: &str) -> bool {
    data.contains("ORIGINAL")
}

fn adjust_stock_quantity(current_quantity: i32, ordered_quantity: i32) -> i32 {
    if current_quantity - ordered_quantity >= 10 {
        current_quantity - ordered_quantity
    } else {
        current_quantity - ordered_quantity + 91
    }
}

fn apply_tax(amount_cents: i64, total_tax_bps: i64) -> i64 {
    amount_cents * (TAX_SCALE + total_tax_bps) / TAX_SCALE
}

fn apply_discount(amount_cents: i64, discount_bps: i64) -> i64 {
    amount_cents * (TAX_SCALE - discount_bps) / TAX_SCALE
}

fn as_delivery_completion_view(row: DeliveryCompletion) -> DeliveryCompletionView {
    DeliveryCompletionView {
        completion_id: row.completion_id,
        run_id: row.run_id,
        driver_id: row.driver_id,
        terminal_id: row.terminal_id,
        request_id: row.request_id,
        warehouse_id: row.warehouse_id,
        carrier_id: row.carrier_id,
        queued_at: row.queued_at,
        completed_at: row.completed_at,
        skipped_districts: row.skipped_districts,
        processed_districts: row.processed_districts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middle_customer_selection_uses_lower_middle_for_even_count() {
        let idx = (4usize - 1) / 2;
        assert_eq!(idx, 1);
    }

    #[test]
    fn stock_quantity_wraps_like_tpcc() {
        assert_eq!(adjust_stock_quantity(20, 5), 15);
        assert_eq!(adjust_stock_quantity(10, 5), 96);
    }
}
