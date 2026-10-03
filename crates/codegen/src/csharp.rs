// Note: the generated code is C# module syntax. It depends on the attributes in crates/bindings-csharp/BSATN.Runtime,
// and the client SDK expands it with the source generator in crates/bindings-csharp/BSATN.Codegen/Client.cs.
use super::util::fmt_fn;

use std::fmt::{self, Write};
use std::ops::Deref;

use super::code_indenter::CodeIndenter;
use super::Lang;
use crate::util::{
    collect_case, iter_indexes, iter_tables, print_auto_generated_file_comment, print_auto_generated_version_comment,
    type_ref_name,
};
use crate::{indent_scope, CodegenOptions, OutputFile};
use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::db::raw_def::v9::{Lifecycle, TableAccess};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::AlgebraicValue;
use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{ConstraintData, ModuleDef, TableDef, TypeDef, ViewDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_schema::type_for_generate::{
    AlgebraicTypeDef, AlgebraicTypeUse, PlainEnumTypeDef, ProductTypeDef, SumTypeDef, TypespaceForGenerate,
};

const INDENT: &str = "    ";

pub struct Csharp<'opts> {
    pub namespace: &'opts str,
}

// The bindings are module declarations (proposal 0040): row types carry `[SpacetimeDB.Table]`,
// and reducers, procedures, and views are bodiless `partial` methods of the `Module` class.
// The client SDK's source generator expands them into the client API: `RemoteTables`,
// `RemoteReducers`, `DbConnection`, and so on.

/// The class that holds the reducer, procedure, and view declarations, as in C# modules.
/// It is `Module`, unless the module already uses that name for a type or a function.
fn module_class(module: &ModuleDef) -> &'static str {
    let taken = module
        .types()
        .map(|typ| collect_case(Case::Pascal, typ.accessor_name.name_segments()))
        .chain(module.reducers().map(|r| r.accessor_name.deref().to_case(Case::Pascal)))
        .chain(
            module
                .procedures()
                .map(|p| p.accessor_name.deref().to_case(Case::Pascal)),
        )
        .chain(module.views().map(|v| v.accessor_name.deref().to_case(Case::Pascal)))
        .any(|name| name == "Module");
    if taken {
        "SpacetimeDBModule"
    } else {
        "Module"
    }
}

