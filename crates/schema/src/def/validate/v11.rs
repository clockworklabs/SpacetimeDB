//! V11 reuses V10 structural validation, then resolves declarations after schedules
//! and lifecycle assignments exist. No V11 metadata is decoded as a V10 module.
use super::Result;
use crate::{
    def::{FunctionVisibility, ModuleDef, RawModuleDefVersion},
    error::ValidationError,
};
use spacetimedb_lib::db::raw_def::{v10, v11};
use spacetimedb_sats::raw_identifier::RawIdentifier;
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub fn validate(def: v11::RawModuleDefV11) -> Result<ModuleDef> {
    let mut seen_sections = HashSet::new();
    let mut declared = BTreeMap::new();
    let mut sections = Vec::new();
    let mut capabilities = BTreeSet::new();
    for section in def.sections {
        if !seen_sections.insert(std::mem::discriminant(&section)) {
            return Err(ValidationError::DuplicateModuleSection {
                section: format!("{:?}", std::mem::discriminant(&section)),
            }
            .into());
        }
        if let v11::RawModuleDefV11Section::Capabilities(names) = section {
            if names.len() > 32 {
                return Err(ValidationError::InvalidModuleCapabilities.into());
            }
            for name in names {
                if name.is_empty()
                    || name.len() > 64
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
                    || !capabilities.insert(name)
                {
                    return Err(ValidationError::InvalidModuleCapabilities.into());
                }
            }
            continue;
        }
        sections.push(match section {
            v11::RawModuleDefV11Section::Reducers(rows) => v10::RawModuleDefV10Section::Reducers(
                rows.into_iter()
                    .map(|row| {
                        insert_declaration(&mut declared, &row.source_name, row.declared_visibility)?;
                        Ok(v10::RawReducerDefV10 {
                            source_name: row.source_name,
                            params: row.params,
                            visibility: v10::FunctionVisibility::ClientCallable,
                            ok_return_type: row.ok_return_type,
                            err_return_type: row.err_return_type,
                        })
                    })
                    .collect::<Result<_>>()?,
            ),
            v11::RawModuleDefV11Section::Procedures(rows) => v10::RawModuleDefV10Section::Procedures(
                rows.into_iter()
                    .map(|row| {
                        insert_declaration(&mut declared, &row.source_name, row.declared_visibility)?;
                        Ok(v10::RawProcedureDefV10 {
                            source_name: row.source_name,
                            params: row.params,
                            visibility: v10::FunctionVisibility::ClientCallable,
                            return_type: row.return_type,
                        })
                    })
                    .collect::<Result<_>>()?,
            ),
            v11::RawModuleDefV11Section::Typespace(value) => v10::RawModuleDefV10Section::Typespace(value),
            v11::RawModuleDefV11Section::Types(value) => v10::RawModuleDefV10Section::Types(value),
            v11::RawModuleDefV11Section::Tables(value) => v10::RawModuleDefV10Section::Tables(value),
            v11::RawModuleDefV11Section::Views(value) => v10::RawModuleDefV10Section::Views(value),
            v11::RawModuleDefV11Section::Schedules(value) => v10::RawModuleDefV10Section::Schedules(value),
            v11::RawModuleDefV11Section::LifeCycleReducers(value) => {
                v10::RawModuleDefV10Section::LifeCycleReducers(value)
            }
            v11::RawModuleDefV11Section::RowLevelSecurity(value) => {
                v10::RawModuleDefV10Section::RowLevelSecurity(value)
            }
            v11::RawModuleDefV11Section::CaseConversionPolicy(value) => {
                v10::RawModuleDefV10Section::CaseConversionPolicy(value)
            }
            v11::RawModuleDefV11Section::ExplicitNames(value) => v10::RawModuleDefV10Section::ExplicitNames(value),
            v11::RawModuleDefV11Section::HttpHandlers(value) => v10::RawModuleDefV10Section::HttpHandlers(value),
            v11::RawModuleDefV11Section::HttpRoutes(value) => v10::RawModuleDefV10Section::HttpRoutes(value),
            _ => unreachable!("all V11 sections are handled"),
        });
    }
    let mut module = super::v10::validate(v10::RawModuleDefV10 { sections })?;
    for reducer in module.reducers.values_mut() {
        let source_name = RawIdentifier::from(reducer.accessor_name.clone());
        let declaration = declared.get(&source_name).copied().flatten();
        if reducer.lifecycle.is_some() {
            if declaration.is_some_and(|visibility| visibility != v11::FunctionVisibility::Internal) {
                return Err(ValidationError::InvalidLifecycleVisibility { function: source_name }.into());
            }
            reducer.visibility = FunctionVisibility::Internal;
        } else if let Some(visibility) = declaration {
            reducer.visibility = visibility.into();
        }
    }
    for procedure in module.procedures.values_mut() {
        if let Some(visibility) = declared
            .get(&RawIdentifier::from(procedure.accessor_name.clone()))
            .copied()
            .flatten()
        {
            procedure.visibility = visibility.into();
        }
    }
    module.raw_module_def_version = RawModuleDefVersion::V11;
    module.capabilities = capabilities;
    Ok(module)
}

