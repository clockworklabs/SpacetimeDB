//! C++ receiver bindings: the types and reducer handles a C++ sender module needs
//! to call another module's reducers through an outbox.

use crate::code_indenter::{CodeIndenter, Indenter};
use crate::util::{
    dependency_scoped_type_name, iter_reducers, iter_types, print_auto_generated_file_comment, type_ref_name,
};
use crate::{CodegenVisibility, OutputFile};
use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_lib::sats::AlgebraicTypeRef;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;

const INDENT: &str = "    ";

/// The C++ namespace used when bindings are generated without a dependency name.
pub const DEFAULT_NAMESPACE: &str = "remote_module";

/// The file name of the generated header.
pub const HEADER_FILE: &str = "bindings.h";

/// Generate root-module receiver bindings; submodule receivers are not supported yet.
pub(super) fn generate_cpp_module_bindings(module: &ModuleDef, dependency_name: Option<&str>) -> Vec<OutputFile> {
    let namespace = escape_identifier(dependency_name.unwrap_or(DEFAULT_NAMESPACE));
    let mut reducers = iter_reducers(module, CodegenVisibility::OnlyPublic).collect::<Vec<_>>();
    // Equivalent modules can register their reducers in different orders across languages.
    reducers.sort_by(|left, right| left.name.cmp(&right.name));

    let refs = reducer_type_refs(module, &reducers);
    let types = iter_types(module)
        .filter(|typ| refs.contains(&typ.ty))
        .collect::<Vec<_>>();
    let names = CppNames::new(module, &namespace, &types, &reducers);

    let mut output = CodeIndenter::new(String::new(), INDENT);
    let out = &mut output;
    print_auto_generated_file_comment(out);
    writeln!(
        out,
        "// Receiver bindings: call this module's reducers from another database through an outbox."
    );
    writeln!(out);
    writeln!(out, "#pragma once");
    writeln!(out);
    for include in [
        "array",
        "cstdint",
        "cstdlib",
        "optional",
        "string",
        "string_view",
        "tuple",
        "utility",
        "variant",
        "vector",
    ] {
        writeln!(out, "#include <{include}>");
    }
    writeln!(out, "#include <spacetimedb.h>");
    writeln!(out);

    writeln!(out, "namespace {namespace} {{");
    writeln!(out);
    writeln!(out, "namespace detail {{");
    writeln!(out, "struct module_tag {{}};");
    writeln!(out, "}} // namespace detail");
    writeln!(out);
    writeln!(out, "/// The identity of a database that runs this receiver module.");
    writeln!(
        out,
        "using Identity = ::SpacetimeDB::RemoteIdentity<detail::module_tag>;"
    );

    if !types.is_empty() {
        writeln!(out);
        for typ in &types {
            if !matches!(module.typespace_for_generate()[typ.ty], AlgebraicTypeDef::PlainEnum(_)) {
                writeln!(out, "struct {};", names.types[&typ.ty]);
            }
        }
        for typ in definition_order(module, &types) {
            writeln!(out);
            define_type(module, &names, out, typ);
        }
    }
    writeln!(out);
    writeln!(out, "}} // namespace {namespace}");

    if !types.is_empty() {
        writeln!(out);
        writeln!(out, "namespace SpacetimeDB::bsatn {{");
        for typ in &types {
            writeln!(out);
            declare_type_traits(&names, out, typ);
        }
        for typ in &types {
            writeln!(out);
            let sats_name = dependency_name.map_or_else(
                || type_ref_name(module, typ.ty),
                |dependency| dependency_scoped_type_name(dependency, &typ.accessor_name),
            );
            define_type_traits(module, &names, out, typ, &sats_name, dependency_name.is_some());
        }
        writeln!(out);
        writeln!(out, "}} // namespace SpacetimeDB::bsatn");
    }

    writeln!(out);
    writeln!(out, "namespace {namespace}::reducers {{");
    for reducer in &reducers {
        writeln!(out);
        define_reducer_handle(module, &names, out, reducer);
    }
    writeln!(out);
    writeln!(out, "}} // namespace {namespace}::reducers");

    vec![OutputFile {
        filename: HEADER_FILE.to_owned(),
        code: output.into_inner(),
    }]
}

/// C++ names chosen for generated items, unique within their namespaces.
struct CppNames {
    namespace: String,
    types: BTreeMap<AlgebraicTypeRef, String>,
    reducers: BTreeMap<String, String>,
}

