//! Module type registration system.
//!
//! This module rewrites the core type registration logic from the C++
//! `module_type_registration.cpp` in idiomatic Rust. It handles:
//!
//! - Type registration and caching
//! - Circular reference detection
//! - Primitive, special, option, result, and ScheduleAt type classification
//! - Complex type (struct/enum) processing for the module typespace

use spacetimedb_lib::db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Section, RawScopedTypeNameV10, RawTypeDefV10};
use spacetimedb_lib::RawModuleDef;
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_sats::typespace::Typespace;
use spacetimedb_sats::{
    AlgebraicType, AlgebraicTypeRef, ArrayType, ProductType, ProductTypeElement, SumType, SumTypeVariant,
};
use std::collections::{HashMap, HashSet};

/// Error state from type registration
#[derive(Debug, Clone)]
pub struct RegistrationError {
    pub message: String,
    pub type_description: String,
}

/// Module type registration system.
///
/// This is the Rust equivalent of C++ `ModuleTypeRegistration`.
/// It owns a typespace and type defs, and handles type registration.
pub struct ModuleTypeRegistration {
    /// Cache mapping type names to typespace indices
    type_name_cache: HashMap<String, AlgebraicTypeRef>,
    /// Set of types currently being registered (for cycle detection)
    types_being_registered: HashSet<String>,
    /// Error if registration failed
    error: Option<RegistrationError>,
    /// The typespace being built
    typespace: Typespace,
    /// Type definition exports
    type_defs: Vec<RawTypeDefV10>,
}

