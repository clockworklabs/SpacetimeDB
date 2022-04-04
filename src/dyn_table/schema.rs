
pub enum DataLayout {
    SOA,
    AOS,
    BOTH
}

pub enum IndexType {
    Hash,
    BTree,
}

pub enum ConstraintType {
    Unique,
}

#[derive(PartialEq)]
pub enum ColType {
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    Bool,
    // F32,
    // F64,
}

impl ColType {
    pub fn size(&self) -> u8 {
        match self {
            ColType::U8 => 1,
            ColType::U16 => 2,
            ColType::U32 => 4,
            ColType::U64 => 8,
            ColType::U128 => 16,
            ColType::I8 => 1,
            ColType::I16 => 2,
            ColType::I32 => 4,
            ColType::I64 => 8,
            ColType::I128 => 16,
            ColType::Bool => 1,
            // ColType::F32 => 4,
            // ColType::F64 => 8,
        }
    }
}

pub struct Column {
    pub col_type: ColType,
    pub name: String,
    pub constraints: Vec<ConstraintType>,
    pub indexes: Vec<IndexType>,
}

pub struct Schema {
    pub columns: Vec<Column>,
    pub data_layout: DataLayout,
}

impl Schema {

    pub fn column_by_name(&self, col_name: &str) -> &Column {
        self.columns.iter().find(|c| c.name == col_name).unwrap()
    }

    pub fn column_index_by_name(&self, col_name: &str) -> usize {
        self.columns.iter().position(|c| c.name == col_name).unwrap()
    }

    pub fn row_size(&self) -> usize {
        let mut size: usize = 0;
        for c in &self.columns {
            size += c.col_type.size() as usize
        }
        size
    }

}