impl CppNames {
    fn new(module: &ModuleDef, namespace: &str, types: &[&TypeDef], reducers: &[&ReducerDef]) -> Self {
        // `Identity` and `detail` are declared in the same namespace as the receiver's types.
        let mut used = BTreeSet::from(["Identity".to_owned(), "detail".to_owned(), "reducers".to_owned()]);
        let mut type_names = BTreeMap::new();
        for typ in types {
            let name = unique_name(&mut used, escape_identifier(&type_ref_name(module, typ.ty)));
            type_names.insert(typ.ty, name);
        }
        let mut used = BTreeSet::new();
        let mut reducer_names = BTreeMap::new();
        for reducer in reducers {
            let name = escape_identifier(&reducer.accessor_name.deref().to_case(Case::Snake));
            reducer_names.insert(reducer.name.to_string(), unique_name(&mut used, name));
        }
        Self {
            namespace: namespace.to_owned(),
            types: type_names,
            reducers: reducer_names,
        }
    }

    fn qualified_type(&self, r: &AlgebraicTypeRef) -> String {
        format!("::{}::{}", self.namespace, self.types[r])
    }
}

fn unique_name(used: &mut BTreeSet<String>, mut name: String) -> String {
    while !used.insert(name.clone()) {
        name.push('_');
    }
    name
}

fn reducer_type_refs(module: &ModuleDef, reducers: &[&ReducerDef]) -> BTreeSet<AlgebraicTypeRef> {
    let mut pending = Vec::new();
    for reducer in reducers {
        for (_, ty) in &reducer.params_for_generate.elements {
            ty.for_each_ref(|r| pending.push(r));
        }
    }
    let mut refs = BTreeSet::new();
    while let Some(r) = pending.pop() {
        if !refs.insert(r) {
            continue;
        }
        for (_, ty) in members(&module.typespace_for_generate()[r]) {
            ty.for_each_ref(|nested| pending.push(nested));
        }
    }
    refs
}

fn members(def: &AlgebraicTypeDef) -> Vec<(Identifier, AlgebraicTypeUse)> {
    match def {
        AlgebraicTypeDef::Product(product) => product.elements.to_vec(),
        AlgebraicTypeDef::Sum(sum) => sum.variants.to_vec(),
        AlgebraicTypeDef::PlainEnum(_) => Vec::new(),
    }
}

/// Order definitions so that every type a definition holds by value is defined first.
/// Types reached only through `std::vector` can stay incomplete, which permits recursion.
fn definition_order<'a>(module: &ModuleDef, types: &[&'a TypeDef]) -> Vec<&'a TypeDef> {
    fn by_value_refs(ty: &AlgebraicTypeUse, out: &mut Vec<AlgebraicTypeRef>) {
        match ty {
            AlgebraicTypeUse::Ref(r) => out.push(*r),
            AlgebraicTypeUse::Option(inner) => by_value_refs(inner, out),
            AlgebraicTypeUse::Result { ok_ty, err_ty } => {
                by_value_refs(ok_ty, out);
                by_value_refs(err_ty, out);
            }
            _ => {}
        }
    }
    fn visit<'a>(
        module: &ModuleDef,
        by_ref: &BTreeMap<AlgebraicTypeRef, &'a TypeDef>,
        r: AlgebraicTypeRef,
        visited: &mut BTreeSet<AlgebraicTypeRef>,
        order: &mut Vec<&'a TypeDef>,
    ) {
        if !visited.insert(r) {
            return;
        }
        let mut deps = Vec::new();
        for (_, ty) in members(&module.typespace_for_generate()[r]) {
            by_value_refs(&ty, &mut deps);
        }
        for dep in deps {
            visit(module, by_ref, dep, visited, order);
        }
        order.push(by_ref[&r]);
    }

    let by_ref = types.iter().map(|typ| (typ.ty, *typ)).collect::<BTreeMap<_, _>>();
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for typ in types {
        visit(module, &by_ref, typ.ty, &mut visited, &mut order);
    }
    order
}

