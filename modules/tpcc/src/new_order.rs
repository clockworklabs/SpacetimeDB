use spacetimedb::{
    procedure, reducer, table, Identity, ProcedureContext, ReducerContext, SpacetimeType, Table, Timestamp, TxContext,
};
use spacetimedb_sats::serde::SerdeWrapper;

use crate::{
    district, find_customer_by_id, find_district, find_stock, find_warehouse, item, order_line, pack_order_key,
    remote::{call_remote_function, get_spacetimedb_uri, remote_warehouse_home},
    stock, District, Item, OrderLine, Stock, WarehouseId, DISTRICTS_PER_WAREHOUSE, TAX_SCALE,
};

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

#[procedure]
pub fn new_order(
    ctx: &mut ProcedureContext,
    w_id: WarehouseId,
    d_id: u8,
    c_id: u32,
    order_lines: Vec<NewOrderLineInput>,
) -> Result<NewOrderResult, String> {
    let start_time = ctx.timestamp;
    log::debug!("Starting `new_order` transaction at {start_time:?}");

    let res = (|| {
        ensure!(
            (1..=DISTRICTS_PER_WAREHOUSE).contains(&d_id),
            "district id out of range"
        );
        ensure!(
            (5..=15).contains(&order_lines.len()),
            "new-order requires between 5 and 15 order lines"
        );

        // Setup TX: validate warehouse, district, customer ID.
        // NON-CONFORMANT: These never change in TPC-C,
        // so we don't need to include the checks in the same transaction as the rest of the work.
        let (warehouse, district, customer, spacetimedb_uri) = ctx.try_with_tx(|tx| {
            let warehouse = find_warehouse(tx, w_id)?;
            let district = find_district(tx, w_id, d_id)?;
            let customer = find_customer_by_id(tx, w_id, d_id, c_id)?;
            let spacetimedb_uri = get_spacetimedb_uri(tx);
            Ok::<_, String>((warehouse, district, customer, spacetimedb_uri))
        })?;

        let PartitionedItems {
            local_database_items,
            remote_database_items,
            all_local_warehouse,
        } =
        // Look up all of the items in the order, and fail if any of them doesn't exist.
        // If they all exist, sort them into two groups:
        // - `local_database_items`, items in warehouses managed by this database.
        // - `remote_database_items`, items in warehouses managed by remote databases.
        // Also compute `all_local_warehouse`, which says if all of the items are in the warehouse `w_id`.
        // NON-CONFORMANT: This is a separate transaction from the later one,
        // which updates stock quantities for the local items and records the new order.
        // In a real system, an item might change between the two, but none of the TPC-C transactions writes to items.
        // We (ab)use this knowledge to skip compensating for writes to items.
            partition_local_from_remote_database_items(ctx, w_id, &order_lines)?;

        // NON-CONFORMANT: We reserve items from the remote database extra-transactionally.
        // If our TPC-C transaction fails, we'll roll back those reservations.
        // This opens us up to dirty read isolation hazards,
        // where a concurrent transaction may observe a change in stock quantity that later rolls back.
        // This will never happen with only the TPC-C transactions,
        // as stock quantity is only written by the `new_order` transaction,
        // and `new_order` can only fail prior to updating the stock quantity, due to non-existent items.
        // We (ab)use this knowledge to skip compensating for rollbacks to prevent dirty reads.
        let remote_item_reservations = reserve_remote_items(ctx, &spacetimedb_uri, d_id, &remote_database_items)?;

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

            let line_results = local_database_items
                .iter()
                .map(|local_item| claim_stock_for_local_database_item(tx, local_item, d_id))
                .chain(remote_database_items.iter().zip(remote_item_reservations.iter()).map(
                    |(remote_item, reserved_item)| remote_item_to_processed_new_order_item(remote_item, reserved_item),
                ))
                .map(|processed_item| insert_order_line(tx, w_id, d_id, order_id, processed_item))
                .collect::<Vec<_>>();

            let subtotal_cents = line_results.iter().map(|line_result| line_result.amount_cents).sum();

            let taxed = apply_tax(
                subtotal_cents,
                i64::from(warehouse.w_tax_bps) + i64::from(district.d_tax_bps),
            );
            let total_amount_cents = apply_discount(taxed, i64::from(customer.c_discount_bps));

            Ok(NewOrderResult {
                warehouse_tax_bps: warehouse.w_tax_bps,
                district_tax_bps: district.d_tax_bps,
                customer_discount_bps: customer.c_discount_bps,
                customer_last: customer.c_last.clone(),
                customer_credit: customer.c_credit.clone(),
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
                    &remote_database_items,
                    remote_item_reservations,
                );
                Ok(result)
            }
            Err(e) => {
                rollback_all_remote_item_reservations(
                    ctx,
                    &spacetimedb_uri,
                    &remote_database_items,
                    remote_item_reservations,
                );
                Err(e)
            }
        }
    })();

    match &res {
        Ok(_) => {
            log::debug!("Successfully finished `new_order` at {start_time:?}");
        }
        Err(e) => {
            log::error!("Failed `new_order` at {start_time:?}: {e}");
        }
    }
    res
}

