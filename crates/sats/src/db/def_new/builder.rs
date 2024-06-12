use super::*;

const BUILDER_SANITY_MASK: u32 = 0x7FFF_FFFF;

/// A builder for a `DatabaseDef`.
/// Remark: the Refs returned by methods on this struct are invalidated once `.build()` is called.
/// Remark: using an invalid Ref with this data structure will panic.
pub struct DatabaseDefBuilder {
    /// The database definition in progress.
    /// This MAY NOT MEET all of the constraints on DatabaseDef; in particular, its key lookup tables are empty.
    /// In addition, its sanity value is not the same as that which will be used after `.build()` is called.
    /// This is because `.build()` applies a canonical ordering to Refs.
    in_progress: DatabaseDef,

    /// The sanity value that will be used after the DatabaseDef is built.
    final_sanity: u32,
}

impl DatabaseDefBuilder {
    // invariant: looking up a constructed `Ref` in the `in_progress` table will return the correct value until `build` is called, after
    // which that `Ref` is invalidated.
    // The key lookup tables are updated in `build`. They cannot be updated before `build` because we allow adding duplicate tables while constructing a schema.
    // In addition, the "position" field of `ColumnDef`s is invalid until `build` is called.
    // However, the "columns" field of `TableDef` is maintained correctly here, although it is sorted in `build`.

    /// Create a new `DatabaseDefBuilder`.
    /// `sanity` should only be set if deserializing.
    fn new_internal(sanity: Option<u32>) -> Self {
        let final_sanity = match sanity {
            Some(sanity) => sanity,
            None => rand::random(),
        };
        Self {
            in_progress: DatabaseDef {
                tables: Vec::new(),
                columns: Vec::new(),
                sequences: Vec::new(),
                schedules: Vec::new(),
                unique_constraints: Vec::new(),
                indexes: Vec::new(),

                tables_by_key: HashMap::new(),
                columns_by_key: HashMap::new(),
                sequences_by_key: HashMap::new(),
                schedules_by_key: HashMap::new(),
                unique_constraints_by_key: HashMap::new(),
                indexes_by_key: HashMap::new(),

                sanity: final_sanity ^ BUILDER_SANITY_MASK,
            },
            final_sanity,
        }
    }

    /// Add a new element to its storage and return a builder Ref to it.
    /// Does not update indexes.
    fn add<T: SchemaEntity>(&mut self, entity: T) -> Ref<T> {
        let id = T::storage(&self.in_progress).len() as u32;
        let ref_ = Ref::new(self.in_progress.sanity, id);
        T::storage_mut(&mut self.in_progress).push(entity);
        ref_
    }

    fn index_mut<T: SchemaEntity>(&mut self, entity: Ref<T>) -> &mut T {
        assert_eq!(
            self.in_progress.sanity, entity.sanity,
            "Using a reference from a different schema is forbidden: db.sanity {:?} != index.sanity {:?}",
            self.in_progress.sanity, entity.sanity
        );
        &mut T::storage_mut(&mut self.in_progress)[entity.id as usize]
    }

    /// Create a new DatabaseDefBuilder.
    /// Starts out empty.
    pub fn new() -> Self {
        Self::new_internal(None)
    }

    /// Add a table to the database.
    /// The returned Ref is invalidated once `.build()` is called.
    pub fn add_table(&mut self, name: Identifier) -> Ref<TableDef> {
        self.add(TableDef {
            name,
            fields: Vec::new(),
            _rest: (),
        })
    }

    /// Add a column to the table in the database.
    /// The order columns are added to the table does not affect the final result.
    /// The returned Ref is invalidated once `.build()` is called.
    pub fn add_column(&mut self, table: Ref<TableDef>, name: Identifier, type_: AlgebraicType) -> Ref<ColumnDef> {
        let ref_ = self.add(ColumnDef {
            table,
            name,
            type_,
            position: ColId(0), // This will be updated later.
            _rest: (),
        });
        self.index_mut(table).fields.push(ref_);
        ref_
    }

    /// Add a sequence to the database.
    pub fn add_sequence(
        &mut self,
        column: Ref<ColumnDef>,
        start_type: AlgebraicType,
        start: AlgebraicValue,
    ) -> Ref<SequenceDef> {
        self.add(SequenceDef {
            column,
            start,
            start_type,
            _rest: (),
        })
    }
}
