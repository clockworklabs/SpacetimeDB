use super::util::fmt_fn;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::db::def::TableSchema;
use spacetimedb_lib::{
    sats::{AlgebraicTypeRef, ArrayType, BuiltinType, MapType},
    AlgebraicType, ProductType, ProductTypeElement, ReducerDef, SumType, TableDesc,
};
use spacetimedb_primitives::ColList;
use std::fmt::{self, Write};

use super::{code_indenter::CodeIndenter, csharp::is_enum, GenCtx, GenItem};

enum MaybePrimitive<'a> {
    Primitive(&'static str),
    Array(&'a ArrayType),
    Map(&'a MapType),
}

fn maybe_primitive(b: &BuiltinType) -> MaybePrimitive {
    MaybePrimitive::Primitive(match b {
        BuiltinType::Bool => "bool",
        BuiltinType::I8 => "int",
        BuiltinType::U8 => "int",
        BuiltinType::I16 => "int",
        BuiltinType::U16 => "int",
        BuiltinType::I32 => "int",
        BuiltinType::U32 => "int",
        BuiltinType::I64 => "int",
        BuiltinType::U64 => "int",
        BuiltinType::I128 => "int",
        BuiltinType::U128 => "int",
        BuiltinType::String => "str",
        BuiltinType::F32 => "float",
        BuiltinType::F64 => "float",
        BuiltinType::Array(ty) => return MaybePrimitive::Array(ty),
        BuiltinType::Map(m) => return MaybePrimitive::Map(m),
    })
}

fn convert_builtintype<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    b: &'a BuiltinType,
    value: impl fmt::Display + 'a,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match maybe_primitive(b) {
        MaybePrimitive::Primitive(p) => {
            write!(f, "{p}({value})")
        }
        MaybePrimitive::Array(ArrayType { elem_ty }) if **elem_ty == AlgebraicType::U8 => {
            write!(f, "bytes.fromhex({value})")
        }
        MaybePrimitive::Array(ArrayType { elem_ty }) => {
            let convert_type = convert_type(ctx, vecnest + 1, elem_ty, "item", ref_prefix);
            write!(f, "[{convert_type} for item in {value}]")
        }
        MaybePrimitive::Map(_) => unimplemented!(),
    })
}

fn convert_type<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    ty: &'a AlgebraicType,
    value: impl fmt::Display + 'a,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Product(product) => {
            if product.is_identity() {
                write!(f, "Identity.from_string({value}[0])")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Sum(sum_type) => match sum_type.as_option() {
            Some(inner_ty) => write!(
                f,
                "{} if '0' in {value} else None",
                convert_type(ctx, vecnest, inner_ty, format!("{value}['0']"), ref_prefix),
            ),
            None => unimplemented!(),
        },
        AlgebraicType::Builtin(b) => fmt::Display::fmt(&convert_builtintype(ctx, vecnest, b, &value, ref_prefix), f),
        AlgebraicType::Ref(r) => {
            let name = python_typename(ctx, *r);
            let algebraic_type = &ctx.typespace.types[r.idx()];
            match algebraic_type {
                // for enums in json this comes over as a dictionary where the key is actually the enum index
                AlgebraicType::Sum(sum_type) if is_enum(sum_type) => write!(f, "{name}(int(next(iter({value})))+1)"),
                _ => {
                    write!(f, "{name}({value})")
                }
            }
        }
    })
}

// can maybe do something fancy with this in the future
fn python_typename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> &str {
    ctx.names[typeref.idx()].as_deref().expect("tuples should have names")
}

fn ty_fmt<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, ref_prefix: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Sum(_sum_type) => {
            unimplemented!()
        }
        AlgebraicType::Product(prod) => {
            // The only type that is allowed here is the identity type. All other types should fail.
            if prod.is_identity() {
                write!(f, "Identity")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Builtin(b) => match maybe_primitive(b) {
            MaybePrimitive::Primitive(p) => f.write_str(p),
            MaybePrimitive::Array(ArrayType { elem_ty }) if **elem_ty == AlgebraicType::U8 => f.write_str("bytes"),
            MaybePrimitive::Array(ArrayType { elem_ty }) => {
                write!(f, "List[{}]", ty_fmt(ctx, elem_ty, ref_prefix))
            }
            MaybePrimitive::Map(ty) => {
                write!(
                    f,
                    "Dict[{}, {}]",
                    ty_fmt(ctx, &ty.ty, ref_prefix),
                    ty_fmt(ctx, &ty.key_ty, ref_prefix)
                )
            }
        },
        AlgebraicType::Ref(r) => write!(f, "{}{}", ref_prefix, python_typename(ctx, *r)),
    })
}

