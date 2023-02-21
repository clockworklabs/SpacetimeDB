use std::fmt::{self, Write};

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, BuiltinType, MapType};
use spacetimedb_lib::{ReducerDef, TableDef, TupleDef, TypeDef};

use super::code_indenter::CodeIndenter;
use super::{GenCtx, INDENT};

const NAMESPACE: &str = "SpacetimeDB";

enum MaybePrimitive<'a> {
    Primitive(&'static str),
    Array { ty: &'a AlgebraicType },
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
        BuiltinType::Array { ty } => return MaybePrimitive::Array { ty },
        BuiltinType::Map(m) => return MaybePrimitive::Map(m),
    })
}

fn ty_fmt<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        TypeDef::Sum(_) => unimplemented!(),
        TypeDef::Product(_) => unimplemented!(),
        TypeDef::Builtin(b) => match maybe_primitive(b) {
            MaybePrimitive::Primitive(p) => f.write_str(p),
            MaybePrimitive::Array { ty } if *ty == AlgebraicType::U8 => f.write_str("byte[]"),
            MaybePrimitive::Array { ty } => return write!(f, "System.Collections.Generic.List<{}>", ty_fmt(ctx, ty)),
            MaybePrimitive::Map(_) => todo!(),
        },
        TypeDef::Ref(r) => f.write_str(csharp_typename(ctx, *r)),
    })
}
fn convert_builtintype<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    b: &'a BuiltinType,
    value: impl fmt::Display + 'a,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match maybe_primitive(b) {
        MaybePrimitive::Primitive(csharp_type) => {
            write!(f, "({csharp_type}){value}.GetValue(TypeDef.Def.{b:?})")
    }
        MaybePrimitive::Array { ty } if *ty == AlgebraicType::U8 => {
            write!(f, "(byte[]){value}.GetValue(TypeDef.Def.Bytes)")
        }
        MaybePrimitive::Array { ty } => {
            let csharp_type = ty_fmt(ctx, ty);
            writeln!(
                f,
                "((System.Func<System.Collections.Generic.List<{csharp_type}>>)(() => {{"
            )?;
            writeln!(
                f,
                "\tvar vec{vecnest} = new System.Collections.Generic.List<{}>();",
                csharp_type
            )?;
            writeln!(
                f,
                "\tvar vec{vecnest}_source = {value}.GetValue(TypeDef.Def.Vec) as System.Collections.Generic.List<SpacetimeDB.TypeValue>;",
            )?;
            writeln!(f, "\tforeach(var entry in vec{vecnest}_source!)")?;
            writeln!(f, "\t{{")?;
            writeln!(
                f,
                "\t\tvec{vecnest}.Add({});",
                convert_type(ctx, vecnest + 1, ty, "entry")
            )?;
            writeln!(f, "\treturn vec{vecnest};")?;
            write!(f, "}}))()")
        }
        MaybePrimitive::Map(_) => todo!(),
    })
}

fn convert_type<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    ty: &'a AlgebraicType,
    value: impl fmt::Display + 'a,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        TypeDef::Product(_) => unimplemented!(),
        TypeDef::Sum(_) => unimplemented!(),
        TypeDef::Builtin(b) => fmt::Display::fmt(&convert_builtintype(ctx, vecnest, b, &value), f),
        TypeDef::Ref(r) => {
            let name = csharp_typename(ctx, *r);
            write!(f, "({name}){value}",)
        }
    })
}

// can maybe do something fancy with this in the future
fn csharp_typename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> &str {
    ctx.names[typeref.idx()].as_deref().expect("tuples should have names")
}

fn fmt_fn(f: impl Fn(&mut fmt::Formatter) -> fmt::Result) -> impl fmt::Display {
    struct FDisplay<F>(F);
    impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Display for FDisplay<F> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (self.0)(f)
        }
    }
    FDisplay(f)
}

macro_rules! indent_scope {
    ($x:ident) => {
        let mut $x = $x.indented(1);
    };
}