impl Default for ModuleTypeRegistration {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleTypeRegistration {
    pub fn new() -> Self {
        Self {
            type_name_cache: HashMap::new(),
            types_being_registered: HashSet::new(),
            error: None,
            typespace: Typespace { types: Vec::new() },
            type_defs: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.type_name_cache.clear();
        self.types_being_registered.clear();
        self.error = None;
        self.typespace.types.clear();
        self.type_defs.clear();
    }

    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    pub fn error(&self) -> Option<&RegistrationError> {
        self.error.as_ref()
    }

    /// Access the typespace
    pub fn typespace(&self) -> &Typespace {
        &self.typespace
    }

    /// Access the type definitions
    pub fn type_defs(&self) -> &[RawTypeDefV10] {
        &self.type_defs
    }

    // ============================================================
    // Type classification (all `&self` free, pure functions)
    // ============================================================

    pub fn is_primitive(ty: &AlgebraicType) -> bool {
        ty.is_bool()
            || ty.is_u8()
            || ty.is_u16()
            || ty.is_u32()
            || ty.is_u64()
            || ty.is_u128()
            || ty.is_u256()
            || ty.is_i8()
            || ty.is_i16()
            || ty.is_i32()
            || ty.is_i64()
            || ty.is_i128()
            || ty.is_i256()
            || ty.is_f32()
            || ty.is_f64()
            || ty.is_string()
    }

    pub fn is_special_type(ty: &AlgebraicType) -> bool {
        let Some(product) = ty.as_product() else { return false };
        if product.elements.len() != 1 {
            return false;
        }
        product.elements[0].has_name("__identity__")
            || product.elements[0].has_name("__connection_id__")
            || product.elements[0].has_name("__timestamp_micros_since_unix_epoch__")
            || product.elements[0].has_name("__time_duration_micros__")
            || product.elements[0].has_name("__uuid__")
    }

    pub fn is_option_type(ty: &AlgebraicType) -> bool {
        let Some(sum) = ty.as_sum() else { return false };
        sum.variants.len() == 2 && sum.variants[0].has_name("some") && sum.variants[1].has_name("none")
    }

    pub fn is_result_type(ty: &AlgebraicType) -> bool {
        let Some(sum) = ty.as_sum() else { return false };
        sum.variants.len() == 2 && sum.variants[0].has_name("ok") && sum.variants[1].has_name("err")
    }

    pub fn is_schedule_at_type(ty: &AlgebraicType) -> bool {
        let Some(sum) = ty.as_sum() else { return false };
        sum.variants.len() == 2 && sum.variants[0].has_name("Interval") && sum.variants[1].has_name("Time")
    }

    pub fn is_unit_type(ty: &AlgebraicType) -> bool {
        ty.as_product().is_some_and(|p| p.elements.is_empty())
    }

    // ============================================================
    // Type conversion helpers
    // ============================================================

    fn convert_unit_type() -> AlgebraicType {
        AlgebraicType::Product(ProductType {
            elements: Vec::new().into(),
        })
    }

    fn convert_array(&mut self, elem: AlgebraicType) -> AlgebraicType {
        let elem = self.register_type(elem, "");
        AlgebraicType::Array(ArrayType {
            elem_ty: Box::new(elem),
        })
    }

    fn convert_special_type(&mut self, ty: &AlgebraicType) -> AlgebraicType {
        let Some(product) = ty.as_product() else {
            return AlgebraicType::U8;
        };
        let elements: Box<[_]> = product
            .elements
            .iter()
            .map(|f| ProductTypeElement {
                name: f.name.clone(),
                algebraic_type: self.register_type(f.algebraic_type.clone(), ""),
            })
            .collect();
        AlgebraicType::Product(ProductType { elements })
    }

    fn convert_inline_sum(&mut self, ty: &AlgebraicType) -> AlgebraicType {
        let Some(sum) = ty.as_sum() else {
            return AlgebraicType::U8;
        };
        let variants: Vec<_> = sum
            .variants
            .iter()
            .map(|v| SumTypeVariant {
                name: v.name.clone(),
                algebraic_type: self.register_type(v.algebraic_type.clone(), ""),
            })
            .collect();
        AlgebraicType::Sum(SumType {
            variants: variants.into(),
        })
    }

    // ============================================================
    // Name handling
    // ============================================================

    pub fn extract_type_name(cpp_type: &str) -> String {
        let mut name = cpp_type;
        if let Some(pos) = name.rfind("::") {
            name = &name[pos + 2..];
        }
        if let Some(pos) = name.find('<') {
            name = &name[..pos];
        }
        name.to_owned()
    }

    pub fn parse_namespace_and_name(qualified_name: &str) -> (Vec<RawIdentifier>, RawIdentifier) {
        if let Some(last_dot) = qualified_name.rfind('.') {
            let ns = &qualified_name[..last_dot];
            let name = RawIdentifier::new(&qualified_name[last_dot + 1..]);
            let scope: Vec<_> = ns
                .split('.')
                .filter(|s| !s.is_empty())
                .map(RawIdentifier::new)
                .collect();
            (scope, name)
        } else {
            (Vec::new(), RawIdentifier::new(qualified_name))
        }
    }

    pub fn describe_type(ty: &AlgebraicType) -> String {
        match ty {
            AlgebraicType::Ref(r) => format!("Ref({})", r.0),
            AlgebraicType::Bool => "Bool".into(),
            AlgebraicType::I8 => "I8".into(),
            AlgebraicType::U8 => "U8".into(),
            AlgebraicType::I16 => "I16".into(),
            AlgebraicType::U16 => "U16".into(),
            AlgebraicType::I32 => "I32".into(),
            AlgebraicType::U32 => "U32".into(),
            AlgebraicType::I64 => "I64".into(),
            AlgebraicType::U64 => "U64".into(),
            AlgebraicType::I128 => "I128".into(),
            AlgebraicType::U128 => "U128".into(),
            AlgebraicType::I256 => "I256".into(),
            AlgebraicType::U256 => "U256".into(),
            AlgebraicType::F32 => "F32".into(),
            AlgebraicType::F64 => "F64".into(),
            AlgebraicType::String => "String".into(),
            AlgebraicType::Array(arr) => format!("Array({})", Self::describe_type(&arr.elem_ty)),
            AlgebraicType::Product(p) => {
                if p.elements.is_empty() {
                    return "Product{}".into();
                }
                let elems: Vec<_> = p
                    .elements
                    .iter()
                    .map(|e| {
                        let t = Self::describe_type(&e.algebraic_type);
                        match &e.name {
                            Some(n) => format!("{n}: {t}"),
                            None => t,
                        }
                    })
                    .collect();
                format!("Product{{{}}}", elems.join(", "))
            }
            AlgebraicType::Sum(s) => {
                if s.variants.is_empty() {
                    return "Sum{}".into();
                }
                if Self::is_option_type(ty) {
                    return format!("Option<{}>", Self::describe_type(&s.variants[0].algebraic_type));
                }
                let vars: Vec<_> = s
                    .variants
                    .iter()
                    .map(|v| {
                        let n = v.name.as_deref().unwrap_or_default();
                        format!("{n}: {}", Self::describe_type(&v.algebraic_type))
                    })
                    .collect();
                format!("Sum{{{}}}", vars.join(" | "))
            }
        }
    }

    // ============================================================
    // Core registration
    // ============================================================

    pub fn register_type(&mut self, ty: AlgebraicType, explicit_name: &str) -> AlgebraicType {
        // 1. Primitives
        if Self::is_primitive(&ty) {
            return ty;
        }
        // 2. Refs
        if let AlgebraicType::Ref(r) = ty {
            return AlgebraicType::Ref(r);
        }
        // 3. Arrays
        if let AlgebraicType::Array(arr) = &ty {
            return self.convert_array((*arr.elem_ty).clone());
        }
        // 4. Unit types
        if Self::is_unit_type(&ty) && explicit_name.is_empty() {
            return Self::convert_unit_type();
        }
        // 5. Special types
        if Self::is_special_type(&ty) {
            return self.convert_special_type(&ty);
        }
        // 5b. ScheduleAt
        if Self::is_schedule_at_type(&ty) {
            return self.convert_inline_sum(&ty);
        }
        // 6. Options
        if Self::is_option_type(&ty) {
            return self.convert_inline_sum(&ty);
        }
        // 7. Results
        if Self::is_result_type(&ty) {
            return self.convert_inline_sum(&ty);
        }

        // === Complex types below ===

        // 8. Type name
        let mut type_name = if !explicit_name.is_empty() {
            explicit_name.to_owned()
        } else {
            String::new()
        };
        if let Some(pos) = type_name.rfind("::") {
            type_name = type_name[pos + 2..].to_owned();
        }

        if type_name.is_empty() {
            self.error = Some(RegistrationError {
                type_description: Self::describe_type(&ty),
                message: format!("Missing type name for complex type: {}", Self::describe_type(&ty)),
            });
            return AlgebraicType::U8;
        }

        // 9. Circular ref detection
        if self.types_being_registered.contains(&type_name) {
            self.error = Some(RegistrationError {
                type_description: Self::describe_type(&ty),
                message: format!("Recursive type reference detected: '{type_name}' is referencing itself"),
            });
            return AlgebraicType::U8;
        }

        // 10. Cache check
        if let Some(&idx) = self.type_name_cache.get(&type_name) {
            return AlgebraicType::Ref(idx);
        }

        // 11. Register
        self.register_complex_type(ty, &type_name)
    }

    fn register_complex_type(&mut self, ty: AlgebraicType, type_name: &str) -> AlgebraicType {
        self.types_being_registered.insert(type_name.to_owned());
        let idx = AlgebraicTypeRef(self.typespace.types.len() as u32);

        let processed = match &ty {
            AlgebraicType::Product(_) => self.process_product(&ty),
            AlgebraicType::Sum(_) => self.process_sum(&ty),
            _ => {
                self.types_being_registered.remove(type_name);
                return Self::convert_unit_type();
            }
        };

        self.typespace.types.push(processed);

        let (scope, name) = Self::parse_namespace_and_name(type_name);
        self.type_defs.push(RawTypeDefV10 {
            source_name: RawScopedTypeNameV10 {
                scope: scope.into(),
                source_name: name,
            },
            ty: idx,
            custom_ordering: true,
        });

        self.type_name_cache.insert(type_name.to_owned(), idx);
        self.types_being_registered.remove(type_name);
        AlgebraicType::Ref(idx)
    }

    fn process_product(&mut self, ty: &AlgebraicType) -> AlgebraicType {
        let Some(product) = ty.as_product() else {
            return AlgebraicType::U8;
        };
        let elements: Box<[_]> = product
            .elements
            .iter()
            .map(|f| ProductTypeElement {
                name: f.name.clone(),
                algebraic_type: self.register_type(f.algebraic_type.clone(), ""),
            })
            .collect();
        AlgebraicType::Product(ProductType { elements })
    }

    fn process_sum(&mut self, ty: &AlgebraicType) -> AlgebraicType {
        let Some(sum) = ty.as_sum() else {
            return AlgebraicType::U8;
        };
        let variants: Vec<_> = sum
            .variants
            .iter()
            .map(|v| SumTypeVariant {
                name: v.name.clone(),
                algebraic_type: self.register_type(v.algebraic_type.clone(), ""),
            })
            .collect();
        AlgebraicType::Sum(SumType {
            variants: variants.into(),
        })
    }

    /// Build the final `RawModuleDefV10`
    pub fn build_module_def(&self) -> RawModuleDefV10 {
        let mut module = RawModuleDefV10::default();
        module
            .sections
            .push(RawModuleDefV10Section::Typespace(self.typespace.clone()));
        module
            .sections
            .push(RawModuleDefV10Section::Types(self.type_defs.clone()));
        module
    }
}

/// Serialize the module definition to BSATN bytes
pub fn serialize_module_def(reg: &ModuleTypeRegistration) -> Vec<u8> {
    let module = reg.build_module_def();
    let versioned = RawModuleDef::V10(module);
    spacetimedb_sats::bsatn::to_vec(&versioned).expect("failed to serialize module definition")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // Type classification
    // ============================================================

    #[test]
    fn is_primitive_all() {
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::Bool));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::U8));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::U16));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::U32));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::U64));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::U128));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::U256));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::I8));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::I16));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::I32));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::I64));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::I128));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::I256));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::F32));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::F64));
        assert!(ModuleTypeRegistration::is_primitive(&AlgebraicType::String));
    }

    #[test]
    fn is_primitive_rejects_composite() {
        assert!(!ModuleTypeRegistration::is_primitive(&AlgebraicType::Array(
            ArrayType {
                elem_ty: Box::new(AlgebraicType::U8)
            }
        )));
        assert!(!ModuleTypeRegistration::is_primitive(&AlgebraicType::Product(
            ProductType {
                elements: vec![].into()
            }
        )));
        assert!(!ModuleTypeRegistration::is_primitive(&AlgebraicType::Sum(SumType {
            variants: vec![].into()
        })));
        assert!(!ModuleTypeRegistration::is_primitive(&AlgebraicType::Ref(
            AlgebraicTypeRef(0)
        )));
    }

    #[test]
    fn is_special_type_all_specials() {
        for name in [
            "__identity__",
            "__connection_id__",
            "__timestamp_micros_since_unix_epoch__",
            "__time_duration_micros__",
            "__uuid__",
        ] {
            let ty = AlgebraicType::Product(ProductType {
                elements: vec![ProductTypeElement {
                    name: Some(RawIdentifier::new(name)),
                    algebraic_type: AlgebraicType::U8,
                }]
                .into(),
            });
            assert!(ModuleTypeRegistration::is_special_type(&ty), "{name} should be special");
        }
    }

    #[test]
    fn is_special_type_non_special() {
        let normal = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("x")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        assert!(!ModuleTypeRegistration::is_special_type(&normal));
        assert!(!ModuleTypeRegistration::is_special_type(&AlgebraicType::U8));
        assert!(!ModuleTypeRegistration::is_special_type(&AlgebraicType::Sum(SumType {
            variants: vec![].into()
        })));
    }

    #[test]
    fn is_special_type_multiple_fields() {
        let multi = AlgebraicType::Product(ProductType {
            elements: vec![
                ProductTypeElement {
                    name: Some(RawIdentifier::new("__identity__")),
                    algebraic_type: AlgebraicType::U8,
                },
                ProductTypeElement {
                    name: Some(RawIdentifier::new("other")),
                    algebraic_type: AlgebraicType::U8,
                },
            ]
            .into(),
        });
        assert!(!ModuleTypeRegistration::is_special_type(&multi));
    }

    fn make_option() -> AlgebraicType {
        AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant {
                    name: Some(RawIdentifier::new("some")),
                    algebraic_type: AlgebraicType::U8,
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("none")),
                    algebraic_type: AlgebraicType::Product(ProductType {
                        elements: vec![].into(),
                    }),
                },
            ]
            .into(),
        })
    }

    fn make_result() -> AlgebraicType {
        AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant {
                    name: Some(RawIdentifier::new("ok")),
                    algebraic_type: AlgebraicType::U8,
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("err")),
                    algebraic_type: AlgebraicType::String,
                },
            ]
            .into(),
        })
    }

    fn make_schedule_at() -> AlgebraicType {
        AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant {
                    name: Some(RawIdentifier::new("Interval")),
                    algebraic_type: AlgebraicType::I64,
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("Time")),
                    algebraic_type: AlgebraicType::I64,
                },
            ]
            .into(),
        })
    }

    #[test]
    fn is_option_type_valid() {
        assert!(ModuleTypeRegistration::is_option_type(&make_option()));
    }
    #[test]
    fn is_option_type_wrong_names() {
        assert!(!ModuleTypeRegistration::is_option_type(&make_result()));
    }
    #[test]
    fn is_option_type_wrong_count() {
        let one = AlgebraicType::Sum(SumType {
            variants: vec![SumTypeVariant {
                name: Some(RawIdentifier::new("some")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        assert!(!ModuleTypeRegistration::is_option_type(&one));
        let three = AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant {
                    name: Some(RawIdentifier::new("some")),
                    algebraic_type: AlgebraicType::U8,
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("none")),
                    algebraic_type: AlgebraicType::U8,
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("extra")),
                    algebraic_type: AlgebraicType::U8,
                },
            ]
            .into(),
        });
        assert!(!ModuleTypeRegistration::is_option_type(&three));
    }
    #[test]
    fn is_option_type_non_sum() {
        assert!(!ModuleTypeRegistration::is_option_type(&AlgebraicType::U8));
        assert!(!ModuleTypeRegistration::is_option_type(&AlgebraicType::Product(
            ProductType {
                elements: vec![].into()
            }
        )));
    }

    #[test]
    fn is_result_type_valid() {
        assert!(ModuleTypeRegistration::is_result_type(&make_result()));
    }
    #[test]
    fn is_result_type_wrong_names() {
        assert!(!ModuleTypeRegistration::is_result_type(&make_option()));
    }
    #[test]
    fn is_result_type_non_sum() {
        assert!(!ModuleTypeRegistration::is_result_type(&AlgebraicType::Bool));
    }

    #[test]
    fn is_schedule_at_type_valid() {
        assert!(ModuleTypeRegistration::is_schedule_at_type(&make_schedule_at()));
    }
    #[test]
    fn is_schedule_at_type_wrong_names() {
        let wrong = AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant {
                    name: Some(RawIdentifier::new("Interval")),
                    algebraic_type: AlgebraicType::I64,
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("Wrong")),
                    algebraic_type: AlgebraicType::I64,
                },
            ]
            .into(),
        });
        assert!(!ModuleTypeRegistration::is_schedule_at_type(&wrong));
    }
    #[test]
    fn is_schedule_at_type_non_sum() {
        assert!(!ModuleTypeRegistration::is_schedule_at_type(&AlgebraicType::Product(
            ProductType {
                elements: vec![].into()
            }
        )));
    }

    #[test]
    fn is_unit_type_empty_product() {
        let unit = AlgebraicType::Product(ProductType {
            elements: vec![].into(),
        });
        assert!(ModuleTypeRegistration::is_unit_type(&unit));
    }
    #[test]
    fn is_unit_type_non_unit() {
        let non = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("x")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        assert!(!ModuleTypeRegistration::is_unit_type(&non));
    }
    #[test]
    fn is_unit_type_non_product() {
        assert!(!ModuleTypeRegistration::is_unit_type(&AlgebraicType::Sum(SumType {
            variants: vec![].into()
        })));
        assert!(!ModuleTypeRegistration::is_unit_type(&AlgebraicType::U8));
    }

    // ============================================================
    // Name handling
    // ============================================================

    #[test]
    fn extract_type_name_no_namespace() {
        assert_eq!(ModuleTypeRegistration::extract_type_name("MyType"), "MyType");
    }
    #[test]
    fn extract_type_name_with_namespace() {
        assert_eq!(
            ModuleTypeRegistration::extract_type_name("SpacetimeDB::Internal::MyType"),
            "MyType"
        );
    }
    #[test]
    fn extract_type_name_with_template() {
        assert_eq!(ModuleTypeRegistration::extract_type_name("std::vector<int>"), "vector");
        assert_eq!(ModuleTypeRegistration::extract_type_name("MyType<i32>"), "MyType");
    }
    #[test]
    fn extract_type_name_template_no_namespace() {
        assert_eq!(
            ModuleTypeRegistration::extract_type_name("HashMap<String, i32>"),
            "HashMap"
        );
    }

    #[test]
    fn parse_namespace_no_namespace() {
        let (scope, name) = ModuleTypeRegistration::parse_namespace_and_name("MyType");
        assert!(scope.is_empty());
        assert_eq!(&*name, "MyType");
    }
    #[test]
    fn parse_namespace_single_level() {
        let (scope, name) = ModuleTypeRegistration::parse_namespace_and_name("A.MyType");
        assert_eq!(scope, vec![RawIdentifier::new("A")]);
        assert_eq!(&*name, "MyType");
    }
    #[test]
    fn parse_namespace_nested() {
        let (scope, name) = ModuleTypeRegistration::parse_namespace_and_name("A.B.MyType");
        assert_eq!(scope, vec![RawIdentifier::new("A"), RawIdentifier::new("B")]);
        assert_eq!(&*name, "MyType");
    }
    #[test]
    fn parse_namespace_deeply_nested() {
        let (scope, name) = ModuleTypeRegistration::parse_namespace_and_name("SpacetimeDB.Internal.MyType");
        assert_eq!(
            scope,
            vec![RawIdentifier::new("SpacetimeDB"), RawIdentifier::new("Internal")]
        );
        assert_eq!(&*name, "MyType");
    }

    // ============================================================
    // Describe type
    // ============================================================

    #[test]
    fn describe_all_primitives() {
        assert_eq!(ModuleTypeRegistration::describe_type(&AlgebraicType::Bool), "Bool");
        assert_eq!(ModuleTypeRegistration::describe_type(&AlgebraicType::U8), "U8");
        assert_eq!(ModuleTypeRegistration::describe_type(&AlgebraicType::U32), "U32");
        assert_eq!(ModuleTypeRegistration::describe_type(&AlgebraicType::I64), "I64");
        assert_eq!(ModuleTypeRegistration::describe_type(&AlgebraicType::F64), "F64");
        assert_eq!(ModuleTypeRegistration::describe_type(&AlgebraicType::String), "String");
    }

    #[test]
    fn describe_array() {
        let arr = AlgebraicType::Array(ArrayType {
            elem_ty: Box::new(AlgebraicType::U32),
        });
        assert_eq!(ModuleTypeRegistration::describe_type(&arr), "Array(U32)");
    }
    #[test]
    fn describe_array_nested() {
        let arr = AlgebraicType::Array(ArrayType {
            elem_ty: Box::new(AlgebraicType::Array(ArrayType {
                elem_ty: Box::new(AlgebraicType::U8),
            })),
        });
        assert_eq!(ModuleTypeRegistration::describe_type(&arr), "Array(Array(U8))");
    }

    #[test]
    fn describe_empty_product() {
        assert_eq!(
            ModuleTypeRegistration::describe_type(&AlgebraicType::Product(ProductType {
                elements: vec![].into()
            })),
            "Product{}"
        );
    }
    #[test]
    fn describe_product_unnamed() {
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![
                ProductTypeElement {
                    name: None,
                    algebraic_type: AlgebraicType::U8,
                },
                ProductTypeElement {
                    name: None,
                    algebraic_type: AlgebraicType::String,
                },
            ]
            .into(),
        });
        let d = ModuleTypeRegistration::describe_type(&ty);
        assert!(d.contains("U8"));
        assert!(d.contains("String"));
        assert!(d.starts_with("Product{"));
    }
    #[test]
    fn describe_product_named() {
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![
                ProductTypeElement {
                    name: Some(RawIdentifier::new("x")),
                    algebraic_type: AlgebraicType::U8,
                },
                ProductTypeElement {
                    name: Some(RawIdentifier::new("y")),
                    algebraic_type: AlgebraicType::String,
                },
            ]
            .into(),
        });
        let d = ModuleTypeRegistration::describe_type(&ty);
        assert!(d.contains("x: U8"));
        assert!(d.contains("y: String"));
    }

    #[test]
    fn describe_empty_sum() {
        assert_eq!(
            ModuleTypeRegistration::describe_type(&AlgebraicType::Sum(SumType {
                variants: vec![].into()
            })),
            "Sum{}"
        );
    }
    #[test]
    fn describe_option() {
        assert_eq!(ModuleTypeRegistration::describe_type(&make_option()), "Option<U8>");
    }
    #[test]
    fn describe_sum_non_option() {
        let sum = AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant {
                    name: Some(RawIdentifier::new("A")),
                    algebraic_type: AlgebraicType::U8,
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("B")),
                    algebraic_type: AlgebraicType::String,
                },
            ]
            .into(),
        });
        let d = ModuleTypeRegistration::describe_type(&sum);
        assert!(d.contains("A: U8"));
        assert!(d.contains("B: String"));
        assert!(d.contains(" | "));
    }
    #[test]
    fn describe_ref() {
        assert_eq!(
            ModuleTypeRegistration::describe_type(&AlgebraicType::Ref(AlgebraicTypeRef(42))),
            "Ref(42)"
        );
    }

    // ============================================================
    // Registration state
    // ============================================================

    #[test]
    fn new_is_clean() {
        let reg = ModuleTypeRegistration::new();
        assert!(!reg.has_error());
        assert!(reg.error().is_none());
    }
    #[test]
    fn clear_resets_state() {
        let mut reg = ModuleTypeRegistration::new();
        reg.error = Some(RegistrationError {
            message: "err".into(),
            type_description: "desc".into(),
        });
        reg.types_being_registered.insert("Foo".into());
        reg.type_name_cache.insert("Foo".into(), AlgebraicTypeRef(0));
        reg.typespace.types.push(AlgebraicType::U8);
        reg.type_defs.push(RawTypeDefV10 {
            source_name: RawScopedTypeNameV10 {
                scope: vec![].into(),
                source_name: RawIdentifier::new("X"),
            },
            ty: AlgebraicTypeRef(0),
            custom_ordering: false,
        });
        reg.clear();
        assert!(!reg.has_error());
        assert!(reg.error().is_none());
        assert!(reg.types_being_registered.is_empty());
        assert!(reg.type_name_cache.is_empty());
        assert!(reg.typespace.types.is_empty());
        assert!(reg.type_defs.is_empty());
    }
    #[test]
    fn has_error_when_set() {
        let mut reg = ModuleTypeRegistration::new();
        reg.error = Some(RegistrationError {
            message: "fail".into(),
            type_description: "desc".into(),
        });
        assert!(reg.has_error());
    }

    #[test]
    fn convert_unit_type() {
        let unit = ModuleTypeRegistration::convert_unit_type();
        assert!(matches!(unit, AlgebraicType::Product(ref p) if p.elements.is_empty()));
    }

    // ============================================================
    // register_type — primitives
    // ============================================================

    #[test]
    fn register_primitive_bool() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(matches!(
            reg.register_type(AlgebraicType::Bool, ""),
            AlgebraicType::Bool
        ));
    }
    #[test]
    fn register_primitive_u32() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(matches!(reg.register_type(AlgebraicType::U32, ""), AlgebraicType::U32));
    }
    #[test]
    fn register_primitive_string() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(matches!(
            reg.register_type(AlgebraicType::String, ""),
            AlgebraicType::String
        ));
    }
    #[test]
    fn register_primitive_ignores_name() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(matches!(
            reg.register_type(AlgebraicType::U64, "MyAlias"),
            AlgebraicType::U64
        ));
    }

    #[test]
    fn register_ref_passthrough() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(
            matches!(reg.register_type(AlgebraicType::Ref(AlgebraicTypeRef(5)), ""), AlgebraicType::Ref(r) if r.0 == 5)
        );
    }

    #[test]
    fn register_array_of_primitive() {
        let mut reg = ModuleTypeRegistration::new();
        let arr = AlgebraicType::Array(ArrayType {
            elem_ty: Box::new(AlgebraicType::U8),
        });
        assert!(matches!(reg.register_type(arr, ""), AlgebraicType::Array(_)));
    }
    #[test]
    fn register_array_preserves_element() {
        let mut reg = ModuleTypeRegistration::new();
        let arr = AlgebraicType::Array(ArrayType {
            elem_ty: Box::new(AlgebraicType::I64),
        });
        let result = reg.register_type(arr, "");
        if let AlgebraicType::Array(a) = result {
            assert!(matches!(*a.elem_ty, AlgebraicType::I64));
        } else {
            panic!("expected Array");
        }
    }

    // ============================================================
    // register_type — inlined composites
    // ============================================================

    #[test]
    fn register_option_is_inlined() {
        let mut reg = ModuleTypeRegistration::new();
        let result = reg.register_type(make_option(), "MyOption");
        assert!(matches!(result, AlgebraicType::Sum(_)));
    }
    #[test]
    fn register_result_is_inlined() {
        let mut reg = ModuleTypeRegistration::new();
        let result = reg.register_type(make_result(), "MyResult");
        assert!(matches!(result, AlgebraicType::Sum(_)));
    }
    #[test]
    fn register_schedule_at_is_inlined() {
        let mut reg = ModuleTypeRegistration::new();
        let result = reg.register_type(make_schedule_at(), "MyScheduleAt");
        assert!(matches!(result, AlgebraicType::Sum(_)));
    }

    #[test]
    fn register_special_identity_inlined() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("__identity__")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        assert!(matches!(reg.register_type(ty, ""), AlgebraicType::Product(_)));
    }
    #[test]
    fn register_special_connection_id_inlined() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("__connection_id__")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        assert!(matches!(reg.register_type(ty, ""), AlgebraicType::Product(_)));
    }
    #[test]
    fn register_special_timestamp_inlined() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("__timestamp_micros_since_unix_epoch__")),
                algebraic_type: AlgebraicType::I64,
            }]
            .into(),
        });
        assert!(matches!(reg.register_type(ty, ""), AlgebraicType::Product(_)));
    }
    #[test]
    fn register_special_duration_inlined() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("__time_duration_micros__")),
                algebraic_type: AlgebraicType::I64,
            }]
            .into(),
        });
        assert!(matches!(reg.register_type(ty, ""), AlgebraicType::Product(_)));
    }
    #[test]
    fn register_special_uuid_inlined() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("__uuid__")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        assert!(matches!(reg.register_type(ty, ""), AlgebraicType::Product(_)));
    }

    // ============================================================
    // register_type — error paths
    // ============================================================

    #[test]
    fn register_missing_name_sets_error() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("x")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        let result = reg.register_type(ty, "");
        assert!(matches!(result, AlgebraicType::U8));
        assert!(reg.has_error());
        assert!(reg.error().unwrap().message.contains("Missing type name"));
    }

    #[test]
    fn register_circular_ref_sets_error() {
        let mut reg = ModuleTypeRegistration::new();
        reg.types_being_registered.insert("Recursive".into());
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("x")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        let result = reg.register_type(ty, "Recursive");
        assert!(matches!(result, AlgebraicType::U8));
        assert!(reg.has_error());
        assert!(reg.error().unwrap().message.contains("Recursive type reference"));
    }

    // ============================================================
    // convert_special_type
    // ============================================================

    #[test]
    fn convert_special_converts_fields() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("__identity__")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        let result = reg.convert_special_type(&ty);
        assert!(matches!(result, AlgebraicType::Product(ref p) if p.elements.len() == 1));
    }
    #[test]
    fn convert_special_non_product_fallback() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(matches!(
            reg.convert_special_type(&AlgebraicType::U8),
            AlgebraicType::U8
        ));
    }

    // ============================================================
    // convert_inline_sum
    // ============================================================

    #[test]
    fn convert_inline_sum_variants() {
        let mut reg = ModuleTypeRegistration::new();
        let sum = make_option();
        let result = reg.convert_inline_sum(&sum);
        assert!(matches!(result, AlgebraicType::Sum(ref s) if s.variants.len() == 2));
    }
    #[test]
    fn convert_inline_sum_non_sum_fallback() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(matches!(
            reg.convert_inline_sum(&AlgebraicType::Bool),
            AlgebraicType::U8
        ));
    }

    // ============================================================
    // process_product / process_sum
    // ============================================================

    #[test]
    fn process_product_preserves_fields() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![
                ProductTypeElement {
                    name: Some(RawIdentifier::new("a")),
                    algebraic_type: AlgebraicType::U8,
                },
                ProductTypeElement {
                    name: Some(RawIdentifier::new("b")),
                    algebraic_type: AlgebraicType::String,
                },
            ]
            .into(),
        });
        let result = reg.process_product(&ty);
        if let AlgebraicType::Product(ref p) = result {
            assert_eq!(p.elements.len(), 2);
            assert_eq!(p.elements[0].name.as_ref().map(|r| &**r), Some("a"));
            assert_eq!(p.elements[1].name.as_ref().map(|r| &**r), Some("b"));
        } else {
            panic!("expected Product");
        }
    }
    #[test]
    fn process_product_non_product_fallback() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(matches!(reg.process_product(&AlgebraicType::U8), AlgebraicType::U8));
    }

    #[test]
    fn process_sum_preserves_variants() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant {
                    name: Some(RawIdentifier::new("A")),
                    algebraic_type: AlgebraicType::U8,
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("B")),
                    algebraic_type: AlgebraicType::I32,
                },
            ]
            .into(),
        });
        let result = reg.process_sum(&ty);
        if let AlgebraicType::Sum(ref s) = result {
            assert_eq!(s.variants.len(), 2);
            assert_eq!(s.variants[0].name.as_ref().map(|r| &**r), Some("A"));
            assert_eq!(s.variants[1].name.as_ref().map(|r| &**r), Some("B"));
        } else {
            panic!("expected Sum");
        }
    }
    #[test]
    fn process_sum_non_sum_fallback() {
        let mut reg = ModuleTypeRegistration::new();
        assert!(matches!(reg.process_sum(&AlgebraicType::Bool), AlgebraicType::U8));
    }

    // ============================================================
    // Complex type registration (structs/enums)
    // ============================================================

    #[test]
    fn register_struct_adds_to_typespace() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("x")),
                algebraic_type: AlgebraicType::U32,
            }]
            .into(),
        });
        let result = reg.register_type(ty, "MyStruct");
        assert!(matches!(result, AlgebraicType::Ref(r) if r.0 == 0));
        assert_eq!(reg.typespace.types.len(), 1);
        assert_eq!(reg.type_defs.len(), 1);
        assert_eq!(&*reg.type_defs[0].source_name.source_name, "MyStruct");
    }

    #[test]
    fn register_enum_adds_to_typespace() {
        let mut reg = ModuleTypeRegistration::new();
        let ty = AlgebraicType::Sum(SumType {
            variants: vec![
                SumTypeVariant {
                    name: Some(RawIdentifier::new("A")),
                    algebraic_type: AlgebraicType::Product(ProductType {
                        elements: vec![].into(),
                    }),
                },
                SumTypeVariant {
                    name: Some(RawIdentifier::new("B")),
                    algebraic_type: AlgebraicType::Product(ProductType {
                        elements: vec![].into(),
                    }),
                },
            ]
            .into(),
        });
        let result = reg.register_type(ty, "MyEnum");
        assert!(matches!(result, AlgebraicType::Ref(r) if r.0 == 0));
        assert_eq!(reg.typespace.types.len(), 1);
        assert_eq!(reg.type_defs.len(), 1);
    }

    #[test]
    fn register_nested_struct() {
        let mut reg = ModuleTypeRegistration::new();
        let inner = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("y")),
                algebraic_type: AlgebraicType::U8,
            }]
            .into(),
        });
        let outer = AlgebraicType::Product(ProductType {
            elements: vec![ProductTypeElement {
                name: Some(RawIdentifier::new("inner")),
                algebraic_type: inner.clone(),
            }]
            .into(),
        });

        // First register Inner
        reg.register_type(inner, "Inner");
        // Then register Outer
        let result = reg.register_type(outer, "Outer");
        assert!(matches!(result, AlgebraicType::Ref(r) if r.0 == 1));
        assert_eq!(reg.typespace.types.len(), 2);
        assert_eq!(reg.type_defs.len(), 2);
    }

    // ============================================================
    // build / serialize
    // ============================================================

    #[test]
    fn build_module_def_contains_sections() {
        let mut reg = ModuleTypeRegistration::new();
        reg.register_type(
            AlgebraicType::Product(ProductType {
                elements: vec![ProductTypeElement {
                    name: Some(RawIdentifier::new("x")),
                    algebraic_type: AlgebraicType::U8,
                }]
                .into(),
            }),
            "TestStruct",
        );
        let module = reg.build_module_def();
        assert_eq!(module.sections.len(), 2);
        assert!(matches!(&module.sections[0], RawModuleDefV10Section::Typespace(_)));
        assert!(matches!(&module.sections[1], RawModuleDefV10Section::Types(_)));
    }

    #[test]
    fn serialize_module_def_produces_bytes() {
        let mut reg = ModuleTypeRegistration::new();
        reg.register_type(
            AlgebraicType::Product(ProductType {
                elements: vec![ProductTypeElement {
                    name: Some(RawIdentifier::new("x")),
                    algebraic_type: AlgebraicType::U8,
                }]
                .into(),
            }),
            "TestStruct",
        );
        let bytes = serialize_module_def(&reg);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn serialize_empty_module() {
        let reg = ModuleTypeRegistration::new();
        let bytes = serialize_module_def(&reg);
        assert!(!bytes.is_empty());
    }
}
