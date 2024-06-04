use super::util::fmt_fn;

use std::fmt::{self, Write};
use std::ops::Deref;

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
                            write!(f, "{namespace}.{name}")
                        }
                    } else {
                        write!(f, "{namespace}.{name}")
                    }
                }
                _ => {
                    write!(f, "{namespace}.{name}")
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

struct CsharpAutogen {
    output: CodeIndenter<String>,
}

impl std::ops::Deref for CsharpAutogen {
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
    pub fn new(namespace: &str, extra_usings: &[&str]) -> Self {
        let mut output = CodeIndenter::new(String::new());

        writeln!(
            output,
            "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE"
        );
        writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.");
        writeln!(output, "// <auto-generated />");
        writeln!(output);

        writeln!(output, "#nullable enable");
        writeln!(output);

        writeln!(output, "using System;");
        if namespace != "SpacetimeDB" {
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
    let mut output = CsharpAutogen::new(namespace, &[]);

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
        writeln!(output, "public partial class {sum_namespace}");
        writeln!(output, "{{");
        output.indent(1);

        writeln!(output, "public partial class Types");
        writeln!(output, "{{");
        output.indent(1);
    }

    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "public enum {sum_type_name}");
    indented_block(&mut output, |output| {
        for variant in &*sum_type.variants {
            let variant_name = variant
                .name
                .as_ref()
                .expect("All sum variants should have names!")
                .replace("r#", "");
            writeln!(output, "{variant_name},");
        }
    });

    if sum_namespace.is_some() {
        for _ in 0..2 {
            output.dedent(1);
            writeln!(output, "}}");
        }
    }

    output.into_inner()
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
    let mut output = CsharpAutogen::new(
        namespace,
        &[
            "System.Collections.Generic",
            "System.Linq",
            "System.Runtime.Serialization",
        ],
    );

    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "[DataContract]");
    write!(output, "public partial class {name}");
    if let Some(schema) = &schema {
        write!(
            output,
            " : SpacetimeDB.{parent}<{name}, {namespace}.ReducerEvent>",
            parent = if schema.pk().is_some() {
                "DatabaseTableWithPrimaryKey"
            } else {
                "DatabaseTable"
            }
        );
    }
    writeln!(output);
    indented_block(&mut output, |output| {
        for field in &*product_type.elements {
            let field_name = field
                .name
                .as_ref()
                .expect("autogen'd tuples should have field names")
                .replace("r#", "");

            writeln!(output, "[DataMember(Name = \"{field_name}\")]");
            writeln!(
                output,
                "public {} {}{};",
                ty_fmt(ctx, &field.algebraic_type, namespace),
                field_name.to_case(Case::Pascal),
                default_init(ctx, &field.algebraic_type)
            );
        }
        writeln!(output);

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
                writeln!(
                    output,
                    "private static Dictionary<{type_name}, {name}> {field_name}_Index = new(16);"
                );
                unique_indexes.push(field_name);
            }
            if !unique_indexes.is_empty() {
                writeln!(output);
                // OnInsert method for updating indexes
                writeln!(output, "public override void InternalOnValueInserted()");
                indented_block(output, |output| {
                    for col in schema.columns() {
                        let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                        if !constraints[&ColList::new(col.col_pos)].has_unique() {
                            continue;
                        }
                        writeln!(output, "{field_name}_Index[{field_name}] = this;");
                    }
                });
                writeln!(output);
                // OnDelete method for updating indexes
                writeln!(output, "public override void InternalOnValueDeleted()");
                indented_block(output, |output| {
                    for col in schema.columns() {
                        let field_name = col.col_name.replace("r#", "").to_case(Case::Pascal);
                        if !constraints[&ColList::new(col.col_pos)].has_unique() {
                            continue;
                        }
                        writeln!(output, "{field_name}_Index.Remove({field_name});");
                    }
                });
                writeln!(output);
            }

            // If this is a table, we want to include functions for accessing the table data
            // Insert the funcs for accessing this struct
            autogen_csharp_access_funcs_for_struct(output, name, product_type, name, schema);
            writeln!(output);
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

fn autogen_csharp_access_funcs_for_struct(
    output: &mut CodeIndenter<String>,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
    _table_name: &str,
    schema: &TableSchema,
) {
    let constraints = schema.column_constraints();
    for col in schema.columns() {
        let is_unique = constraints[&ColList::new(col.col_pos)].has_unique();

        let col_i: usize = col.col_pos.into();

        let field = &product_type.elements[col_i];
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let csharp_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);

        let csharp_field_type = match field_type {
            AlgebraicType::Product(product) => {
                if product.is_identity() {
                    "SpacetimeDB.Identity"
                } else if product.is_address() {
                    "SpacetimeDB.Address"
                } else {
                    continue;
                }
            }
            AlgebraicType::Sum(_) | AlgebraicType::Ref(_) => continue,
            AlgebraicType::Builtin(b) => match maybe_primitive(b) {
                MaybePrimitive::Primitive(ty) => ty,
                _ => continue,
            },
        };

        if is_unique {
            writeln!(
                output,
                "public static {struct_name_pascal_case}? FindBy{csharp_field_name_pascal}({csharp_field_type} value)"
            );
            indented_block(output, |output| {
                writeln!(
                    output,
                    "{csharp_field_name_pascal}_Index.TryGetValue(value, out var r);"
                );
                writeln!(output, "return r;");
            });
            writeln!(output);
        }

        writeln!(
            output,
            "public static IEnumerable<{struct_name_pascal_case}> FilterBy{csharp_field_name_pascal}({csharp_field_type} value)"
        );
        indented_block(output, |output| {
            if is_unique {
                // Yield a single item iff `FindBy` returns a non-null record.
                writeln!(output, "if (FindBy{csharp_field_name_pascal}(value) is {{}} found)");
                indented_block(output, |output| {
                    writeln!(output, "yield return found;");
                });
            } else {
                writeln!(output, "return Query(x => x.{csharp_field_name_pascal} == value);");
            }
        });
        writeln!(output);
    }

    if let Some(primary_col_index) = schema.pk() {
        writeln!(
            output,
            "public override object GetPrimaryKeyValue() => {col_name_pascal_case};",
            col_name_pascal_case = primary_col_index.col_name.replace("r#", "").to_case(Case::Pascal)
        );
    }
}

pub fn autogen_csharp_reducer(ctx: &GenCtx, reducer: &ReducerDef, namespace: &str) -> String {
    let func_name = &*reducer.name;
    // let reducer_pascal_name = func_name.to_case(Case::Pascal);
    let func_name_pascal_case = func_name.to_case(Case::Pascal);

    let mut output = CsharpAutogen::new(namespace, &[]);

    //Args struct
    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(
        output,
        "public partial class {func_name_pascal_case}ArgsStruct : IReducerArgs"
    );

    let mut func_params: String = String::new();
    let mut field_inits: String = String::new();

    indented_block(&mut output, |output| {
        writeln!(
            output,
            "ReducerType IReducerArgs.ReducerType => ReducerType.{func_name_pascal_case};"
        );
        writeln!(output, "string IReducerArgsBase.ReducerName => \"{func_name}\";");
        writeln!(output, "bool IReducerArgs.InvokeHandler(ReducerEvent reducerEvent) => Reducer.On{func_name_pascal_case}(reducerEvent, this);");
        if !reducer.args.is_empty() {
            writeln!(output);
        }
        for (arg_i, arg) in reducer.args.iter().enumerate() {
            let name = arg
                .name
                .as_deref()
                .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
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
            );
            write!(func_params, "{arg_type_str} {arg_name}").unwrap();
            write!(field_inits, "{field_name} = {arg_name}").unwrap();
        }
    });

    writeln!(output);

    writeln!(output, "public static partial class Reducer");
    indented_block(&mut output, |output| {
        let delegate_separator = if !reducer.args.is_empty() { ", " } else { "" };
        writeln!(
            output,
            "public delegate void {func_name_pascal_case}Handler(ReducerEvent reducerEvent{delegate_separator}{func_params});"
        );
        writeln!(
            output,
            "public static event {func_name_pascal_case}Handler? On{func_name_pascal_case}Event;"
        );

        writeln!(output);

        writeln!(output, "public static void {func_name_pascal_case}({func_params})");
        indented_block(output, |output| {
            writeln!(
                output,
                "SpacetimeDBClient.instance.InternalCallReducer(new {func_name_pascal_case}ArgsStruct {{ {field_inits} }});"
            );
        });
        writeln!(output);

        writeln!(
            output,
            "public static bool On{func_name_pascal_case}(ReducerEvent reducerEvent, {func_name_pascal_case}ArgsStruct args)"
        );
        indented_block(output, |output| {
            writeln!(output, "if (On{func_name_pascal_case}Event == null) return false;");
            writeln!(output, "On{func_name_pascal_case}Event(");
            // Write out arguments one per line
            {
                indent_scope!(output);
                write!(output, "reducerEvent");
                for (i, arg) in reducer.args.iter().enumerate() {
                    writeln!(output, ",");
                    let arg_name = arg
                        .name
                        .as_deref()
                        .map_or_else(|| format!("Arg{i}"), |name| name.to_case(Case::Pascal));
                    write!(output, "args.{arg_name}");
                }
                writeln!(output);
            }
            writeln!(output, ");");
            writeln!(output, "return true;");
        });
    });
    writeln!(output);

    output.into_inner()
}