fn define_type(module: &ModuleDef, names: &CppNames, out: &mut Indenter, typ: &TypeDef) {
    let name = &names.types[&typ.ty];
    match &module.typespace_for_generate()[typ.ty] {
        AlgebraicTypeDef::Product(product) => {
            writeln!(out, "struct {name} {{");
            out.with_indent(|out| {
                for (field, ty) in product.elements.iter() {
                    writeln!(out, "{} {};", cpp_type(names, ty), field_name(field));
                }
            });
            writeln!(out, "}};");
        }
        AlgebraicTypeDef::PlainEnum(plain) => {
            writeln!(out, "enum class {name} : uint8_t {{");
            out.with_indent(|out| {
                for (index, variant) in plain.variants.iter().enumerate() {
                    writeln!(out, "{} = {index},", variant_name(variant));
                }
            });
            writeln!(out, "}};");
        }
        AlgebraicTypeDef::Sum(sum) => {
            let payloads = sum
                .variants
                .iter()
                .map(|(_, ty)| cpp_type(names, ty))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(out, "struct {name} {{");
            out.with_indent(|out| {
                writeln!(out, "enum class Tag : uint8_t {{");
                out.with_indent(|out| {
                    for (index, (variant, _)) in sum.variants.iter().enumerate() {
                        writeln!(out, "{} = {index},", variant_name(variant));
                    }
                });
                writeln!(out, "}};");
                writeln!(out);
                writeln!(out, "std::variant<{payloads}> value;");
                writeln!(out);
                writeln!(out, "Tag tag() const {{ return static_cast<Tag>(value.index()); }}");
                for (index, (variant, ty)) in sum.variants.iter().enumerate() {
                    let variant = variant_name(variant);
                    writeln!(out);
                    if matches!(ty, AlgebraicTypeUse::Unit) {
                        writeln!(out, "static {name} {variant}() {{");
                        out.with_indent(|out| {
                            writeln!(out, "{name} result;");
                            writeln!(out, "result.value.template emplace<{index}>();");
                            writeln!(out, "return result;");
                        });
                    } else {
                        writeln!(out, "static {name} {variant}({} payload) {{", cpp_type(names, ty));
                        out.with_indent(|out| {
                            writeln!(out, "{name} result;");
                            writeln!(out, "result.value.template emplace<{index}>(std::move(payload));");
                            writeln!(out, "return result;");
                        });
                    }
                    writeln!(out, "}}");
                }
            });
            writeln!(out, "}};");
        }
    }
}

/// Declare the BSATN traits of a receiver type. Every declaration precedes every
/// definition, so types can refer to each other in any order.
fn declare_type_traits(names: &CppNames, out: &mut Indenter, typ: &TypeDef) {
    let ty = names.qualified_type(&typ.ty);
    writeln!(out, "template<>");
    writeln!(out, "struct bsatn_traits<{ty}> {{");
    out.with_indent(|out| {
        writeln!(out, "static void serialize(Writer& writer, const {ty}& value);");
        writeln!(out, "static {ty} deserialize(Reader& reader);");
        writeln!(out, "static AlgebraicType algebraic_type();");
    });
    writeln!(out, "}};");
    writeln!(out);
    writeln!(out, "template<>");
    writeln!(out, "struct algebraic_type_of<{ty}> {{");
    out.with_indent(|out| {
        writeln!(
            out,
            "static AlgebraicType get() {{ return bsatn_traits<{ty}>::algebraic_type(); }}"
        );
    });
    writeln!(out, "}};");
}