fn convert_typedef<'a>(ctx: &'a GenCtx, ty: &'a TypeDef) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        TypeDef::Product(_) => unimplemented!(),
        TypeDef::Sum(_) => unimplemented!(),
        TypeDef::Builtin(b) => match maybe_primitive(b) {
            MaybePrimitive::Primitive(_) => {
                write!(f, "SpacetimeDB.TypeDef.BuiltInType(SpacetimeDB.TypeDef.Def.{:?})", b)
        }
            MaybePrimitive::Array { ty } => write!(f, "SpacetimeDB.TypeDef.GetVec({})", convert_typedef(ctx, ty)),
            MaybePrimitive::Map(_) => todo!(),
        },
        TypeDef::Ref(r) => write!(f, "{}.GetTypeDef()", csharp_typename(ctx, *r)),
    })
}

fn convert_tupledef<'a>(ctx: &'a GenCtx, tuple: &'a TupleDef) -> impl fmt::Display + 'a {
    fmt_fn(move |f| {
        writeln!(f, "TypeDef.Tuple(new ElementDef[]")?;
        writeln!(f, "{{")?;
        for (i, elem) in tuple.elements.iter().enumerate() {
            writeln!(
                f,
                "{INDENT}new SpacetimeDB.ElementDef({i}, {}),",
                convert_typedef(ctx, &elem.algebraic_type)
            )?;
        }
        write!(f, "}})")
    })
}

pub fn autogen_csharp_tuple(ctx: &GenCtx, name: &str, tuple: &TupleDef) -> String {
    autogen_csharp_tuple_table_common(ctx, name, tuple, None)
}
pub fn autogen_csharp_table(ctx: &GenCtx, name: &str, table: &TableDef) -> String {
    let tuple = ctx.typespace[table.data].as_product().unwrap();
    autogen_csharp_tuple_table_common(ctx, name, tuple, Some(&table.unique_columns))
}
fn autogen_csharp_tuple_table_common(
    ctx: &GenCtx,
    name: &str,
    tuple: &TupleDef,
    unique_columns: Option<&[u8]>,
) -> String {
    let mut output = CodeIndenter::new(String::new());

    let struct_name_pascal_case = name.replace("r#", "").to_case(Case::Pascal);

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "using System;").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "namespace {NAMESPACE}").unwrap();
    writeln!(output, "{{").unwrap();
    {
        indent_scope!(output);
        writeln!(
            output,
            "public partial class {struct_name_pascal_case} : IDatabaseTable"
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);

            for field in &tuple.elements {
                let field_name = field
                    .name
                    .as_ref()
                    .expect("autogen'd tuples should have field names")
                    .replace("r#", "");
                writeln!(output, "[Newtonsoft.Json.JsonProperty(\"{field_name}\")]").unwrap();
                writeln!(
                    output,
                    "public {} {};",
                    ty_fmt(ctx, &field.algebraic_type),
                    field_name.to_case(Case::Pascal)
                )
                .unwrap();
            }

            writeln!(output).unwrap();

            writeln!(output, "public static TypeDef GetTypeDef()").unwrap();
            writeln!(output, "{{").unwrap();
            {
                indent_scope!(output);
                writeln!(output, "return {};", convert_tupledef(ctx, tuple)).unwrap();
            }
            writeln!(output, "}}").unwrap();
            writeln!(output).unwrap();

            write!(
                output,
                "{}",
                autogen_csharp_tuple_to_struct(ctx, &struct_name_pascal_case, tuple)
            )
            .unwrap();

            // If this is a table, we want to include functions for accessing the table data
            if let Some(unique_columns) = unique_columns {
                // Insert the funcs for accessing this struct
                autogen_csharp_access_funcs_for_struct(
                    &mut output,
                    &struct_name_pascal_case,
                    tuple,
                    name,
                    unique_columns,
                );

                writeln!(
                    output,
                    "public static event Action<{struct_name_pascal_case}> OnInsert;"
                )
                .unwrap();
                writeln!(
                    output,
                    "public static event Action<{struct_name_pascal_case}, {struct_name_pascal_case}> OnUpdate;"
                )
                .unwrap();
                writeln!(
                    output,
                    "public static event Action<{struct_name_pascal_case}> OnDelete;"
                )
                .unwrap();

                writeln!(
                    output,
                    "public static event Action<NetworkManager.TableOp, {struct_name_pascal_case}, {struct_name_pascal_case}> OnRowUpdate;"
                )
                .unwrap();

                writeln!(output).unwrap();

                writeln!(output, "public static void OnInsertEvent(object newValue)").unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(output, "OnInsert?.Invoke(({struct_name_pascal_case})newValue);").unwrap();
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();

                writeln!(
                    output,
                    "public static void OnUpdateEvent(object oldValue, object newValue)"
                )
                .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(
                        output,
                        "OnUpdate?.Invoke(({struct_name_pascal_case})oldValue,({struct_name_pascal_case})newValue);"
                    )
                    .unwrap();
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();

                writeln!(output, "public static void OnDeleteEvent(object oldValue)").unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(output, "OnDelete?.Invoke(({struct_name_pascal_case})oldValue);").unwrap();
                }
                writeln!(output, "}}").unwrap();
                writeln!(output).unwrap();

                writeln!(
                    output,
                    "public static void OnRowUpdateEvent(NetworkManager.TableOp op, object oldValue, object newValue)"
                )
                .unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(
                        output,
                        "OnRowUpdate?.Invoke(op, ({struct_name_pascal_case})oldValue,({struct_name_pascal_case})newValue);"
                    )
                    .unwrap();
                }
                writeln!(output, "}}").unwrap();
            }
        }
        writeln!(output, "}}").unwrap();
    }
    writeln!(output, "}}").unwrap();

    output.into_inner()
}

