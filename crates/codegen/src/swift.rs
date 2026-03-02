use super::Lang;
use super::util::{AUTO_GENERATED_PREFIX, collect_case, type_ref_name};
use crate::util::iter_table_names_and_types;
use crate::{CodegenOptions, OutputFile};
use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_schema::def::{ModuleDef, ProcedureDef, ReducerDef, TableDef, TypeDef};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse};
use std::fmt::Write;
use std::ops::Deref;

pub struct Swift;

fn write_generated_file_preamble(code: &mut String) {
    writeln!(&mut *code, "{AUTO_GENERATED_PREFIX}. EDITS TO THIS FILE").unwrap();
    writeln!(
        &mut *code,
        "// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD."
    )
    .unwrap();
    writeln!(&mut *code).unwrap();
}

fn get_swift_type_for_primitive(prim: PrimitiveType) -> &'static str {
    match prim {
        PrimitiveType::Bool => "Bool",
        PrimitiveType::I8 => "Int8",
        PrimitiveType::U8 => "UInt8",
        PrimitiveType::I16 => "Int16",
        PrimitiveType::U16 => "UInt16",
        PrimitiveType::I32 => "Int32",
        PrimitiveType::U32 => "UInt32",
        PrimitiveType::I64 => "Int64",
        PrimitiveType::U64 => "UInt64",
        PrimitiveType::I128 => "String", // Represent as decimal string for cross-toolchain compatibility.
        PrimitiveType::U128 => "String", // Represent as decimal string for cross-toolchain compatibility.
        PrimitiveType::I256 => "String", // Swift doesn't have native 256 bigint
        PrimitiveType::U256 => "String",
        PrimitiveType::F32 => "Float",
        PrimitiveType::F64 => "Double",
    }
}

fn get_swift_type_use(module: &ModuleDef, ty: &AlgebraicTypeUse) -> String {
    match ty {
        AlgebraicTypeUse::Primitive(prim) => get_swift_type_for_primitive(*prim).to_string(),
        AlgebraicTypeUse::Ref(type_ref) => type_ref_name(module, *type_ref),
        AlgebraicTypeUse::Identity => "Identity".to_string(), // Requires Identity SDK struct
        AlgebraicTypeUse::String => "String".to_string(),
        AlgebraicTypeUse::Array(inner) => format!("[{}]", get_swift_type_use(module, inner)),
        AlgebraicTypeUse::Option(inner) => format!("{}?", get_swift_type_use(module, inner)),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => format!(
            "SpacetimeResult<{}, {}>",
            get_swift_type_use(module, ok_ty),
            get_swift_type_use(module, err_ty)
        ),
        AlgebraicTypeUse::Unit => "()".to_string(),
        AlgebraicTypeUse::Never => "Never".to_string(),
        AlgebraicTypeUse::ConnectionId => "ClientConnectionId".to_string(),
        AlgebraicTypeUse::ScheduleAt => "ScheduleAt".to_string(), // Requires ScheduleAt SDK struct
        AlgebraicTypeUse::Timestamp => "UInt64".to_string(),
        AlgebraicTypeUse::TimeDuration => "UInt64".to_string(),
        AlgebraicTypeUse::Uuid => "String".to_string(), // Usually treated as string in iOS without further casting
    }
}