fn define_type_traits(
    module: &ModuleDef,
    names: &CppNames,
    out: &mut Indenter,
    typ: &TypeDef,
    sats_name: &str,
    exact_scope: bool,
) {
    let ty = names.qualified_type(&typ.ty);
    let register = if exact_scope {
        "getOrRegisterExactScope"
    } else {
        "getOrRegister"
    };
    let def = &module.typespace_for_generate()[typ.ty];

    writeln!(
        out,
        "inline void bsatn_traits<{ty}>::serialize(Writer& writer, const {ty}& value) {{"
    );
    out.with_indent(|out| match def {
        AlgebraicTypeDef::Product(product) => {
            for (field, _) in product.elements.iter() {
                writeln!(
                    out,
                    "::SpacetimeDB::bsatn::serialize(writer, value.{});",
                    field_name(field)
                );
            }
            if product.elements.is_empty() {
                writeln!(out, "(void)writer;");
                writeln!(out, "(void)value;");
            }
        }
        AlgebraicTypeDef::PlainEnum(_) => writeln!(out, "writer.write_u8(static_cast<uint8_t>(value));"),
        AlgebraicTypeDef::Sum(sum) => {
            writeln!(out, "writer.write_u8(static_cast<uint8_t>(value.value.index()));");
            writeln!(out, "switch (value.value.index()) {{");
            for index in 0..sum.variants.len() {
                writeln!(
                    out,
                    "case {index}: ::SpacetimeDB::bsatn::serialize(writer, std::get<{index}>(value.value)); break;"
                );
            }
            writeln!(out, "default: std::abort();");
            writeln!(out, "}}");
        }
    });
    writeln!(out, "}}");
    writeln!(out);

    writeln!(out, "inline {ty} bsatn_traits<{ty}>::deserialize(Reader& reader) {{");
    out.with_indent(|out| match def {
        AlgebraicTypeDef::Product(product) => {
            writeln!(out, "{ty} value;");
            for (field, field_ty) in product.elements.iter() {
                writeln!(
                    out,
                    "value.{} = ::SpacetimeDB::bsatn::deserialize<{}>(reader);",
                    field_name(field),
                    cpp_type(names, field_ty)
                );
            }
            if product.elements.is_empty() {
                writeln!(out, "(void)reader;");
            }
            writeln!(out, "return value;");
        }
        AlgebraicTypeDef::PlainEnum(plain) => {
            writeln!(out, "uint8_t tag = reader.read_u8();");
            writeln!(out, "if (tag >= {}) {{", plain.variants.len());
            out.with_indent(|out| writeln!(out, "std::abort();"));
            writeln!(out, "}}");
            writeln!(out, "return static_cast<{ty}>(tag);");
        }
        AlgebraicTypeDef::Sum(sum) => {
            writeln!(out, "{ty} value;");
            writeln!(out, "switch (reader.read_u8()) {{");
            for (index, (_, variant_ty)) in sum.variants.iter().enumerate() {
                writeln!(
                    out,
                    "case {index}: value.value.template emplace<{index}>(::SpacetimeDB::bsatn::deserialize<{}>(reader)); break;",
                    cpp_type(names, variant_ty)
                );
            }
            writeln!(out, "default: std::abort();");
            writeln!(out, "}}");
            writeln!(out, "return value;");
        }
    });
    writeln!(out, "}}");
    writeln!(out);

    writeln!(out, "inline AlgebraicType bsatn_traits<{ty}>::algebraic_type() {{");
    out.with_indent(|out| {
        writeln!(
            out,
            "return ::SpacetimeDB::Internal::LazyTypeRegistrar<{ty}>::{register}([]() -> AlgebraicType {{"
        );
        out.with_indent(|out| match def {
            AlgebraicTypeDef::Product(product) => {
                writeln!(out, "ProductTypeBuilder builder;");
                for (field, field_ty) in product.elements.iter() {
                    writeln!(
                        out,
                        "builder.with_field<{}>({});",
                        cpp_type(names, field_ty),
                        cpp_string_literal(&field.deref().to_case(Case::Snake))
                    );
                }
                writeln!(out, "return AlgebraicType::make_product(builder.build());");
            }
            AlgebraicTypeDef::PlainEnum(plain) => {
                writeln!(out, "SumTypeBuilder builder;");
                for variant in plain.variants.iter() {
                    writeln!(
                        out,
                        "builder.with_unit_variant({});",
                        cpp_string_literal(&variant.deref().to_case(Case::Pascal))
                    );
                }
                writeln!(out, "return AlgebraicType::make_sum(builder.build());");
            }
            AlgebraicTypeDef::Sum(sum) => {
                writeln!(out, "std::vector<SumTypeVariant> variants;");
                for (variant, variant_ty) in sum.variants.iter() {
                    writeln!(
                        out,
                        "variants.emplace_back({}, bsatn_traits<{}>::algebraic_type());",
                        cpp_string_literal(&variant.deref().to_case(Case::Pascal)),
                        cpp_type(names, variant_ty)
                    );
                }
                writeln!(
                    out,
                    "return AlgebraicType::make_sum(std::make_unique<SumTypeSchema>(std::move(variants)));"
                );
            }
        });
        writeln!(out, "}}, {});", cpp_string_literal(sats_name));
    });
    writeln!(out, "}}");
}