struct LocalDatabaseItem {
    idx: usize,
    line: NewOrderLineInput,
    item: Item,
    is_remote_warehouse: bool,
}

struct RemoteDatabaseItem {
    idx: usize,
    line: NewOrderLineInput,
    item: Item,
    remote_database_identity: Identity,
}

struct PartitionedItems {
    local_database_items: Vec<LocalDatabaseItem>,
    remote_database_items: Vec<RemoteDatabaseItem>,

    /// Are all items from the same warehouse as the requesting terminal?
    ///
    /// Note that this may be false even if all items are partitioned into [`Self::local_database_items`],
    /// as we may manage multiple warehouses with a single database.
    all_local_warehouse: bool,
}

fn partition_local_from_remote_database_items(
    ctx: &mut ProcedureContext,
    local_warehouse_id: WarehouseId,
    order_lines: &[NewOrderLineInput],
) -> Result<PartitionedItems, String> {
    ctx.try_with_tx(|tx| {
        let mut local_database_items: Vec<LocalDatabaseItem> = Vec::with_capacity(order_lines.len());
        let mut remote_database_items: Vec<RemoteDatabaseItem> = Vec::with_capacity(order_lines.len());

        // Whether this order applies only to a single warehouse.
        // This may be `false` even when `remote_database_items_to_get` is non-empty,
        // as we may run multiple warehouses from the same database.
        let mut all_local_warehouse = true;

        for (idx, line) in order_lines.iter().enumerate() {
            ensure!(line.quantity > 0, "order line quantity must be positive");

            let is_remote_warehouse = line.supply_w_id == local_warehouse_id;
            all_local_warehouse &= !is_remote_warehouse;

            // TECHNICALLY NON-CONFORMANT: If we encounter a non-existent item in the order,
            // we'll short-circuit and exit here.
            // TPC-C technically requires, in 2.4.2.3, that we still retrieve and process all the valid item numbers.
            // This would be a horrendous pain to implement, so we won't.
            // We don't do the things the spec tells us it doesn't want us to do, namely:
            // - changing the execution of other steps
            // - using a different type of transaction
            // But we do skip inspecting some number of valid items and stocks.
            let item = find_item(tx, line.item_id)?;

            match remote_warehouse_home(tx, line.supply_w_id) {
                None => {
                    // Warehouse is local to this database.
                    // We'll actually "process" the items, i.e. decrement the stock and sum the order price,
                    // after we look up and process all the remote items.
                    local_database_items.push(LocalDatabaseItem {
                        idx,
                        line: line.clone(),
                        item,
                        is_remote_warehouse,
                    });
                }
                Some(remote_database_identity) => {
                    // Warehouse is on another database; we'll have to do a remote request.
                    // This is *really* non-conformant.
                    // TODO(docs): link to blog post justifying this.
                    remote_database_items.push(RemoteDatabaseItem {
                        idx,
                        line: line.clone(),
                        item,
                        remote_database_identity,
                    });
                }
            }
        }

        Ok(PartitionedItems {
            local_database_items,
            remote_database_items,
            all_local_warehouse,
        })
    })
}

