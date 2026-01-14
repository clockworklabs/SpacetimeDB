use std::any::TypeId;
use std::collections::{btree_map, BTreeMap};

use spacetimedb_primitives::ColList;
use spacetimedb_sats::typespace::TypespaceBuilder;
use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType, SpacetimeType, Typespace};

use crate::db::raw_def::v8::RawIndexDefV8;
use crate::db::raw_def::v9::{
    sats_name_to_scoped_name, RawConstraintDataV9, RawConstraintDefV9, RawIdentifier, RawIndexDefV9, RawProcedureDefV9,
    RawSequenceDefV9, RawTableDefBuilder, RawTableDefV9, RawTypeDefV9, RawViewDefV9, TableAccess, TableType,
};

/// The main module definition for V10
#[derive(Default)]
pub struct RawModuleDefV10 {
    pub sections: Vec<RawModuleDefV10Section>,
}

type RawTypeDefV10 = RawTypeDefV9;

#[derive(Debug, Clone)]
pub enum RawModuleDefV10Section {
    Typespace(Typespace),
    Types(Vec<RawTypeDefV10>),
    Tables(Vec<RawTableDefV10>),
    Indexes(Vec<RawIndexDefV10>),
    Reducers(Vec<RawReducerDefV10>),
    Procedures(Vec<RawProceduresDefV10>),
    Views(Vec<RawViewDefV10>),
    Schedules(Vec<RawScheduleDefV10>),
    LifeCycleReducers(Vec<RawLifeCycleReducerDefV10>),
}

/// Table definitions
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct RawTableDefV10 {
    pub name: RawIdentifier,
    pub product_type_ref: AlgebraicTypeRef,
    pub primary_key: ColList,
    pub indexes: Vec<RawIndexDefV10>,
    pub constraints: Vec<RawConstraintDefV10>,
    pub sequences: Vec<RawSequenceDefV10>,
    pub table_type: TableType,
    pub table_access: TableAccess,
}

/// Reducer definition
#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
pub struct RawReducerDefV10 {
    pub name: RawIdentifier,
    pub params: ProductType,
}

/// Schedule definition
#[derive(Debug, Clone)]
pub struct RawScheduleDefV10 {
    pub table_name: RawIdentifier,
    pub function_name: RawIdentifier,
}

/// Life cycle reducer
#[derive(Debug, Clone)]
pub struct RawLifeCycleReducerDefV10 {
    pub lifecycle_spec: RawLifeCycleSpecV10,
    pub function_name: RawIdentifier,
}

#[derive(Debug, Clone)]
pub enum RawLifeCycleSpecV10 {
    Create,
    Update,
    Delete,
}

// Aliases for consistency
type RawIndexDefV10 = RawIndexDefV9;
type RawProceduresDefV10 = RawProcedureDefV9;
type RawViewDefV10 = RawViewDefV9;
type RawConstraintDefV10 = RawConstraintDefV9;
type RawSequenceDefV10 = RawSequenceDefV9;

/// Builder for a V10 module
#[derive(Default)]
pub struct RawModuleDefV10Builder {
    module: RawModuleDefV10,
    type_map: BTreeMap<TypeId, AlgebraicTypeRef>,
}

impl RawModuleDefV10Builder {
    pub fn new() -> Self {
        Default::default()
    }

