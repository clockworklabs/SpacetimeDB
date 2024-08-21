use super::util::fmt_fn;

use std::fmt;
use std::ops::Deref;

use convert_case::{Case, Casing};
use spacetimedb_lib::sats::product_type::IDENTITY_TAG;
use spacetimedb_lib::sats::{
    AlgebraicType, AlgebraicTypeRef, ArrayType, ProductType, ProductTypeElement, SumType, SumTypeVariant,
};
use spacetimedb_lib::{ReducerDef, TableDesc};
use spacetimedb_primitives::ColList;
use spacetimedb_schema::schema::TableSchema;

use super::code_indenter::CodeIndenter;
use super::{GenCtx, GenItem, INDENT};

fn scalar_or_string_to_ts(ty: &AlgebraicType) -> Option<(&str, &str)> {
    Some(match ty {
        AlgebraicType::Bool => ("boolean", "Boolean"),
        AlgebraicType::I8
        | AlgebraicType::U8
        | AlgebraicType::I16
        | AlgebraicType::U16
        | AlgebraicType::I32
        | AlgebraicType::U32
        | AlgebraicType::F32
        | AlgebraicType::F64 => ("number", "Number"),
        AlgebraicType::I64
        | AlgebraicType::U64
        | AlgebraicType::I128
        | AlgebraicType::U128
        | AlgebraicType::I256
        | AlgebraicType::U256 => ("BigInt", "BigInt"),
        AlgebraicType::String => ("string", "String"),
        _ => return None,
    })
}

fn ty_fmt<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, ref_prefix: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        ty if ty.is_identity() => write!(f, "Identity"),
        ty if ty.is_address() => write!(f, "Address"),
        ty if ty.is_schedule_at() => write!(f, "ScheduleAt"),
        AlgebraicType::Sum(sum_type) => {
            if let Some(inner_ty) = sum_type.as_option() {
                write!(f, "{} | null", ty_fmt(ctx, inner_ty, ref_prefix))
            } else {
                unimplemented!()
            }
        }
        // All other types should fail.
        AlgebraicType::Product(_) => unimplemented!(),
        ty if ty.is_bytes() => f.write_str("Uint8Array"),
        AlgebraicType::Array(ty) => write!(f, "{}[]", ty_fmt(ctx, &ty.elem_ty, ref_prefix)),
        AlgebraicType::Map(ty) => {
            write!(
                f,
                "Map<{}, {}>",
                ty_fmt(ctx, &ty.ty, ref_prefix),
                ty_fmt(ctx, &ty.key_ty, ref_prefix)
            )
        }
        AlgebraicType::Ref(r) => write!(f, "{}{}", ref_prefix, typescript_typename(ctx, *r)),
        ty => f.write_str(scalar_or_string_to_ts(ty).unwrap().0),
    })
}

fn convert_type<'a>(
    ctx: &'a GenCtx,
    ty: &'a AlgebraicType,
    value: impl fmt::Display + 'a,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| {
        match ty {
        ty if ty.is_identity() => write!(f, "{value}.asIdentity()"),
        ty if ty.is_address() => write!(f, "{value}.asAddress()"),
        ty if ty.is_schedule_at() => write!(f, "{value}.asScheduleAt()"),
        AlgebraicType::Product(_) => unimplemented!(),
        AlgebraicType::Sum(sum_type) => match sum_type.as_option() {
            Some(inner_ty @ AlgebraicType::Ref(_)) => fmt::Display::fmt(
                &format!(
                    "function() {{ const value = {}.asSumValue().tag == 1 ? null : {}.asSumValue().value; return value ? {} : null; }}()",
                    value,
                    value,
                    convert_type(
                        ctx,
                        inner_ty,
                        "value",
                        ref_prefix
                    )
                ),
                f,
            ),
            Some(inner_ty) if ty.is_scalar() => write!(
                f,
                "{}.asSumValue().tag == 1 ? null : {}.asSumValue().value{}",
                value,
                value,
                &convert_type(ctx, inner_ty, "", ref_prefix),
            ),
            Some(inner_ty) => fmt::Display::fmt(
                &convert_type(
                    ctx,
                    inner_ty,
                    format_args!("{value}.asSumValue().tag == 1 ? null : {value}.asSumValue().value"),
                    ref_prefix
                ),
                f,
            ),
            None => unimplemented!(),
        },
        ty if ty.is_bytes() => write!(f, "{value}.asBytes()"),
        AlgebraicType::Array(ArrayType { elem_ty }) => {
            let typescript_type = ty_fmt(ctx, elem_ty, ref_prefix);
            let el_conv = convert_type(ctx, elem_ty, "el", ref_prefix);
            writeln!(f, "{value}.asArray().map(el => {el_conv}) as {typescript_type}[];")
        }
        AlgebraicType::Map(_) => todo!(),
        AlgebraicType::Ref(r) => {
            let name = typescript_typename(ctx, *r);
            write!(f, "{ref_prefix}{name}.fromValue({value})")
        }
        ty => {
            let typescript_as_type = scalar_or_string_to_ts(ty).unwrap().1;
            write!(f, "{value}.as{typescript_as_type}()")
        }
    }
    })
}

