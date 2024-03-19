use super::util::fmt_fn;

use std::fmt::{self, Write};

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::db::def::TableSchema;
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, ArrayType, BuiltinType, MapType, ProductType, SumType};
use spacetimedb_lib::{ReducerDef, TableDesc};
use spacetimedb_primitives::ColList;

use super::code_indenter::CodeIndenter;
use super::{GenCtx, GenItem};

enum MaybePrimitive<'a> {
    Primitive(&'static str),
    Array(&'a ArrayType),
    Map(&'a MapType),
}

fn maybe_primitive(b: &BuiltinType) -> MaybePrimitive {
    MaybePrimitive::Primitive(match b {
        BuiltinType::Bool => "bool",
        BuiltinType::I8 => "sbyte",
        BuiltinType::U8 => "byte",
        BuiltinType::I16 => "short",
        BuiltinType::U16 => "ushort",
        BuiltinType::I32 => "int",
        BuiltinType::U32 => "uint",
        BuiltinType::I64 => "long",
        BuiltinType::U64 => "ulong",
        // BuiltinType::I128 => "int128", Not a supported type in csharp
        // BuiltinType::U128 => "uint128", Not a supported type in csharp
        BuiltinType::I128 => panic!("i128 not supported for csharp"),
        BuiltinType::U128 => panic!("i128 not supported for csharp"),
        BuiltinType::String => "string",
        BuiltinType::F32 => "float",
        BuiltinType::F64 => "double",
        BuiltinType::Array(ty) => return MaybePrimitive::Array(ty),
        BuiltinType::Map(m) => return MaybePrimitive::Map(m),
    })
}

fn ty_fmt<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, namespace: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Sum(sum_type) => {
            // This better be an option type
            if let Some(inner_ty) = sum_type.as_option() {
                write!(f, "{}?", ty_fmt(ctx, inner_ty, namespace))
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Product(prod) => {
            // The only type that is allowed here is the identity type. All other types should fail.
            if prod.is_identity() {
                write!(f, "SpacetimeDB.Identity")
            } else if prod.is_address() {
                write!(f, "SpacetimeDB.Address")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Builtin(b) => match maybe_primitive(b) {
            MaybePrimitive::Primitive(p) => f.write_str(p),
            MaybePrimitive::Array(ArrayType { elem_ty }) if **elem_ty == AlgebraicType::U8 => f.write_str("byte[]"),
            MaybePrimitive::Array(ArrayType { elem_ty }) => {
                write!(
                    f,
                    "System.Collections.Generic.List<{}>",
                    ty_fmt(ctx, elem_ty, namespace)
                )
            }
            MaybePrimitive::Map(ty) => {
                write!(
                    f,
                    "System.Collections.Generic.Dictionary<{}, {}>",
                    ty_fmt(ctx, &ty.ty, namespace),
                    ty_fmt(ctx, &ty.key_ty, namespace)
                )
            }
        },
        AlgebraicType::Ref(r) => {
            let name = csharp_typename(ctx, *r);
            match &ctx.typespace.types[r.idx()] {
                AlgebraicType::Sum(sum_type) => {
                    if sum_type.is_simple_enum() {
                        let parts: Vec<&str> = name.split('.').collect();
                        if parts.len() >= 2 {
                            let enum_namespace = parts[0];
                            let enum_name = parts[1];
                            write!(f, "{namespace}.{enum_namespace}.Types.{enum_name}")
                        } else {
                            write!(f, "{}.{}", namespace, name)
                        }
                    } else {
                        write!(f, "{}.{}", namespace, name)
                    }
                }
                _ => {
                    write!(f, "{}.{}", namespace, name)
                }
            }
        }
    })
}

fn default_init(ctx: &GenCtx, ty: &AlgebraicType) -> &'static str {
    match ty {
        AlgebraicType::Sum(sum_type) => {
            // Options have a default value of null which is fine for us, and simple enums have their own default.
            if sum_type.as_option().is_some() || sum_type.is_simple_enum() {
                ""
            } else {
                unimplemented!()
            }
        }
        // For product types, we can just use the default constructor.
        AlgebraicType::Product(_) => " = new()",
        AlgebraicType::Builtin(b) => match b {
            // Strings must have explicit default value of "".
            BuiltinType::String => r#" = """#,
            // Byte arrays must be initialized to an empty array.
            BuiltinType::Array(a) if *a.elem_ty == AlgebraicType::U8 => " = Array.Empty<byte>()",
            // Lists and Dictionaries must be instantiated with new().
            BuiltinType::Array(_) | BuiltinType::Map(_) => " = new()",
            _ => "",
        },
        AlgebraicType::Ref(r) => default_init(ctx, &ctx.typespace.types[r.idx()]),
    }
}

// can maybe do something fancy with this in the future
fn csharp_typename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> &str {
    ctx.names[typeref.idx()].as_deref().expect("tuples should have names")
}

macro_rules! indent_scope {
    ($x:ident) => {
        let mut $x = $x.indented(1);
    };
}

macro_rules! block {
    ($output:ident, $block:block) => {
        writeln!($output, "{{").unwrap();
        {
            indent_scope!($output);
            $block
        }
        writeln!($output, "}}").unwrap();
    };
}

fn autogen_csharp_header(namespace: &str, extra_usings: &[&str]) -> CodeIndenter<String> {
    let mut output = String::new();

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE"
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "#nullable enable").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "using System;").unwrap();
    if namespace != "SpacetimeDB" {
        writeln!(output, "using SpacetimeDB;").unwrap();
    }
    for extra_using in extra_usings {
        writeln!(output, "using {extra_using};").unwrap();
    }
    writeln!(output).unwrap();

    writeln!(output, "namespace {}", namespace).unwrap();
    writeln!(output, "{{").unwrap();

    let mut output = CodeIndenter::new(output);
    output.indent(1);
    output
}