impl Lang for Swift {
    fn generate_table_file_from_schema(
        &self,
        module: &ModuleDef,
        table: &TableDef,
        _schema: TableSchema,
    ) -> OutputFile {
        let table_name = table.name.deref();
        let table_name_pascal = table.accessor_name.deref().to_case(Case::Pascal);
        let row_type = type_ref_name(module, table.product_type_ref);

        let mut code = String::new();
        write_generated_file_preamble(&mut code);
        writeln!(&mut code, "import Foundation\n").unwrap();
        writeln!(&mut code, "public struct {}Table {{", table_name_pascal).unwrap();

        // Expose a public cache accessor that UI can subscribe to
        writeln!(
            &mut code,
            "    @MainActor public static var cache: TableCache<{}> {{",
            row_type
        )
        .unwrap();
        writeln!(
            &mut code,
            "        return SpacetimeClient.clientCache.getTableCache(tableName: \"{}\")",
            table_name
        )
        .unwrap();
        writeln!(&mut code, "    }}").unwrap();

        // Write the generic index mapping for any Unique constraints
        writeln!(&mut code, "}}").unwrap();

        OutputFile {
            filename: format!("{}Table.swift", table_name_pascal),
            code,
        }
    }

    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile> {
        let type_name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
        let mut code = String::new();

        write_generated_file_preamble(&mut code);
        writeln!(&mut code, "import Foundation\n").unwrap();

        match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Product(product) => {
                writeln!(&mut code, "public struct {}: Codable, Sendable {{", type_name).unwrap();
                for (name, ty) in &product.elements {
                    let field_name = name.deref().to_case(Case::Camel);
                    let swift_ty = get_swift_type_use(module, ty);
                    writeln!(&mut code, "    public var {}: {}", field_name, swift_ty).unwrap();
                }
                writeln!(&mut code, "}}").unwrap();
            }
            AlgebraicTypeDef::Sum(sum) => {
                writeln!(&mut code, "public enum {}: Codable, Sendable {{", type_name).unwrap();
                for (name, ty) in sum.variants.iter() {
                    let case_name = name.deref().to_case(Case::Camel);
                    if matches!(ty, AlgebraicTypeUse::Unit) {
                        writeln!(&mut code, "    case {}", case_name).unwrap();
                    } else {
                        let swift_ty = get_swift_type_use(module, ty);
                        writeln!(&mut code, "    case {}({})", case_name, swift_ty).unwrap();
                    }
                }

                writeln!(&mut code, "").unwrap();
                writeln!(&mut code, "    public init(from decoder: Decoder) throws {{").unwrap();
                writeln!(&mut code, "        var container = try decoder.singleValueContainer()").unwrap();
                writeln!(&mut code, "        let tag = try container.decode(UInt8.self)").unwrap();
                writeln!(&mut code, "        switch tag {{").unwrap();
                for (idx, (name, ty)) in sum.variants.iter().enumerate() {
                    let case_name = name.deref().to_case(Case::Camel);
                    writeln!(&mut code, "        case UInt8({idx}):").unwrap();
                    if matches!(ty, AlgebraicTypeUse::Unit) {
                        writeln!(&mut code, "            self = .{}", case_name).unwrap();
                    } else {
                        let swift_ty = get_swift_type_use(module, ty);
                        writeln!(
                            &mut code,
                            "            self = .{}(try container.decode({}.self))",
                            case_name, swift_ty
                        )
                        .unwrap();
                    }
                }
                writeln!(&mut code, "        default:").unwrap();
                writeln!(&mut code, "            throw BSATNDecodingError.invalidType").unwrap();
                writeln!(&mut code, "        }}").unwrap();
                writeln!(&mut code, "    }}").unwrap();

                writeln!(&mut code, "").unwrap();
                writeln!(&mut code, "    public func encode(to encoder: Encoder) throws {{").unwrap();
                writeln!(&mut code, "        var container = encoder.singleValueContainer()").unwrap();
                writeln!(&mut code, "        switch self {{").unwrap();
                for (idx, (name, ty)) in sum.variants.iter().enumerate() {
                    let case_name = name.deref().to_case(Case::Camel);
                    if matches!(ty, AlgebraicTypeUse::Unit) {
                        writeln!(&mut code, "        case .{}:", case_name).unwrap();
                        writeln!(&mut code, "            try container.encode(UInt8({idx}))").unwrap();
                    } else {
                        writeln!(&mut code, "        case .{}(let value):", case_name).unwrap();
                        writeln!(&mut code, "            try container.encode(UInt8({idx}))").unwrap();
                        writeln!(&mut code, "            try container.encode(value)").unwrap();
                    }
                }
                writeln!(&mut code, "        }}").unwrap();
                writeln!(&mut code, "    }}").unwrap();
                writeln!(&mut code, "}}").unwrap();
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                writeln!(&mut code, "public enum {}: UInt8, Codable, Sendable {{", type_name).unwrap();
                for (idx, name) in plain_enum.variants.iter().enumerate() {
                    let case_name = name.deref().to_case(Case::Camel);
                    writeln!(&mut code, "    case {} = {}", case_name, idx).unwrap();
                }

                writeln!(&mut code, "").unwrap();
                writeln!(&mut code, "    public init(from decoder: Decoder) throws {{").unwrap();
                writeln!(&mut code, "        let container = try decoder.singleValueContainer()").unwrap();
                writeln!(&mut code, "        let tag = try container.decode(UInt8.self)").unwrap();
                writeln!(&mut code, "        guard let value = Self(rawValue: tag) else {{").unwrap();
                writeln!(&mut code, "            throw BSATNDecodingError.invalidType").unwrap();
                writeln!(&mut code, "        }}").unwrap();
                writeln!(&mut code, "        self = value").unwrap();
                writeln!(&mut code, "    }}").unwrap();

                writeln!(&mut code, "").unwrap();
                writeln!(&mut code, "    public func encode(to encoder: Encoder) throws {{").unwrap();
                writeln!(&mut code, "        var container = encoder.singleValueContainer()").unwrap();
                writeln!(&mut code, "        try container.encode(self.rawValue)").unwrap();
                writeln!(&mut code, "    }}").unwrap();
                writeln!(&mut code, "}}").unwrap();
            }
        }