// can maybe do something fancy with this in the future
fn typescript_typename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> &str {
    ctx.names[typeref.idx()].as_deref().expect("tuples should have names")
}

fn typescript_filename(ctx: &GenCtx, typeref: AlgebraicTypeRef) -> String {
    ctx.names[typeref.idx()]
        .as_deref()
        .expect("tuples should have names")
        .to_case(Case::Snake)
}

macro_rules! indent_scope {
    ($x:ident) => {
        let mut $x = $x.indented(1);
    };
}

fn convert_algebraic_type<'a>(ctx: &'a GenCtx, ty: &'a AlgebraicType, ref_prefix: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        ty if ty.is_schedule_at() => write!(f, "ScheduleAt.getAlgebraicType()"),
        AlgebraicType::Product(product_type) => write!(f, "{}", convert_product_type(ctx, product_type, ref_prefix)),
        AlgebraicType::Sum(sum_type) => write!(f, "{}", convert_sum_type(ctx, sum_type, ref_prefix)),
        AlgebraicType::Array(ty) => write!(
            f,
            "AlgebraicType.createArrayType({})",
            convert_algebraic_type(ctx, &ty.elem_ty, ref_prefix)
        ),
        AlgebraicType::Map(_) => todo!(),
        AlgebraicType::Ref(r) => write!(f, "{ref_prefix}{}.getAlgebraicType()", typescript_typename(ctx, *r)),
        ty => write!(f, "AlgebraicType.create{ty:?}Type()"),
    })
}

fn convert_product_type<'a>(
    ctx: &'a GenCtx,
    product_type: &'a ProductType,
    ref_prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| {
        writeln!(f, "AlgebraicType.createProductType([")?;
        for elem in &*product_type.elements {
            writeln!(
                f,
                "{INDENT}new ProductTypeElement(\"{}\", {}),",
                elem.name
                    .to_owned()
                    .map(|s| {
                        if &*s == IDENTITY_TAG {
                            s.into()
                        } else {
                            typescript_field_name(s.deref().to_case(Case::Camel))
                        }
                    })
                    .unwrap_or("null".into()),
                convert_algebraic_type(ctx, &elem.algebraic_type, ref_prefix)
            )?;
        }
        write!(f, "])")
    })
}

fn convert_sum_type<'a>(ctx: &'a GenCtx, sum_type: &'a SumType, ref_prefix: &'a str) -> impl fmt::Display + 'a {
    fmt_fn(move |f| {
        writeln!(f, "AlgebraicType.createSumType([")?;
        for elem in &*sum_type.variants {
            writeln!(
                f,
                "\tnew SumTypeVariant({}, {}),",
                elem.name
                    .to_owned()
                    .map(|s| format!("\"{s}\""))
                    .unwrap_or("null".into()),
                convert_algebraic_type(ctx, &elem.algebraic_type, ref_prefix)
            )?;
        }
        write!(f, "])")
    })
}

