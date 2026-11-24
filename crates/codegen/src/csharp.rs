use super::code_indenter::CodeIndenter;
use super::util::{
    collect_case, fmt_fn, iter_indexes, iter_tables, print_auto_generated_file_comment,
    print_auto_generated_version_comment, type_ref_name,
};
use super::Lang;
use crate::{indent_scope, CodegenOptions, CodegenVisibility, OutputFile};
use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::db::raw_def::v9::{Lifecycle, TableAccess};
use spacetimedb_lib::db::view::{extract_view_return_product_type_ref, ViewKind};
use spacetimedb_lib::sats::{layout::PrimitiveType, AlgebraicTypeRef, AlgebraicValue};
use spacetimedb_schema::def::{ModuleDef, ProcedureDef, ReducerDef, TableDef, TypeDef, ViewDef};
use spacetimedb_schema::schema::{TableCachePolicy, TableSchema};
use spacetimedb_schema::type_for_generate::{
    AlgebraicTypeDef, AlgebraicTypeUse, PlainEnumTypeDef, ProductTypeDef, SumTypeDef, TypespaceForGenerate,
};
use std::borrow::Cow;
use std::fmt::{self, Write};
use std::ops::Deref;

const INDENT: &str = "    ";

pub struct Csharp<'opts> {
    pub namespace: &'opts str,
}

enum FunctionKind {
    Reducer,
    Procedure,
    View { anonymous: bool },
}

impl Csharp<'_> {
    fn namespace(&self, module: &ModuleDef) -> String {
        let mut ns = self.namespace.to_owned();
        for segment in module.accessor_path().segments() {
            write!(ns, ".{}", identifier(segment)).unwrap();
        }
        ns
    }

    fn schema_file(&self, module: &ModuleDef, filename: String, code: String) -> OutputFile {
        let mut directory = String::from("schema");
        for segment in module.accessor_path().segments() {
            write!(directory, "/{segment}").unwrap();
        }
        OutputFile {
            filename: format!("{directory}/{filename}.g.cs"),
            code,
        }
    }

    fn function(
        &self,
        module: &ModuleDef,
        kind: FunctionKind,
        accessor: &str,
        attribute: String,
        params: &ProductTypeDef,
        result: String,
    ) -> OutputFile {
        let (kind, context) = match kind {
            FunctionKind::Reducer => ("Reducer", "ReducerContext"),
            FunctionKind::Procedure => ("Procedure", "ProcedureContext"),
            FunctionKind::View { anonymous: true } => ("View", "AnonymousViewContext"),
            FunctionKind::View { anonymous: false } => ("View", "ViewContext"),
        };
        let method = accessor.to_case(Case::Pascal);
        let mut output = CsharpAutogen::new(&self.namespace(module), &[], false);
        let container = module_class(module);
        writeln!(output, "public static partial class {container}");
        indented_block(&mut output, |out| {
            writeln!(out, "[SpacetimeDB.{attribute}]");
            let mut context_name = "ctx".to_owned();
            while params.into_iter().any(|(id, _)| id.deref() == context_name) {
                context_name.push('_');
            }
            write!(
                out,
                "public static partial {result} {method}(SpacetimeDB.{context} {context_name}"
            );
            for (name, ty) in params {
                write!(out, ", {} {}", ty_fmt(module, ty), identifier(name));
            }
            writeln!(out, ");");
        });
        self.schema_file(module, format!("{method}.{kind}"), output.into_inner())
    }

    fn scope_files(&self, module: &ModuleDef, visibility: CodegenVisibility, files: &mut Vec<OutputFile>) {
        let visible_tables = iter_tables(module, visibility).collect::<Vec<_>>();
        for typ in module.types() {
            let name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
            let namespace = self.namespace(module);
            let code = match &module.typespace_for_generate()[typ.ty] {
                AlgebraicTypeDef::Sum(sum) => autogen_csharp_sum(module, name.clone(), sum, &namespace),
                AlgebraicTypeDef::PlainEnum(en) => autogen_csharp_plain_enum(name.clone(), en, &namespace),
                AlgebraicTypeDef::Product(_) => {
                    let tables = visible_tables
                        .iter()
                        .copied()
                        .filter(|t| t.product_type_ref == typ.ty)
                        .collect::<Vec<_>>();
                    // A row belonging to one table is emitted inline with its table attribute.
                    if tables.len() == 1 && !has_shared_row(module, tables[0]) {
                        continue;
                    }
                    let mut out = CsharpAutogen::new(&namespace, &["System.Runtime.Serialization"], false);
                    writeln!(out, "[SpacetimeDB.Type]");
                    write_product_fields(module, &mut out, &name, typ.ty, &tables);
                    out.into_inner()
                }
            };
            files.push(self.schema_file(module, format!("{name}.Type"), code));
        }

        // The common client traversal omits Init, since it cannot be invoked by clients.
        if module.path().is_empty()
            && visibility == CodegenVisibility::IncludePrivate
            && let Some((_, init)) = module.lifecycle_reducer(Lifecycle::Init)
        {
            files.push(self.generate_reducer_file(module, init));
        }

        // Reference an existing declaration wherever possible. An empty submodule still
        // needs a marker so the SDK can identify the directory and its descendants.
        let needs_marker = !module.path().is_empty() && scope_marker(module).is_none();
        if !module.submodules().is_empty() || needs_marker {
            let mut out = CodeIndenter::new(String::new(), INDENT);
            print_auto_generated_file_comment(&mut out);
            for (canonical, child) in module.submodules() {
                let accessor = child.mount_accessor_name().unwrap();
                let marker = scope_marker(child).unwrap_or_else(|| module_class(child));
                writeln!(out, "[assembly: SpacetimeDB.Namespace(typeof(global::{}.{marker}), Accessor = \"{accessor}\", Name = \"{canonical}\")]", self.namespace(child));
            }
            if needs_marker {
                writeln!(
                    out,
                    "namespace {} {{ public static partial class {} {{ }} }}",
                    self.namespace(module),
                    module_class(module)
                );
            }
            files.push(self.schema_file(module, "Module".into(), out.into_inner()));
        }

        for child in module.submodules().values() {
            self.scope_files(child, CodegenVisibility::OnlyPublic, files);
        }
    }
}