fn autogen_csharp_tuple_to_struct(ctx: &GenCtx, struct_name_pascal_case: &str, tuple: &TupleDef) -> String {
    let mut output_contents_header: String = String::new();
    let mut output_contents_return: String = String::new();

    writeln!(
        output_contents_header,
        "public static explicit operator {struct_name_pascal_case}(TypeValue value)",
    )
    .unwrap();
    writeln!(output_contents_header, "{{").unwrap();
    writeln!(
        output_contents_header,
        "\tvar tupleValue = value.GetValue(TypeDef.Def.Tuple) as TypeValue[];"
    )
    .unwrap();
    writeln!(output_contents_header, "\tif (tupleValue == null)").unwrap();
    writeln!(output_contents_header, "\t{{").unwrap();
    writeln!(
        output_contents_header,
        "\t\tthrow new System.InvalidOperationException($\"Invalid value (must be Tuple): {{value.TypeDef.Type}}\");"
    )
    .unwrap();
    writeln!(output_contents_header, "\t}}").unwrap();
    writeln!(output_contents_header).unwrap();
    // vec conversion go here
    writeln!(output_contents_return, "\treturn new {}", struct_name_pascal_case).unwrap();
    writeln!(output_contents_return, "\t{{").unwrap();

    for (idx, field) in tuple.elements.iter().enumerate() {
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let csharp_field_name = field_name.to_string().replace("r#", "").to_case(Case::Pascal);

                writeln!(
                    output_contents_return,
            "\t\t{csharp_field_name} = {},",
            convert_type(ctx, 0, field_type, format_args!("tupleValue[{idx}]"))
                )
                .unwrap();
            }

    // End Struct
    writeln!(output_contents_return, "\t}};").unwrap();
    // End Func
    writeln!(output_contents_return, "}}").unwrap();

    output_contents_header + &output_contents_return
}

fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{").unwrap();
    let res = f(&mut output.indented(1));
    writeln!(output, "}}").unwrap();
    res
}