fn serialize_type<'a>(
    ctx: &'a GenCtx,
    ty: &'a AlgebraicType,
    value: &'a str,
    prefix: &'a str,
) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicType::Product(prod) => {
            if prod.is_special() {
                write!(f, "Array.from({value}.toUint8Array())")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Sum(sum_type) => {
            if let Some(inner_ty) = sum_type.as_option() {
                write!(
                    f,
                    "{value} ? {{ \"some\": {} }} : {{ \"none\": [] }}",
                    serialize_type(ctx, inner_ty, value, prefix)
                )
            } else if sum_type.is_schedule_at() {
                write!(f, "ScheduleAt.serialize({value})")
            } else {
                unimplemented!()
            }
        }
        AlgebraicType::Ref(r) => {
            let typename = typescript_typename(ctx, *r);
            write!(f, "{prefix}{typename}.serialize({value})")
        }
        ty if ty.is_bytes() => write!(f, "Array.from({value})"),
        AlgebraicType::Array(ArrayType { elem_ty }) => match &**elem_ty {
            ty if ty.is_scalar_or_string() || ty.is_map() => write!(f, "{value}"),
            t => write!(f, "{value}.map(el => {})", serialize_type(ctx, t, "el", prefix)),
        },
        _ => write!(f, "{value}"),
    })
}