impl Lang for Csharp<'_> {
    /// Only used for tables of submodules, which C# bindings don't otherwise support.
    /// Top-level tables are declared on their row types in [`Lang::generate_type_files_with_options`].
    fn generate_table_file_from_schema(
        &self,
        module: &ModuleDef,
        table: &TableDef,
        _schema: TableSchema,
    ) -> OutputFile {
        let name = type_ref_name(module, table.product_type_ref);
        let product = module.typespace_for_generate()[table.product_type_ref]
            .as_product()
            .unwrap();
        OutputFile {
            filename: format!("Tables/{}.g.cs", table.accessor_name.deref().to_case(Case::Pascal)),
            code: autogen_csharp_tuple(module, name, product, &[table], self.namespace),
        }
    }

    fn generate_table_files(&self, _module: &ModuleDef, _table: &TableDef) -> Vec<OutputFile> {
        // C# declares a table with `[SpacetimeDB.Table]` on its row type, in the type's file.
        vec![]
    }

    fn generate_view_file(&self, module: &ModuleDef, view: &ViewDef) -> OutputFile {
        let mut output = CsharpAutogen::new(self.namespace, &[], false);
        writeln!(output, "public static partial class {}", module_class(module));
        indented_block(&mut output, |output| {
            let accessor = view.accessor_name.deref().to_case(Case::Pascal);
            let row_type = type_ref_name(module, view.product_type_ref);
            let return_type = match &view.return_type_for_generate {
                AlgebraicTypeUse::Option(_) => format!("{row_type}?"),
                _ => format!("System.Collections.Generic.List<{row_type}>"),
            };
            let context = if view.is_anonymous {
                "SpacetimeDB.AnonymousViewContext"
            } else {
                "SpacetimeDB.ViewContext"
            };
            let params = params_after_ctx(module, view.params_for_generate.into_iter(), self.namespace);

            let mut args = vec![
                format!("Accessor = \"{accessor}\""),
                format!("Name = \"{}\"", view.name),
            ];
            if view.is_public {
                args.push("Public = true".to_owned());
            }
            let schema = TableSchema::from_view_def_for_codegen(module, view);
            if let Some(pk) = schema.pk() {
                args.push(format!(
                    "PrimaryKey = \"{}\"",
                    pk.col_name.deref().to_case(Case::Pascal)
                ));
            }

            writeln!(output, "[SpacetimeDB.View({})]", args.join(", "));
            writeln!(
                output,
                "public static partial {return_type} {accessor}({context} ctx{params});"
            );
        });

        OutputFile {
            filename: format!("Views/{}.g.cs", view.accessor_name.deref().to_case(Case::Pascal)),
            code: output.into_inner(),
        }
    }

    fn generate_submodule_view_file(&self, owning_def: &ModuleDef, view: &ViewDef) -> OutputFile {
        let mut file = self.generate_view_file(owning_def, view);
        let ns_path = owning_def.accessor_path().join_segments("/");
        file.filename = format!("{}/{}", ns_path, file.filename);
        file
    }

    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile> {
        self.generate_type_files_with_options(module, typ, &CodegenOptions::default())
    }

    fn generate_type_files_with_options(
        &self,
        module: &ModuleDef,
        typ: &TypeDef,
        options: &CodegenOptions,
    ) -> Vec<OutputFile> {
        let name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
        let filename = format!("Types/{name}.g.cs");
        let code = match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Sum(sum) => autogen_csharp_sum(module, name.clone(), sum, self.namespace),
            AlgebraicTypeDef::Product(prod) => {
                // A row type declares each table that stores it.
                let tables = iter_tables(module, options.visibility)
                    .filter(|table| table.product_type_ref == typ.ty)
                    .collect::<Vec<_>>();
                autogen_csharp_tuple(module, name.clone(), prod, &tables, self.namespace)
            }
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                autogen_csharp_plain_enum(name.clone(), plain_enum, self.namespace)
            }
        };

        vec![OutputFile { filename, code }]
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &spacetimedb_schema::def::ReducerDef) -> OutputFile {
        let mut output = CsharpAutogen::new(self.namespace, &[], false);
        writeln!(output, "public static partial class {}", module_class(module));
        indented_block(&mut output, |output| {
            let func_name = reducer.accessor_name.deref().to_case(Case::Pascal);
            let func_params = params_after_ctx(module, reducer.params_for_generate.into_iter(), self.namespace);
            let kind = match reducer.lifecycle {
                None => "",
                Some(Lifecycle::OnConnect) => "SpacetimeDB.ReducerKind.ClientConnected, ",
                Some(Lifecycle::OnDisconnect) => "SpacetimeDB.ReducerKind.ClientDisconnected, ",
                // `iter_reducers` leaves out `init`.
                Some(_) => "",
            };
            writeln!(output, "[SpacetimeDB.Reducer({kind}Name = \"{}\")]", reducer.name);
            writeln!(
                output,
                "public static partial void {func_name}(SpacetimeDB.ReducerContext ctx{func_params});"
            );
        });

        OutputFile {
            filename: format!("Reducers/{}.g.cs", reducer.accessor_name.deref().to_case(Case::Pascal)),
            code: output.into_inner(),
        }
    }

    fn generate_procedure_file(
        &self,
        module: &ModuleDef,
        procedure: &spacetimedb_schema::def::ProcedureDef,
    ) -> OutputFile {
        let mut output = CsharpAutogen::new(self.namespace, &[], false);
        writeln!(output, "public static partial class {}", module_class(module));
        indented_block(&mut output, |output| {
            let func_name = procedure.accessor_name.deref().to_case(Case::Pascal);
            let func_params = params_after_ctx(module, procedure.params_for_generate.into_iter(), self.namespace);
            let return_type = ty_fmt_with_ns(module, &procedure.return_type_for_generate, self.namespace);
            writeln!(output, "[SpacetimeDB.Procedure(Name = \"{}\")]", procedure.name);
            writeln!(
                output,
                "public static partial {return_type} {func_name}(SpacetimeDB.ProcedureContext ctx{func_params});"
            );
        });

        OutputFile {
            filename: format!(
                "Procedures/{}.g.cs",
                procedure.accessor_name.deref().to_case(Case::Pascal)
            ),
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef, _options: &CodegenOptions) -> Vec<OutputFile> {
        let mut output = CsharpAutogen::new(
            self.namespace,
            &[],
            true, // print the version in the globals file
        );

        // The reducers, procedures, and views are declared in their own files, so `Module` is empty
        // here. The file still carries the CLI version, and it keeps the name of the file that used
        // to hold the client API, so regenerating older bindings overwrites that file instead of
        // listing it for deletion.
        writeln!(output, "public static partial class {}", module_class(module));
        indented_block(&mut output, |_| {});

        vec![OutputFile {
            filename: "SpacetimeDBClient.g.cs".to_owned(),
            code: output.into_inner(),
        }]
    }
}