fn scope_marker(module: &ModuleDef) -> Option<String> {
    module
        .types()
        .map(|ty| collect_case(Case::Pascal, ty.accessor_name.name_segments()))
        .min()
        .or_else(|| {
            let has_functions = module.reducers().any(|r| !r.visibility.is_private())
                || module.procedures().any(|p| !p.visibility.is_private())
                || module.views().next().is_some();
            has_functions.then(|| module_class(module))
        })
}

// Namespace accessors and parameter names retain their original casing and can be C# keywords.
fn identifier(name: &str) -> Cow<'_, str> {
    match name {
        "abstract" | "as" | "base" | "bool" | "break" | "byte" | "case" | "catch" | "char" | "checked" | "class"
        | "const" | "continue" | "decimal" | "default" | "delegate" | "do" | "double" | "else" | "enum" | "event"
        | "explicit" | "extern" | "false" | "finally" | "fixed" | "float" | "for" | "foreach" | "goto" | "if"
        | "implicit" | "in" | "int" | "interface" | "internal" | "is" | "lock" | "long" | "namespace" | "new"
        | "null" | "object" | "operator" | "out" | "override" | "params" | "private" | "protected" | "public"
        | "readonly" | "ref" | "return" | "sbyte" | "sealed" | "short" | "sizeof" | "stackalloc" | "static"
        | "string" | "struct" | "switch" | "this" | "throw" | "true" | "try" | "typeof" | "uint" | "ulong"
        | "unchecked" | "unsafe" | "ushort" | "using" | "virtual" | "void" | "volatile" | "while" => {
            Cow::Owned(format!("@{name}"))
        }
        _ => Cow::Borrowed(name),
    }
}