pub fn autogen_typescript_sum(ctx: &GenCtx, name: &str, sum_type: &SumType) -> String {
    let mut output = CodeIndenter::new(String::new());

    let sum_type_name = name.replace("r#", "").to_case(Case::Pascal);

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    );
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.");
    writeln!(output);

    writeln!(output, "// @ts-ignore");
    writeln!(output, "import {{ __SPACETIMEDB__, AlgebraicType, SumTypeVariant, AlgebraicValue }} from \"@clockworklabs/spacetimedb-sdk\";");

    let mut imports = Vec::new();
    generate_imports_variants(ctx, &sum_type.variants, &mut imports, Some("__"));

    for import in imports {
        writeln!(output, "// @ts-ignore");
        writeln!(output, "{import}");
    }

    writeln!(output);

    writeln!(output, "export namespace {sum_type_name} {{");

    let mut names = Vec::new();
    {
        indent_scope!(output);

        writeln!(output, "export function getAlgebraicType(): AlgebraicType {{");

        {
            indent_scope!(output);

            writeln!(output, "return {};", convert_sum_type(ctx, sum_type, "__"));
        }
        writeln!(output, "}}\n");

        let serialize = format!("export function serialize(value: {sum_type_name}): object {{");
        writeln!(output, "{serialize}");

        {
            indent_scope!(output);

            if sum_type.is_simple_enum() {
                // for a simple enum we can simplify the fromValue function
                writeln!(output, "const result: {{[key: string]: any}} = {{}};");
                writeln!(output, "result[value.tag] = [];");
                writeln!(output, "return result;");
            } else {
                writeln!(output, "switch(value.tag) {{");
                {
                    indent_scope!(output);

                    for variant in sum_type.variants.iter() {
                        let variant_name = variant
                            .name
                            .as_ref()
                            .expect("All sum variants should have names!")
                            .replace("r#", "")
                            .to_case(Case::Pascal);
                        writeln!(output, "case \"{variant_name}\":");

                        let field_type = &variant.algebraic_type;
                        let to_return = match field_type {
                            AlgebraicType::Product(p) if p.elements.is_empty() => {
                                format!("{{ \"{variant_name}\": [] }}")
                            }
                            _ => format!(
                                "{{ \"{variant_name}\": {} }}",
                                serialize_type(ctx, field_type, "value.value", "__")
                            ),
                        };
                        writeln!(output, "\treturn {to_return};");
                    }

                    writeln!(output, "default:");
                    writeln!(output, "\tthrow(\"unreachable\");");
                }
                writeln!(output, "}}");
            }
        }
        writeln!(output, "}}");

        writeln!(output);

        for variant in &*sum_type.variants {
            let variant_name = variant
                .name
                .as_ref()
                .expect("All sum variants should have names!")
                .replace("r#", "")
                .to_case(Case::Pascal);

            names.push(variant_name.clone());

            let a_type = match variant.algebraic_type {
                AlgebraicType::Product(_) => "undefined".to_string(),
                _ => {
                    format!("{}", ty_fmt(ctx, &variant.algebraic_type, "__"))
                }
            };
            writeln!(
                output,
                "export type {variant_name} = {{ tag: \"{variant_name}\", value: {a_type} }};"
            );

            // export an object or a function representing an enum value, so people
            // can pass it as an argument
            match variant.algebraic_type {
                AlgebraicType::Product(_) => writeln!(
                    output,
                    "export const {variant_name} = {{ tag: \"{variant_name}\", value: undefined }};"
                ),
                _ => writeln!(
                    output,
                    "export const {variant_name} = (value: {a_type}): {variant_name} => ({{ tag: \"{variant_name}\", value }});"
                ),
            };
        }

        writeln!(output);

        let from_value = format!("export function fromValue(value: AlgebraicValue): {sum_type_name} {{");
        writeln!(output, "{from_value}");

        {
            indent_scope!(output);

            writeln!(output, "let sumValue = value.asSumValue();");

            if sum_type.is_simple_enum() {
                // for a simple enum we can simplify the fromValue function
                writeln!(output, "let tag = sumValue.tag;");
                writeln!(
                    output,
                    "let variant = {sum_type_name}.getAlgebraicType().sum.variants[tag];"
                );
                writeln!(
                    output,
                    "return {{ tag: variant.name, value: undefined }} as {sum_type_name};"
                );
            } else {
                writeln!(output, "switch(sumValue.tag) {{");
                {
                    indent_scope!(output);

                    for (i, variant) in sum_type.variants.iter().enumerate() {
                        let variant_name = variant
                            .name
                            .as_ref()
                            .expect("All sum variants should have names!")
                            .replace("r#", "")
                            .to_case(Case::Pascal);
                        writeln!(output, "case {i}:");

                        let field_type = &variant.algebraic_type;
                        let result = match field_type {
                            AlgebraicType::Product(product_type) if product_type.elements.is_empty() => {
                                format!("{{ tag: \"{variant_name}\", value: undefined }}")
                            }
                            _ => format!(
                                "{{ tag: \"{variant_name}\", value: {} }}",
                                convert_type(ctx, field_type, "sumValue.value", "__")
                            ),
                        };
                        writeln!(output, "\treturn {result};");
                    }

                    writeln!(output, "default:");
                    writeln!(output, "\tthrow(\"unreachable\");");
                }
                writeln!(output, "}}");
            }
        }
        writeln!(output, "}}");
    }

    writeln!(output, "}}");

    let names = names
        .iter()
        .map(|s| format!("{sum_type_name}.{s}"))
        .collect::<Vec<String>>()
        .join(" | ");
    writeln!(output, "\nexport type {sum_type_name} = {names};");
    writeln!(output, "export default {sum_type_name};");

    output.into_inner()
}

const RESERVED_KEYWORDS: [&str; 36] = [
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
];

fn typescript_field_name(field_name: String) -> String {
    if RESERVED_KEYWORDS
        .into_iter()
        .map(String::from)
        .collect::<Vec<String>>()
        .contains(&field_name)
    {
        return format!("_{field_name}");
    }

    field_name
}

pub fn autogen_typescript_tuple(ctx: &GenCtx, name: &str, tuple: &ProductType) -> String {
    autogen_typescript_product_table_common(ctx, name, tuple, None)
}
pub fn autogen_typescript_table(ctx: &GenCtx, table: &TableDesc) -> String {
    let tuple = ctx.typespace[table.data].as_product().unwrap();
    autogen_typescript_product_table_common(
        ctx,
        &table.schema.table_name,
        tuple,
        Some(TableSchema::from_def(0.into(), table.schema.clone())),
    )
}