/// The attributes that declare `tables` on their shared row type, without brackets.
struct RowTypeAttrs {
    on_type: Vec<String>,
    on_fields: Vec<Vec<String>>,
}

impl RowTypeAttrs {
    fn new(module: &ModuleDef, tables: &[&TableDef], product: &ProductTypeDef) -> Self {
        let field_name = |col: ColId| product.elements[col.idx()].0.deref().to_case(Case::Pascal);

        let mut on_type = Vec::new();
        let mut per_table = Vec::new();
        for table in tables {
            let accessor = table.accessor_name.deref().to_case(Case::Pascal);

            let mut args = vec![
                format!("Accessor = \"{accessor}\""),
                format!("Name = \"{}\"", table.name),
            ];
            if table.table_access == TableAccess::Public {
                args.push("Public = true".to_owned());
            }
            if table.is_event {
                args.push("Event = true".to_owned());
            }
            if let Some(schedule) = &table.schedule {
                let function = module
                    .reducers()
                    .find(|r| *r.name == *schedule.function_name)
                    .map(|r| r.accessor_name.to_string())
                    .or_else(|| {
                        module
                            .procedures()
                            .find(|p| *p.name == *schedule.function_name)
                            .map(|p| p.accessor_name.to_string())
                    })
                    .unwrap_or_else(|| schedule.function_name.to_string());
                args.push(format!("Scheduled = \"{}\"", function.to_case(Case::Pascal)));
                args.push(format!("ScheduledAt = \"{}\"", field_name(schedule.at_column)));
            }
            on_type.push(format!("SpacetimeDB.Table({})", args.join(", ")));

            let unique_columns = table
                .constraints
                .values()
                .filter_map(|constraint| match &constraint.data {
                    ConstraintData::Unique(unique) => unique.columns.as_singleton(),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let is_unique = |col: ColId| table.primary_key == Some(col) || unique_columns.contains(&col);

            let mut type_attrs = Vec::new();
            let mut field_attrs = vec![Vec::new(); product.elements.len()];
            for column in &table.columns {
                let col = column.col_id;
                let attrs = &mut field_attrs[col.idx()];
                if table.primary_key == Some(col) {
                    attrs.push("SpacetimeDB.PrimaryKey".to_owned());
                } else if is_unique(col) {
                    attrs.push("SpacetimeDB.Unique".to_owned());
                }
                if table.sequences.values().any(|sequence| sequence.column == col) {
                    attrs.push("SpacetimeDB.AutoInc".to_owned());
                }
                if let Some(value) = column
                    .default_value
                    .as_ref()
                    .and_then(|value| default_value_literal(module, &column.ty_for_generate, value))
                {
                    attrs.push(format!("SpacetimeDB.Default({value})"));
                }
            }
            for index in iter_indexes(table) {
                let Some(index_accessor) = index.accessor_name.as_ref() else {
                    continue;
                };
                let index_accessor = index_accessor.deref().to_case(Case::Pascal);
                let columns = index.algorithm.columns();
                // C# only has btree indexes. Clients use btrees whatever the index algorithm on the host.
                match columns.as_singleton() {
                    Some(col) => {
                        let field = field_name(col);
                        // `[PrimaryKey]` and `[Unique]` already declare this index.
                        if is_unique(col) && index_accessor == field {
                            continue;
                        }
                        let accessor_arg = if index_accessor == field {
                            String::new()
                        } else {
                            format!("Accessor = \"{index_accessor}\", ")
                        };
                        field_attrs[col.idx()]
                            .push(format!("SpacetimeDB.Index.BTree({accessor_arg}Name = \"{}\")", index.name));
                    }
                    None => type_attrs.push(format!(
                        "SpacetimeDB.Index.BTree(Accessor = \"{index_accessor}\", Name = \"{}\", Columns = new[] {{ {} }})",
                        index.name,
                        columns.iter().map(|col| format!("\"{}\"", field_name(col))).join(", ")
                    )),
                }
            }
            per_table.push((accessor, type_attrs, field_attrs));
        }

        on_type.extend(scope_attrs(
            &per_table
                .iter()
                .map(|(accessor, type_attrs, _)| (accessor.as_str(), &type_attrs[..]))
                .collect::<Vec<_>>(),
        ));
        let on_fields = (0..product.elements.len())
            .map(|i| {
                scope_attrs(
                    &per_table
                        .iter()
                        .map(|(accessor, _, field_attrs)| (accessor.as_str(), &field_attrs[i][..]))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        Self { on_type, on_fields }
    }
}

/// Merges the attributes that each table, given by its accessor, puts on one item.
/// An attribute that only some of the tables put there names the tables it applies to.
fn scope_attrs(per_table: &[(&str, &[String])]) -> Vec<String> {
    let mut distinct: Vec<&String> = Vec::new();
    for (_, attrs) in per_table {
        for attr in *attrs {
            if !distinct.contains(&attr) {
                distinct.push(attr);
            }
        }
    }
    let mut out = Vec::new();
    for attr in distinct {
        let having = per_table
            .iter()
            .filter(|(_, attrs)| attrs.contains(attr))
            .collect::<Vec<_>>();
        if having.len() == per_table.len() {
            out.push(attr.clone());
        } else {
            for (accessor, _) in having {
                out.push(match attr.strip_suffix(')') {
                    Some(args) => format!("{args}, Table = \"{accessor}\")"),
                    None => format!("{attr}(Table = \"{accessor}\")"),
                });
            }
        }
    }
    out
}

/// The C# constant for a column default, if `[SpacetimeDB.Default]` can express it.
fn default_value_literal(module: &ModuleDef, ty: &AlgebraicTypeUse, value: &AlgebraicValue) -> Option<String> {
    Some(match (ty, value) {
        (AlgebraicTypeUse::Option(_), AlgebraicValue::Sum(sum)) if sum.tag == 1 => "null!".to_owned(),
        (AlgebraicTypeUse::Option(inner), AlgebraicValue::Sum(sum)) => {
            return default_value_literal(module, inner, &sum.value)
        }
        (AlgebraicTypeUse::Ref(r), AlgebraicValue::Sum(sum)) => match &module.typespace_for_generate()[*r] {
            AlgebraicTypeDef::PlainEnum(plain_enum) => format!(
                "{}.{}",
                type_ref_name(module, *r),
                plain_enum.variants.get(sum.tag as usize)?.deref().to_case(Case::Pascal)
            ),
            _ => return None,
        },
        (_, AlgebraicValue::Bool(v)) => v.to_string(),
        (_, AlgebraicValue::I8(v)) => format!("(sbyte){v}"),
        (_, AlgebraicValue::U8(v)) => format!("(byte){v}"),
        (_, AlgebraicValue::I16(v)) => format!("(short){v}"),
        (_, AlgebraicValue::U16(v)) => format!("(ushort){v}"),
        (_, AlgebraicValue::I32(v)) => v.to_string(),
        (_, AlgebraicValue::U32(v)) => format!("{v}U"),
        (_, AlgebraicValue::I64(v)) => format!("{v}L"),
        (_, AlgebraicValue::U64(v)) => format!("{v}UL"),
        (_, AlgebraicValue::F32(v)) if v.into_inner().is_finite() => format!("{}F", v.into_inner()),
        (_, AlgebraicValue::F64(v)) if v.into_inner().is_finite() => format!("{}D", v.into_inner()),
        (_, AlgebraicValue::String(v)) => {
            let mut literal = String::from("\"");
            for c in v.chars() {
                match c {
                    '"' => literal.push_str("\\\""),
                    '\\' => literal.push_str("\\\\"),
                    '\n' => literal.push_str("\\n"),
                    '\r' => literal.push_str("\\r"),
                    '\t' => literal.push_str("\\t"),
                    c if c.is_control() => write!(literal, "\\u{:04x}", c as u32).unwrap(),
                    c => literal.push(c),
                }
            }
            literal.push('"');
            literal
        }
        _ => return None,
    })
}

fn ty_fmt<'a>(module: &'a ModuleDef, ty: &'a AlgebraicTypeUse) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicTypeUse::Identity => f.write_str("SpacetimeDB.Identity"),
        AlgebraicTypeUse::ConnectionId => f.write_str("SpacetimeDB.ConnectionId"),
        AlgebraicTypeUse::ScheduleAt => f.write_str("SpacetimeDB.ScheduleAt"),
        AlgebraicTypeUse::Timestamp => f.write_str("SpacetimeDB.Timestamp"),
        AlgebraicTypeUse::TimeDuration => f.write_str("SpacetimeDB.TimeDuration"),
        AlgebraicTypeUse::Uuid => f.write_str("SpacetimeDB.Uuid"),
        AlgebraicTypeUse::Unit => f.write_str("SpacetimeDB.Unit"),
        AlgebraicTypeUse::Option(inner_ty) => write!(f, "{}?", ty_fmt(module, inner_ty)),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => write!(
            f,
            "SpacetimeDB.Result<{}, {}>",
            ty_fmt(module, ok_ty),
            ty_fmt(module, err_ty)
        ),
        AlgebraicTypeUse::Array(elem_ty) => write!(f, "System.Collections.Generic.List<{}>", ty_fmt(module, elem_ty)),
        AlgebraicTypeUse::String => f.write_str("string"),
        AlgebraicTypeUse::Ref(r) => f.write_str(&type_ref_name(module, *r)),
        AlgebraicTypeUse::Primitive(prim) => f.write_str(match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "sbyte",
            PrimitiveType::U8 => "byte",
            PrimitiveType::I16 => "short",
            PrimitiveType::U16 => "ushort",
            PrimitiveType::I32 => "int",
            PrimitiveType::U32 => "uint",
            PrimitiveType::I64 => "long",
            PrimitiveType::U64 => "ulong",
            PrimitiveType::I128 => "I128",
            PrimitiveType::U128 => "U128",
            PrimitiveType::I256 => "I256",
            PrimitiveType::U256 => "U256",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
        }),
        AlgebraicTypeUse::Never => unimplemented!(),
    })
}

/// Like `ty_fmt`, but prefixes type references with the provided namespace.
fn ty_fmt_with_ns<'a>(module: &'a ModuleDef, ty: &'a AlgebraicTypeUse, namespace: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicTypeUse::Identity => f.write_str("SpacetimeDB.Identity"),
        AlgebraicTypeUse::ConnectionId => f.write_str("SpacetimeDB.ConnectionId"),
        AlgebraicTypeUse::ScheduleAt => f.write_str("SpacetimeDB.ScheduleAt"),
        AlgebraicTypeUse::Timestamp => f.write_str("SpacetimeDB.Timestamp"),
        AlgebraicTypeUse::TimeDuration => f.write_str("SpacetimeDB.TimeDuration"),
        AlgebraicTypeUse::Uuid => f.write_str("SpacetimeDB.Uuid"),
        AlgebraicTypeUse::Unit => f.write_str("SpacetimeDB.Unit"),
        AlgebraicTypeUse::Option(inner_ty) => write!(f, "{}?", ty_fmt_with_ns(module, inner_ty, namespace)),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => write!(
            f,
            "SpacetimeDB.Result<{}, {}>",
            ty_fmt_with_ns(module, ok_ty, namespace),
            ty_fmt_with_ns(module, err_ty, namespace)
        ),
        AlgebraicTypeUse::Array(elem_ty) => write!(
            f,
            "System.Collections.Generic.List<{}>",
            ty_fmt_with_ns(module, elem_ty, namespace)
        ),
        AlgebraicTypeUse::String => f.write_str("string"),
        AlgebraicTypeUse::Ref(r) => write!(f, "{}.{}", namespace, type_ref_name(module, *r)),
        AlgebraicTypeUse::Primitive(prim) => f.write_str(match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "sbyte",
            PrimitiveType::U8 => "byte",
            PrimitiveType::I16 => "short",
            PrimitiveType::U16 => "ushort",
            PrimitiveType::I32 => "int",
            PrimitiveType::U32 => "uint",
            PrimitiveType::I64 => "long",
            PrimitiveType::U64 => "ulong",
            PrimitiveType::I128 => "I128",
            PrimitiveType::U128 => "U128",
            PrimitiveType::I256 => "I256",
            PrimitiveType::U256 => "U256",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
        }),
        AlgebraicTypeUse::Never => unimplemented!(),
    })
}