fn module_class(module: &ModuleDef) -> String {
    let mut name = String::from("Module");
    while module
        .types()
        .any(|ty| collect_case(Case::Pascal, ty.accessor_name.name_segments()) == name)
        || module
            .reducers()
            .any(|r| r.accessor_name.as_ref().to_case(Case::Pascal) == name)
        || module
            .procedures()
            .any(|p| p.accessor_name.deref().to_case(Case::Pascal) == name)
        || module
            .views()
            .any(|v| v.accessor_name.deref().to_case(Case::Pascal) == name)
    {
        name.push('_');
    }
    name
}

fn has_shared_row(module: &ModuleDef, table: &TableDef) -> bool {
    module
        .tables()
        .any(|other| other.product_type_ref == table.product_type_ref && other.name != table.name)
}

fn write_product_fields(
    module: &ModuleDef,
    out: &mut CodeIndenter<String>,
    name: &str,
    ty: AlgebraicTypeRef,
    tables: &[&TableDef],
) {
    let product = module.typespace_for_generate()[ty].as_product().unwrap();
    let wire_product = module.typespace()[ty].as_product().unwrap();
    writeln!(out, "[DataContract]");
    writeln!(out, "public sealed partial class {name}");
    indented_block(out, |out| {
        for (i, (field, ty)) in product.elements.iter().enumerate() {
            for table in tables {
                let col = &table.columns[i];
                let scope = if tables.len() > 1 {
                    format!("(Table = \"{}\")", table.accessor_name.deref().to_case(Case::Pascal))
                } else {
                    String::new()
                };
                if table.primary_key == Some(col.col_id) {
                    writeln!(out, "[SpacetimeDB.PrimaryKey{scope}]");
                } else if table
                    .constraints
                    .values()
                    .any(|c| c.data.unique_columns().and_then(|cols| cols.as_singleton()) == Some(col.col_id))
                {
                    writeln!(out, "[SpacetimeDB.Unique{scope}]");
                }
                if table.sequences.values().any(|s| s.column == col.col_id) {
                    writeln!(out, "[SpacetimeDB.AutoInc{scope}]");
                }
            }
            if let Some(first) = tables.first()
                && tables
                    .iter()
                    .all(|table| table.columns[i].default_value == first.columns[i].default_value)
                && let Some(default) = first.columns[i].default_value.as_ref().and_then(default_literal)
            {
                writeln!(out, "[SpacetimeDB.Default({default})]");
            }
            let wire = wire_product.elements[i].name.as_deref().expect("named product field");
            writeln!(out, "[DataMember(Name = \"{wire}\")]");
            writeln!(
                out,
                "public {} {};",
                ty_fmt(module, ty),
                field.deref().to_case(Case::Pascal)
            );
        }
    });
}

// Only CLR constants can be arguments of a C# attribute.
fn default_literal(value: &AlgebraicValue) -> Option<String> {
    Some(match value {
        AlgebraicValue::Bool(v) => v.to_string(),
        AlgebraicValue::I8(v) => format!("(sbyte){v}"),
        AlgebraicValue::U8(v) => format!("(byte){v}"),
        AlgebraicValue::I16(v) => format!("(short){v}"),
        AlgebraicValue::U16(v) => format!("(ushort){v}"),
        AlgebraicValue::I32(v) => v.to_string(),
        AlgebraicValue::U32(v) => format!("{v}U"),
        AlgebraicValue::I64(v) => format!("{v}L"),
        AlgebraicValue::U64(v) => format!("{v}UL"),
        AlgebraicValue::F32(v) if v.into_inner().is_finite() => format!("{v}F"),
        AlgebraicValue::F64(v) if v.into_inner().is_finite() => format!("{v}D"),
        AlgebraicValue::String(v) => serde_json::to_string(v).unwrap(),
        _ => return None,
    })
}