fn define_reducer_handle(module: &ModuleDef, names: &CppNames, out: &mut Indenter, reducer: &ReducerDef) {
    let handle = &names.reducers[&reducer.name.to_string()];
    let hash = spacetimedb_schema::def::reducer_signature_hash(
        &reducer.params(),
        &reducer.ok_return_type,
        &reducer.err_return_type,
        module.typespace(),
    );
    let params = &reducer.params_for_generate.elements;
    let types = params
        .iter()
        .map(|(_, ty)| cpp_type(names, ty))
        .collect::<Vec<_>>()
        .join(", ");
    let param_names = params
        .iter()
        .map(|(name, _)| cpp_string_literal(&param_name(name)))
        .collect::<Vec<_>>()
        .join(", ");

    writeln!(out, "/// The receiver's `{}` reducer.", reducer.name.deref());
    writeln!(out, "struct {handle} {{");
    out.with_indent(|out| {
        writeln!(
            out,
            "static constexpr std::string_view name = {};",
            cpp_string_literal(reducer.name.deref())
        );
        writeln!(
            out,
            "static constexpr std::string_view signature_hash = \"{}\";",
            hash.to_hex()
        );
        writeln!(out, "using identity_type = ::{}::Identity;", names.namespace);
        writeln!(out, "using param_types = std::tuple<{types}>;");
        writeln!(
            out,
            "static constexpr std::array<std::string_view, {}> param_names{{{{{param_names}}}}};",
            params.len()
        );
    });
    writeln!(out, "}};");
}

/// The name an outbox column must have to supply a receiver parameter.
fn param_name(name: &Identifier) -> String {
    name.deref().to_case(Case::Snake)
}

fn field_name(name: &Identifier) -> String {
    escape_identifier(&name.deref().to_case(Case::Snake))
}

fn variant_name(name: &Identifier) -> String {
    let name = escape_identifier(&name.deref().to_case(Case::Pascal));
    // Sum types declare these members next to their variant constructors.
    match name.as_str() {
        "Tag" | "value" | "tag" => format!("{name}_"),
        _ => name,
    }
}

fn cpp_type(names: &CppNames, ty: &AlgebraicTypeUse) -> String {
    match ty {
        AlgebraicTypeUse::Ref(r) => names.qualified_type(r),
        AlgebraicTypeUse::Array(elem) => format!("std::vector<{}>", cpp_type(names, elem)),
        AlgebraicTypeUse::Option(inner) => format!("std::optional<{}>", cpp_type(names, inner)),
        AlgebraicTypeUse::Result { ok_ty, err_ty } => format!(
            "::SpacetimeDB::Result<{}, {}>",
            cpp_type(names, ok_ty),
            cpp_type(names, err_ty)
        ),
        AlgebraicTypeUse::ScheduleAt => "::SpacetimeDB::ScheduleAt".to_owned(),
        AlgebraicTypeUse::Identity => "::SpacetimeDB::Identity".to_owned(),
        AlgebraicTypeUse::ConnectionId => "::SpacetimeDB::ConnectionId".to_owned(),
        AlgebraicTypeUse::Timestamp => "::SpacetimeDB::Timestamp".to_owned(),
        AlgebraicTypeUse::TimeDuration => "::SpacetimeDB::TimeDuration".to_owned(),
        AlgebraicTypeUse::Uuid => "::SpacetimeDB::Uuid".to_owned(),
        AlgebraicTypeUse::Unit | AlgebraicTypeUse::Never => "std::monostate".to_owned(),
        AlgebraicTypeUse::String => "std::string".to_owned(),
        AlgebraicTypeUse::Primitive(prim) => match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "int8_t",
            PrimitiveType::U8 => "uint8_t",
            PrimitiveType::I16 => "int16_t",
            PrimitiveType::U16 => "uint16_t",
            PrimitiveType::I32 => "int32_t",
            PrimitiveType::U32 => "uint32_t",
            PrimitiveType::I64 => "int64_t",
            PrimitiveType::U64 => "uint64_t",
            PrimitiveType::I128 => "::SpacetimeDB::I128",
            PrimitiveType::U128 => "::SpacetimeDB::U128",
            PrimitiveType::I256 => "::SpacetimeDB::I256",
            PrimitiveType::U256 => "::SpacetimeDB::U256",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
        }
        .to_owned(),
    }
}

fn cpp_string_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for c in value.chars() {
        match c {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            c if c.is_ascii() && !c.is_ascii_control() => literal.push(c),
            c => {
                let mut buf = [0; 4];
                for byte in c.encode_utf8(&mut buf).bytes() {
                    literal.push_str(&format!("\\x{byte:02x}\"\""));
                }
            }
        }
    }
    literal.push('"');
    literal
}

/// Append `_` to C++ keywords so they can be used as identifiers.
pub(super) fn escape_identifier(name: &str) -> String {
    if crate::cpp::is_cpp_keyword(name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}