fn autogen_csharp_footer(output: CodeIndenter<String>) -> String {
    let mut output = output.into_inner();
    writeln!(output, "}}").unwrap();
    output
}

pub fn autogen_csharp_sum(
    /* will be used in future for tagged enum */ _ctx: &GenCtx,
    name: &str,
    sum_type: &SumType,
    namespace: &str,
) -> String {
    if sum_type.is_simple_enum() {
        autogen_csharp_enum(name, sum_type, namespace)
    } else {
        unimplemented!();
    }
}

pub fn autogen_csharp_enum(name: &str, sum_type: &SumType, namespace: &str) -> String {
    let mut output = autogen_csharp_header(namespace, &[]);

    let mut sum_namespace = None;
    let mut sum_type_name = name.replace("r#", "").to_case(Case::Pascal);
    if sum_type_name.contains('.') {
        let split: Vec<&str> = sum_type_name.split('.').collect();
        if split.len() != 2 {
            panic!("Enum names cannot contain more than one namespace prefix. Example: MyNamespace.MyEnum");
        }

        sum_namespace = Some(split[0].to_string().to_case(Case::Pascal));
        sum_type_name = split[1].to_string().to_case(Case::Pascal);
    }

    if let Some(sum_namespace) = &sum_namespace {
        writeln!(output, "public partial class {sum_namespace}").unwrap();
        writeln!(output, "{{").unwrap();
        output.indent(1);

        writeln!(output, "public partial class Types").unwrap();
        writeln!(output, "{{").unwrap();
        output.indent(1);
    }

    writeln!(output, "[SpacetimeDB.Type]").unwrap();
    writeln!(output, "public enum {sum_type_name}").unwrap();
    block!(output, {
        for variant in &sum_type.variants {
            let variant_name = variant
                .name
                .as_ref()
                .expect("All sum variants should have names!")
                .replace("r#", "");
            writeln!(output, "{},", variant_name).unwrap();
        }
    });

    if sum_namespace.is_some() {
        for _ in 0..2 {
            output.dedent(1);
            writeln!(output, "}}").unwrap();
        }
    }

    autogen_csharp_footer(output)
}

pub fn autogen_csharp_tuple(ctx: &GenCtx, name: &str, tuple: &ProductType, namespace: &str) -> String {
    autogen_csharp_product_table_common(ctx, name, tuple, None, namespace)
}

pub fn autogen_csharp_table(ctx: &GenCtx, table: &TableDesc, namespace: &str) -> String {
    let tuple = ctx.typespace[table.data].as_product().unwrap();
    autogen_csharp_product_table_common(
        ctx,
        &table.schema.table_name,
        tuple,
        Some(
            table
                .schema
                .clone()
                .into_schema(0.into())
                .validated()
                .expect("Failed to generate table due to validation errors"),
        ),
        namespace,
    )
}