fn insert_declaration(
    declared: &mut BTreeMap<RawIdentifier, Option<v11::FunctionVisibility>>,
    name: &RawIdentifier,
    visibility: Option<v11::FunctionVisibility>,
) -> Result<()> {
    if declared.insert(name.clone(), visibility).is_some() {
        return Err(ValidationError::DuplicateName { name: name.clone() }.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_lib::{db::raw_def::v9, RawModuleDef, ScheduleAt};
    use spacetimedb_sats::{AlgebraicType, ProductType};
    use v11::{FunctionVisibility as Declared, RawModuleDefV11Builder};

    fn scheduled_module(visibility: Option<Declared>, procedure: bool) -> ModuleDef {
        let mut builder = RawModuleDefV11Builder::new();
        let at = builder.add_type::<ScheduleAt>();
        let row = builder
            .build_table_with_new_type(
                "Jobs",
                ProductType::from([("id", AlgebraicType::U64), ("at", at)]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(v9::btree(0), "jobs_id_idx")
            .finish();
        let params = ProductType::from([("job", AlgebraicType::Ref(row))]);
        if procedure {
            builder.add_procedure_with_visibility("run_job", params, AlgebraicType::unit(), visibility);
        } else {
            builder.add_reducer_with_visibility("run_job", params, visibility);
        }
        builder.add_schedule("Jobs", 1, "run_job");
        builder.finish().try_into().unwrap()
    }

    #[test]
    fn explicit_scheduled_visibility_overrides_the_private_default() {
        for procedure in [false, true] {
            for (selection, expected) in [
                (None, FunctionVisibility::Private),
                (Some(Declared::Private), FunctionVisibility::Private),
                (Some(Declared::Internal), FunctionVisibility::Internal),
                (Some(Declared::ClientCallable), FunctionVisibility::ClientCallable),
            ] {
                let module = scheduled_module(selection, procedure);
                let visibility = if procedure {
                    &module.procedure("run_job").unwrap().visibility
                } else {
                    &module.reducer("run_job").unwrap().visibility
                };
                assert_eq!(visibility, &expected);
                assert_eq!(module.raw_module_def_version(), RawModuleDefVersion::V11);
            }
        }
    }

    #[test]
    fn ordinary_defaults_and_lifecycle_restrictions() {
        let mut builder = RawModuleDefV11Builder::new();
        builder.add_reducer("ordinary", ProductType::unit());
        builder.add_procedure("ordinary_procedure", ProductType::unit(), AlgebraicType::unit());
        builder.add_lifecycle_reducer(v9::Lifecycle::Init, "initialize", ProductType::unit());
        let module: ModuleDef = builder.finish().try_into().unwrap();
        assert!(module.reducer("ordinary").unwrap().visibility.is_client_callable());
        assert!(module
            .procedure("ordinary_procedure")
            .unwrap()
            .visibility
            .is_client_callable());
        assert!(module.reducer("initialize").unwrap().visibility.is_internal());
        for selection in [Declared::Private, Declared::ClientCallable] {
            let mut builder = RawModuleDefV11Builder::new();
            builder.add_lifecycle_reducer_with_visibility(
                v9::Lifecycle::Init,
                "initialize",
                ProductType::unit(),
                Some(selection),
            );
            assert!(ModuleDef::try_from(builder.finish())
                .unwrap_err()
                .to_string()
                .contains("must have Internal visibility"));
        }
        let mut builder = RawModuleDefV11Builder::new();
        builder.add_lifecycle_reducer_with_visibility(
            v9::Lifecycle::Init,
            "initialize",
            ProductType::unit(),
            Some(Declared::Internal),
        );
        assert!(ModuleDef::try_from(builder.finish()).is_ok());
    }

    #[test]
    fn duplicate_definitions_sections_and_lifecycles_are_rejected() {
        let mut builder = RawModuleDefV11Builder::new();
        builder.add_reducer("same", ProductType::unit());
        builder.add_procedure("same", ProductType::unit(), AlgebraicType::unit());
        assert!(ModuleDef::try_from(builder.finish()).is_err());
        let raw = v11::RawModuleDefV11 {
            sections: vec![
                v11::RawModuleDefV11Section::Reducers(vec![]),
                v11::RawModuleDefV11Section::Reducers(vec![]),
            ],
        };
        assert!(ModuleDef::try_from(raw)
            .unwrap_err()
            .to_string()
            .contains("repeated V11 section"));
        let mut builder = RawModuleDefV11Builder::new();
        builder.add_lifecycle_reducer(v9::Lifecycle::Init, "a", ProductType::unit());
        builder.add_lifecycle_reducer(v9::Lifecycle::Init, "b", ProductType::unit());
        assert!(ModuleDef::try_from(builder.finish()).is_err());
    }

    #[test]
    fn resolved_v11_roundtrips_without_reapplying_defaults_and_rejects_legacy_exports() {
        for selection in [None, Some(Declared::Internal), Some(Declared::ClientCallable)] {
            let module = scheduled_module(selection, false);
            assert!(v9::RawModuleDefV9::try_from(module.clone()).is_err());
            assert!(v10::RawModuleDefV10::try_from(module.clone()).is_err());
            let RawModuleDef::V11(raw) = module.clone().into_raw() else {
                panic!("lost source version")
            };
            assert!(raw.reducers().all(|reducer| reducer.declared_visibility.is_some()));
            let bytes = spacetimedb_lib::bsatn::to_vec(&RawModuleDef::V11(raw)).unwrap();
            let roundtrip: RawModuleDef = spacetimedb_lib::bsatn::from_slice(&bytes).unwrap();
            let roundtrip: ModuleDef = roundtrip.try_into().unwrap();
            assert_eq!(
                roundtrip.reducer("run_job").unwrap().visibility,
                module.reducer("run_job").unwrap().visibility
            );
            assert_eq!(roundtrip.raw_module_def_version(), RawModuleDefVersion::V11);
        }
    }

    #[test]
    fn legacy_v9_schedules_stay_public_and_v10_schedules_stay_private() {
        let mut builder = v9::RawModuleDefV9Builder::new();
        let at = builder.add_type::<ScheduleAt>();
        let row = builder
            .build_table_with_new_type(
                "jobs",
                ProductType::from([("id", AlgebraicType::U64), ("at", at)]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index(v9::btree(0), "jobs_id_idx")
            .with_schedule("run_job", 1)
            .finish();
        builder.add_reducer("run_job", ProductType::from([("job", row.into())]), None);
        let v9: ModuleDef = builder.finish().try_into().unwrap();
        assert!(v9.reducer("run_job").unwrap().visibility.is_client_callable());
        assert!(matches!(v9.into_raw(), RawModuleDef::V9(_)));

        let mut builder = v10::RawModuleDefV10Builder::new();
        let at = builder.add_type::<ScheduleAt>();
        let row = builder
            .build_table_with_new_type(
                "jobs",
                ProductType::from([("id", AlgebraicType::U64), ("at", at)]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(v9::btree(0), "jobs_id_idx")
            .finish();
        builder.add_reducer("run_job", ProductType::from([("job", row.into())]));
        builder.add_schedule("jobs", 1, "run_job");
        let v10: ModuleDef = builder.finish().try_into().unwrap();
        assert!(v10.reducer("run_job").unwrap().visibility.is_private());
        assert!(matches!(v10.into_raw(), RawModuleDef::V10(_)));
    }

    #[test]
    fn capabilities_are_explicit_bounded_and_preserved() {
        let bare: ModuleDef = RawModuleDefV11Builder::new().finish().try_into().unwrap();
        assert!(!bare.supports_hosted_auth_v1());
        let mut builder = RawModuleDefV11Builder::new();
        builder.add_capability("hosted_auth_v1");
        let module: ModuleDef = builder.finish().try_into().unwrap();
        assert!(module.supports_hosted_auth_v1());
        let reloaded: ModuleDef = module.into_raw().try_into().unwrap();
        assert!(reloaded.supports_hosted_auth_v1());
        for names in [
            vec!["".to_string()],
            vec!["Uppercase".to_string()],
            vec!["with-dash".to_string()],
            vec!["a".repeat(65)],
            vec!["duplicate".to_string(); 2],
            (0..33).map(|i| format!("cap_{i}")).collect(),
        ] {
            let mut builder = RawModuleDefV11Builder::new();
            for name in names {
                builder.add_capability(name);
            }
            assert!(ModuleDef::try_from(builder.finish()).is_err());
        }
    }

    #[test]
    fn narrowing_function_visibility_is_a_reported_client_break() {
        let module = |visibility| {
            let mut builder = RawModuleDefV11Builder::new();
            builder.add_reducer_with_visibility("run_now", ProductType::unit(), Some(visibility));
            ModuleDef::try_from(builder.finish()).unwrap()
        };
        let public = module(Declared::ClientCallable);
        let internal = module(Declared::Internal);
        let plan = crate::auto_migrate::ponder_migrate(&public, &internal).unwrap();
        assert!(plan.breaks_client());
        let display = plan
            .pretty_print(crate::auto_migrate::PrettyPrintStyle::NoColor)
            .unwrap();
        assert!(display.contains("run_now"));
        assert!(display.contains("Internal"));
        assert!(!crate::auto_migrate::ponder_migrate(&internal, &public)
            .unwrap()
            .breaks_client());
    }

    #[test]
    fn visibility_authority_is_cumulative_without_elevating_the_owner() {
        for (visibility, external, owner, internal) in [
            (FunctionVisibility::Internal, false, false, true),
            (FunctionVisibility::Private, false, true, true),
            (FunctionVisibility::ClientCallable, true, true, true),
        ] {
            assert_eq!(visibility.allows_invocation(false, false), external);
            assert_eq!(visibility.allows_invocation(false, true), owner);
            assert_eq!(visibility.allows_invocation(true, false), internal);
        }
    }
}