macro_rules! indent_scope {
    ($x:ident) => {
        let mut $x = $x.indented(1);
    };
}

fn python_filename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> String {
    ctx.names[typeref.idx()]
        .as_deref()
        .expect("tuples should have names")
        .to_case(Case::Snake)
}

pub fn autogen_python_table(ctx: &GenCtx, table: &TableDesc) -> String {
    let tuple = ctx.typespace[table.data].as_product().unwrap();
    autogen_python_product_table_common(
        ctx,
        &table.schema.table_name,
        tuple,
        Some(table.schema.clone().into_schema(0.into())),
    )
}

fn generate_imports(ctx: &GenCtx, elements: &Vec<ProductTypeElement>, imports: &mut Vec<String>) {
    for field in elements {
        _generate_imports(ctx, &field.algebraic_type, imports);
    }
}

fn _generate_imports(ctx: &GenCtx, ty: &AlgebraicType, imports: &mut Vec<String>) {
    match ty {
        AlgebraicType::Builtin(b) => match b {
            BuiltinType::Array(ArrayType { elem_ty }) => _generate_imports(ctx, elem_ty, imports),
            BuiltinType::Map(map_type) => {
                _generate_imports(ctx, &map_type.key_ty, imports);
                _generate_imports(ctx, &map_type.ty, imports);
            }
            _ => {}
        },
        AlgebraicType::Sum(sum_type) => {
            if let Some(inner_ty) = sum_type.as_option() {
                _generate_imports(ctx, inner_ty, imports)
            }
        }
        AlgebraicType::Ref(r) => {
            let class_name = python_typename(ctx, *r).to_string();
            let filename = python_filename(ctx, *r);

            let import = format!("from .{filename} import {class_name}");
            imports.push(import);
        }
        _ => {}
    }
}