fn reserve_remote_items(
    ctx: &mut ProcedureContext,
    spacetimedb_uri: &str,
    district_id: u8,
    remote_database_items: &[RemoteDatabaseItem],
) -> Result<Vec<ReserveItemOutput>, String> {
    let mut remote_item_reservations: Vec<ReserveItemOutput> = Vec::with_capacity(remote_database_items.len());

    for RemoteDatabaseItem {
        line,
        remote_database_identity,
        ..
    } in remote_database_items
    {
        match call_remote_function(
            ctx,
            spacetimedb_uri,
            *remote_database_identity,
            "reserve_item_for_remote_order",
            ReserveItemInput {
                line: NewOrderLineInput::clone(line),
                district: district_id,
            },
        ) {
            Err(e) => {
                rollback_all_remote_item_reservations(
                    ctx,
                    spacetimedb_uri,
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
                            spacetimedb_uri,
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

    Ok(remote_item_reservations)
}

fn rollback_all_remote_item_reservations(
    ctx: &mut ProcedureContext,
    spacetimedb_uri: &str,
    remote_items: &[RemoteDatabaseItem],
    reservations: Vec<ReserveItemOutput>,
) {
    for (remote_item, reservation) in remote_items.iter().zip(reservations.into_iter()) {
        if let Err(e) = call_remote_function(
            ctx,
            spacetimedb_uri,
            remote_item.remote_database_identity,
            "rollback_item_reservation",
            reservation.rollback_token,
        ) {
            log::error!("Error rollinb back item reservation: {e}");
        }
    }
}

fn confirm_all_remote_item_reservations(
    ctx: &mut ProcedureContext,
    spacetimedb_uri: &str,
    remote_items: &[RemoteDatabaseItem],
    reservations: Vec<ReserveItemOutput>,
) {
    for (remote_item, reservation) in remote_items.iter().zip(reservations.into_iter()) {
        if let Err(e) = call_remote_function(
            ctx,
            spacetimedb_uri,
            remote_item.remote_database_identity,
            "confirm_item_reservation",
            reservation.rollback_token,
        ) {
            log::error!("Error confirming item reservation: {e}");
        }
    }
}

struct ProcessedNewOrderItem {
    idx: usize,
    line: NewOrderLineInput,
    item: Item,
    district_stock_info: String,
    stock_data: String,
    updated_quantity: i32,
}

fn claim_stock_for_local_database_item(
    tx: &TxContext,
    local_item: &LocalDatabaseItem,
    district_id: u8,
) -> ProcessedNewOrderItem {
    let stock =
        find_stock(tx, local_item.line.supply_w_id, local_item.line.item_id).expect("Stock should exist for all items");
    let updated_quantity = adjust_stock_quantity(stock.s_quantity, local_item.line.quantity as i32);
    tx.db.stock().stock_key().update(Stock {
        s_quantity: updated_quantity,
        s_ytd: stock.s_ytd + local_item.line.quantity as u64,
        s_order_cnt: stock.s_order_cnt + 1,
        s_remote_cnt: stock.s_remote_cnt + u32::from(local_item.is_remote_warehouse),
        ..stock.clone()
    });

    ProcessedNewOrderItem {
        idx: local_item.idx,
        line: local_item.line.clone(),
        item: local_item.item.clone(),
        district_stock_info: district_stock_info(&stock, district_id),
        stock_data: stock.s_data.clone(),
        updated_quantity,
    }
}

fn remote_item_to_processed_new_order_item(
    remote_item: &RemoteDatabaseItem,
    reserved_item: &ReserveItemOutput,
) -> ProcessedNewOrderItem {
    ProcessedNewOrderItem {
        idx: remote_item.idx,
        line: remote_item.line.clone(),
        item: remote_item.item.clone(),
        district_stock_info: reserved_item.s_dist.clone(),
        stock_data: reserved_item.s_data.clone(),
        updated_quantity: reserved_item.updated_quantity,
    }
}

fn insert_order_line(
    tx: &TxContext,
    warehouse_id: WarehouseId,
    district_id: u8,
    order_id: u32,
    processed_item: ProcessedNewOrderItem,
) -> NewOrderLineResult {
    let ProcessedNewOrderItem {
        idx,
        line,
        item,
        district_stock_info,
        stock_data,
        updated_quantity,
    } = processed_item;
    let line_amount_cents = line.quantity as i64 * item.i_price_cents;
    let brand_generic = if contains_original(&item.i_data) && contains_original(&stock_data) {
        "B"
    } else {
        "G"
    };
    tx.db.order_line().insert(OrderLine {
        order_line_key: pack_order_line_key(warehouse_id, district_id, order_id, (idx + 1) as u8),
        ol_w_id: warehouse_id,
        ol_d_id: district_id,
        ol_o_id: order_id,
        ol_number: (idx + 1) as u8,
        ol_i_id: line.item_id,
        ol_supply_w_id: line.supply_w_id,
        ol_delivery_d: None,
        ol_quantity: line.quantity,
        ol_amount_cents: line_amount_cents,
        ol_dist_info: district_stock_info,
    });

    NewOrderLineResult {
        item_id: item.i_id,
        item_name: item.i_name,
        supply_w_id: line.supply_w_id,
        quantity: line.quantity,
        stock_quantity: updated_quantity,
        item_price_cents: item.i_price_cents,
        amount_cents: line_amount_cents,
        brand_generic: brand_generic.to_string(),
    }
}

#[derive(SpacetimeType)]
pub struct ReserveItemOutput {
    s_dist: String,
    s_data: String,
    updated_quantity: i32,
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

        let ReservedItemLog { rollback_token, .. } = tx.db.reserved_item_log().insert(ReservedItemLog {
            rollback_token: 0,
            line: line.clone(),
        });

        let reserved_quantity = line.quantity;
        let updated_quantity = adjust_stock_quantity(stock.s_quantity, reserved_quantity as i32);

        let reserved = ReserveItemOutput {
            s_dist: district_stock_info(&stock, district),
            s_data: stock.s_data.clone(),
            updated_quantity,
            rollback_token,
        };

        tx.db.stock().stock_key().update(Stock {
            s_quantity: updated_quantity,
            s_ytd: stock.s_ytd + u64::from(reserved_quantity),
            s_order_cnt: stock.s_order_cnt + 1,
            // This must be an order from a remote warehouse, it's coming from a whole different database.
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

fn apply_tax(amount_cents: i64, total_tax_bps: i64) -> i64 {
    amount_cents * (TAX_SCALE + total_tax_bps) / TAX_SCALE
}

fn apply_discount(amount_cents: i64, discount_bps: i64) -> i64 {
    amount_cents * (TAX_SCALE - discount_bps) / TAX_SCALE
}

fn find_item(tx: &spacetimedb::TxContext, item_id: u32) -> Result<Item, String> {
    tx.db
        .item()
        .i_id()
        .find(item_id)
        .ok_or_else(|| format!("item {item_id} not found"))
}

// public for test in lib.rs
pub fn adjust_stock_quantity(current_quantity: i32, ordered_quantity: i32) -> i32 {
    assert!(ordered_quantity >= 1);
    assert!(ordered_quantity <= 10);
    if current_quantity - ordered_quantity >= 10 {
        current_quantity - ordered_quantity
    } else {
        current_quantity - ordered_quantity + 91
    }
}

/// NON-CONFORMANT: we're abusing the fact that TPC-C updates stock quantities in a predictable way
/// which is both commutative and associative to be able to roll back stock reservations.
fn reverse_stock_quantity(current_quantity: i32, ordered_quantity: i32) -> i32 {
    assert!(ordered_quantity >= 1);
    assert!(ordered_quantity <= 10);
    if current_quantity + ordered_quantity >= 91 {
        current_quantity + ordered_quantity - 91
    } else {
        current_quantity + ordered_quantity
    }
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

// public for test in lib.rs
pub fn pack_order_line_key(w_id: u16, d_id: u8, o_id: u32, ol_number: u8) -> u64 {
    pack_order_key(w_id, d_id, o_id) * 100 + u64::from(ol_number)
}
