use spacetimedb_lib::bsatn::to_vec;
use spacetimedb_lib::ProductValue;

pub type Row = ProductValue;

pub fn row_to_bytes(row: &Row) -> Vec<u8> {
    to_vec(row).expect("row serialization must not fail")
}

pub fn row_projection_to_bytes(row: &Row, columns: &[usize]) -> Vec<u8> {
    let elements = columns
        .iter()
        .map(|&column| row.elements[column].clone())
        .collect::<Vec<_>>();
    row_to_bytes(&ProductValue {
        elements: elements.into_boxed_slice(),
    })
}

pub fn normalize_rows(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by_key(row_to_bytes);
    rows
}
