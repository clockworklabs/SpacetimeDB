use remote::reset_remote_warehouses;
use spacetimedb::{
    log_stopwatch::LogStopwatch, reducer, table, ReducerContext, ScheduleAt, SpacetimeType, Table, Timestamp,
};
use std::collections::BTreeSet;

macro_rules! ensure {
    ($cond:expr, $($arg:tt)+) => {
        if !($cond) {
            return Err(format!($($arg)+));
        }
    };
}

mod new_order;
mod payment;
mod remote;

const DISTRICTS_PER_WAREHOUSE: u8 = 10;
const CUSTOMERS_PER_DISTRICT: u32 = 3_000;
const ITEMS: u32 = 100_000;
const MAX_C_DATA_LEN: usize = 500;
const TAX_SCALE: i64 = 10_000;

#[derive(Clone, Debug, SpacetimeType)]
pub enum CustomerSelector {
    ById(u32),
    ByLastName(String),
}

type WarehouseId = u16;

#[derive(Clone, Debug, SpacetimeType)]
pub struct OrderStatusLineResult {
    pub item_id: u32,
    pub supply_w_id: WarehouseId,
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
    pub warehouse_id: WarehouseId,
    pub district_id: u8,
    pub threshold: i32,
    pub low_stock_count: u32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct DeliveryQueueAck {
    pub scheduled_id: u64,
    pub queued_at: Timestamp,
    pub warehouse_id: WarehouseId,
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
    pub warehouse_id: WarehouseId,
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
    pub w_id: WarehouseId,
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
    #[primary_key]
    pub district_key: u32,
    pub d_w_id: WarehouseId,
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
    #[primary_key]
    pub customer_key: u64,
    pub c_w_id: WarehouseId,
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
    pub h_c_w_id: WarehouseId,
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
    #[primary_key]
    pub stock_key: u64,
    pub s_w_id: WarehouseId,
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
    #[primary_key]
    pub order_key: u64,
    pub o_w_id: WarehouseId,
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
    #[primary_key]
    pub new_order_key: u64,
    pub no_w_id: WarehouseId,
    pub no_d_id: u8,
    pub no_o_id: u32,
}

#[table(
    accessor = order_line,
    index(accessor = by_w_d_o_number, btree(columns = [ol_w_id, ol_d_id, ol_o_id, ol_number]))
)]
#[derive(Clone, Debug)]
pub struct OrderLine {
    #[primary_key]
    pub order_line_key: u64,
    pub ol_w_id: WarehouseId,
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
    pub w_id: WarehouseId,
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
    pub warehouse_id: WarehouseId,
    pub carrier_id: u8,
    pub queued_at: Timestamp,
    pub completed_at: Timestamp,
    pub skipped_districts: u8,
    pub processed_districts: u8,
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
    reset_remote_warehouses(ctx);
    Ok(())
}