fn autogen_csharp_product_table_common(
    ctx: &GenCtx,
    name: &str,
    product_type: &ProductType,
    schema: Option<TableSchema>,
    namespace: &str,
) -> String {
    let mut output = autogen_csharp_header(
        namespace,
        &[
            "System.Collections.Generic",
            "System.Linq",
            "System.Runtime.Serialization",
        ],
    );

    writeln!(output, "[SpacetimeDB.Type]").unwrap();
    writeln!(output, "[DataContract]").unwrap();
    write!(output, "public partial class {name}").unwrap();
    if let Some(schema) = &schema {
        write!(
            output,
            " : SpacetimeDB.{parent}<{name}, {namespace}.ReducerEvent>",
            parent = if schema.pk().is_some() {
                "DatabaseTableWithPrimaryKey"
            } else {
                "DatabaseTable"
            }
        )
        .unwrap();
    }
    writeln!(output).unwrap();
    block!(output, {
        for field in &product_type.elements {
            let field_name = field
                .name
                .as_ref()
                .expect("autogen'd tuples should have field names")
                .replace("r#", "");

            writeln!(output, "[DataMember(Name = \"{field_name}\")]").unwrap();
            writeln!(
                output,
                "public {} {}{};",
                ty_fmt(ctx, &field.algebraic_type, namespace),
                field_name.to_case(Case::Pascal),
                default_init(ctx, &field.algebraic_type)
            )
            .unwrap();
        }
        writeln!(output).unwrap();

        // If this is a table, we want to generate event accessor and indexes
        if let Some(schema) = &schema {
            let constraints = schema.column_constraints();
            let mut unique_indexes = Vec::new();
            // Declare custom index dictionaries
            for col in schema.columns() {
                let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                if !constraints[&ColList::new(col.col_pos)].has_unique() {
                    continue;
                }
                let type_name = ty_fmt(ctx, &col.col_type, namespace);
                let comparer = if format!("{}", type_name) == "byte[]" {
                    ", SpacetimeDB.ByteArrayComparer.Instance"
                } else {
                    ""
                };
                writeln!(
                    output,
                    "private static Dictionary<{type_name}, {name}> {field_name}_Index = new (16{comparer});"
                )
                .unwrap();
                unique_indexes.push(field_name);
            }
            if !unique_indexes.is_empty() {
                writeln!(output).unwrap();
                // OnInsert method for updating indexes
                writeln!(output, "public override void InternalOnValueInserted()").unwrap();
                block!(output, {
                    for col in schema.columns() {
                        let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                        if !constraints[&ColList::new(col.col_pos)].has_unique() {
                            continue;
                        }
                        writeln!(output, "{field_name}_Index[{field_name}] = this;").unwrap();
                    }
                });
                writeln!(output).unwrap();
                // OnDelete method for updating indexes
                writeln!(output, "public override void InternalOnValueDeleted()").unwrap();
                block!(output, {
                    for col in schema.columns() {
                        let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                        if !constraints[&ColList::new(col.col_pos)].has_unique() {
                            continue;
                        }
                        writeln!(output, "{field_name}_Index.Remove({field_name});").unwrap();
                    }
                });
                writeln!(output).unwrap();
            }

            // If this is a table, we want to include functions for accessing the table data
            // Insert the funcs for accessing this struct
            autogen_csharp_access_funcs_for_struct(&mut output, name, product_type, name, schema, ctx, namespace);
            writeln!(output).unwrap();
        }
    });

    autogen_csharp_footer(output)
}

fn autogen_csharp_access_funcs_for_struct(
    output: &mut CodeIndenter<String>,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
    _table_name: &str,
    schema: &TableSchema,
    ctx: &GenCtx,
    namespace: &str,
) {
    let constraints = schema.column_constraints();
    for col in schema.columns() {
        let is_unique = constraints[&ColList::new(col.col_pos)].has_unique();

        let col_i: usize = col.col_pos.into();

        let field = &product_type.elements[col_i];
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let csharp_field_type = ty_fmt(ctx, field_type, namespace);
        let csharp_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);

        let filter_return_type = fmt_fn(|f| {
            if is_unique {
                write!(f, "{struct_name_pascal_case}?")
            } else {
                write!(f, "IEnumerable<{struct_name_pascal_case}>")
            }
        });

        writeln!(
            output,
            "public static {filter_return_type} FilterBy{}({} value)",
            csharp_field_name_pascal, csharp_field_type
        )
        .unwrap();

        block!(output, {
            if is_unique {
                writeln!(
                    output,
                    "{csharp_field_name_pascal}_Index.TryGetValue(value, out var r);"
                )
                .unwrap();
                writeln!(output, "return r;").unwrap();
            } else {
                write!(output, "return Query(x => x.{csharp_field_name_pascal} == value)").unwrap();

                if is_unique {
                    write!(output, ".SingleOrDefault()").unwrap();
                }

                writeln!(output, ";").unwrap();
            }
        });
        writeln!(output).unwrap();
    }

    if let Some(primary_col_index) = schema.pk() {
        writeln!(
            output,
            "public override object GetPrimaryKeyValue() => {col_name_pascal_case};",
            col_name_pascal_case = primary_col_index.col_name.replace("r#", "").to_case(Case::Pascal)
        )
        .unwrap();
    }
}