fn autogen_python_product_table_common(
    ctx: &GenCtx,
    name: &str,
    product_type: &ProductType,
    schema: Option<TableSchema>,
) -> String {
    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "# THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    )
    .unwrap();
    writeln!(output, "# WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    if schema.is_some() {
        writeln!(output, "from __future__ import annotations").unwrap();
        writeln!(output, "from typing import List, Iterator, Callable").unwrap();
        writeln!(output).unwrap();
        writeln!(
            output,
            "from spacetimedb_sdk.spacetimedb_client import SpacetimeDBClient, Identity, Address"
        )
        .unwrap();
        writeln!(output, "from spacetimedb_sdk.spacetimedb_client import ReducerEvent").unwrap();
    } else {
        writeln!(output, "from typing import List").unwrap();
    }

    let mut imports = Vec::new();
    generate_imports(ctx, &product_type.elements, &mut imports);

    for import in imports {
        writeln!(output, "{import}").unwrap();
    }
    writeln!(output).unwrap();

    writeln!(output, "class {name}:").unwrap();
    {
        indent_scope!(output);

        // if this is a table, mark it as such
        let is_table_str = match schema {
            Some(_) => "True",
            None => "False",
        };
        writeln!(output, "is_table_class = {is_table_str}").unwrap();
        writeln!(output).unwrap();

        if let Some(schema) = schema {
            // if this table has a primary key add it to the codegen
            if let Some(primary_key) = schema.pk().map(|idx| {
                let field_name = product_type.elements[usize::from(idx.col_pos)]
                    .name
                    .as_ref()
                    .expect("autogen'd tuples should have field names")
                    .replace("r#", "")
                    .to_case(Case::Snake);
                format!("\"{}\"", field_name)
            }) {
                writeln!(output, "primary_key = {}", primary_key).unwrap();
                writeln!(output).unwrap();
            }

            writeln!(output, "@classmethod").unwrap();
            writeln!(
                output,
                "def register_row_update(cls, callback: Callable[[str,{name},{name},ReducerEvent], None]):"
            )
            .unwrap();
            {
                indent_scope!(output);
                writeln!(
                    output,
                    "SpacetimeDBClient.instance._register_row_update(\"{name}\",callback)"
                )
                .unwrap()
            }
            writeln!(output).unwrap();

            writeln!(output, "@classmethod").unwrap();
            writeln!(output, "def iter(cls) -> Iterator[{name}]:").unwrap();
            {
                indent_scope!(output);
                writeln!(
                    output,
                    "return SpacetimeDBClient.instance._get_table_cache(\"{name}\").values()"
                )
                .unwrap();
            }
            writeln!(output).unwrap();

            let constraints = schema.column_constraints();
            for (idx, field) in product_type.elements.iter().enumerate() {
                let attr = constraints[&ColList::new(idx.into())];

                let field_type = &field.algebraic_type;

                match field_type {
                    AlgebraicType::Product(product) => {
                        if !product.is_special() {
                            continue;
                        }
                    }
                    AlgebraicType::Sum(sum) => {
                        if sum.as_option().is_none() {
                            continue;
                        }
                    }
                    AlgebraicType::Ref(_) => {
                        // TODO: We don't allow filtering on enums or tuples right now, its possible we may consider it for the future.
                        continue;
                    }
                    AlgebraicType::Builtin(b) => match maybe_primitive(b) {
                        MaybePrimitive::Array(ArrayType { elem_ty }) => {
                            if elem_ty.as_builtin().is_none() {
                                // TODO: We don't allow filtering based on an array type, but we might want other functionality here in the future.
                                continue;
                            }
                        }
                        MaybePrimitive::Map(_) => {
                            // TODO: It would be nice to be able to say, give me all entries where this vec contains this value, which we can do.
                            continue;
                        }
                        _ => (),
                    },
                };

                let field_name = field
                    .name
                    .as_ref()
                    .expect("autogen'd tuples should have field names")
                    .replace("r#", "")
                    .to_case(Case::Snake);

                writeln!(output, "@classmethod").unwrap();
                if attr.has_unique() {
                    writeln!(output, "def filter_by_{field_name}(cls, {field_name}) -> {name}:").unwrap();
                } else {
                    writeln!(output, "def filter_by_{field_name}(cls, {field_name}) -> List[{name}]:").unwrap();
                }
                {
                    indent_scope!(output);
                    if attr.has_unique() {
                        writeln!(output, "return next(iter([column_value for column_value in SpacetimeDBClient.instance._get_table_cache(\"{name}\").values() if column_value.{field_name} == {field_name}]), None)").unwrap();
                    } else {
                        writeln!(output, "return [column_value for column_value in SpacetimeDBClient.instance._get_table_cache(\"{name}\").values() if column_value.{field_name} == {field_name}]").unwrap();
                    }
                }
                writeln!(output).unwrap();
            }
        }

        writeln!(output, "def __init__(self, data: List[object]):").unwrap();
        {
            indent_scope!(output);

            writeln!(output, "self.data = {{}}").unwrap();
            for (idx, field) in product_type.elements.iter().enumerate() {
                let field_name = field
                    .name
                    .as_ref()
                    .expect("autogen'd tuples should have field names")
                    .replace("r#", "")
                    .to_case(Case::Snake);

                let field_type = &field.algebraic_type;
                let python_field_name = field_name.to_string().replace("r#", "");
                writeln!(
                    output,
                    "self.data[\"{python_field_name}\"] = {}",
                    convert_type(ctx, 0, field_type, format_args!("data[{idx}]"), "")
                )
                .unwrap()
            }
        }
        writeln!(output).unwrap();

        writeln!(output, "def encode(self) -> List[object]:").unwrap();
        {
            indent_scope!(output);

            let mut reducer_args = Vec::new();
            for field in product_type.elements.iter() {
                let field_name = field
                    .name
                    .as_deref()
                    .unwrap_or_else(|| panic!("autogen'd tuples should have field names"));

                let python_field_name = field_name.to_string().replace("r#", "").to_case(Case::Snake);
                match &field.algebraic_type {
                    AlgebraicType::Sum(sum_type) if sum_type.as_option().is_some() => {
                        reducer_args.push(format!("{{'0': [self.{}]}}", python_field_name))
                    }
                    AlgebraicType::Sum(_) => unimplemented!(),
                    AlgebraicType::Product(_) => {
                        reducer_args.push(format!("self.{python_field_name}"));
                    }
                    AlgebraicType::Builtin(_) => {
                        reducer_args.push(format!("self.{python_field_name}"));
                    }
                    AlgebraicType::Ref(type_ref) => {
                        let ref_type = &ctx.typespace.types[type_ref.idx()];
                        if let AlgebraicType::Sum(sum_type) = ref_type {
                            if is_enum(sum_type) {
                                reducer_args.push(format!("{{str({}.value): []}}", python_field_name))
                            } else {
                                unimplemented!()
                            }
                        } else {
                            reducer_args.push(format!("self.{python_field_name}.encode()"));
                        }
                    }
                }
            }
            let reducer_args_str = reducer_args.join(", ");

            writeln!(output, "return [{}]", reducer_args_str).unwrap();
        }
        writeln!(output).unwrap();

        writeln!(output, "def __getattr__(self, name: str):").unwrap();
        {
            indent_scope!(output);
            writeln!(output, "return self.data.get(name)").unwrap();
        }
    }

    output.into_inner()
}

