use super::col_type::ColType;

#[derive(Debug)]
pub struct Column {
    pub col_id: u32,
    pub col_type: ColType,
}
