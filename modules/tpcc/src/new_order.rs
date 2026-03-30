use std::collections::HashMap;

use crate::{
    district, find_customer_by_id, find_district, find_stock, find_warehouse, item, order_line, pack_order_key,
    remote::{remote_warehouse_home, simulate_remote_call},
    stock, District, Item, OrderLine, Stock, WarehouseId, DISTRICTS_PER_WAREHOUSE, TAX_SCALE,
};
use spacetimedb::{log_stopwatch::LogStopwatch, reducer, Identity, ReducerContext, SpacetimeType, Table, Timestamp};

#[derive(Clone, Debug, SpacetimeType)]
pub struct NewOrderLineInput {
    pub item_id: u32,
    pub supply_w_id: u32,
    pub quantity: u32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct NewOrderLineResult {
    pub item_id: u32,
    pub item_name: String,
    pub supply_w_id: u32,
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

#[reducer]
pub fn new_order(
    ctx: &ReducerContext,
    w_id: u32,
    d_id: u8,
    c_id: u32,
    order_lines: Vec<NewOrderLineInput>,
) -> Result<NewOrderResult, String> {
    let _timer = LogStopwatch::new("new_order");
    log::debug!("Starting `new_order` transaction at {}", ctx.timestamp);

    ensure!(
        (1..=DISTRICTS_PER_WAREHOUSE).contains(&d_id),
        "district id out of range"
    );
    ensure!(
        (5..=15).contains(&order_lines.len()),
        "new-order requires between 5 and 15 order lines"
    );

    let warehouse = find_warehouse(ctx, w_id)?;

    let district = find_district(ctx, w_id, d_id)?;
    let order_id = district.d_next_o_id;
    ctx.db.district().district_key().update(District {
        d_next_o_id: order_id + 1,
        ..district
    });

    let customer = find_customer_by_id(ctx, w_id, d_id, c_id)?;

    let all_local_warehouse = order_lines.iter().all(|order_line| order_line.supply_w_id == w_id);

    // TECHNICALLY NON-CONFORMANT: If we encounter a non-existent item in the order,
    // we'll short-circuit and exit here.
    // TPC-C technically requires, in 2.4.2.3, that we still retrieve and process all the valid item numbers.
    // This would be a horrendous pain to implement, so we won't.
    // We don't do the things the spec tells us it doesn't want us to do, namely:
    // - changing the execution of other steps
    // - using a different type of transaction
    // But we do skip inspecting some number of valid items and stocks.
    let items = order_lines
        .iter()
        .map(|line| find_item(ctx, line.item_id))
        .collect::<Result<Vec<Item>, String>>()?;

    let (remote_order_lines, local_order_lines) = partition_lines_by_database(ctx, &order_lines);

    let remote_order_outputs = remote_order_lines
        .into_iter()
        .map(|(remote_database_identity, lines)| {
            let input = OrderMultipleItemsInput {
                lines,
                district: d_id,
                terminal_warehouse: w_id,
            };
            call_remote_order_multiple_items_and_decrement_stock(ctx, remote_database_identity, input)
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flat_map(|v| v.into_iter())
        .collect::<Vec<OrderItemOutput>>();

    let local_order_outputs = local_order_lines
        .into_iter()
        .map(|line| order_item_and_decrement_stock(ctx, line.line, line.index, d_id, w_id))
        .collect::<Result<Vec<_>, String>>()?;

    let line_results = remote_order_outputs
        .into_iter()
        .chain(local_order_outputs)
        .map(|output| {
            (
                items[output.index as usize].clone(),
                order_lines[output.index as usize].clone(),
                output,
            )
        })
        .map(|(item, line, order_item_output)| ProcessedNewOrderItem {
            index: order_item_output.index,
            line,
            item,
            district_stock_info: order_item_output.s_dist,
            stock_data: order_item_output.s_data,
            updated_quantity: order_item_output.updated_quantity,
        })
        .map(|processed_item| insert_order_line(ctx, w_id, d_id, order_id, processed_item))
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
        entry_d: ctx.timestamp,
        total_amount_cents,
        all_local: all_local_warehouse,
        lines: line_results,
    })
}

#[derive(SpacetimeType)]
struct NewOrderLineAndIndex {
    line: NewOrderLineInput,
    index: u8,
}

fn partition_lines_by_database(
    ctx: &ReducerContext,
    order_lines: &[NewOrderLineInput],
) -> (HashMap<Identity, Vec<NewOrderLineAndIndex>>, Vec<NewOrderLineAndIndex>) {
    let mut remote_lines: HashMap<Identity, Vec<_>> = HashMap::new();
    let mut local_lines = Vec::with_capacity(order_lines.len());
    for (index, line) in order_lines.iter().cloned().enumerate() {
        let index = index as u8;
        if let Some(remote_database_identity) = remote_warehouse_home(ctx, line.supply_w_id) {
            remote_lines
                .entry(remote_database_identity)
                .or_default()
                .push(NewOrderLineAndIndex { line, index });
        } else {
            local_lines.push(NewOrderLineAndIndex { line, index });
        }
    }

    (remote_lines, local_lines)
}

fn call_remote_order_multiple_items_and_decrement_stock(
    ctx: &ReducerContext,
    remote_database_identity: Identity,
    input: OrderMultipleItemsInput,
) -> Result<Vec<OrderItemOutput>, String> {
    // call_remote_reducer(
    //     ctx,
    //     remote_database_identity,
    //     "order_multiple_items_and_decrement_stocks",
    //     &input,
    // )
    simulate_remote_call(
        ctx,
        remote_database_identity,
        "order_multiple_items_and_decrement_stocks",
        &input,
    )?;
    Ok(simulated_remote_order_outputs(input))
}

struct ProcessedNewOrderItem {
    index: u8,
    line: NewOrderLineInput,
    item: Item,
    district_stock_info: String,
    stock_data: String,
    updated_quantity: i32,
}

fn insert_order_line(
    tx: &ReducerContext,
    warehouse_id: u32,
    district_id: u8,
    order_id: u32,
    processed_item: ProcessedNewOrderItem,
) -> NewOrderLineResult {
    let ProcessedNewOrderItem {
        index,
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
        order_line_key: pack_order_line_key(warehouse_id, district_id, order_id, index + 1),
        ol_w_id: warehouse_id,
        ol_d_id: district_id,
        ol_o_id: order_id,
        ol_number: index + 1,
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
pub struct OrderItemOutput {
    s_dist: String,
    s_data: String,
    updated_quantity: i32,
    index: u8,
}

#[derive(SpacetimeType)]
pub struct OrderMultipleItemsInput {
    lines: Vec<NewOrderLineAndIndex>,
    district: u8,
    terminal_warehouse: WarehouseId,
}

fn simulated_remote_order_outputs(input: OrderMultipleItemsInput) -> Vec<OrderItemOutput> {
    input
        .lines
        .into_iter()
        .map(|line| simulated_remote_order_output(line, input.district))
        .collect()
}

fn simulated_remote_order_output(line: NewOrderLineAndIndex, district: u8) -> OrderItemOutput {
    let stock_data = if line.line.item_id % 10 == 0 {
        "SIMULATED ORIGINAL STOCK"
    } else {
        "SIMULATED REMOTE STOCK"
    };

    OrderItemOutput {
        s_dist: format!("REMOTE-D{district:02}"),
        s_data: stock_data.to_string(),
        updated_quantity: adjust_stock_quantity(100, line.line.quantity as i32),
        index: line.index,
    }
}

#[reducer]
fn order_multiple_items_and_decrement_stocks(
    ctx: &ReducerContext,
    input: OrderMultipleItemsInput,
) -> Result<Vec<OrderItemOutput>, String> {
    let _timer = LogStopwatch::new("order_multiple_items_and_decrement_stock");
    let OrderMultipleItemsInput {
        lines,
        district,
        terminal_warehouse,
    } = input;
    lines
        .into_iter()
        .map(|line| order_item_and_decrement_stock(ctx, line.line, line.index, district, terminal_warehouse))
        .collect()
}

fn order_item_and_decrement_stock(
    ctx: &ReducerContext,
    line: NewOrderLineInput,
    index: u8,
    district: u8,
    terminal_warehouse: WarehouseId,
) -> Result<OrderItemOutput, String> {
    let is_remote_warehouse = terminal_warehouse == line.supply_w_id;

    let stock = find_stock(ctx, line.supply_w_id, line.item_id)?;

    let ordered_quantity = line.quantity;
    let updated_quantity = adjust_stock_quantity(stock.s_quantity, ordered_quantity as i32);

    let output = OrderItemOutput {
        s_dist: district_stock_info(&stock, district),
        s_data: stock.s_data.clone(),
        updated_quantity,
        index,
    };

    ctx.db.stock().stock_key().update(Stock {
        s_quantity: updated_quantity,
        s_ytd: stock.s_ytd + u64::from(ordered_quantity),
        s_order_cnt: stock.s_order_cnt + 1,
        s_remote_cnt: stock.s_remote_cnt + is_remote_warehouse as u32,
        ..stock
    });

    Ok(output)
}

fn apply_tax(amount_cents: i64, total_tax_bps: i64) -> i64 {
    amount_cents * (TAX_SCALE + total_tax_bps) / TAX_SCALE
}

fn apply_discount(amount_cents: i64, discount_bps: i64) -> i64 {
    amount_cents * (TAX_SCALE - discount_bps) / TAX_SCALE
}

fn find_item(tx: &ReducerContext, item_id: u32) -> Result<Item, String> {
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
pub fn pack_order_line_key(w_id: u32, d_id: u8, o_id: u32, ol_number: u8) -> u64 {
    pack_order_key(w_id, d_id, o_id) * 100 + u64::from(ol_number)
}