pub fn autogen_python_sum(ctx: &GenCtx, name: &str, sum_type: &SumType) -> String {
    if is_enum(sum_type) {
        autogen_python_enum(ctx, name, sum_type)
    } else {
        unimplemented!()
    }
}

pub fn autogen_python_enum(_ctx: &GenCtx, name: &str, sum_type: &SumType) -> String {
    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "# THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    )
    .unwrap();
    writeln!(output, "# WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "from enum import Enum").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "class {name}(Enum):").unwrap();
    {
        indent_scope!(output);
        for (idx, variant) in sum_type.variants.iter().enumerate() {
            let variant_name = variant
                .name
                .as_ref()
                .expect("All sum variants should have names!")
                .replace("r#", "");
            let python_idx = idx + 1;
            writeln!(output, "{variant_name} = {python_idx}").unwrap();
        }
    }

    output.into_inner()
}

pub fn autogen_python_tuple(ctx: &GenCtx, name: &str, tuple: &ProductType) -> String {
    autogen_python_product_table_common(ctx, name, tuple, None)
}

fn encode_builtintype<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    b: &'a BuiltinType,
    value: impl fmt::Display + 'a,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match maybe_primitive(b) {
        MaybePrimitive::Primitive(_) => {
            write!(f, "{value}")
        }
        MaybePrimitive::Array(ArrayType { elem_ty }) if **elem_ty == AlgebraicType::U8 => {
            write!(f, "{value}.hex()")
        }
        MaybePrimitive::Array(ArrayType { elem_ty }) => {
            let convert_type = encode_type(ctx, vecnest + 1, elem_ty, "item", ref_prefix);
            write!(f, "[{convert_type} for item in {value}]")
        }
        MaybePrimitive::Map(_) => unimplemented!(),
    })
}

pub fn encode_type<'a>(
    ctx: &'a GenCtx,
    vecnest: usize,
    ty: &'a AlgebraicType,
    value: impl fmt::Display + 'a,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Product(product) => {
            if product.is_identity() {
                write!(f, "Identity.from_string({value})")
            } else if product.is_address() {
                write!(f, "Address.from_string({value})")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Sum(sum_type) => match sum_type.as_option() {
            Some(inner_ty) => write!(
                f,
                "{{'0': {}}} if value is not None else {{}}",
                encode_type(ctx, vecnest, inner_ty, format!("{value}"), ref_prefix),
            ),
            None => unimplemented!(),
        },
        AlgebraicType::Builtin(b) => fmt::Display::fmt(&encode_builtintype(ctx, vecnest, b, &value, ref_prefix), f),
        AlgebraicType::Ref(r) => {
            let algebraic_type = &ctx.typespace.types[r.idx()];
            match algebraic_type {
                // for enums in json this comes over as a dictionary where the key is actually the enum index
                AlgebraicType::Sum(sum_type) if is_enum(sum_type) => write!(f, "{{str({value}.value-1): []}}"),
                _ => {
                    write!(f, "{value}.encode()")
                }
            }
        }
    })
}