fn generate_imports(ctx: &GenCtx, elements: &[ProductTypeElement], imports: &mut Vec<String>, prefix: Option<&str>) {
    for field in elements {
        _generate_imports(ctx, &field.algebraic_type, imports, prefix);
    }
}

// TODO: refactor to allow passing both elements and variants
fn generate_imports_variants(
    ctx: &GenCtx,
    variants: &[SumTypeVariant],
    imports: &mut Vec<String>,
    prefix: Option<&str>,
) {
    for variant in variants {
        _generate_imports(ctx, &variant.algebraic_type, imports, prefix);
    }
}

fn _generate_imports(ctx: &GenCtx, ty: &AlgebraicType, imports: &mut Vec<String>, prefix: Option<&str>) {
    match ty {
        AlgebraicType::Array(ty) => _generate_imports(ctx, &ty.elem_ty, imports, prefix),
        AlgebraicType::Map(map_type) => {
            _generate_imports(ctx, &map_type.key_ty, imports, prefix);
            _generate_imports(ctx, &map_type.ty, imports, prefix);
        }
        AlgebraicType::Ref(r) => {
            let class_name = typescript_typename(ctx, *r).to_string();
            let filename = typescript_filename(ctx, *r);

            let imported_as = match prefix {
                Some(prefix) => format!("{class_name} as {prefix}{class_name}"),
                None => class_name,
            };
            let import = format!("import {{ {imported_as} }} from \"./{filename}\";");
            imports.push(import);
        }
        // Generate imports for the fields of anonymous sum types like `Option<T>`.
        AlgebraicType::Sum(s) => {
            for variant in &*s.variants {
                _generate_imports(ctx, &variant.algebraic_type, imports, prefix);
            }
        }
        // Do we need to generate imports for fields of anonymous product types as well?
        _ => {}
    }
}

fn autogen_typescript_product_table_common(
    ctx: &GenCtx,
    name: &str,
    product_type: &ProductType,
    schema: Option<TableSchema>,
) -> String {
    let mut output = CodeIndenter::new(String::new());

    let struct_name_pascal_case = name.replace("r#", "").to_case(Case::Pascal);

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE",
    );
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.");
    writeln!(output);

    writeln!(output, "// @ts-ignore");
    writeln!(output, "import {{ __SPACETIMEDB__, AlgebraicType, ProductType, ProductTypeElement, SumType, SumTypeVariant, DatabaseTable, AlgebraicValue, ReducerEvent, Identity, Address, ScheduleAt, ClientDB, SpacetimeDBClient }} from \"@clockworklabs/spacetimedb-sdk\";");

    let mut imports = Vec::new();
    generate_imports(ctx, &product_type.elements, &mut imports, None);

    for import in imports {
        writeln!(output, "// @ts-ignore");
        writeln!(output, "{import}");
    }

    writeln!(output);

    writeln!(output, "export class {struct_name_pascal_case} extends DatabaseTable");
    writeln!(output, "{{");
    {
        indent_scope!(output);

        writeln!(output, "public static db: ClientDB = __SPACETIMEDB__.clientDB;");
        writeln!(output, "public static tableName = \"{struct_name_pascal_case}\";");

        let mut constructor_signature = Vec::new();
        let mut constructor_assignments = Vec::new();
        for field in &*product_type.elements {
            let field_name = field
                .name
                .as_ref()
                .expect("autogen'd tuples should have field names")
                .replace("r#", "");
            let field_name_camel_case = typescript_field_name(field_name.to_case(Case::Camel));
            let arg = format!("{}: {}", field_name_camel_case, ty_fmt(ctx, &field.algebraic_type, ""));
            let assignment = format!("this.{field_name_camel_case} = {field_name_camel_case};");

            writeln!(output, "public {arg};");

            constructor_signature.push(arg);
            constructor_assignments.push(assignment);
        }

        writeln!(output);

        if let Some(schema) = &schema {
            // if this table has a primary key add it to the codegen
            if let Some(primary_key) = schema
                .pk()
                .map(|field| format!("\"{}\"", field.col_name.deref().to_case(Case::Camel)))
            {
                writeln!(output, "public static primaryKey: string | undefined = {primary_key};");
                writeln!(output);
            }
        } else {
            writeln!(output, "public static primaryKey: string | undefined = undefined;");
            writeln!(output);
        }

        writeln!(output, "constructor({}) {{", constructor_signature.join(", "));
        writeln!(output, "super();");
        {
            indent_scope!(output);

            for assignment in constructor_assignments {
                writeln!(output, "{assignment}");
            }
        }

        writeln!(output, "}}");

        writeln!(output);

        writeln!(
            output,
            "public static serialize(value: {struct_name_pascal_case}): object {{"
        );

        {
            indent_scope!(output);

            writeln!(output, "return [");

            let mut args = Vec::new();
            for field in &*product_type.elements {
                let field_name = field
                    .name
                    .as_ref()
                    .expect("autogen'd tuples should have field names")
                    .replace("r#", "");

                let field_name_camel_case = typescript_field_name(field_name.to_case(Case::Camel));
                let value_field = format!("value.{field_name_camel_case}");
                let arg = format!("{}", serialize_type(ctx, &field.algebraic_type, &value_field, ""));
                args.push(arg);
            }

            writeln!(output, "{}", args.join(", "));

            writeln!(output, "];");
        }

        writeln!(output, "}}");

        writeln!(output);

        writeln!(output, "public static getAlgebraicType(): AlgebraicType");
        writeln!(output, "{{");
        {
            indent_scope!(output);
            writeln!(output, "return {};", convert_product_type(ctx, product_type, ""));
        }
        writeln!(output, "}}");
        writeln!(output);

        write!(
            output,
            "{}",
            autogen_typescript_product_value_to_struct(ctx, &struct_name_pascal_case, product_type)
        );

        writeln!(output);

        if let Some(schema) = &schema {
            autogen_typescript_access_funcs_for_struct(
                &mut output,
                &struct_name_pascal_case,
                product_type,
                name,
                schema,
            );

            writeln!(output);
        }
    }
    writeln!(output, "}}");

    writeln!(output, "\nexport default {struct_name_pascal_case};");

    output.into_inner()
}

