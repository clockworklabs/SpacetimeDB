//! Version 11 module definitions: explicit function visibility with contextual defaults.
//!
//! Non-function sections retain their V10 wire shapes. V11 is a distinct top-level
//! variant, so hosts unaware of Internal visibility reject the entire definition.

use super::v10;
use super::v10::*;
use super::v9::Lifecycle;
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_sats::typespace::TypespaceBuilder;
use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, ProductType, SpacetimeType, Typespace};
use std::{
    any::TypeId,
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, SpacetimeType)]
#[sats(crate = crate)]
pub enum FunctionVisibilityV11 {
    Private,
    ClientCallable,
    Internal,
}

pub use FunctionVisibilityV11 as FunctionVisibility;

#[derive(Default, Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawModuleDefV11 {
    pub sections: Vec<RawModuleDefV11Section>,
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
#[non_exhaustive]
pub enum RawModuleDefV11Section {
    Typespace(Typespace),
    Types(Vec<RawTypeDefV10>),
    Tables(Vec<RawTableDefV10>),
    Reducers(Vec<RawReducerDefV11>),
    Procedures(Vec<RawProcedureDefV11>),
    Views(Vec<RawViewDefV10>),
    Schedules(Vec<RawScheduleDefV10>),
    LifeCycleReducers(Vec<RawLifeCycleReducerDefV10>),
    RowLevelSecurity(Vec<RawRowLevelSecurityDefV10>),
    CaseConversionPolicy(CaseConversionPolicy),
    ExplicitNames(ExplicitNames),
    HttpHandlers(Vec<RawHttpHandlerDefV10>),
    HttpRoutes(Vec<RawHttpRouteDefV10>),
    /// Module bindings capabilities, independent of function visibility.
    Capabilities(Vec<RawIdentifier>),
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawReducerDefV11 {
    pub source_name: RawIdentifier,
    pub params: ProductType,
    /// None selects the context default; Some preserves the author's selection.
    pub declared_visibility: Option<FunctionVisibility>,
    pub ok_return_type: AlgebraicType,
    pub err_return_type: AlgebraicType,
}

#[derive(Debug, Clone, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "test", derive(PartialEq, Eq, PartialOrd, Ord))]
pub struct RawProcedureDefV11 {
    pub source_name: RawIdentifier,
    pub params: ProductType,
    /// None selects the context default; Some preserves the author's selection.
    pub declared_visibility: Option<FunctionVisibility>,
    pub return_type: AlgebraicType,
}

/// Shares the unchanged V10 table/type builders while emitting only V11.
#[derive(Default)]
pub struct RawModuleDefV11Builder {
    inner: RawModuleDefV10Builder,
    type_map: BTreeMap<TypeId, AlgebraicTypeRef>,
    declared_visibility: BTreeMap<RawIdentifier, FunctionVisibility>,
    capabilities: Vec<RawIdentifier>,
}

impl Deref for RawModuleDefV11Builder {
    type Target = RawModuleDefV10Builder;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl DerefMut for RawModuleDefV11Builder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl RawModuleDefV11Builder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_type<T: SpacetimeType>(&mut self) -> AlgebraicType {
        TypespaceBuilder::add_type::<T>(self)
    }

    pub fn add_reducer_with_visibility(
        &mut self,
        name: impl Into<RawIdentifier>,
        params: ProductType,
        visibility: Option<FunctionVisibility>,
    ) {
        let name = name.into();
        self.inner.add_reducer(name.clone(), params);
        if let Some(visibility) = visibility {
            self.declared_visibility.insert(name, visibility);
        }
    }

    pub fn add_lifecycle_reducer_with_visibility(
        &mut self,
        lifecycle: Lifecycle,
        name: impl Into<RawIdentifier>,
        params: ProductType,
        visibility: Option<FunctionVisibility>,
    ) {
        let name = name.into();
        self.inner.add_lifecycle_reducer(lifecycle, name.clone(), params);
        if let Some(visibility) = visibility {
            self.declared_visibility.insert(name, visibility);
        }
    }

    pub fn add_procedure_with_visibility(
        &mut self,
        name: impl Into<RawIdentifier>,
        params: ProductType,
        return_type: AlgebraicType,
        visibility: Option<FunctionVisibility>,
    ) {
        let name = name.into();
        self.inner.add_procedure(name.clone(), params, return_type);
        if let Some(visibility) = visibility {
            self.declared_visibility.insert(name, visibility);
        }
    }

    pub fn add_capability(&mut self, capability: impl Into<RawIdentifier>) {
        self.capabilities.push(capability.into());
    }