pub fn autogen_csharp_globals(items: &[GenItem], namespace: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();

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
        .map(|reducer| reducer.name.deref().to_case(Case::Pascal))
        .collect();

    let mut output = CsharpAutogen::new(namespace, &[]);

    writeln!(output, "public enum ReducerType");
    indented_block(&mut output, |output| {
        writeln!(output, "None,");
        for reducer_name in &reducer_names {
            writeln!(output, "{reducer_name},");
        }
    });
    writeln!(output);

    writeln!(output, "public interface IReducerArgs : IReducerArgsBase");
    indented_block(&mut output, |output| {
        writeln!(output, "ReducerType ReducerType {{ get; }}");
        writeln!(output, "bool InvokeHandler(ReducerEvent reducerEvent);");
    });
    writeln!(output);

    writeln!(output, "public partial class ReducerEvent : ReducerEventBase");
    indented_block(&mut output, |output| {
        writeln!(output, "public IReducerArgs Args {{ get; }}");
        writeln!(output);
        writeln!(output, "public string ReducerName => Args.ReducerName;");
        writeln!(output);
        writeln!(
            output,
            r#"[Obsolete("ReducerType is deprecated, please match directly on type of .Args instead.")]"#
        );
        writeln!(output, "public ReducerType Reducer => Args.ReducerType;");
        writeln!(output);
        writeln!(
            output,
            "public ReducerEvent(IReducerArgs args) : base() => Args = args;"
        );
        writeln!(
            output,
            "public ReducerEvent(ClientApi.Event dbEvent, IReducerArgs args) : base(dbEvent) => Args = args;"
        );
        writeln!(output);
        // Properties for reducer args
        for reducer_name in &reducer_names {
            writeln!(
                output,
                r#"[Obsolete("Accessors that implicitly cast `Args` are deprecated, please match `Args` against the desired type explicitly instead.")]"#
            );
            writeln!(
                output,
                "public {reducer_name}ArgsStruct {reducer_name}Args => ({reducer_name}ArgsStruct)Args;"
            );
        }
        writeln!(output);
        // Event handlers.
        writeln!(
            output,
            "public override bool InvokeHandler() => Args.InvokeHandler(this);"
        );
    });
    writeln!(output);

    writeln!(
        output,
        "public class SpacetimeDBClient : SpacetimeDBClientBase<ReducerEvent>"
    );
    indented_block(&mut output, |output| {
        writeln!(output, "protected SpacetimeDBClient()");
        indented_block(output, |output| {
            for item in items {
                if let GenItem::Table(table) = item {
                    writeln!(
                        output,
                        "clientDB.AddTable<{table_name}>();",
                        table_name = table.schema.table_name
                    );
                }
            }
        });
        writeln!(output);

        writeln!(output, "public static readonly SpacetimeDBClient instance = new();");
        writeln!(output);

        writeln!(
            output,
            "protected override ReducerEvent? ReducerEventFromDbEvent(ClientApi.Event dbEvent)"
        );
        indented_block(output, |output| {
            writeln!(output, "var argBytes = dbEvent.FunctionCall.ArgBytes;");
            writeln!(output, "IReducerArgs? args = dbEvent.FunctionCall.Reducer switch {{");
            {
                indent_scope!(output);
                for (reducer, reducer_name) in std::iter::zip(&reducers, &reducer_names) {
                    let reducer_str_name = &reducer.name;
                    writeln!(
                        output,
                        "\"{reducer_str_name}\" => BSATNHelpers.FromProtoBytes<{reducer_name}ArgsStruct>(argBytes),"
                    );
                }
                writeln!(output, "_ => null");
            }
            writeln!(output, "}};");
            writeln!(output, "return args is null ? null : new ReducerEvent(dbEvent, args);");
        });
    });

    results.push(("_Globals/SpacetimeDBClient.cs".to_owned(), output.into_inner()));

    // Note: Unity requires script classes to have the same name as the file they are in.
    // That's why we're generating a separate file for Unity-specific code.

    let mut output = CsharpAutogen::new(namespace, &["UnityEngine"]);

    writeln!(output, "// This class is only used in Unity projects.");
    writeln!(
        output,
        "// Attach this to a gameobject in your scene to use SpacetimeDB."
    );
    writeln!(output, "#if UNITY_5_3_OR_NEWER");
    writeln!(output, "public class UnityNetworkManager : MonoBehaviour");
    indented_block(&mut output, |output| {
        writeln!(
            output,
            "private void OnDestroy() => SpacetimeDBClient.instance.Close();"
        );
        writeln!(output, "private void Update() => SpacetimeDBClient.instance.Update();");
    });
    writeln!(output, "#endif");

    results.push(("_Globals/UnityNetworkManager.cs".to_owned(), output.into_inner()));

    results
}