fn autogen_csharp_access_funcs_for_struct(
    output: &mut CodeIndenter<String>,
    struct_name_pascal_case: &str,
    tuple: &TupleDef,
    table_name: &str,
    unique_columns: &[u8],
) {
    let it = Iterator::chain(
        unique_columns.iter().copied().zip(std::iter::repeat(true)),
        (0..tuple.elements.len())
            .map(|i| i as u8)
            .filter(|i| unique_columns.binary_search(i).is_err())
            .zip(std::iter::repeat(false)),
    );
    writeln!(
        output,
        "public static System.Collections.Generic.IEnumerable<{struct_name_pascal_case}> Iter()"
    )
    .unwrap();
    indented_block(output, |output| {
        writeln!(
            output,
            "foreach(var entry in NetworkManager.clientDB.GetEntries(\"{table_name}\"))",
        )
        .unwrap();
        indented_block(output, |output| {
            writeln!(
                output,
                "var tupleArr = entry.GetValue(TypeDef.Def.Tuple) as TypeValue[];"
            )
            .unwrap();
            // TODO: best way to handle this?
            writeln!(output, "if (tupleArr == null) continue;").unwrap();
            writeln!(output, "yield return ({struct_name_pascal_case})entry;").unwrap();
        });
    });

    writeln!(output, "public static int Count()").unwrap();
    indented_block(output, |output| {
        writeln!(output, "return NetworkManager.clientDB.Count(\"{table_name}\");",).unwrap();
    });

    for (col_i, is_unique) in it {
        let field = &tuple.elements[col_i as usize];
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let csharp_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);

        let (field_type, csharp_field_type) = match field_type {
            TypeDef::Product(_) | TypeDef::Ref(_) => {
                // TODO: We don't allow filtering on tuples right now, its possible we may consider it for the future.
                continue;
            }
            TypeDef::Sum(_) => unimplemented!(),
            TypeDef::Builtin(b) => match maybe_primitive(b) {
                MaybePrimitive::Primitive(ty) => (b, ty),
                MaybePrimitive::Array { .. } | MaybePrimitive::Map(_) => {
                // TODO: We don't allow filtering based on a vec type, but we might want other functionality here in the future.
                // TODO: It would be nice to be able to say, give me all entries where this vec contains this value, which we can do.
                continue;
            }
            },
        };

        let filter_return_type = fmt_fn(|f| {
            if is_unique {
                f.write_str(struct_name_pascal_case)
            } else {
                write!(f, "System.Collections.Generic.IEnumerable<{}>", struct_name_pascal_case)
            }
        });

        writeln!(
            output,
            "public static {filter_return_type} FilterBy{}({} value)",
            csharp_field_name_pascal, csharp_field_type
        )
        .unwrap();

        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);
            writeln!(
                output,
                "foreach(var entry in NetworkManager.clientDB.GetEntries(\"{}\"))",
                table_name
            )
            .unwrap();
            writeln!(output, "{{").unwrap();
            {
                indent_scope!(output);
                writeln!(
                    output,
                    "var tupleArr = entry.GetValue(TypeDef.Def.Tuple) as TypeValue[];"
                )
                .unwrap();
                writeln!(output, "if (tupleArr == null) continue;").unwrap();

                writeln!(
                    output,
                    "var compareValue = ({})tupleArr[{}].GetValue(TypeDef.Def.{:?});",
                    csharp_field_type, col_i, field_type
                )
                .unwrap();
                writeln!(output, "if (compareValue == value)").unwrap();

                {
                    indent_scope!(output);
                    if is_unique {
                        writeln!(output, "return ({struct_name_pascal_case})entry;").unwrap();
                    } else {
                        writeln!(output, "yield return ({struct_name_pascal_case})entry;").unwrap();
                    }
                }
            }
            // End foreach
            writeln!(output, "}}").unwrap();

            if is_unique {
                writeln!(output, "return null;").unwrap();
            }
        }
        // End Func
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();
    }
}

// fn convert_enumdef(tuple: &EnumDef) -> impl fmt::Display + '_ {
//     fmt_fn(move |f| {
//         writeln!(f, "TypeDef.Tuple(new ElementDef[]")?;
//         writeln!(f, "{{")?;
//         for (i, elem) in tuple.elements.iter().enumerate() {
//             let comma = if i == tuple.elements.len() - 1 { "" } else { "," };
//             writeln!(f, "{INDENT}{}{}", convert_elementdef(elem), comma)?;
//         }
//         writeln!(f, "}}")
//     })
// }