pub fn autogen_python_reducer(ctx: &GenCtx, reducer: &ReducerDef) -> String {
    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "# THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    )
    .unwrap();
    writeln!(output, "# WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.").unwrap();
    writeln!(output).unwrap();

    writeln!(output, "from typing import List, Callable, Optional").unwrap();
    writeln!(output).unwrap();

    writeln!(
        output,
        "from spacetimedb_sdk.spacetimedb_client import SpacetimeDBClient"
    )
    .unwrap();
    writeln!(output, "from spacetimedb_sdk.spacetimedb_client import Identity").unwrap();
    writeln!(output, "from spacetimedb_sdk.spacetimedb_client import Address").unwrap();

    writeln!(output).unwrap();

    let mut imports = Vec::new();
    generate_imports(
        ctx,
        &reducer.args.clone().into_iter().collect::<Vec<ProductTypeElement>>(),
        &mut imports,
    );

    for import in imports {
        writeln!(output, "{import}").unwrap();
    }
    writeln!(output).unwrap();

    writeln!(output, "reducer_name = \"{}\"", reducer.name).unwrap();
    writeln!(output).unwrap();

    let mut func_call = Vec::new();
    let mut func_arguments = Vec::new();
    let mut func_types = Vec::new();
    for arg in reducer.args.iter() {
        let arg_name = arg
            .name
            .as_deref()
            .unwrap_or_else(|| panic!("reducer args should have names: {}", reducer.name));

        let arg_type = ty_fmt(ctx, &arg.algebraic_type, "");

        func_call.push(arg_name);
        func_arguments.push(format!("{arg_name}: {arg_type}"));
        func_types.push(arg_type.to_string());
    }

    let func_arguments_str = func_arguments.join(", ");
    let func_types_str = func_types.join(", ");
    let mut func_call_str = func_call.join(", ");
    if !func_call.is_empty() {
        func_call_str = format!(", {func_call_str}");
    }

    let callback_sig_str = if !func_types.is_empty() {
        format!(", {func_types_str}")
    } else {
        func_types_str
    };

    writeln!(
        output,
        "def {}({}):",
        reducer.name.to_case(Case::Snake),
        func_arguments_str
    )
    .unwrap();
    {
        indent_scope!(output);

        for arg in reducer.args.iter() {
            let field_name = arg
                .name
                .as_deref()
                .unwrap_or_else(|| panic!("reducer args should have names: {}", reducer.name));

            let field_type = &arg.algebraic_type;
            let python_field_name = field_name.to_string().replace("r#", "");
            writeln!(
                output,
                "{python_field_name} = {}",
                encode_type(ctx, 0, field_type, format_args!("{python_field_name}"), "")
            )
            .unwrap();
        }

        writeln!(
            output,
            "SpacetimeDBClient.instance._reducer_call(\"{}\"{})",
            reducer.name, func_call_str
        )
        .unwrap();
    }
    writeln!(output).unwrap();

    writeln!(
        output,
        "def register_on_{}(callback: Callable[[Identity, Optional[Address], str, str{}], None]):",
        reducer.name.to_case(Case::Snake),
        callback_sig_str
    )
    .unwrap();
    {
        indent_scope!(output);

        writeln!(
            output,
            "SpacetimeDBClient.instance._register_reducer(\"{}\", callback)",
            reducer.name
        )
        .unwrap();
    }
    writeln!(output).unwrap();

    writeln!(output, "def _decode_args(data):").unwrap();
    {
        indent_scope!(output);

        let mut decode_strs = Vec::new();
        for (idx, arg) in reducer.args.iter().enumerate() {
            let field_type = &arg.algebraic_type;
            decode_strs.push(format!(
                "{}",
                convert_type(ctx, 0, field_type, format_args!("data[{idx}]"), "")
            ));
        }

        writeln!(output, "return [{}]", decode_strs.join(", ")).unwrap();
    }

    output.into_inner()
}

pub fn autogen_python_globals(_ctx: &GenCtx, _items: &[GenItem]) -> Vec<Vec<(String, String)>> {
    vec![] //TODO
}