impl Lang for Csharp<'_> {
    fn generate_table_file_from_schema(&self, module: &ModuleDef, table: &TableDef, schema: TableSchema) -> OutputFile {
        let namespace = self.namespace(module);
        let mut out = CsharpAutogen::new(&namespace, &["System.Runtime.Serialization"], false);
        let accessor = table.accessor_name.deref().to_case(Case::Pascal);
        let row = type_ref_name(module, table.product_type_ref);
        let product = module.typespace_for_generate()[table.product_type_ref]
            .as_product()
            .unwrap();
        write!(
            out,
            "[SpacetimeDB.Table(Accessor = \"{accessor}\", Name = \"{}\"",
            table.name
        );
        if table.table_access == TableAccess::Public {
            write!(out, ", Public = true");
        }
        if table.is_event {
            write!(out, ", Event = true");
        }
        if table.cache_policy != TableCachePolicy::RequiredInMemory {
            write!(
                out,
                ", CachePolicy = SpacetimeDB.TableCachePolicy.{:?}",
                table.cache_policy
            );
        }
        if let Some(schedule) = &table.schedule {
            let function = &schedule.function_name;
            let accessor = module
                .reducers()
                .find(|r| r.name.local() == function)
                .map(|r| r.accessor_name.as_ref())
                .or_else(|| {
                    module
                        .procedures()
                        .find(|p| &p.name == function)
                        .map(|p| p.accessor_name.deref())
                })
                .unwrap_or(function);
            let at = product.elements[schedule.at_column.idx()]
                .0
                .deref()
                .to_case(Case::Pascal);
            write!(out, ", Scheduled = \"{}\"", accessor.to_case(Case::Pascal));
            if at != "ScheduledAt" {
                write!(out, ", ScheduledAt = \"{at}\"");
            }
        }
        writeln!(out, ")]");
        let shared = has_shared_row(module, table);
        let table_arg = if shared {
            format!(", Table = \"{accessor}\"")
        } else {
            String::new()
        };
        for index in iter_indexes(table) {
            let cols = index.algorithm.columns();
            let columns = cols
                .iter()
                .map(|id| format!("\"{}\"", product.elements[id.idx()].0.deref().to_case(Case::Pascal)))
                .join(", ");
            let index_accessor = index.accessor_name.as_ref().map(|n| n.deref().to_case(Case::Pascal));
            let unique = schema.is_unique(&cols);
            if let Some(col) = cols.as_singleton()
                && unique
            {
                let field = product.elements[col.idx()].0.deref().to_case(Case::Pascal);
                if index_accessor.as_ref().is_none_or(|a| a == &field) {
                    continue;
                }
            }
            if let Some(index_accessor) = &index_accessor
                && !(unique && cols.as_singleton().is_none())
            {
                writeln!(out, "[SpacetimeDB.Index.BTree(Accessor = \"{index_accessor}\", Name = {}, Columns = new[] {{ {columns} }}{table_arg})]", serde_json::to_string(index.name.as_ref()).unwrap());
            } else {
                let handle = index_accessor
                    .map(|n| format!("\"{n}\""))
                    .unwrap_or_else(|| "null".into());
                writeln!(
                    out,
                    "[SpacetimeDB.ClientIndex(new[] {{ {columns} }}, {handle}, {unique}{table_arg})]"
                );
            }
        }
        if shared {
            writeln!(out, "public sealed partial class {row} {{ }}");
        } else {
            write_product_fields(module, &mut out, &row, table.product_type_ref, &[table]);
        }
        self.schema_file(module, format!("{accessor}.Table"), out.into_inner())
    }

    fn generate_type_files(&self, _: &ModuleDef, _: &TypeDef) -> Vec<OutputFile> {
        // scope_files emits named types with knowledge of the selected tables and visibility.
        Vec::new()
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        let kind = match reducer.lifecycle {
            None => "",
            Some(Lifecycle::Init) => "SpacetimeDB.ReducerKind.Init, ",
            Some(Lifecycle::OnConnect) => "SpacetimeDB.ReducerKind.ClientConnected, ",
            Some(Lifecycle::OnDisconnect) => "SpacetimeDB.ReducerKind.ClientDisconnected, ",
            Some(other) => unimplemented!("unsupported C# lifecycle reducer {other:?}"),
        };
        self.function(
            module,
            FunctionKind::Reducer,
            reducer.accessor_name.as_ref(),
            format!("Reducer({kind}Name = \"{}\")", reducer.name.local()),
            &reducer.params_for_generate,
            "void".into(),
        )
    }

    fn generate_procedure_file(&self, module: &ModuleDef, procedure: &ProcedureDef) -> OutputFile {
        self.function(
            module,
            FunctionKind::Procedure,
            &procedure.accessor_name,
            format!("Procedure(Name = \"{}\")", procedure.name),
            &procedure.params_for_generate,
            ty_fmt(module, &procedure.return_type_for_generate).to_string(),
        )
    }

    fn generate_view_file(&self, module: &ModuleDef, view: &ViewDef) -> OutputFile {
        let accessor = view.accessor_name.deref().to_case(Case::Pascal);
        let pk = view
            .primary_key
            .map(|id| {
                format!(
                    ", PrimaryKey = \"{}\"",
                    view.return_columns[id.idx()]
                        .accessor_name
                        .deref()
                        .to_case(Case::Pascal)
                )
            })
            .unwrap_or_default();
        let result = match extract_view_return_product_type_ref(&view.return_type) {
            Some((row, ViewKind::Query)) => format!("SpacetimeDB.IQuery<{}>", type_ref_name(module, row)),
            _ => ty_fmt(module, &view.return_type_for_generate).to_string(),
        };
        self.function(
            module,
            FunctionKind::View {
                anonymous: view.is_anonymous,
            },
            &accessor,
            format!(
                "View(Accessor = \"{accessor}\", Name = \"{}\", Public = {}{pk})",
                view.name, view.is_public
            ),
            &view.params_for_generate,
            result,
        )
    }

    fn generate_submodule_table_file(&self, module: &ModuleDef, table: &TableDef) -> OutputFile {
        self.generate_table_file(module, table)
    }
    fn generate_submodule_view_file(&self, module: &ModuleDef, view: &ViewDef) -> OutputFile {
        self.generate_view_file(module, view)
    }
    fn generate_submodule_reducer_file(&self, module: &ModuleDef, reducer: &ReducerDef) -> OutputFile {
        self.generate_reducer_file(module, reducer)
    }
    fn generate_submodule_procedure_file(&self, module: &ModuleDef, proc: &ProcedureDef) -> OutputFile {
        self.generate_procedure_file(module, proc)
    }

    fn generate_global_files(&self, module: &ModuleDef, options: &CodegenOptions) -> Vec<OutputFile> {
        let mut out = CsharpAutogen::new(self.namespace, &[], true);
        writeln!(out, "[SpacetimeDB.ClientConnection(\"schema\")]");
        writeln!(out, "public sealed class DbConnection : ModuleConnectionBase {{ }}");
        let mut files = vec![OutputFile {
            filename: "SpacetimeDBClient.g.cs".into(),
            code: out.into_inner(),
        }];
        self.scope_files(module, options.visibility, &mut files);
        files
    }
}