pub fn autogen_csharp_reducer(ctx: &GenCtx, reducer: &ReducerDef, namespace: &str) -> String {
    let func_name = &*reducer.name;
    // let reducer_pascal_name = func_name.to_case(Case::Pascal);
    let func_name_pascal_case = func_name.to_case(Case::Pascal);

    let mut output = autogen_csharp_header(namespace, &["ClientApi"]);

    //Args struct
    writeln!(output, "[SpacetimeDB.Type]").unwrap();
    writeln!(
        output,
        "public partial class {func_name_pascal_case}ArgsStruct : IReducerArgs"
    )
    .unwrap();

    let mut func_params: String = String::new();
    let mut field_inits: String = String::new();

    block!(output, {
        writeln!(
            output,
            "ReducerType IReducerArgs.ReducerType => ReducerType.{func_name_pascal_case};"
        )
        .unwrap();
        writeln!(output, "string IReducerArgsBase.ReducerName => \"{func_name}\";").unwrap();
        writeln!(output, "bool IReducerArgs.InvokeHandler(ReducerEvent reducerEvent) => Reducer.On{func_name_pascal_case}(reducerEvent, this);").unwrap();
        if !reducer.args.is_empty() {
            writeln!(output).unwrap();
        }
        for (arg_i, arg) in reducer.args.iter().enumerate() {
            let name = arg
                .name
                .as_deref()
                .unwrap_or_else(|| panic!("reducer args should have names: {}", func_name));
            let arg_name = name.to_case(Case::Camel);
            let field_name = name.to_case(Case::Pascal);
            let arg_type_str = ty_fmt(ctx, &arg.algebraic_type, namespace);

            if arg_i != 0 {
                func_params.push_str(", ");
                field_inits.push_str(", ");
            }
            writeln!(
                output,
                "public {arg_type_str} {field_name}{};",
                default_init(ctx, &arg.algebraic_type)
            )
            .unwrap();
            write!(func_params, "{arg_type_str} {arg_name}").unwrap();
            write!(field_inits, "{field_name} = {arg_name}").unwrap();
        }
    });

    writeln!(output).unwrap();

    writeln!(output, "public static partial class Reducer").unwrap();
    block!(output, {
        let delegate_separator = if !reducer.args.is_empty() { ", " } else { "" };
        writeln!(
            output,
            "public delegate void {func_name_pascal_case}Handler(ReducerEvent reducerEvent{delegate_separator}{func_params});"
        )
        .unwrap();
        writeln!(
            output,
            "public static event {func_name_pascal_case}Handler? On{func_name_pascal_case}Event;"
        )
        .unwrap();

        writeln!(output).unwrap();

        writeln!(output, "public static void {func_name_pascal_case}({func_params})").unwrap();
        block!(output, {
            writeln!(
                output,
                "SpacetimeDBClient.instance.InternalCallReducer(new {func_name_pascal_case}ArgsStruct {{ {field_inits} }});"
            )
            .unwrap();
        });
        writeln!(output).unwrap();

        writeln!(
            output,
            "public static bool On{func_name_pascal_case}(ReducerEvent reducerEvent, {func_name_pascal_case}ArgsStruct args)"
        )
        .unwrap();
        block!(output, {
            writeln!(output, "if (On{func_name_pascal_case}Event == null) return false;").unwrap();
            writeln!(output, "On{func_name_pascal_case}Event(").unwrap();
            // Write out arguments one per line
            {
                indent_scope!(output);
                write!(output, "reducerEvent").unwrap();
                for (i, arg) in reducer.args.iter().enumerate() {
                    writeln!(output, ",").unwrap();
                    let arg_name = arg
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("arg_{i}"))
                        .to_case(Case::Pascal);
                    write!(output, "args.{arg_name}").unwrap();
                }
                writeln!(output).unwrap();
            }
            writeln!(output, ");").unwrap();
            writeln!(output, "return true;").unwrap();
        });
    });
    writeln!(output).unwrap();

    autogen_csharp_footer(output)
}