        vec![OutputFile {
            filename: format!("{}.swift", type_name),
            code,
        }]
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let reducer_name = reducer.name.deref();
        let reducer_name_pascal = reducer.name.deref().to_case(Case::Pascal);

        let mut code = String::new();
        write_generated_file_preamble(&mut code);
        writeln!(&mut code, "import Foundation\n").unwrap();

        writeln!(&mut code, "public enum {} {{", reducer_name_pascal).unwrap();

        // Write the internal args struct used for BSATN encoding
        writeln!(&mut code, "    public struct _Args: Codable, Sendable {{").unwrap();
        for (name, ty) in &reducer.params_for_generate.elements {
            let field_name = name.deref().to_case(Case::Camel);
            let swift_ty = get_swift_type_use(module, ty);
            writeln!(&mut code, "        public var {}: {}", field_name, swift_ty).unwrap();
        }
        writeln!(&mut code, "    }}\n").unwrap();

        // Write a helper struct for invoking the reducer
        write!(&mut code, "    @MainActor public static func invoke(").unwrap();

        let mut first = true;
        for (name, ty) in &reducer.params_for_generate.elements {
            if !first {
                write!(&mut code, ", ").unwrap();
            }
            first = false;
            let field_name = name.deref().to_case(Case::Camel);
            let swift_ty = get_swift_type_use(module, ty);
            write!(&mut code, "{}: {}", field_name, swift_ty).unwrap();
        }
        writeln!(&mut code, ") {{").unwrap();

        // Build the argument struct to encode
        writeln!(&mut code, "        let args = _Args(").unwrap();
        first = true;
        for (name, _ty) in &reducer.params_for_generate.elements {
            if !first {
                writeln!(&mut code, ",").unwrap();
            }
            first = false;
            let field_name = name.deref().to_case(Case::Camel);
            write!(&mut code, "            {}: {}", field_name, field_name).unwrap();
        }
        if !reducer.params_for_generate.elements.is_empty() {
            writeln!(&mut code, "").unwrap();
        }
        writeln!(&mut code, "        )").unwrap();

        // Encode and send
        writeln!(&mut code, "        do {{").unwrap();
        writeln!(&mut code, "            let argBytes = try BSATNEncoder().encode(args)").unwrap();
        writeln!(
            &mut code,
            "            SpacetimeClient.shared?.send(\"{}\", argBytes)",
            reducer_name
        )
        .unwrap();
        writeln!(&mut code, "        }} catch {{").unwrap();
        writeln!(
            &mut code,
            "            print(\"Failed to encode {} arguments: \\(error)\")",
            reducer_name_pascal
        )
        .unwrap();
        writeln!(&mut code, "        }}").unwrap();

        writeln!(&mut code, "    }}").unwrap();
        writeln!(&mut code, "}}").unwrap();

