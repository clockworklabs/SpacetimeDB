use http::Request;
use spacetimedb::{
    http::Timeout, procedure, reducer, sats::serde::SerdeWrapper, table, Identity, ProcedureContext, ReducerContext,
    ScheduleAt, SpacetimeType, Table, Timestamp, TxContext,
};
use std::{collections::BTreeSet, time::Duration};

const DISTRICTS_PER_WAREHOUSE: u8 = 10;
const CUSTOMERS_PER_DISTRICT: u32 = 3_000;
const ITEMS: u32 = 100_000;
const MAX_C_DATA_LEN: usize = 500;
const TAX_SCALE: i64 = 10_000;

#[spacetimedb::table(accessor = spacetimedb_uri)]
struct SpacetimeDbUri {
    uri: String,
}

fn get_spacetimedb_uri(tx: &TxContext) -> String {
    tx.db.spacetimedb_uri().iter().next().unwrap().uri
}

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

type WarehouseId = u16;

#[derive(Clone, Debug, SpacetimeType)]
pub struct NewOrderLineInput {
    pub item_id: u32,
    pub supply_w_id: WarehouseId,
    pub quantity: u32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct NewOrderLineResult {
    pub item_id: u32,
    pub item_name: String,
    pub supply_w_id: WarehouseId,
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

    /// Added by us: the [`Identity`] of the remote database where this warehouse is sharded,
    /// or `None` if this warehouse is sharded in the local database.
    ///
    /// TPC-C 1.4.7: "Attributes may be added and/or duplicated from one table to another
    /// as long as these changes do not improve performance."
    pub remote_database_home: Option<Identity>,
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
        validate_district_row(&row)?;
        ctx.db.district().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_customers(ctx: &ReducerContext, rows: Vec<Customer>) -> Result<(), String> {
    for row in rows {
        validate_customer_row(&row)?;
        ctx.db.customer().insert(row);
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
        validate_stock_row(&row)?;
        ctx.db.stock().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_orders(ctx: &ReducerContext, rows: Vec<OOrder>) -> Result<(), String> {
    for row in rows {
        ctx.db.oorder().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_new_orders(ctx: &ReducerContext, rows: Vec<NewOrder>) -> Result<(), String> {
    for row in rows {
        ctx.db.new_order_row().insert(row);
    }
    Ok(())
}

#[reducer]
pub fn load_order_lines(ctx: &ReducerContext, rows: Vec<OrderLine>) -> Result<(), String> {
    for row in rows {
        ctx.db.order_line().insert(row);
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
    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&d_id),
        "district id out of range"
    );
    ensure!(
        (5..=15).contains(&order_lines.len()),
        "new-order requires between 5 and 15 order lines"
    );

    // Setup TX: validate warehouse, district, customer ID.
    // These never change in TPC-C, so we don't need to include the checks in the same transaction as the rest of the work.
    let (warehouse, district, customer, spacetimedb_uri) = ctx.try_with_tx(|tx| {
        let warehouse = find_warehouse(tx, w_id)?;
        let district = find_district(tx, w_id, d_id)?;
        let customer = find_customer_by_id(tx, w_id, d_id, c_id)?;
        let spacetimedb_uri = get_spacetimedb_uri(tx);
        Ok::<_, String>((warehouse, district, customer, spacetimedb_uri))
    })?;

    let (local_database_items, remote_database_items, all_local_warehouse) = ctx.try_with_tx(|tx| {
        let mut local_database_items: Vec<(usize, NewOrderLineInput, Item, bool)> =
            Vec::with_capacity(order_lines.len());
        let mut remote_database_items: Vec<(usize, NewOrderLineInput, Item, Identity)> =
            Vec::with_capacity(order_lines.len());

        // Whether this order applies only to a single warehouse.
        // This may be `false` even when `remote_database_items_to_get` is non-empty,
        // as we may run multiple warehouses from the same database.
        let mut all_local_warehouse = true;

        for (idx, line) in order_lines.iter().enumerate() {
            ensure!(line.quantity > 0, "order line quantity must be positive");

            let is_remote_warehouse = line.supply_w_id == w_id;
            all_local_warehouse &= is_remote_warehouse;

            let warehouse = tx
                .db
                .warehouse()
                .w_id()
                .find(line.supply_w_id)
                .ok_or_else(|| format!("No such warehouse: {}", line.supply_w_id))?;

            // TECHNICALLY NON-CONFORMANT: If we encounter a non-existent item in the order,
            // we'll short-circuit and exit here.
            // TPC-C technically requires, in 2.4.2.3, that we still retrieve and process all the valid item numbers.
            // This would be a horrendous pain to implement, so we won't.
            // We don't do the things the spec tells us it doesn't want us to do, namely:
            // - changing the execution of other steps
            // - using a different type of transaction
            // But we do skip inspecting some number of valid items and stocks.
            let item = find_item(tx, line.item_id)?;
            match warehouse.remote_database_home {
                None => {
                    // Warehouse is local to this database.
                    // We'll actually "process" the items, i.e. decrement the stock and sum the order price,
                    // after we look up and process all the remote items.
                    local_database_items.push((idx, NewOrderLineInput::clone(line), item, is_remote_warehouse));
                }
                Some(remote_database_identity) => {
                    // Warehouse is on another database; we'll have to do a remote request.
                    // This is *really* non-conformant.
                    // TODO(docs): link to blog post justifying this.
                    remote_database_items.push((idx, NewOrderLineInput::clone(line), item, remote_database_identity));
                }
            }
        }

        Ok::<_, String>((local_database_items, remote_database_items, all_local_warehouse))
    })?;

    let mut remote_item_reservations: Vec<ReserveItemOutput> = Vec::with_capacity(remote_database_items.len());

    for (_idx, line, item, remote_database_ident) in &remote_database_items {
        match call_remote_function(
            ctx,
            &spacetimedb_uri,
            *remote_database_ident,
            "reserve_item_for_remote_order",
            vec![serde_json::json!(spacetimedb_sats::serde::SerdeWrapper(
                ReserveItemInput {
                    line: NewOrderLineInput::clone(line),
                    district: d_id,
                }
            ))],
        ) {
            Err(e) => {
                rollback_all_remote_item_reservations(
                    ctx,
                    &spacetimedb_uri,
                    remote_database_items,
                    remote_item_reservations,
                );
                return Err(format!("Error reserving remote item: {e}"));
            }
            Ok(body) => {
                let body = body.into_string().expect("Body should be valid UTF-8");
                let res: SerdeWrapper<Result<ReserveItemOutput, String>> =
                    serde_json::from_str(&body).expect("Response does not conform to expected schema");
                match res.0 {
                    Err(e) => {
                        rollback_all_remote_item_reservations(
                            ctx,
                            &spacetimedb_uri,
                            remote_database_items,
                            remote_item_reservations,
                        );
                        return Err(format!("Error reserving remote item from database: {e}"));
                    }
                    Ok(output) => remote_item_reservations.push(output),
                }
            }
        };
    }

    match ctx.try_with_tx(|tx| {
        let district = tx
            .db
            .district()
            .district_key()
            .find(district.district_key)
            .expect("District should not have been removed since we retrieved it last");
        let order_id = district.d_next_o_id;
        tx.db.district().district_key().update(District {
            d_next_o_id: order_id + 1,
            ..district
        });

        let mut subtotal_cents = 0;

        let line_results = local_database_items
            .iter()
            .map(|(idx, line, item, is_remote_warehouse)| {
                let stock = find_stock(tx, line.supply_w_id, line.item_id).expect("Stock should exist for all items");
                tx.db.stock().stock_key().update(Stock {
                    s_quantity: adjust_stock_quantity(stock.s_quantity, line.quantity as i32),
                    s_ytd: stock.s_ytd + line.quantity as u64,
                    s_order_cnt: stock.s_order_cnt + 1,
                    s_remote_cnt: stock.s_remote_cnt + u32::from(*is_remote_warehouse),
                    ..stock.clone()
                });

                (idx, line, item, district_stock_info(&stock, d_id), stock.s_data)
            })
            .chain(remote_database_items.iter().zip(remote_item_reservations.iter()).map(
                |((idx, line, item, _remote_db_ident), reservation)| {
                    (idx, line, item, reservation.s_dist, reservation.s_data)
                },
            ))
            .map(|(idx, line, item, s_dist, s_data)| {
                let line_amount_cents = line.quantity as i64 * item.i_price_cents;
                subtotal_cents += line_amount_cents;
                let brand_generic = if contains_original(&item.i_data) && contains_original(&s_data) {
                    "B"
                } else {
                    "G"
                };
                tx.db.order_line().insert(OrderLine {
                    order_line_key: pack_order_line_key(w_id, d_id, order_id, (idx + 1) as u8),
                    ol_w_id: w_id,
                    ol_d_id: d_id,
                    ol_o_id: order_id,
                    ol_number: (idx + 1) as u8,
                    ol_i_id: line.item_id,
                    ol_supply_w_id: line.supply_w_id,
                    ol_delivery_d: None,
                    ol_quantity: line.quantity,
                    ol_amount_cents: line_amount_cents,
                    ol_dist_info: s_dist,
                });

                NewOrderLineResult {
                    item_id: item.i_id,
                    item_name: item.i_name,
                    supply_w_id: line.supply_w_id,
                    quantity: line.quantity,
                    stock_quantity: updated_stock_quantity,
                    item_price_cents: item.i_price_cents,
                    amount_cents: line_amount_cents,
                    brand_generic: brand_generic.to_string(),
                }
            })
            .collect::<Vec<_>>();

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
            all_local: all_local_warehouse,
            lines: line_results,
        })
    }) {
        Ok(result) => {
            confirm_all_remote_item_reservations(
                ctx,
                &spacetimedb_uri,
                remote_database_items,
                remote_item_reservations,
            );
            Ok(result)
        }
        Err(e) => {
            rollback_all_remote_item_reservations(
                ctx,
                &spacetimedb_uri,
                remote_database_items,
                remote_item_reservations,
            );
            Err(e)
        }
    }
}

fn call_remote_function(
    ctx: &mut ProcedureContext,
    spacetimedb_uri: &str,
    database_ident: Identity,
    function_name: &str,
    arguments: Vec<serde_json::Value>,
) -> Result<spacetimedb::http::Body, String> {
    let request = Request::builder()
        .uri(format!(
            "{spacetimedb_uri}/v1/database/{database_ident}/call/{function_name}"
        ))
        .method("POST")
        // TODO(auth): include a token.
        .body(serde_json::json!(arguments).to_string())
        .map_err(|e| format!("Error constructing `Request`: {e}"))?;
    match ctx.http.send(request) {
        Err(e) => Err(format!("Error sending request to remote database {database_ident} at URI {spacetimedb_uri} to call {function_name}: {e}")),
        Ok(response) if response.status() != http::status::StatusCode::OK => Err(format!("Got non-200 response code {} from request to remote database {database_ident} at URI {spacetimedb_uri} when calling {function_name}: {}", response.status(), response.into_body().into_string_lossy())),
        Ok(response) => Ok(response.into_body()),
    }
}

fn rollback_all_remote_item_reservations(
    ctx: &mut ProcedureContext,
    spacetimedb_uri: &str,
    remote_items: Vec<(usize, NewOrderLineInput, Item, Identity)>,
    reservations: Vec<ReserveItemOutput>,
) {
    for ((_idx, _line, _item, remote_database_ident), reservation) in
        remote_items.into_iter().zip(reservations.into_iter())
    {
        if let Err(e) = call_remote_function(
            ctx,
            spacetimedb_uri,
            remote_database_ident,
            "rollback_item_reservation",
            vec![serde_json::json!(reservation.rollback_token)],
        ) {
            log::error!("Error rollinb back item reservation: {e}");
        }
    }
}

fn confirm_all_remote_item_reservations(
    ctx: &mut ProcedureContext,
    spacetimedb_uri: &str,
    remote_items: Vec<(usize, NewOrderLineInput, Item, Identity)>,
    reservations: Vec<ReserveItemOutput>,
) {
    for ((_idx, _line, _item, remote_database_ident), reservation) in
        remote_items.into_iter().zip(reservations.into_iter())
    {
        if let Err(e) = call_remote_function(
            ctx,
            spacetimedb_uri,
            remote_database_ident,
            "confirm_item_reservation",
            vec![serde_json::json!(reservation.rollback_token)],
        ) {
            log::error!("Error confirming item reservation: {e}");
        }
    }
}

#[derive(SpacetimeType)]
pub struct ReserveItemOutput {
    s_dist: String,
    s_data: String,
    rollback_token: u64,
}

#[table(accessor = reserved_item_log)]
pub struct ReservedItemLog {
    #[primary_key]
    #[auto_inc]
    rollback_token: u64,
    line: NewOrderLineInput,
}

#[derive(SpacetimeType)]
pub struct ReserveItemInput {
    line: NewOrderLineInput,
    district: u8,
}

#[procedure]
pub fn reserve_item_for_remote_order(
    ctx: &mut ProcedureContext,
    input: ReserveItemInput,
) -> Result<ReserveItemOutput, String> {
    let ReserveItemInput { line, district } = input;
    ctx.try_with_tx(|tx| {
        let stock = find_stock(tx, line.supply_w_id, line.item_id)?;

        let quantity = line.quantity;

        let ReservedItemLog { rollback_token, .. } = tx.db.reserved_item_log().insert(ReservedItemLog {
            rollback_token: 0,
            line: line.clone(),
        });

        let reserved = ReserveItemOutput {
            s_dist: district_stock_info(&stock, district),
            s_data: stock.s_data.clone(),
            rollback_token,
        };

        tx.db.stock().stock_key().update(Stock {
            s_quantity: adjust_stock_quantity(stock.s_quantity, quantity as i32),
            s_ytd: stock.s_ytd + u64::from(quantity),
            s_order_cnt: stock.s_order_cnt + 1,
            s_remote_cnt: stock.s_remote_cnt + 1,
            ..stock
        });

        Ok(reserved)
    })
}

#[reducer]
pub fn rollback_item_reservation(ctx: &ReducerContext, rollback_token: u64) -> Result<(), String> {
    let line = ctx
        .db
        .reserved_item_log()
        .rollback_token()
        .find(rollback_token)
        .ok_or_else(|| format!("No such rollback token: {rollback_token}"))?
        .line;
    let stock = find_stock(ctx, line.supply_w_id, line.item_id)?;
    let quantity = line.quantity;
    ctx.db.stock().stock_key().update(Stock {
        s_quantity: reverse_stock_quantity(stock.s_quantity, quantity as i32),
        s_ytd: stock.s_ytd - line.quantity as u64,
        s_order_cnt: stock.s_order_cnt - 1,
        s_remote_cnt: stock.s_remote_cnt - 1,
        ..stock
    });
    ctx.db.reserved_item_log().rollback_token().delete(rollback_token);
    Ok(())
}

#[reducer]
pub fn confirm_item_reservation(ctx: &ReducerContext, rollback_token: u64) {
    ctx.db.reserved_item_log().rollback_token().delete(rollback_token);
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

    tx.db.district().district_key().update(District {
        d_next_o_id: district.d_next_o_id + 1,
        ..district.clone()
    });

    tx.db.oorder().insert(OOrder {
        order_key: pack_order_key(w_id, d_id, order_id),
        o_w_id: w_id,
        o_d_id: d_id,
        o_id: order_id,
        o_c_id: c_id,
        o_entry_d: tx.timestamp,
        o_carrier_id: None,
        o_ol_cnt: order_lines.len() as u8,
        o_all_local: all_local,
    });

    tx.db.new_order_row().insert(NewOrder {
        new_order_key: pack_order_key(w_id, d_id, order_id),
        no_w_id: w_id,
        no_d_id: d_id,
        no_o_id: order_id,
    });

    let mut line_results = Vec::with_capacity(touched_items.len());
    let mut subtotal_cents = 0i64;
    for (idx, (line, item, stock)) in touched_items.into_iter().enumerate() {
        let updated_stock_quantity = adjust_stock_quantity(stock.s_quantity, line.quantity as i32);
        tx.db.stock().stock_key().update(Stock {
            s_quantity: updated_stock_quantity,
            s_ytd: stock.s_ytd + u64::from(line.quantity),
            s_order_cnt: stock.s_order_cnt + 1,
            s_remote_cnt: stock.s_remote_cnt + u32::from(line.supply_w_id != w_id),
            ..stock.clone()
        });

        let line_amount_cents = item.i_price_cents * i64::from(line.quantity);
        subtotal_cents += line_amount_cents;
        let dist_info = district_stock_info(&stock, d_id);
        tx.db.order_line().insert(OrderLine {
            order_line_key: pack_order_line_key(w_id, d_id, order_id, (idx + 1) as u8),
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
        });

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

    tx.db.district().district_key().update(District {
        d_ytd_cents: district.d_ytd_cents + req.payment_amount_cents,
        ..district.clone()
    });

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

    tx.db.customer().customer_key().update(updated_customer.clone());

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

    let customer = find_customer_by_id_reducer(ctx, w_id, d_id, order.o_c_id)?;
    ctx.db.customer().customer_key().update(Customer {
        c_balance_cents: customer.c_balance_cents + total_amount_cents,
        c_delivery_cnt: customer.c_delivery_cnt + 1,
        ..customer
    });

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

fn find_item(tx: &spacetimedb::TxContext, item_id: u32) -> Result<Item, String> {
    tx.db
        .item()
        .i_id()
        .find(item_id)
        .ok_or_else(|| format!("item {item_id} not found"))
}

fn find_stock(tx: &ReducerContext, w_id: u16, item_id: u32) -> Result<Stock, String> {
    tx.db
        .stock()
        .by_w_i()
        .filter((w_id, item_id))
        .next()
        .ok_or_else(|| format!("stock ({w_id}, {item_id}) not found"))
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
    assert!(ordered_quantity >= 1);
    assert!(ordered_quantity <= 10);
    if current_quantity - ordered_quantity >= 10 {
        current_quantity - ordered_quantity
    } else {
        current_quantity - ordered_quantity + 91
    }
}

fn reverse_stock_quantity(current_quantity: i32, ordered_quantity: i32) -> i32 {
    assert!(ordered_quantity >= 1);
    assert!(ordered_quantity <= 10);
    if current_quantity + ordered_quantity >= 91 {
        current_quantity + ordered_quantity - 91
    } else {
        current_quantity + ordered_quantity
    }
}

fn apply_tax(amount_cents: i64, total_tax_bps: i64) -> i64 {
    amount_cents * (TAX_SCALE + total_tax_bps) / TAX_SCALE
}

fn apply_discount(amount_cents: i64, discount_bps: i64) -> i64 {
    amount_cents * (TAX_SCALE - discount_bps) / TAX_SCALE
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

fn pack_order_line_key(w_id: u16, d_id: u8, o_id: u32, ol_number: u8) -> u64 {
    pack_order_key(w_id, d_id, o_id) * 100 + u64::from(ol_number)
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

    #[test]
    fn packing_roundtrips_expected_ranges() {
        assert!(pack_customer_key(1, 1, 1) < pack_customer_key(1, 1, 2));
        assert!(pack_order_line_key(1, 1, 1, 1) < pack_order_line_key(1, 1, 1, 2));
    }
}