pub fn autogen_csharp_reducer(ctx: &GenCtx, reducer: &ReducerDef) -> String {
    let func_name = reducer.name.as_ref().expect("reducer should have name");
    // let reducer_pascal_name = func_name.to_case(Case::Pascal);
    let use_namespace = true;
    let func_name_pascal_case = func_name.as_ref().to_case(Case::Pascal);

    let mut output = CodeIndenter::new(String::new());

    let mut func_arguments: String = String::new();
    let mut arg_types: String = String::new();
    let mut arg_names: String = String::new();
    let mut arg_event_parse: String = String::new();
    let arg_count = reducer.args.len();

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE"
    )
    .unwrap();
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "using System;").unwrap();
    writeln!(output, "using ClientApi;").unwrap();
    writeln!(output, "using Newtonsoft.Json.Linq;").unwrap();

    writeln!(output).unwrap();

    if use_namespace {
        writeln!(output, "namespace {NAMESPACE}").unwrap();
        writeln!(output, "{{").unwrap();
        output.indent(1);
    }

    writeln!(output, "public static partial class Reducer").unwrap();
    writeln!(output, "{{").unwrap();

    {
        indent_scope!(output);

        for (arg_i, arg) in reducer.args.iter().enumerate() {
            let name = arg.name.as_deref().expect("reducer args should have names");
            let arg_name = name.to_case(Case::Camel);
            let arg_type_str = ty_fmt(ctx, &arg.algebraic_type);

            if arg_i > 0 {
                func_arguments.push_str(", ");
                arg_names.push_str(", ");
            }
            arg_event_parse.push_str(", ");
            arg_types.push_str(", ");

            write!(func_arguments, "{} {}", arg_type_str, arg_name).unwrap();

            arg_names.push_str(&arg_name);
            write!(arg_event_parse, "args[{}].ToObject<{}>()", arg_i, arg_type_str).unwrap();

            write!(arg_types, "{}", arg_type_str).unwrap();
        }

        writeln!(
            output,
            "public static event Action<ClientApi.Event.Types.Status, Hash{arg_types}> On{func_name_pascal_case}Event;"
        )
        .unwrap();

        writeln!(output).unwrap();

        writeln!(output, "public static void {func_name_pascal_case}({func_arguments})").unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);

            //           NetworkManager.instance.InternalCallReducer(new NetworkManager.Message
            // 			{
            // 				fn = "create_new_player",
            // 				args = new object[] { playerId, position },
            // 			});

            // Tell the network manager to send this message
            // UPGRADE FOR LATER
            // write!(output, "{}\t\tNetworkManager.instance.InternalCallReducer(new Websocket.FunctionCall\n", namespace_tab).unwrap();
            // write!(output, "{}\t\t{{\n", namespace_tab).unwrap();
            // write!(output, "{}\t\t\tReducer = \"{}\",\n", namespace_tab, func_name).unwrap();
            // write!(output, "{}\t\t\tArgBytes = Google.Protobuf.ByteString.CopyFrom(Newtonsoft.Json.JsonConvert.SerializeObject(new object[] {{ {} }}), System.Text.Encoding.UTF8),\n", namespace_tab, arg_names).unwrap();
            // write!(output, "{}\t\t}});\n", namespace_tab).unwrap();

            // TEMPORARY OLD FUNCTIONALITY
            writeln!(
                output,
                "NetworkManager.instance.InternalCallReducer(new NetworkManager.Message",
            )
            .unwrap();
            {
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(output, "fn = \"{func_name}\",").unwrap();
                    writeln!(output, "args = new object[] {{ {arg_names} }},").unwrap();
                }
                writeln!(output, "}});").unwrap();
            }
        }
        // Closing brace for reducer
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();

        writeln!(output, "[ReducerEvent(FunctionName = \"{func_name}\")]").unwrap();
        writeln!(
            output,
            "public static void On{func_name_pascal_case}(ClientApi.Event dbEvent)"
        )
        .unwrap();
        writeln!(output, "{{").unwrap();
        {
            indent_scope!(output);

            writeln!(output, "if(On{func_name_pascal_case}Event != null)").unwrap();
            writeln!(output, "{{").unwrap();
            {
                indent_scope!(output);
                writeln!(output, "var jsonString = dbEvent.FunctionCall.ArgBytes.ToStringUtf8();").unwrap();
                writeln!(
                    output,
                    "var args = Newtonsoft.Json.JsonConvert.DeserializeObject<JArray>(jsonString);"
                )
                .unwrap();

                writeln!(output, "if(args.Count >= {arg_count})").unwrap();
                writeln!(output, "{{").unwrap();
                {
                    indent_scope!(output);
                    writeln!(
                        output,
                        "On{func_name_pascal_case}Event(dbEvent.Status, Hash.From(dbEvent.CallerIdentity.ToByteArray()){arg_event_parse});"
                    )
                    .unwrap();
                }
                // Closing brace for if count is valid
                writeln!(output, "}}").unwrap();
            }
            // Closing brace for if event is registered
            writeln!(output, "}}").unwrap();
        }

        // Closing brace for Event parsing function
        writeln!(output, "}}").unwrap();
    }
    // Closing brace for class
    writeln!(output, "}}").unwrap();

    if use_namespace {
        output.dedent(1);
        writeln!(output, "}}").unwrap();
    }

    output.into_inner()
}