fn autogen_typescript_product_value_to_struct(
    ctx: &GenCtx,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
) -> String {
    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "public static fromValue(value: AlgebraicValue): {struct_name_pascal_case}",
    );
    writeln!(output, "{{");
    {
        indent_scope!(output);
        writeln!(output, "let productValue = value.asProductValue();");

        let mut constructor_args = Vec::new();
        // vec conversion go here
        for (idx, field) in product_type.elements.iter().enumerate() {
            let field_name = field
                .name
                .as_ref()
                .expect("autogen'd product types should have field names");
            let field_type = &field.algebraic_type;
            let typescript_field_name = typescript_field_name(field_name.to_string().replace("r#", ""));
            constructor_args.push(format!("__{typescript_field_name}"));

            writeln!(
                output,
                "let __{typescript_field_name} = {};",
                convert_type(ctx, field_type, format_args!("productValue.elements[{idx}]"), "")
            );
        }

        writeln!(output, "return new this({});", constructor_args.join(", "));
    }
    writeln!(output, "}}");

    output.into_inner()
}

#[allow(dead_code)]
fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{");
    let res = f(&mut output.indented(1));
    writeln!(output, "}}");
    res
}

fn autogen_typescript_access_funcs_for_struct(
    output: &mut CodeIndenter<String>,
    struct_name_pascal_case: &str,
    product_type: &ProductType,
    table_name: &str,
    table: &TableSchema,
) {
    let constraints = table.column_constraints();
    for col in table.columns() {
        let is_unique = constraints[&ColList::new(col.col_pos)].has_unique();
        let field = &product_type.elements[col.col_pos.idx()];
        let field_name = field.name.as_ref().expect("autogen'd tuples should have field names");
        let field_type = &field.algebraic_type;
        let typescript_field_name_pascal = field_name.replace("r#", "").to_case(Case::Pascal);
        let typescript_field_name_camel = field_name.replace("r#", "").to_case(Case::Camel);

        let typescript_field_type = match field_type {
            ty if ty.is_identity() => "Identity",
            ty if ty.is_address() => "Address",
            AlgebraicType::Product(_)
            | AlgebraicType::Ref(_)
            | AlgebraicType::Sum(_)
            | AlgebraicType::Array(_)
            | AlgebraicType::Map(_) => {
                // TODO: We don't allow filtering on enums, tuples, arrays, and maps right now.
                // Its possible we may consider it for the future.
                continue;
            }
            ty => scalar_or_string_to_ts(ty).unwrap().0,
        };

        writeln!(
            output,
            "public static *filterBy{typescript_field_name_pascal}(value: {typescript_field_type}): IterableIterator<{struct_name_pascal_case}>"
        );

        writeln!(output, "{{");
        {
            indent_scope!(output);
            writeln!(
                output,
                "for (let instance of this.db.getTable(\"{table_name}\").getInstances())"
            );
            writeln!(output, "{{");
            {
                indent_scope!(output);
                let condition = if typescript_field_type == "Identity" || typescript_field_type == "Address" {
                    ".isEqual(value)"
                } else {
                    " === value"
                };
                writeln!(output, "if (instance.{typescript_field_name_camel}{condition}) {{");
                {
                    indent_scope!(output);
                    writeln!(output, "yield instance;");
                }
                writeln!(output, "}}");
            }
            // End foreach
            writeln!(output, "}}");
        }
        // End Func
        writeln!(output, "}}");
        writeln!(output);

        if is_unique {
            writeln!(
                output,
                "public static findBy{typescript_field_name_pascal}(value: {typescript_field_type}): {struct_name_pascal_case} | undefined"
            );

            writeln!(output, "{{");
            {
                indent_scope!(output);
                writeln!(
                    output,
                    "return this.filterBy{typescript_field_name_pascal}(value).next().value;"
                );
            }
            // End Func
            writeln!(output, "}}");
            writeln!(output);
        }
    }
}

