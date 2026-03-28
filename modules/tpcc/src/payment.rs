use spacetimedb::{procedure, ProcedureContext, SpacetimeType, Table, Timestamp, TxContext};
use spacetimedb_sats::serde::SerdeWrapper;

use crate::{
    customer, district, find_district, find_warehouse, history,
    remote::{call_remote_function, get_spacetimedb_uri, remote_warehouse_home},
    resolve_customer, warehouse, Customer, CustomerSelector, District, History, Warehouse, WarehouseId, MAX_C_DATA_LEN,
};

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

    let res = (|| {
        let (warehouse_home, spacetimedb_uri) =
            ctx.with_tx(|tx| (remote_warehouse_home(tx, c_w_id), get_spacetimedb_uri(tx)));
        let payment_request = PaymentRequest {
            terminal_warehouse_id: w_id,
            terminal_district_id: d_id,
            customer_warehouse_id: c_w_id,
            customer_district_id: c_d_id,
            customer_selector: customer,
            payment_amount_cents,
            now,
        };
        let customer = match warehouse_home {
            None => {
                // Customer warehouse is managed by this database.
                ctx.try_with_tx(|tx| {
                    let customer = resolve_customer(tx, c_w_id, c_d_id, &payment_request.customer_selector)?;
                    Ok::<_, String>(update_customer(tx, &payment_request, customer))
                })?
            }
            Some(remote_database) => {
                // Customer warehouse is managed by a remote database.
                // Contact them to update the customer's balance and retrieve their info.
                let body = call_remote_function(
                    ctx,
                    &spacetimedb_uri,
                    remote_database,
                    "process_remote_payment",
                    payment_request.clone(),
                )?
                .into_string()
                .expect("Body should be valid UTF-8");
                let res: SerdeWrapper<Result<Customer, String>> =
                    serde_json::from_str(&body).expect("Response does not conform to expected schema");
                res.0?
            }
        };

        ctx.try_with_tx(|tx| {
            let warehouse = find_warehouse(tx, payment_request.terminal_warehouse_id)?;
            let district = find_district(
                tx,
                payment_request.terminal_warehouse_id,
                payment_request.terminal_district_id,
            )?;

            tx.db.warehouse().w_id().update(Warehouse {
                w_ytd_cents: warehouse.w_ytd_cents + payment_request.payment_amount_cents,
                ..warehouse.clone()
            });

            tx.db.district().district_key().update(District {
                d_ytd_cents: district.d_ytd_cents + payment_request.payment_amount_cents,
                ..district.clone()
            });

            tx.db.history().insert(History {
                history_id: 0,
                h_c_id: customer.c_id,
                h_c_d_id: customer.c_d_id,
                h_c_w_id: customer.c_w_id,
                h_d_id: payment_request.terminal_district_id,
                h_w_id: payment_request.terminal_warehouse_id,
                h_date: payment_request.now,
                h_amount_cents: payment_request.payment_amount_cents,
                h_data: format!("{}    {}", warehouse.w_name, district.d_name),
            });

            Ok(PaymentResult {
                warehouse_name: warehouse.w_name,
                district_name: district.d_name,
                customer_id: customer.c_id,
                customer_first: customer.c_first.clone(),
                customer_middle: customer.c_middle.clone(),
                customer_last: customer.c_last.clone(),
                customer_balance_cents: customer.c_balance_cents,
                customer_credit: customer.c_credit.clone(),
                customer_discount_bps: customer.c_discount_bps,
                payment_amount_cents: payment_request.payment_amount_cents,
                customer_data: if customer.c_credit == "BC" {
                    Some(customer.c_data.clone())
                } else {
                    None
                },
            })
        })
    })();

    match &res {
        Ok(_) => {
            log::debug!("Successfully finished `payment` at {now:?}");
        }
        Err(e) => {
            log::error!("Failed `payment` at {now:?}: {e}");
        }
    }
    res
}

#[derive(SpacetimeType, Clone)]
struct PaymentRequest {
    terminal_warehouse_id: WarehouseId,
    terminal_district_id: u8,
    customer_warehouse_id: WarehouseId,
    customer_district_id: u8,
    customer_selector: CustomerSelector,
    payment_amount_cents: i64,
    now: Timestamp,
}

#[procedure]
fn process_remote_payment(ctx: &mut ProcedureContext, request: PaymentRequest) -> Result<Customer, String> {
    ctx.try_with_tx(|tx| {
        let customer = resolve_customer(
            tx,
            request.customer_warehouse_id,
            request.customer_district_id,
            &request.customer_selector,
        )?;
        Ok(update_customer(tx, &request, customer))
    })
}

fn update_customer(tx: &TxContext, request: &PaymentRequest, customer: Customer) -> Customer {
    let mut updated_customer = Customer {
        c_balance_cents: customer.c_balance_cents - request.payment_amount_cents,
        c_ytd_payment_cents: customer.c_ytd_payment_cents + request.payment_amount_cents,
        c_payment_cnt: customer.c_payment_cnt + 1,
        ..customer
    };

    if updated_customer.c_credit == "BC" {
        let prefix = format!(
            "{} {} {} {} {} {} {}|",
            updated_customer.c_id,
            updated_customer.c_d_id,
            updated_customer.c_w_id,
            request.terminal_district_id,
            request.terminal_warehouse_id,
            request.payment_amount_cents,
            request.now.to_micros_since_unix_epoch()
        );
        updated_customer.c_data = format!("{prefix}{}", updated_customer.c_data);
        updated_customer.c_data.truncate(MAX_C_DATA_LEN);
    }
    tx.db.customer().customer_key().update(updated_customer.clone());
    updated_customer
}