/// Standalone BSATN type definitions used to bootstrap the C# server runtime itself.
pub fn generate_runtime_type(module: &ModuleDef, typ: &TypeDef, namespace: &str) -> OutputFile {
    let name = collect_case(Case::Pascal, typ.accessor_name.name_segments());
    let code = match &module.typespace_for_generate()[typ.ty] {
        AlgebraicTypeDef::Sum(sum) => autogen_csharp_sum(module, name.clone(), sum, namespace),
        AlgebraicTypeDef::Product(product) => autogen_csharp_tuple(module, name.clone(), product, namespace),
        AlgebraicTypeDef::PlainEnum(en) => autogen_csharp_plain_enum(name.clone(), en, namespace),
    };
    OutputFile {
        filename: format!("{name}.g.cs"),
        code,
    }
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

fn autogen_csharp_tuple(module: &ModuleDef, name: String, tuple: &ProductTypeDef, namespace: &str) -> String {
    let mut output = CsharpAutogen::new(
        namespace,
        &["System.Collections.Generic", "System.Runtime.Serialization"],
        false,
    );

    let product_type = tuple;
    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "[DataContract]");
    writeln!(output, "public sealed partial class {name}");
    indented_block(&mut output, |output| {
        let fields = product_type
            .into_iter()
            .map(|(orig_name, ty)| {
                writeln!(output, "[DataMember(Name = \"{orig_name}\")]");

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
    output.into_inner()
}

fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{");
    let res = f(&mut output.indented(1));
    writeln!(output, "}}");
    res
}