// fn convert_enumdef(tuple: &SumType) -> impl fmt::Display + '_ {
//     fmt_fn(move |f| {
//         writeln!(f, "AlgebraicType.Tuple(new ProductTypeElement[]")?;
//         writeln!(f, "{{")?;
//         for (i, elem) in tuple.elements.iter().enumerate() {
//             let comma = if i == tuple.elements.len() - 1 { "" } else { "," };
//             writeln!(f, "{INDENT}{}{}", convert_elementdef(elem), comma)?;
//         }
//         writeln!(f, "}}")
//     })
// }

pub fn autogen_typescript_reducer(ctx: &GenCtx, reducer: &ReducerDef) -> String {
    let func_name = &*reducer.name;
    // let reducer_pascal_name = func_name.to_case(Case::Pascal);
    let reducer_name_pascal_case = func_name.to_case(Case::Pascal);

    let mut output = CodeIndenter::new(String::new());

    writeln!(
        output,
        "// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE"
    );
    writeln!(output, "// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.");
    writeln!(output);

    writeln!(output, "// @ts-ignore");
    writeln!(output, "import {{ __SPACETIMEDB__, AlgebraicType, ProductType, ProductTypeElement, DatabaseTable, AlgebraicValue, ReducerArgsAdapter, SumTypeVariant, Serializer, Identity, Address, ScheduleAt, ReducerEvent, Reducer, SpacetimeDBClient }} from \"@clockworklabs/spacetimedb-sdk\";");

    let mut imports = Vec::new();
    generate_imports(
        ctx,
        &reducer.args.clone().into_iter().collect::<Vec<ProductTypeElement>>(),
        &mut imports,
        None,
    );

    for import in imports {
        writeln!(output, "// @ts-ignore");
        writeln!(output, "{import}");
    }

    writeln!(output);

    let mut func_arguments = Vec::new();
    let mut arg_names = Vec::new();
    for arg in reducer.args.iter() {
        let name = arg
            .name
            .as_deref()
            .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
        let arg_name = format!("_{}", name.to_case(Case::Camel));
        let arg_type_str = ty_fmt(ctx, &arg.algebraic_type, "");

        func_arguments.push(format!("{arg_name}: {arg_type_str}"));
        arg_names.push(arg_name.to_string());
    }

    let full_reducer_name = format!("{reducer_name_pascal_case}Reducer");

    writeln!(output, "export class {full_reducer_name} extends Reducer");
    writeln!(output, "{{");

    {
        indent_scope!(output);

        writeln!(
            output,
            "public static reducerName: string = \"{reducer_name_pascal_case}\";"
        );
        writeln!(output, "public static call({}) {{", func_arguments.join(", "));
        {
            indent_scope!(output);

            writeln!(output, "this.getReducer().call({});", arg_names.join(", "));
        }
        writeln!(output, "}}\n");

        writeln!(output, "public call({}) {{", func_arguments.join(", "));
        {
            indent_scope!(output);

            writeln!(output, "const serializer = this.client.getSerializer();");

            let mut arg_names = Vec::new();
            for arg in reducer.args.iter() {
                let ty = &arg.algebraic_type;
                let name = arg
                    .name
                    .as_deref()
                    .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
                let arg_name = name.to_case(Case::Camel);

                writeln!(output, "let _{arg_name}Type = {};", convert_algebraic_type(ctx, ty, ""));
                writeln!(output, "serializer.write(_{arg_name}Type, _{arg_name});");

                arg_names.push(arg_name);
            }

            writeln!(output, "this.client.call(\"{func_name}\", serializer);");
        }
        // Closing brace for reducer
        writeln!(output, "}}");
        writeln!(output);

        let args: &str = if reducer.args.is_empty() {
            "_adapter: ReducerArgsAdapter"
        } else {
            "adapter: ReducerArgsAdapter"
        };
        writeln!(output, "public static deserializeArgs({args}): any[] {{");

        {
            indent_scope!(output);

            let mut arg_names = Vec::new();
            for arg in reducer.args.iter() {
                let ty = &arg.algebraic_type;
                let name = arg
                    .name
                    .as_deref()
                    .unwrap_or_else(|| panic!("reducer args should have names: {func_name}"));
                let arg_name = typescript_field_name(name.to_case(Case::Camel));

                writeln!(output, "let {arg_name}Type = {};", convert_algebraic_type(ctx, ty, ""));
                writeln!(
                    output,
                    "let {arg_name}Value = AlgebraicValue.deserialize({arg_name}Type, adapter.next())"
                );
                writeln!(
                    output,
                    "let {arg_name} = {};",
                    convert_type(ctx, ty, format_args!("{arg_name}Value"), "")
                );

                arg_names.push(arg_name);
            }

            writeln!(output, "return [{}];", arg_names.join(", "));
        }

        writeln!(output, "}}");

        writeln!(output);

        writeln!(
            output,
            "public static on(callback: (reducerEvent: ReducerEvent, {}) => void) {{",
            func_arguments.join(", ")
        );
        {
            indent_scope!(output);

            writeln!(output, "this.getReducer().on(callback);");
        }
        writeln!(output, "}}");

        // OnCreatePlayerEvent(dbEvent.Status, Identity.From(dbEvent.CallerIdentity.ToByteArray()), args[0].ToObject<string>());
        writeln!(
            output,
            "public on(callback: (reducerEvent: ReducerEvent, {}) => void)",
            func_arguments.join(", ")
        );
        writeln!(output, "{{");
        {
            indent_scope!(output);

            writeln!(
                output,
                "this.client.on(\"reducer:{reducer_name_pascal_case}\", callback);"
            );
        }

        // Closing brace for Event parsing function
        writeln!(output, "}}");
    }
    // Closing brace for class
    writeln!(output, "}}");

    writeln!(output);

    writeln!(output, "\nexport default {reducer_name_pascal_case}Reducer");

    output.into_inner()
}

pub fn autogen_typescript_globals(_ctx: &GenCtx, _items: &[GenItem]) -> Vec<(String, String)> {
    vec![] //TODO
}