fn default_init(ctx: &TypespaceForGenerate, ty: &AlgebraicTypeUse) -> Option<&'static str> {
    match ty {
        // Options (`T?`) have a default value of null which is fine for us.
        AlgebraicTypeUse::Option(_) => None,
        AlgebraicTypeUse::Ref(r) => match &ctx[*r] {
            // TODO: generate some proper default here (what would it be for tagged enums?).
            AlgebraicTypeDef::Sum(_) => Some("null!"),
            // Simple enums have their own default (variant with value of zero).
            AlgebraicTypeDef::PlainEnum(_) => None,
            AlgebraicTypeDef::Product(_) => Some("new()"),
        },
        // See Sum(_) handling above.
        AlgebraicTypeUse::ScheduleAt => Some("null!"),
        AlgebraicTypeUse::Array(_) => Some("new()"),
        // Strings must have explicit default value of "".
        AlgebraicTypeUse::String => Some(r#""""#),
        // Primitives are initialized to zero automatically.
        AlgebraicTypeUse::Primitive(_) => None,
        // Result<,> must be explicitly initialized.
        AlgebraicTypeUse::Result { .. } => Some("default!"),
        // these are structs, they are initialized to zero-filled automatically
        AlgebraicTypeUse::Unit
        | AlgebraicTypeUse::Identity
        | AlgebraicTypeUse::ConnectionId
        | AlgebraicTypeUse::Timestamp
        | AlgebraicTypeUse::TimeDuration
        | AlgebraicTypeUse::Uuid => None,
        AlgebraicTypeUse::Never => unimplemented!("never types are not yet supported in C# output"),
    }
}