    pub fn finish(self) -> RawModuleDefV11 {
        let declared = self.declared_visibility;
        let mut sections: Vec<_> = self
            .inner
            .finish()
            .sections
            .into_iter()
            .map(|section| match section {
                RawModuleDefV10Section::Typespace(value) => RawModuleDefV11Section::Typespace(value),
                RawModuleDefV10Section::Types(value) => RawModuleDefV11Section::Types(value),
                RawModuleDefV10Section::Tables(value) => RawModuleDefV11Section::Tables(value),
                RawModuleDefV10Section::Reducers(rows) => RawModuleDefV11Section::Reducers(
                    rows.into_iter()
                        .map(|row| RawReducerDefV11 {
                            declared_visibility: declared.get(&row.source_name).copied(),
                            source_name: row.source_name,
                            params: row.params,
                            ok_return_type: row.ok_return_type,
                            err_return_type: row.err_return_type,
                        })
                        .collect(),
                ),
                RawModuleDefV10Section::Procedures(rows) => RawModuleDefV11Section::Procedures(
                    rows.into_iter()
                        .map(|row| RawProcedureDefV11 {
                            declared_visibility: declared.get(&row.source_name).copied(),
                            source_name: row.source_name,
                            params: row.params,
                            return_type: row.return_type,
                        })
                        .collect(),
                ),
                RawModuleDefV10Section::Views(value) => RawModuleDefV11Section::Views(value),
                RawModuleDefV10Section::Schedules(value) => RawModuleDefV11Section::Schedules(value),
                RawModuleDefV10Section::LifeCycleReducers(value) => RawModuleDefV11Section::LifeCycleReducers(value),
                RawModuleDefV10Section::RowLevelSecurity(value) => RawModuleDefV11Section::RowLevelSecurity(value),
                RawModuleDefV10Section::CaseConversionPolicy(value) => {
                    RawModuleDefV11Section::CaseConversionPolicy(value)
                }
                RawModuleDefV10Section::ExplicitNames(value) => RawModuleDefV11Section::ExplicitNames(value),
                RawModuleDefV10Section::HttpHandlers(value) => RawModuleDefV11Section::HttpHandlers(value),
                RawModuleDefV10Section::HttpRoutes(value) => RawModuleDefV11Section::HttpRoutes(value),
            })
            .collect();
        if !self.capabilities.is_empty() {
            sections.push(RawModuleDefV11Section::Capabilities(self.capabilities));
        }
        RawModuleDefV11 { sections }
    }
}

impl TypespaceBuilder for RawModuleDefV11Builder {
    fn add(
        &mut self,
        typeid: TypeId,
        source_name: Option<&'static str>,
        make_ty: impl FnOnce(&mut Self) -> AlgebraicType,
    ) -> AlgebraicType {
        if let Some(reference) = self.type_map.get(&typeid) {
            return AlgebraicType::Ref(*reference);
        }
        let reference = self.inner.typespace_mut().add(AlgebraicType::unit());
        self.type_map.insert(typeid, reference);
        if let Some(name) = source_name {
            self.inner.types_mut().push(RawTypeDefV10 {
                source_name: v10::sats_name_to_scoped_name_v10(name),
                ty: reference,
                custom_ordering: true,
            });
        }
        let ty = make_ty(self);
        self.inner.typespace_mut()[reference] = ty;
        AlgebraicType::Ref(reference)
    }
}

impl RawModuleDefV11 {
    pub fn reducers(&self) -> impl Iterator<Item = &RawReducerDefV11> {
        self.sections
            .iter()
            .filter_map(|section| match section {
                RawModuleDefV11Section::Reducers(rows) => Some(rows),
                _ => None,
            })
            .flatten()
    }
    pub fn tables(&self) -> impl Iterator<Item = &RawTableDefV10> {
        self.sections
            .iter()
            .filter_map(|section| match section {
                RawModuleDefV11Section::Tables(rows) => Some(rows),
                _ => None,
            })
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RawModuleDef, SpacetimeType};

    #[derive(SpacetimeType)]
    #[sats(crate = crate)]
    enum LegacyRawModuleDef {
        V8BackCompat(crate::RawModuleDefV8),
        V9(super::super::v9::RawModuleDefV9),
        V10(RawModuleDefV10),
    }

    #[test]
    fn legacy_decoder_rejects_v11_instead_of_ignoring_visibility() {
        let bytes = crate::bsatn::to_vec(&RawModuleDef::V11(RawModuleDefV11::default())).unwrap();
        assert_eq!(bytes[0], 3);
        assert!(crate::bsatn::from_slice::<LegacyRawModuleDef>(&bytes).is_err());
    }
}