#[reducer]
pub fn load_warehouses(ctx: &ReducerContext, rows: Vec<Warehouse>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_warehouses");
    for row in rows {
        validate_warehouse_row(&row)?;
        ctx.db.warehouse().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_districts(ctx: &ReducerContext, rows: Vec<District>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_districts");
    for row in rows {
        validate_district_row(&row)?;
        ctx.db.district().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_customers(ctx: &ReducerContext, rows: Vec<Customer>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_customers");
    for row in rows {
        validate_customer_row(&row)?;
        ctx.db.customer().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_history(ctx: &ReducerContext, rows: Vec<History>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_history");
    for mut row in rows {
        row.history_id = 0;
        ctx.db.history().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_items(ctx: &ReducerContext, rows: Vec<Item>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_items");
    for row in rows {
        validate_item_row(&row)?;
        ctx.db.item().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_stocks(ctx: &ReducerContext, rows: Vec<Stock>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_stocks");
    for row in rows {
        validate_stock_row(&row)?;
        ctx.db.stock().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_orders(ctx: &ReducerContext, rows: Vec<OOrder>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_orders");
    for row in rows {
        ctx.db.oorder().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_new_orders(ctx: &ReducerContext, rows: Vec<NewOrder>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_new_orders");
    for row in rows {
        ctx.db.new_order_row().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_order_lines(ctx: &ReducerContext, rows: Vec<OrderLine>) -> Result<(), String> {
    let _timer = LogStopwatch::new("load_order_lines");
    for row in rows {
        ctx.db.order_line().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn order_status(
    ctx: &ReducerContext,
    w_id: u16,
    d_id: u8,
    customer: CustomerSelector,
) -> Result<OrderStatusResult, String> {
    let _timer = LogStopwatch::new("order_status");

    let customer = resolve_customer(ctx, w_id, d_id, &customer)?;

    let mut latest_order: Option<OOrder> = None;
    for row in ctx
        .db
        .oorder()
        .by_w_d_c_o_id()
        .filter((w_id, d_id, customer.c_id, 0u32..))
    {
        latest_order = Some(row);
    }

    let mut lines = Vec::new();
    if let Some(order) = &latest_order {
        for line in ctx
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

#[reducer]
pub fn stock_level(ctx: &ReducerContext, w_id: u16, d_id: u8, threshold: i32) -> Result<StockLevelResult, String> {
    let _timer = LogStopwatch::new("stock_level");

    let district = find_district(ctx, w_id, d_id)?;
    let start_o_id = district.d_next_o_id.saturating_sub(20);
    let end_o_id = district.d_next_o_id;

    let mut item_ids = BTreeSet::new();
    for line in ctx
        .db
        .order_line()
        .by_w_d_o_number()
        .filter((w_id, d_id, start_o_id..end_o_id))
    {
        item_ids.insert(line.ol_i_id);
    }

    let mut low_stock_count = 0u32;
    for item_id in item_ids {
        let stock = find_stock(ctx, w_id, item_id)?;
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

#[reducer]
pub fn queue_delivery(
    ctx: &ReducerContext,
    run_id: String,
    driver_id: String,
    terminal_id: u32,
    request_id: u64,
    w_id: u16,
    carrier_id: u8,
) -> Result<DeliveryQueueAck, String> {
    let _timer = LogStopwatch::new("queue_delivery");

    let queued_at = ctx.timestamp;

    ensure_warehouse_exists(ctx, w_id)?;
    ensure!((1..=10).contains(&carrier_id), "carrier_id must be in the range 1..=10");

    let job = ctx.db.delivery_job().insert(DeliveryJob {
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
}

#[reducer]
pub fn delivery_progress(ctx: &ReducerContext, run_id: String) -> Result<DeliveryProgress, String> {
    let _timer = LogStopwatch::new("delivery_progress");
    let pending_jobs = ctx.db.delivery_job().by_run_id().filter(&run_id).count() as u64;
    let completed_jobs = ctx
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
}

#[reducer]
pub fn fetch_delivery_completions(
    ctx: &ReducerContext,
    run_id: String,
    after_completion_id: u64,
    limit: u32,
) -> Result<Vec<DeliveryCompletionView>, String> {
    let _timer = LogStopwatch::new("fetch_delivery_completions");

    let limit = limit as usize;
    let rows = ctx
        .db
        .delivery_completion()
        .by_run_completion()
        .filter((&run_id, after_completion_id.saturating_add(1)..))
        .take(limit)
        .map(as_delivery_completion_view)
        .collect();
    Ok(rows)
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
        next_job.scheduled_id = 0;
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
        row.district_key == pack_district_key(row.d_w_id, row.d_id),
        "district row has mismatched packed key"
    );
    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&row.d_id),
        "district id out of range"
    );
    Ok(())
}

fn validate_customer_row(row: &Customer) -> Result<(), String> {
    ensure!(
        row.customer_key == pack_customer_key(row.c_w_id, row.c_d_id, row.c_id),
        "customer row has mismatched packed key"
    );
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
    ensure!(
        row.stock_key == pack_stock_key(row.s_w_id, row.s_i_id),
        "stock row has mismatched packed key"
    );
    ensure!((1..=ITEMS).contains(&row.s_i_id), "stock item id out of range");
    Ok(())
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

    let order_key = pack_order_key(w_id, d_id, new_order.no_o_id);
    let order = ctx
        .db
        .oorder()
        .order_key()
        .find(order_key)
        .ok_or_else(|| "delivery referenced missing order".to_string())?;

    ctx.db.new_order_row().new_order_key().delete(new_order.new_order_key);
    ctx.db.oorder().order_key().update(OOrder {
        o_carrier_id: Some(carrier_id),
        ..order.clone()
    });

    let mut total_amount_cents = 0i64;
    let order_lines: Vec<_> = ctx
        .db
        .order_line()
        .by_w_d_o_number()
        .filter((w_id, d_id, order.o_id, 0u8..))
        .collect();
    for line in order_lines {
        total_amount_cents += line.ol_amount_cents;
        ctx.db.order_line().order_line_key().update(OrderLine {
            ol_delivery_d: Some(delivered_at),
            ..line
        });
    }

    let customer = find_customer_by_id(ctx, w_id, d_id, order.o_c_id)?;
    ctx.db.customer().customer_key().update(Customer {
        c_balance_cents: customer.c_balance_cents + total_amount_cents,
        c_delivery_cnt: customer.c_delivery_cnt + 1,
        ..customer
    });

    Ok(true)
}

fn resolve_customer(tx: &ReducerContext, w_id: u16, d_id: u8, selector: &CustomerSelector) -> Result<Customer, String> {
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

fn find_warehouse(tx: &ReducerContext, w_id: u16) -> Result<Warehouse, String> {
    tx.db
        .warehouse()
        .w_id()
        .find(w_id)
        .ok_or_else(|| format!("warehouse {w_id} not found"))
}

fn ensure_warehouse_exists(tx: &ReducerContext, w_id: u16) -> Result<(), String> {
    find_warehouse(tx, w_id).map(|_| ())
}

fn find_district(tx: &ReducerContext, w_id: u16, d_id: u8) -> Result<District, String> {
    tx.db
        .district()
        .by_w_d()
        .filter((w_id, d_id))
        .next()
        .ok_or_else(|| format!("district ({w_id}, {d_id}) not found"))
}

fn find_customer_by_id(tx: &ReducerContext, w_id: u16, d_id: u8, c_id: u32) -> Result<Customer, String> {
    tx.db
        .customer()
        .by_w_d_c_id()
        .filter((w_id, d_id, c_id))
        .next()
        .ok_or_else(|| format!("customer ({w_id}, {d_id}, {c_id}) not found"))
}

fn find_stock(tx: &ReducerContext, w_id: u16, item_id: u32) -> Result<Stock, String> {
    tx.db
        .stock()
        .by_w_i()
        .filter((w_id, item_id))
        .next()
        .ok_or_else(|| format!("stock ({w_id}, {item_id}) not found"))
}

fn pack_district_key(w_id: u16, d_id: u8) -> u32 {
    (u32::from(w_id) * 100) + u32::from(d_id)
}

fn pack_customer_key(w_id: u16, d_id: u8, c_id: u32) -> u64 {
    ((u64::from(w_id) * 100) + u64::from(d_id)) * 10_000 + u64::from(c_id)
}

fn pack_stock_key(w_id: u16, item_id: u32) -> u64 {
    u64::from(w_id) * 1_000_000 + u64::from(item_id)
}

fn pack_order_key(w_id: u16, d_id: u8, o_id: u32) -> u64 {
    ((u64::from(w_id) * 100) + u64::from(d_id)) * 10_000_000 + u64::from(o_id)
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

mod test {
    use spacetimedb::{procedure, ProcedureContext};

    use crate::new_order::{adjust_stock_quantity, pack_order_line_key};

    use super::*;

    #[procedure]
    fn test(_ctx: &mut ProcedureContext) -> Result<String, String> {
        let mut errors = vec![];

        macro_rules! test_fail {
            ($reason:expr) => {
                errors.push($reason);
            };
        }

        #[allow(unused)]
        macro_rules! test_assert {
            ($test_name:literal, $condition:expr) => {
                let condition = $condition;
                if !condition {
                    test_fail!(format!(
                        "{}: {} returned false",
                        $test_name,
                        stringify!($condition)
                    ));
                }
            };
        }

        macro_rules! test_assert_eq {
            ($test_name:literal, $lhs:expr, $rhs:expr) => {
                let lhs = $lhs;
                let rhs = $rhs;
                let condition = lhs == rhs;
                if !condition {
                    test_fail!(format!(
                        "{}: expected {} == {}, but got:
{} => {},
{} => {}",
                        $test_name,
                        stringify!($lhs),
                        stringify!($rhs),
                        stringify!($lhs),
                        lhs,
                        stringify!($rhs),
                        rhs,
                    ));
                }
            };
        }

        macro_rules! test_assert_lt {
            ($test_name:literal, $lhs:expr, $rhs:expr) => {
                let lhs = $lhs;
                let rhs = $rhs;
                let condition = lhs < rhs;
                if !condition {
                    test_fail!(format!(
                        "{}: expected {} < {}, but got:
{} => {},
{} => {}",
                        $test_name,
                        stringify!($lhs),
                        stringify!($rhs),
                        stringify!($lhs),
                        lhs,
                        stringify!($rhs),
                        rhs,
                    ));
                }
            };
        }

        let idx = (4usize - 1) / 2;
        test_assert_eq!("middle_customer_selection_uses_lower_middle_for_even_count", idx, 1);

        test_assert_eq!("stock_quantity_wraps_like_tpcc", adjust_stock_quantity(20, 5), 15);
        test_assert_eq!("stock_quantity_wraps_like_tpcc", adjust_stock_quantity(10, 5), 96);

        test_assert_lt!(
            "packing_roundtrips_expected_ranges",
            pack_customer_key(1, 1, 1),
            pack_customer_key(1, 1, 2)
        );
        test_assert_lt!(
            "packing_roundtrips_expected_ranges",
            pack_order_line_key(1, 1, 1, 1),
            pack_order_line_key(1, 1, 1, 2)
        );

        if errors.is_empty() {
            Ok("All tests passed.".to_string())
        } else {
            let mut output = format!("Saw {} test failures:\n", errors.len());
            for error in errors {
                output.push_str(&error);
                output.push('\n');
            }
            Err(output)
        }
    }
}