struct CsharpAutogen {
    output: CodeIndenter<String>,
}

impl Deref for CsharpAutogen {
    type Target = CodeIndenter<String>;

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

impl std::ops::DerefMut for CsharpAutogen {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.output
    }
}

impl CsharpAutogen {
    pub fn new(namespace: &str, extra_usings: &[&str], include_version: bool) -> Self {
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output);
        if include_version {
            print_auto_generated_version_comment(&mut output);
        }

        writeln!(output, "#nullable enable");
        writeln!(output);

        writeln!(output, "using System;");
        // Don't emit `using SpacetimeDB;` if we are going to be nested in the SpacetimeDB namespace.
        if namespace
            .split('.')
            .next()
            .expect("split always returns at least one string")
            != "SpacetimeDB"
        {
            writeln!(output, "using SpacetimeDB;");
        }
        for extra_using in extra_usings {
            writeln!(output, "using {extra_using};");
        }
        writeln!(output);

        writeln!(output, "namespace {namespace}");
        writeln!(output, "{{");
        output.indent(1);

        Self { output }
    }

    pub fn into_inner(mut self) -> String {
        self.dedent(1);
        writeln!(self, "}}");

        self.output.into_inner()
    }
}

fn autogen_csharp_sum(module: &ModuleDef, sum_type_name: String, sum_type: &SumTypeDef, namespace: &str) -> String {
    let mut output = CsharpAutogen::new(namespace, &[], false);

    writeln!(output, "[SpacetimeDB.Type]");
    write!(
        output,
        "public partial record {sum_type_name} : SpacetimeDB.TaggedEnum<("
    );
    {
        indent_scope!(output);
        for (i, (variant_name, variant_ty)) in sum_type.variants.iter().enumerate() {
            if i != 0 {
                write!(output, ",");
            }
            writeln!(output);
            let variant_name = variant_name.deref().to_case(Case::Pascal);
            write!(output, "{} {variant_name}", ty_fmt(module, variant_ty));
        }
        // If we have fewer than 2 variants, we need to add some dummy variants to make the tuple work.
        match sum_type.variants.len() {
            0 => {
                writeln!(output);
                writeln!(output, "SpacetimeDB.Unit _Reserved1,");
                write!(output, "SpacetimeDB.Unit _Reserved2");
            }
            1 => {
                writeln!(output, ",");
                write!(output, "SpacetimeDB.Unit _Reserved");
            }
            _ => {}
        }
    }
    writeln!(output);
    writeln!(output, ")>;");

    output.into_inner()
}

