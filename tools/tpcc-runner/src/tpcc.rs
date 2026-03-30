use rand::Rng;
use std::time::Duration;

use crate::summary::TransactionKind;

pub const DISTRICTS_PER_WAREHOUSE: u8 = 10;
pub const CUSTOMERS_PER_DISTRICT: u32 = 3_000;
pub const ITEMS: u32 = 100_000;
pub const NEW_ORDER_START: u32 = 2_101;

const LAST_NAME_PARTS: [&str; 10] = [
    "BAR", "OUGHT", "ABLE", "PRI", "PRES", "ESE", "ANTI", "CALLY", "ATION", "EING",
];

#[derive(Clone, Debug)]
pub struct RunConstants {
    pub c_last: u32,
    pub c_id: u32,
    pub order_line_item: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalAssignment {
    pub terminal_id: u32,
    pub warehouse_id: u32,
    pub district_id: u8,
}

pub fn terminal_id(warehouse_id: u32, district_id: u8) -> u32 {
    debug_assert!(warehouse_id > 0);
    debug_assert!((1..=DISTRICTS_PER_WAREHOUSE).contains(&district_id));
    (warehouse_id - 1) * u32::from(DISTRICTS_PER_WAREHOUSE) + u32::from(district_id)
}

pub fn choose_transaction<R: Rng>(rng: &mut R) -> TransactionKind {
    let roll = rng.random_range(1..=100);
    match roll {
        1..=45 => TransactionKind::NewOrder,
        46..=88 => TransactionKind::Payment,
        89..=92 => TransactionKind::OrderStatus,
        93..=96 => TransactionKind::Delivery,
        _ => TransactionKind::StockLevel,
    }
}

pub fn generate_run_constants<R: Rng>(rng: &mut R) -> RunConstants {
    RunConstants {
        c_last: rng.random_range(0..=255),
        c_id: rng.random_range(0..=1_023),
        order_line_item: rng.random_range(0..=8_191),
    }
}

pub fn nurand<R: Rng>(rng: &mut R, a: u32, x: u32, y: u32, c: u32) -> u32 {
    (((rng.random_range(0..=a) | rng.random_range(x..=y)) + c) % (y - x + 1)) + x
}

pub fn customer_id<R: Rng>(rng: &mut R, constants: &RunConstants) -> u32 {
    nurand(rng, 1_023, 1, CUSTOMERS_PER_DISTRICT, constants.c_id)
}

pub fn item_id<R: Rng>(rng: &mut R, constants: &RunConstants) -> u32 {
    nurand(rng, 8_191, 1, ITEMS, constants.order_line_item)
}

pub fn customer_last_name<R: Rng>(rng: &mut R, constants: &RunConstants) -> String {
    make_last_name(nurand(rng, 255, 0, 999, constants.c_last))
}

pub fn make_last_name(num: u32) -> String {
    let hundreds = ((num / 100) % 10) as usize;
    let tens = ((num / 10) % 10) as usize;
    let ones = (num % 10) as usize;
    format!(
        "{}{}{}",
        LAST_NAME_PARTS[hundreds], LAST_NAME_PARTS[tens], LAST_NAME_PARTS[ones]
    )
}

pub fn alpha_string<R: Rng>(rng: &mut R, min_len: usize, max_len: usize) -> String {
    let len = rng.random_range(min_len..=max_len);
    (0..len).map(|_| (b'A' + rng.random_range(0..26)) as char).collect()
}

pub fn numeric_string<R: Rng>(rng: &mut R, min_len: usize, max_len: usize) -> String {
    let len = rng.random_range(min_len..=max_len);
    (0..len).map(|_| (b'0' + rng.random_range(0..10)) as char).collect()
}

pub fn alpha_numeric_string<R: Rng>(rng: &mut R, min_len: usize, max_len: usize) -> String {
    let len = rng.random_range(min_len..=max_len);
    (0..len)
        .map(|_| {
            if rng.random_bool(0.5) {
                (b'A' + rng.random_range(0..26)) as char
            } else {
                (b'0' + rng.random_range(0..10)) as char
            }
        })
        .collect()
}

pub fn zip_code<R: Rng>(rng: &mut R) -> String {
    format!("{}11111", numeric_string(rng, 4, 4))
}

pub fn maybe_with_original<R: Rng>(rng: &mut R, min_len: usize, max_len: usize) -> String {
    let mut data = alpha_numeric_string(rng, min_len, max_len);
    if rng.random_bool(0.10) && data.len() >= 8 {
        let start = rng.random_range(0..=(data.len() - 8));
        data.replace_range(start..start + 8, "ORIGINAL");
    }
    data
}

pub fn pack_district_key(w_id: u32, d_id: u8) -> u32 {
    (w_id * 100) + u32::from(d_id)
}

pub fn pack_customer_key(w_id: u32, d_id: u8, c_id: u32) -> u64 {
    ((u64::from(w_id) * 100) + u64::from(d_id)) * 10_000 + u64::from(c_id)
}

pub fn pack_stock_key(w_id: u32, item_id: u32) -> u64 {
    u64::from(w_id) * 1_000_000 + u64::from(item_id)
}

pub fn pack_order_key(w_id: u32, d_id: u8, o_id: u32) -> u64 {
    ((u64::from(w_id) * 100) + u64::from(d_id)) * 10_000_000 + u64::from(o_id)
}

pub fn pack_order_line_key(w_id: u32, d_id: u8, o_id: u32, ol_number: u8) -> u64 {
    pack_order_key(w_id, d_id, o_id) * 100 + u64::from(ol_number)
}

pub fn keying_time(kind: TransactionKind, scale: f64) -> Duration {
    scaled_duration(
        match kind {
            TransactionKind::NewOrder => 18.0,
            TransactionKind::Payment => 3.0,
            TransactionKind::OrderStatus => 2.0,
            TransactionKind::Delivery => 2.0,
            TransactionKind::StockLevel => 2.0,
        },
        scale,
    )
}

pub fn think_time<R: Rng>(kind: TransactionKind, scale: f64, rng: &mut R) -> Duration {
    let mean_secs = match kind {
        TransactionKind::NewOrder => 12.0,
        TransactionKind::Payment => 12.0,
        TransactionKind::OrderStatus => 10.0,
        TransactionKind::Delivery => 5.0,
        TransactionKind::StockLevel => 5.0,
    };
    if scale <= 0.0 {
        return Duration::ZERO;
    }
    let mean_secs = mean_secs * scale;
    let uniform = rng.random_range(f64::MIN_POSITIVE..1.0);
    let sample = (-mean_secs * uniform.ln()).min(mean_secs * 10.0);
    Duration::from_secs_f64(sample)
}

fn scaled_duration(base_secs: f64, scale: f64) -> Duration {
    if scale <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(base_secs * scale)
    }
}
