pub struct Column {
    pub col_type: ColType,
    pub name: String,
    pub constraints: Vec<ConstraintType>,
    pub indexes: Vec<IndexType>,
}

pub struct Schema {
    pub name: String,
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