        OutputFile {
            filename: format!("{}.swift", reducer_name_pascal),
            code,
        }
    }

    fn generate_procedure_file(&self, module: &ModuleDef, procedure: &ProcedureDef) -> OutputFile {
        let procedure_name = procedure.name.deref();
        let procedure_name_pascal = procedure.name.deref().to_case(Case::Pascal);
        let return_swift_ty = get_swift_type_use(module, &procedure.return_type_for_generate);
        let return_is_unit = matches!(&procedure.return_type_for_generate, AlgebraicTypeUse::Unit);
        let callback_return_ty = if return_is_unit {
            "Void".to_string()
        } else {
            return_swift_ty.clone()
        };

        let mut code = String::new();
        write_generated_file_preamble(&mut code);
        writeln!(&mut code, "import Foundation\n").unwrap();

        writeln!(&mut code, "public enum {}Procedure {{", procedure_name_pascal).unwrap();

        writeln!(&mut code, "    public struct _Args: Codable, Sendable {{").unwrap();
        for (name, ty) in &procedure.params_for_generate.elements {
            let field_name = name.deref().to_case(Case::Camel);
            let swift_ty = get_swift_type_use(module, ty);
            writeln!(&mut code, "        public var {}: {}", field_name, swift_ty).unwrap();
        }
        writeln!(&mut code, "    }}\n").unwrap();

        write!(&mut code, "    @MainActor public static func invoke(").unwrap();
        let mut first = true;
        for (name, ty) in &procedure.params_for_generate.elements {
            if !first {
                write!(&mut code, ", ").unwrap();
            }
            first = false;
            let field_name = name.deref().to_case(Case::Camel);
            let swift_ty = get_swift_type_use(module, ty);
            write!(&mut code, "{}: {}", field_name, swift_ty).unwrap();
        }
        if !procedure.params_for_generate.elements.is_empty() {
            write!(&mut code, ", ").unwrap();
        }
        write!(
            &mut code,
            "callback: ((Result<{}, Error>) -> Void)? = nil",
            callback_return_ty
        )
        .unwrap();
        writeln!(&mut code, ") {{").unwrap();

        writeln!(&mut code, "        let args = _Args(").unwrap();
        first = true;
        for (name, _ty) in &procedure.params_for_generate.elements {
            if !first {
                writeln!(&mut code, ",").unwrap();
            }
            first = false;
            let field_name = name.deref().to_case(Case::Camel);
            write!(&mut code, "            {}: {}", field_name, field_name).unwrap();
        }
        if !procedure.params_for_generate.elements.is_empty() {
            writeln!(&mut code, "").unwrap();
        }
        writeln!(&mut code, "        )").unwrap();

        writeln!(&mut code, "        do {{").unwrap();
        writeln!(&mut code, "            let argBytes = try BSATNEncoder().encode(args)").unwrap();
        writeln!(&mut code, "            if let callback {{").unwrap();
        if return_is_unit {
            writeln!(
                &mut code,
                "                SpacetimeClient.shared?.sendProcedure(\"{}\", argBytes, decodeReturn: {{ _ in () }}, completion: callback)",
                procedure_name
            )
            .unwrap();
        } else {
            writeln!(
                &mut code,
                "                SpacetimeClient.shared?.sendProcedure(\"{}\", argBytes, responseType: {}.self, completion: callback)",
                procedure_name, return_swift_ty
            )
            .unwrap();
        }
        writeln!(&mut code, "            }} else {{").unwrap();
        writeln!(
            &mut code,
            "                SpacetimeClient.shared?.sendProcedure(\"{}\", argBytes)",
            procedure_name
        )
        .unwrap();
        writeln!(&mut code, "            }}").unwrap();
        writeln!(&mut code, "        }} catch {{").unwrap();
        writeln!(
            &mut code,
            "            print(\"Failed to encode {}Procedure arguments: \\(error)\")",
            procedure_name_pascal
        )
        .unwrap();
        writeln!(&mut code, "        }}").unwrap();
        writeln!(&mut code, "    }}").unwrap();
        writeln!(&mut code, "}}").unwrap();

        OutputFile {
            filename: format!("{}Procedure.swift", procedure.name.deref().to_case(Case::Pascal)),
            code,
        }
    }

    fn generate_global_files(&self, module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile> {
        let mut code = String::new();
        write_generated_file_preamble(&mut code);
        writeln!(&mut code, "import Foundation\n").unwrap();

        writeln!(&mut code, "public enum SpacetimeModule {{").unwrap();
        writeln!(&mut code, "    @MainActor public static func registerTables() {{").unwrap();
        // Register all tables
        for (table_name, _accessor_name, product_type_ref) in iter_table_names_and_types(module, options.visibility) {
            let row_type = type_ref_name(module, product_type_ref);
            writeln!(
                &mut code,
                "        SpacetimeClient.clientCache.registerTable(tableName: \"{}\", rowType: {}.self)",
                table_name.deref(),
                row_type
            )
            .unwrap();
        }
        writeln!(&mut code, "    }}").unwrap();
        writeln!(&mut code, "}}").unwrap();

        vec![OutputFile {
            filename: "SpacetimeModule.swift".to_string(),
            code,
        }]
    }
}