fn autogen_csharp_plain_enum(enum_type_name: String, enum_type: &PlainEnumTypeDef, namespace: &str) -> String {
    let mut output = CsharpAutogen::new(namespace, &[], false);

    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "public enum {enum_type_name}");
    indented_block(&mut output, |output| {
        for variant in &*enum_type.variants {
            let variant = variant.deref().to_case(Case::Pascal);
            writeln!(output, "{variant},");
        }
    });

    output.into_inner()
}

/// A product type, which declares `tables` if it is their row type.
fn autogen_csharp_tuple(
    module: &ModuleDef,
    name: String,
    tuple: &ProductTypeDef,
    tables: &[&TableDef],
    namespace: &str,
) -> String {
    let mut output = CsharpAutogen::new(
        namespace,
        &["System.Collections.Generic", "System.Runtime.Serialization"],
        false,
    );

    let attrs = (!tables.is_empty()).then(|| RowTypeAttrs::new(module, tables, tuple));
    autogen_csharp_product_common(module, &mut output, name, tuple, attrs.as_ref());

    output.into_inner()
}

fn autogen_csharp_product_common(
    module: &ModuleDef,
    output: &mut CodeIndenter<String>,
    name: String,
    product_type: &ProductTypeDef,
    table_attrs: Option<&RowTypeAttrs>,
) {
    match table_attrs {
        // `[SpacetimeDB.Table]` makes the type a BSATN type as well.
        Some(attrs) => {
            for attr in &attrs.on_type {
                writeln!(output, "[{attr}]");
            }
        }
        None => writeln!(output, "[SpacetimeDB.Type]"),
    }
    writeln!(output, "[DataContract]");
    writeln!(output, "public sealed partial class {name}");
    indented_block(output, |output| {
        let fields = product_type
            .into_iter()
            .enumerate()
            .map(|(i, (orig_name, ty))| {
                writeln!(output, "[DataMember(Name = \"{orig_name}\")]");
                for attr in table_attrs.map_or(&[][..], |attrs| &attrs.on_fields[i]) {
                    writeln!(output, "[{attr}]");
                }

                let field_name = orig_name.deref().to_case(Case::Pascal);
                let ty = ty_fmt(module, ty).to_string();

                writeln!(output, "public {ty} {field_name};");

                (field_name, ty)
            })
            .collect::<Vec<_>>();

        // If we don't have any fields, the default constructor is fine, otherwise we need to generate our own.
        if !fields.is_empty() {
            writeln!(output);

            // Generate fully-parameterized constructor.
            write!(output, "public {name}(");
            if fields.len() > 1 {
                writeln!(output);
            }
            {
                indent_scope!(output);
                for (i, (field_name, ty)) in fields.iter().enumerate() {
                    if i != 0 {
                        writeln!(output, ",");
                    }
                    write!(output, "{ty} {field_name}");
                }
            }
            if fields.len() > 1 {
                writeln!(output);
            }
            writeln!(output, ")");
            indented_block(output, |output| {
                for (field_name, _ty) in fields.iter() {
                    writeln!(output, "this.{field_name} = {field_name};");
                }
            });
            writeln!(output);

            // Generate default constructor.
            writeln!(output, "public {name}()");
            indented_block(output, |output| {
                for ((field_name, _ty), (_field, field_ty)) in fields.iter().zip(product_type) {
                    if let Some(default) = default_init(module.typespace_for_generate(), field_ty) {
                        writeln!(output, "this.{field_name} = {default};");
                    }
                }
            });
        }
    });
}

fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{");
    let res = f(&mut output.indented(1));
    writeln!(output, "}}");
    res
}

/// Builds the C# parameters that follow a function's context parameter, each preceded by `, `.
fn params_after_ctx<'a, I>(module: &ModuleDef, params_iter: I, namespace: &str) -> String
where
    I: Iterator<Item = &'a (Identifier, AlgebraicTypeUse)>,
{
    let mut func_params = String::new();

    for (arg_name, arg_ty) in params_iter {
        let arg_type_str = ty_fmt_with_ns(module, arg_ty, namespace);
        let arg_name = arg_name.deref().to_case(Case::Camel);

        write!(func_params, ", {arg_type_str} {arg_name}").unwrap();
    }

    func_params
}