pub fn autogen_csharp_globals(items: &[GenItem], namespace: &str) -> Vec<Vec<(String, String)>> {
    let reducers: Vec<&ReducerDef> = items
        .iter()
        .filter_map(|i| {
            if let GenItem::Reducer(reducer) = i {
                Some(reducer)
            } else {
                None
            }
        })
        .collect();
    let reducer_names: Vec<String> = reducers
        .iter()
        .map(|reducer| reducer.name.to_case(Case::Pascal))
        .collect();

    let mut output = autogen_csharp_header(namespace, &["System.Runtime.CompilerServices", "ClientApi"]);

    writeln!(output, "public enum ReducerType").unwrap();
    block!(output, {
        writeln!(output, "None,").unwrap();
        for reducer_name in &reducer_names {
            writeln!(output, "{reducer_name},").unwrap();
        }
    });
    writeln!(output).unwrap();

    writeln!(output, "public interface IReducerArgs : IReducerArgsBase").unwrap();
    block!(output, {
        writeln!(output, "ReducerType ReducerType {{ get; }}").unwrap();
        writeln!(output, "bool InvokeHandler(ReducerEvent reducerEvent);").unwrap();
    });
    writeln!(output).unwrap();

    writeln!(output, "public partial class ReducerEvent : ReducerEventBase").unwrap();
    block!(output, {
        writeln!(output, "public IReducerArgs Args {{ get; }}").unwrap();
        writeln!(output).unwrap();
        writeln!(output, "public string ReducerName => Args.ReducerName;").unwrap();
        writeln!(output).unwrap();
        writeln!(
            output,
            r#"[Obsolete("ReducerType is deprecated, please match directly on type of .Args instead.")]"#
        )
        .unwrap();
        writeln!(output, "public ReducerType Reducer => Args.ReducerType;").unwrap();
        writeln!(output).unwrap();
        writeln!(
            output,
            "public ReducerEvent(IReducerArgs args) : base() => Args = args;"
        )
        .unwrap();
        writeln!(
            output,
            "public ReducerEvent(ClientApi.Event dbEvent, IReducerArgs args) : base(dbEvent) => Args = args;"
        )
        .unwrap();
        writeln!(output).unwrap();
        writeln!(
            output,
            "public static ReducerEvent? FromDbEvent(ClientApi.Event dbEvent)"
        )
        .unwrap();
        block!(output, {
            writeln!(output, "var argBytes = dbEvent.FunctionCall.ArgBytes;").unwrap();
            writeln!(output, "IReducerArgs? args = dbEvent.FunctionCall.Reducer switch {{").unwrap();
            {
                indent_scope!(output);
                for (reducer, reducer_name) in std::iter::zip(&reducers, &reducer_names) {
                    let reducer_str_name = &reducer.name;
                    writeln!(
                        output,
                        "\"{reducer_str_name}\" => BSATNHelpers.FromProtoBytes<{reducer_name}ArgsStruct>(argBytes),"
                    )
                    .unwrap();
                }
                writeln!(output, "_ => null").unwrap();
            }
            writeln!(output, "}};").unwrap();
            writeln!(output, "return args is null ? null : new ReducerEvent(dbEvent, args);").unwrap();
        });
        writeln!(output).unwrap();
        // Properties for reducer args
        for reducer_name in &reducer_names {
            writeln!(output, r#"[Obsolete("Accessors that implicitly cast `Args` are deprecated, please match `Args` against the desired type explicitly instead.")]"#).unwrap();
            writeln!(
                output,
                "public {reducer_name}ArgsStruct {reducer_name}Args => ({reducer_name}ArgsStruct)Args;"
            )
            .unwrap();
        }
        writeln!(output).unwrap();
        // Event handlers.
        writeln!(
            output,
            "public override bool InvokeHandler() => Args.InvokeHandler(this);"
        )
        .unwrap();
    });
    writeln!(output).unwrap();

    writeln!(output, "public static class ModuleRegistration").unwrap();
    block!(output, {
        writeln!(output, "[ModuleInitializer]").unwrap();
        writeln!(output, "public static void Register()").unwrap();
        block!(output, {
            for item in items {
                if let GenItem::Table(table) = item {
                    writeln!(
                        output,
                        "SpacetimeDBClient.clientDB.AddTable<{table_name}>();",
                        table_name = table.schema.table_name
                    )
                    .unwrap();
                }
            }
            writeln!(output).unwrap();
            writeln!(
                output,
                "SpacetimeDBClient.SetReducerEventFromDbEvent(ReducerEvent.FromDbEvent);"
            )
            .unwrap();
        });
    });

    vec![vec![("_Globals.cs".to_string(), autogen_csharp_footer(output))]]
}