    /// Get mutable access to Typespace, creating it if missing
    fn typespace_mut(&mut self) -> &mut Typespace {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Typespace(_)))
            .unwrap_or_else(|| {
                self.module
                    .sections
                    .push(RawModuleDefV10Section::Typespace(Typespace::EMPTY.clone()));
                self.module.sections.len() - 1
            });

        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Typespace(ts) => ts,
            _ => panic!("Just ensured Typespace section exists"),
        }
    }
    /// Get mutable access to tables section
    fn tables_mut(&mut self) -> &mut Vec<RawTableDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Tables(_)))
            .unwrap_or_else(|| {
                self.module.sections.push(RawModuleDefV10Section::Tables(Vec::new()));
                self.module.sections.len() - 1
            });
        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Tables(tables) => tables,
            _ => panic!("Just ensured Tables section exists"),
        }
    }

    /// Get mutable access to reducers section
    fn reducers_mut(&mut self) -> &mut Vec<RawReducerDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Reducers(_)))
            .unwrap_or_else(|| {
                self.module.sections.push(RawModuleDefV10Section::Reducers(Vec::new()));
                self.module.sections.len() - 1
            });
        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Reducers(reducers) => reducers,
            _ => panic!("Just ensured Reducers section exists"),
        }
    }

    /// Get mutable access to procedures section
    fn procedures_mut(&mut self) -> &mut Vec<RawProceduresDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Procedures(_)))
            .unwrap_or_else(|| {
                self.module
                    .sections
                    .push(RawModuleDefV10Section::Procedures(Vec::new()));
                self.module.sections.len() - 1
            });
        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Procedures(procedures) => procedures,
            _ => panic!("Just ensured Procedures section exists"),
        }
    }

    fn types_mut(&mut self) -> &mut Vec<RawTypeDefV10> {
        let idx = self
            .module
            .sections
            .iter()
            .position(|s| matches!(s, RawModuleDefV10Section::Types(_)))
            .unwrap_or_else(|| {
                self.module.sections.push(RawModuleDefV10Section::Types(Vec::new()));
                self.module.sections.len() - 1
            });
        match &mut self.module.sections[idx] {
            RawModuleDefV10Section::Types(types) => types,
            _ => panic!("Just ensured Types section exists"),
        }
    }

    /// Add a type
    pub fn add_type<T: SpacetimeType>(&mut self) -> AlgebraicType {
        TypespaceBuilder::add_type::<T>(self)
    }

    /// Add a table
    pub fn add_table(&mut self, table: RawTableDefV10) {
        self.tables_mut().push(table);
    }

    /// Add a reducer
    pub fn add_reducer(&mut self, reducer: RawReducerDefV10) {
        self.reducers_mut().push(reducer);
    }

    /// Add a procedure
    pub fn add_procedure(&mut self, procedure: RawProceduresDefV10) {
        self.procedures_mut().push(procedure);
    }

    /// Finish building and return module
    pub fn finish(self) -> RawModuleDefV10 {
        self.module
    }
}
/// Implement TypespaceBuilder for V10

impl TypespaceBuilder for RawModuleDefV10Builder {
    fn add(
        &mut self,
        typeid: TypeId,
        name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType {
        if let btree_map::Entry::Occupied(o) = self.type_map.entry(typeid) {
            AlgebraicType::Ref(*o.get())
        } else {
            let slot_ref = {
                let ts = self.typespace_mut();
                // Bind a fresh alias to the unit type.
                let slot_ref = ts.add(AlgebraicType::unit());
                // Relate `typeid -> fresh alias`.
                self.type_map.insert(typeid, slot_ref);

                // Alias provided? Relate `name -> slot_ref`.
                if let Some(sats_name) = name {
                    let name = sats_name_to_scoped_name(sats_name);

                    self.types_mut().push(RawTypeDefV10 {
                        name,
                        ty: slot_ref,
                        // TODO(1.0): we need to update the `TypespaceBuilder` trait to include
                        // a `custom_ordering` parameter.
                        // For now, we assume all types have custom orderings, since the derive
                        // macro doesn't know about the default ordering yet.
                        custom_ordering: true,
                    });
                }
                slot_ref
            };

            // Borrow of `v` has ended here, so we can now convince the borrow checker.
            let ty = make_ty(self);
            self.typespace_mut()[slot_ref] = ty;
            AlgebraicType::Ref(slot_ref)
        }
    }
}

/// Builder for a table
pub struct RawTableDefBuilderV10<'a> {
    module: &'a mut RawModuleDefV10,
    table: RawTableDefV10,
}

impl<'a> RawTableDefBuilderV10<'a> {
    pub fn new(module: &'a mut RawModuleDefV10, table: RawTableDefV10) -> Self {
        Self { module, table }
    }

    pub fn with_type(mut self, table_type: TableType) -> Self {
        self.table.table_type = table_type;
        self
    }

    pub fn with_access(mut self, access: TableAccess) -> Self {
        self.table.table_access = access;
        self
    }

    pub fn finish(self) {
        self.module
            .sections
            .push(RawModuleDefV10Section::Tables(vec![self.table]));
    }
}
