//! Human-readable rendering of a [`ModuleDef`], for `spacetime describe`.
//!
//! Types are spelled in a language-neutral way: primitives as `fmt_algebraic_type` spells them
//! (`U64`, `Bool`), containers as `Array<T>`, `Option<T>` and `Result<T, E>`, the unit type as
//! `()`, special types by name (`Timestamp`, `Identity`, ...), and named types by their scoped
//! name joined with `.` (`geo.shapes.Point`).
//!
//! [`describe_module`] renders a whole module as sections (Tables, Views, Reducers, Procedures,
//! HTTP routes, Environment variables, Types, then Row-level security), omitting empty sections.
//! [`describe_table`], [`describe_view`], [`describe_reducer`], [`describe_procedure`],
//! [`describe_http_route`], [`describe_env_var`] and [`describe_type`] render a single entity as it
//! appears in its section, and [`describe_tables`], [`describe_views`], [`describe_reducers`],
//! [`describe_procedures`], [`describe_http_routes`], [`describe_env_vars`] and [`describe_types`]
//! render the contents of a whole section. All of them use two-space indentation, end in a single newline, and never leave trailing whitespace. Column
//! widths are worked out from the uncoloured text, so the `AnsiColor` and `NoColor` styles align
//! identically.

use std::collections::HashSet;
use std::fmt;
use std::io;

use convert_case::{Case, Casing};
use itertools::Itertools;
use spacetimedb_lib::db::raw_def::v10::MethodOrAny;
use spacetimedb_lib::db::raw_def::v9::{Lifecycle, TableAccess};
use spacetimedb_lib::environment::{EnvVarType, EnvironmentDeclaration};
use spacetimedb_lib::http::Method as HttpMethod;
use spacetimedb_primitives::ColId;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{AlgebraicType, AlgebraicTypeRef, WithTypespace};

use crate::auto_migrate::PrettyPrintStyle;
use crate::def::{
    ColumnDef, HttpRouteDef, IndexAlgorithm, ModuleDef, ProcedureDef, ReducerDef, TableDef, TypeDef, ViewDef,
};
use crate::identifier::NamespacePath;
use crate::styled_writer::StyledWriter;
use crate::type_for_generate::{AlgebraicTypeDef, AlgebraicTypeUse, ProductTypeDef};

/// Displays the name of an [`AlgebraicTypeUse`], resolving refs through the owning [`ModuleDef`].
pub struct TypeName<'a> {
    def: &'a ModuleDef,
    ty: &'a AlgebraicTypeUse,
}

/// The language-neutral name of `ty`, e.g. `Option<Array<Thumbnail>>`.
pub fn type_name<'a>(def: &'a ModuleDef, ty: &'a AlgebraicTypeUse) -> TypeName<'a> {
    TypeName { def, ty }
}

impl fmt::Display for TypeName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // No wildcard arm: a new `AlgebraicTypeUse` variant must be given a spelling here.
        match self.ty {
            AlgebraicTypeUse::Ref(r) => {
                if let Some((name, _)) = self.def.type_def_from_ref(*r) {
                    write!(f, "{}", name.name_segments().format("."))
                } else if let Some(ty) = self.def.typespace().get(*r) {
                    write!(f, "{}", fmt_algebraic_type(ty))
                } else {
                    write!(f, "{r}")
                }
            }
            AlgebraicTypeUse::Array(elem) => write!(f, "Array<{}>", type_name(self.def, elem)),
            AlgebraicTypeUse::Option(inner) => write!(f, "Option<{}>", type_name(self.def, inner)),
            AlgebraicTypeUse::Result { ok_ty, err_ty } => write!(
                f,
                "Result<{}, {}>",
                type_name(self.def, ok_ty),
                type_name(self.def, err_ty)
            ),
            AlgebraicTypeUse::ScheduleAt => f.write_str("ScheduleAt"),
            AlgebraicTypeUse::Identity => f.write_str("Identity"),
            AlgebraicTypeUse::ConnectionId => f.write_str("ConnectionId"),
            AlgebraicTypeUse::Timestamp => f.write_str("Timestamp"),
            AlgebraicTypeUse::TimeDuration => f.write_str("TimeDuration"),
            AlgebraicTypeUse::Uuid => f.write_str("Uuid"),
            AlgebraicTypeUse::Unit => f.write_str("()"),
            AlgebraicTypeUse::Never => f.write_str("Never"),
            AlgebraicTypeUse::String => f.write_str("String"),
            AlgebraicTypeUse::Primitive(prim) => write!(f, "{}", fmt_algebraic_type(&prim.algebraic_type())),
        }
    }
}

/// Displays a parameter list as `name: Type, name: Type`, without surrounding parentheses.
pub struct ParamList<'a> {
    def: &'a ModuleDef,
    params: &'a ProductTypeDef,
}

/// The parameter list of a reducer, procedure or view, e.g. `name: String, count: U32`.
pub fn param_list<'a>(def: &'a ModuleDef, params: &'a ProductTypeDef) -> ParamList<'a> {
    ParamList { def, params }
}

impl fmt::Display for ParamList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (name, ty)) in self.params.elements.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{name}: {}", type_name(self.def, ty))?;
        }
        Ok(())
    }
}

const INDENT_WIDTH: usize = 2;

const BUFFER_WRITE: &str = "writing to an in-memory buffer cannot fail";

/// Renders the whole module as human-readable text, one section per kind of entity.
///
/// The sections are Tables, Views, Reducers, Procedures, HTTP routes, Environment variables, Types
/// and Row-level security, in that order. Empty sections are omitted, so an empty module renders as `""`.
pub fn describe_module(def: &ModuleDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_module(&mut w, def).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders a single table's block, as it appears under `Tables` in [`describe_module`] but
/// unindented.
///
/// `prefix` and `owning` are the namespace path and owning module returned alongside `table` by
/// [`ModuleDef::all_tables_with_prefix`].
pub fn describe_table(prefix: &NamespacePath, owning: &ModuleDef, table: &TableDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_table_block(&mut w, prefix, owning, table).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders a single view's row, as it appears under `Views` in [`describe_module`] but unindented.
///
/// `prefix` and `owning` are the namespace path and owning module returned alongside `view` by
/// [`ModuleDef::all_views_with_prefix`].
pub fn describe_view(prefix: &NamespacePath, owning: &ModuleDef, view: &ViewDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_view_row(&mut w, prefix, owning, view).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders a single reducer's row, as it appears under `Reducers` in [`describe_module`] but
/// unindented.
///
/// `owning` is the module returned alongside `reducer` by
/// [`ModuleDef::reducer_by_name_with_module`]. The reducer's name is already qualified with its
/// namespace path, so no prefix is needed.
pub fn describe_reducer(owning: &ModuleDef, reducer: &ReducerDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_reducer_row(&mut w, owning, reducer).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders a single procedure's row, as it appears under `Procedures` in [`describe_module`] but
/// unindented.
///
/// `prefix` and `owning` are the namespace path and owning module returned alongside `procedure` by
/// [`ModuleDef::all_procedures_with_prefix`].
pub fn describe_procedure(
    prefix: &NamespacePath,
    owning: &ModuleDef,
    procedure: &ProcedureDef,
    style: PrettyPrintStyle,
) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_procedure_row(&mut w, prefix, owning, procedure).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders a single HTTP route's row, as it appears under `HTTP routes` in [`describe_module`] but
/// unindented.
pub fn describe_http_route(route: &HttpRouteDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_http_route_row(&mut w, route).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders a single environment variable's row, as it appears under `Environment variables` in
/// [`describe_module`] but unindented, and aligned on its own rather than with the other rows.
pub fn describe_env_var(declaration: &EnvironmentDeclaration, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_env_var_rows(&mut w, &[declaration]).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders a single named type's row, as it appears under `Types` in [`describe_module`] but
/// unindented.
///
/// `named` comes from [`sorted_types`] or [`all_named_types`]. A type the `Types` section leaves
/// out, such as a table's row type, renders the same way.
pub fn describe_type(named: &NamedType<'_>, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_type_row(&mut w, named).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders every table in the module, including those in submodules, each as [`describe_table`]
/// renders it, separated by blank lines and in the order [`describe_module`] lists them.
///
/// A module with no tables renders as `""`.
pub fn describe_tables(def: &ModuleDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_table_blocks(&mut w, &sorted_tables(def)).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders every view in the module, including those in submodules, one row each as
/// [`describe_view`] renders it, in the order [`describe_module`] lists them.
///
/// A module with no views renders as `""`.
pub fn describe_views(def: &ModuleDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    for (prefix, owning, view) in sorted_views(def) {
        write_view_row(&mut w, &prefix, owning, view).expect(BUFFER_WRITE);
    }
    w.into_string()
}

/// Renders every reducer in the module, including those in submodules, one row each as
/// [`describe_reducer`] renders it, in the order [`describe_module`] lists them.
///
/// A module with no reducers renders as `""`.
pub fn describe_reducers(def: &ModuleDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    for (_, owning, reducer) in sorted_reducers(def) {
        write_reducer_row(&mut w, owning, reducer).expect(BUFFER_WRITE);
    }
    w.into_string()
}

/// Renders every procedure in the module, including those in submodules, one row each as
/// [`describe_procedure`] renders it, in the order [`describe_module`] lists them.
///
/// A module with no procedures renders as `""`.
pub fn describe_procedures(def: &ModuleDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    for (prefix, owning, procedure) in sorted_procedures(def) {
        write_procedure_row(&mut w, &prefix, owning, procedure).expect(BUFFER_WRITE);
    }
    w.into_string()
}

/// Renders the module's HTTP routes, one row each as [`describe_http_route`] renders it, in
/// declaration order.
///
/// A module with no HTTP routes renders as `""`.
pub fn describe_http_routes(def: &ModuleDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    for route in def.http_routes() {
        write_http_route_row(&mut w, route).expect(BUFFER_WRITE);
    }
    w.into_string()
}

/// Renders the module's environment variable declarations, one aligned row each, sorted by name.
///
/// A module with no environment variables renders as `""`.
pub fn describe_env_vars(def: &ModuleDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    write_env_var_rows(&mut w, &def.environment().declarations().collect_vec()).expect(BUFFER_WRITE);
    w.into_string()
}

/// Renders the named types [`describe_module`] lists under `Types`, one row each as
/// [`describe_type`] renders it, in the same order.
///
/// A module with no such types renders as `""`.
pub fn describe_types(def: &ModuleDef, style: PrettyPrintStyle) -> String {
    let mut w = StyledWriter::new(style, INDENT_WIDTH);
    for named in sorted_types(def) {
        write_type_row(&mut w, &named).expect(BUFFER_WRITE);
    }
    w.into_string()
}

/// Every table in the module, including those in submodules, sorted by qualified name: the order
/// in which [`describe_module`] and [`describe_tables`] list them.
pub fn sorted_tables(def: &ModuleDef) -> Vec<(NamespacePath, &ModuleDef, &TableDef)> {
    def.all_tables_with_prefix()
        .into_iter()
        .sorted_by_cached_key(|(prefix, _, table)| format!("{prefix}{}", table.name))
        .collect_vec()
}

/// Every view in the module, including those in submodules, sorted by qualified name: the order
/// in which [`describe_module`] and [`describe_views`] list them.
pub fn sorted_views(def: &ModuleDef) -> Vec<(NamespacePath, &ModuleDef, &ViewDef)> {
    def.all_views_with_prefix()
        .into_iter()
        .sorted_by_cached_key(|(prefix, _, view)| format!("{prefix}{}", view.name))
        .collect_vec()
}

/// Every reducer in the module, including those in submodules, sorted by qualified name: the order
/// in which [`describe_module`] and [`describe_reducers`] list them.
pub fn sorted_reducers(def: &ModuleDef) -> Vec<(NamespacePath, &ModuleDef, &ReducerDef)> {
    // A reducer's name is already qualified with its namespace path.
    def.all_reducers_with_prefix()
        .into_iter()
        .sorted_by_cached_key(|(_, _, reducer)| reducer.name.to_string())
        .collect_vec()
}

/// Every procedure in the module, including those in submodules, sorted by qualified name: the
/// order in which [`describe_module`] and [`describe_procedures`] list them.
pub fn sorted_procedures(def: &ModuleDef) -> Vec<(NamespacePath, &ModuleDef, &ProcedureDef)> {
    def.all_procedures_with_prefix()
        .into_iter()
        .sorted_by_cached_key(|(prefix, _, procedure)| format!("{prefix}{}", procedure.name))
        .collect_vec()
}

/// The named types [`describe_module`] and [`describe_types`] list, in the order they list them.
///
/// These are the types reachable from a column, or from a view, reducer or procedure's parameters
/// or return type, including the types those refer to in turn. Table row types are left out,
/// because each table's own block already shows its columns. [`all_named_types`] has every type.
pub fn sorted_types(def: &ModuleDef) -> Vec<NamedType<'_>> {
    let tables = def.all_tables_with_prefix();
    let views = def.all_views_with_prefix();
    let reducers = def.all_reducers_with_prefix();
    let procedures = def.all_procedures_with_prefix();

    let column_roots = tables.iter().flat_map(|&(ref prefix, owning, table)| {
        table
            .columns
            .iter()
            .map(move |col| (prefix.clone(), owning, &col.ty_for_generate))
    });
    let view_roots = views.iter().flat_map(|&(ref prefix, owning, view)| {
        param_roots(prefix.clone(), owning, &view.params_for_generate).chain([(
            prefix.clone(),
            owning,
            &view.return_type_for_generate,
        )])
    });
    let reducer_roots = reducers
        .iter()
        .flat_map(|&(ref prefix, owning, reducer)| param_roots(prefix.clone(), owning, &reducer.params_for_generate));
    let procedure_roots = procedures.iter().flat_map(|&(ref prefix, owning, procedure)| {
        param_roots(prefix.clone(), owning, &procedure.params_for_generate).chain([(
            prefix.clone(),
            owning,
            &procedure.return_type_for_generate,
        )])
    });
    reachable_types(
        column_roots
            .chain(view_roots)
            .chain(reducer_roots)
            .chain(procedure_roots),
    )
}

/// Every named type in the module and its submodules, sorted by qualified name.
///
/// Unlike [`sorted_types`], this includes table row types and types nothing refers to.
pub fn all_named_types(def: &ModuleDef) -> Vec<NamedType<'_>> {
    fn collect<'a>(prefix: &NamespacePath, owning: &'a ModuleDef, out: &mut Vec<NamedType<'a>>) {
        out.extend(owning.types().map(|type_def| NamedType::new(prefix, owning, type_def)));
        for (namespace, submodule) in owning.submodules() {
            collect(&prefix.child(namespace.clone()), submodule, out);
        }
    }

    let mut types = Vec::new();
    collect(&NamespacePath::root(), def, &mut types);
    types.sort_by(|a, b| a.qualified.cmp(&b.qualified));
    types
}

fn write_module(w: &mut StyledWriter, def: &ModuleDef) -> io::Result<()> {
    let tables = sorted_tables(def);
    let views = sorted_views(def);
    let reducers = sorted_reducers(def);
    let procedures = sorted_procedures(def);
    // Only the root module's routes are served, and route order matters, so keep declaration order.
    let http_routes = def.http_routes();
    // Only the root module declares environment variables, and the schema keeps them sorted by name.
    let env_vars = def.environment().declarations().collect_vec();
    let types = sorted_types(def);
    let row_level_security = def.row_level_security().map(|rls| &*rls.sql).sorted().collect_vec();

    let mut wrote_section = false;
    if !tables.is_empty() {
        write_section_header(w, &mut wrote_section, "Tables")?;
        write_tables(w, &tables)?;
    }
    if !views.is_empty() {
        write_section_header(w, &mut wrote_section, "Views")?;
        write_rows(w, &views, |w, (prefix, owning, view)| {
            write_view_row(w, prefix, owning, view)
        })?;
    }
    if !reducers.is_empty() {
        write_section_header(w, &mut wrote_section, "Reducers")?;
        write_rows(w, &reducers, |w, (_, owning, reducer)| {
            write_reducer_row(w, owning, reducer)
        })?;
    }
    if !procedures.is_empty() {
        write_section_header(w, &mut wrote_section, "Procedures")?;
        write_rows(w, &procedures, |w, (prefix, owning, procedure)| {
            write_procedure_row(w, prefix, owning, procedure)
        })?;
    }
    if !http_routes.is_empty() {
        write_section_header(w, &mut wrote_section, "HTTP routes")?;
        write_rows(w, http_routes, write_http_route_row)?;
    }
    if !env_vars.is_empty() {
        write_section_header(w, &mut wrote_section, "Environment variables")?;
        w.indent();
        write_env_var_rows(w, &env_vars)?;
        w.dedent();
    }
    if !types.is_empty() {
        write_section_header(w, &mut wrote_section, "Types")?;
        write_rows(w, &types, write_type_row)?;
    }
    if !row_level_security.is_empty() {
        write_section_header(w, &mut wrote_section, "Row-level security")?;
        write_rows(w, &row_level_security, |w, sql| w.write_line(sql))?;
    }
    Ok(())
}

/// The types of a function's parameters, as roots for [`reachable_types`].
///
/// The parameter list's own product type is deliberately not a root: some module languages
/// register it as a named type (`Init` for an `init` reducer), and it is not a type anyone uses.
fn param_roots<'a>(
    prefix: NamespacePath,
    owning: &'a ModuleDef,
    params: &'a ProductTypeDef,
) -> impl Iterator<Item = (NamespacePath, &'a ModuleDef, &'a AlgebraicTypeUse)> + 'a {
    params.elements.iter().map(move |(_, ty)| (prefix.clone(), owning, ty))
}

/// Writes one row per item, indented one level below the section heading.
fn write_rows<T>(
    w: &mut StyledWriter,
    items: &[T],
    mut write_row: impl FnMut(&mut StyledWriter, &T) -> io::Result<()>,
) -> io::Result<()> {
    w.indent();
    for item in items {
        write_row(w, item)?;
    }
    w.dedent();
    Ok(())
}

/// Writes a function's row: `name(param: Type, ...) -> Return  [tag] [tag]`.
///
/// The return type is omitted when `ret` is `None`, and nothing follows the closing parenthesis
/// or return type when there are no tags, so the row never ends in whitespace.
fn write_function_row(
    w: &mut StyledWriter,
    qualified: &str,
    owning: &ModuleDef,
    params: &ProductTypeDef,
    ret: Option<String>,
    tags: &[String],
) -> io::Result<()> {
    w.write_indent()?;
    w.write_colored(qualified, Some(w.colors().table_name), true)?;
    w.write_plain("(")?;
    write_params(w, owning, params)?;
    w.write_plain(")")?;
    if let Some(ret) = ret {
        w.write_plain(" -> ")?;
        w.write_colored(&ret, Some(w.colors().column_type), false)?;
    }
    if !tags.is_empty() {
        w.write_plain("  ")?;
        for (i, tag) in tags.iter().enumerate() {
            if i > 0 {
                w.write_plain(" ")?;
            }
            w.write_colored(tag, Some(w.colors().access), false)?;
        }
    }
    w.write_plain("\n")
}

/// Writes a parameter list as `name: Type, name: Type`, with each type coloured.
fn write_params(w: &mut StyledWriter, owning: &ModuleDef, params: &ProductTypeDef) -> io::Result<()> {
    for (i, (name, ty)) in params.elements.iter().enumerate() {
        if i > 0 {
            w.write_plain(", ")?;
        }
        w.write_plain(&format!("{name}: "))?;
        w.write_colored(&type_name(owning, ty).to_string(), Some(w.colors().column_type), false)?;
    }
    Ok(())
}

fn write_view_row(w: &mut StyledWriter, prefix: &NamespacePath, owning: &ModuleDef, view: &ViewDef) -> io::Result<()> {
    let ret = type_name(owning, &view.return_type_for_generate).to_string();
    let tags = [
        Some(if view.is_public { "[public]" } else { "[private]" }),
        view.is_anonymous.then_some("[anonymous]"),
    ]
    .into_iter()
    .flatten()
    .map(str::to_owned)
    .collect_vec();
    write_function_row(
        w,
        &format!("{prefix}{}", view.name),
        owning,
        &view.params_for_generate,
        Some(ret),
        &tags,
    )
}

fn write_reducer_row(w: &mut StyledWriter, owning: &ModuleDef, reducer: &ReducerDef) -> io::Result<()> {
    // Validation currently forces this to `()`, so it is always omitted in practice.
    let ret = (!reducer.ok_return_type.is_unit()).then(|| fmt_algebraic_type(&reducer.ok_return_type).to_string());
    let tags = [
        reducer
            .lifecycle
            .map(|lifecycle| format!("[lifecycle: {}]", lifecycle_name(lifecycle))),
        reducer.visibility.is_private().then(|| "[private]".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect_vec();
    write_function_row(
        w,
        reducer.name.as_ref(),
        owning,
        &reducer.params_for_generate,
        ret,
        &tags,
    )
}

fn write_procedure_row(
    w: &mut StyledWriter,
    prefix: &NamespacePath,
    owning: &ModuleDef,
    procedure: &ProcedureDef,
) -> io::Result<()> {
    let ret = (!matches!(procedure.return_type_for_generate, AlgebraicTypeUse::Unit))
        .then(|| type_name(owning, &procedure.return_type_for_generate).to_string());
    let tags = procedure
        .visibility
        .is_private()
        .then(|| "[private]".to_owned())
        .into_iter()
        .collect_vec();
    write_function_row(
        w,
        &format!("{prefix}{}", procedure.name),
        owning,
        &procedure.params_for_generate,
        ret,
        &tags,
    )
}

/// The name of a lifecycle as it appears in a reducer's `[lifecycle: ...]` tag.
fn lifecycle_name(lifecycle: Lifecycle) -> String {
    match lifecycle {
        Lifecycle::Init => "init".to_owned(),
        Lifecycle::OnConnect => "client_connected".to_owned(),
        Lifecycle::OnDisconnect => "client_disconnected".to_owned(),
        other => format!("{other:?}").to_case(Case::Snake),
    }
}

/// Writes an HTTP route's row: `METHOD path → handler`.
fn write_http_route_row(w: &mut StyledWriter, route: &HttpRouteDef) -> io::Result<()> {
    // An empty path is valid, but would otherwise be invisible.
    let path = if route.path.is_empty() { "\"\"" } else { &route.path };
    w.write_indent()?;
    w.write_plain(&format!("{} {path} → ", http_method_name(&route.method)))?;
    w.write_colored(&route.handler_name.to_string(), Some(w.colors().table_name), true)?;
    w.write_plain("\n")
}

/// The HTTP method in upper case, `ANY` for a route that matches any method, or an extension
/// method's name as written.
fn http_method_name(method: &MethodOrAny) -> String {
    match method {
        MethodOrAny::Any => "ANY".to_owned(),
        MethodOrAny::Method(method) => match method {
            HttpMethod::Get => "GET".to_owned(),
            HttpMethod::Head => "HEAD".to_owned(),
            HttpMethod::Post => "POST".to_owned(),
            HttpMethod::Put => "PUT".to_owned(),
            HttpMethod::Delete => "DELETE".to_owned(),
            HttpMethod::Connect => "CONNECT".to_owned(),
            HttpMethod::Options => "OPTIONS".to_owned(),
            HttpMethod::Trace => "TRACE".to_owned(),
            HttpMethod::Patch => "PATCH".to_owned(),
            HttpMethod::Extension(name) => name.clone(),
        },
        other => format!("{other:?}").to_uppercase(),
    }
}

/// Writes environment variable declarations as aligned rows: `NAME  type  optional`.
///
/// The type is `String` for any string, or the allowed values as quoted literals separated by `|`.
/// Optional declarations are flagged rather than wrapped in `Option<...>`, because a value is
/// always a string: the flag only says that it may be absent.
fn write_env_var_rows(w: &mut StyledWriter, declarations: &[&EnvironmentDeclaration]) -> io::Result<()> {
    let rows = declarations
        .iter()
        .map(|declaration| {
            (
                &*declaration.name,
                env_var_type_name(&declaration.ty),
                declaration.optional,
            )
        })
        .collect_vec();
    let name_width = rows.iter().map(|(name, _, _)| text_width(name)).max().unwrap_or(0) + 2;
    let type_width = rows.iter().map(|(_, ty, _)| text_width(ty)).max().unwrap_or(0) + 2;
    for (name, ty, optional) in rows {
        w.write_indent()?;
        w.write_colored(name, Some(w.colors().table_name), true)?;
        w.write_plain(&" ".repeat(name_width - text_width(name)))?;
        w.write_colored(&ty, Some(w.colors().column_type), false)?;
        if optional {
            w.write_plain(&" ".repeat(type_width - text_width(&ty)))?;
            w.write_colored("optional", Some(w.colors().access), false)?;
        }
        w.write_plain("\n")?;
    }
    Ok(())
}

/// The spelling of an environment variable's type: `String`, `"literal"`, or `"a" | "b"`.
///
/// Literals are quoted and escaped like Rust strings, so an empty string or one with spaces or
/// quotes stays unambiguous.
fn env_var_type_name(ty: &EnvVarType) -> String {
    match ty {
        EnvVarType::String => "String".to_owned(),
        EnvVarType::StringLiteral(literal) => format!("{literal:?}"),
        EnvVarType::Union(literals) => literals.iter().map(|literal| format!("{literal:?}")).join(" | "),
    }
}

/// Writes a section heading, preceded by a blank line unless it is the first section.
fn write_section_header(w: &mut StyledWriter, wrote_section: &mut bool, title: &str) -> io::Result<()> {
    if std::mem::replace(wrote_section, true) {
        // Not `write_line("")`, which would leave the indent behind as trailing whitespace.
        w.write_plain("\n")?;
    }
    w.write_colored_line(title, Some(w.colors().section_header), true)
}

/// Writes a subsection label such as `Columns:` on its own line.
fn write_subsection_label(w: &mut StyledWriter, label: &str) -> io::Result<()> {
    w.write_colored_line(label, Some(w.colors().section_header), true)
}

fn write_tables(w: &mut StyledWriter, tables: &[(NamespacePath, &ModuleDef, &TableDef)]) -> io::Result<()> {
    w.indent();
    write_table_blocks(w, tables)?;
    w.dedent();
    Ok(())
}

/// Writes each table's block at the writer's current indent, with a blank line between blocks.
fn write_table_blocks(w: &mut StyledWriter, tables: &[(NamespacePath, &ModuleDef, &TableDef)]) -> io::Result<()> {
    for (i, (prefix, owning, table)) in tables.iter().enumerate() {
        if i > 0 {
            w.write_plain("\n")?;
        }
        write_table_block(w, prefix, owning, table)?;
    }
    Ok(())
}

/// A column's row under `Columns:`, as uncoloured text so widths can be measured.
struct ColumnRow {
    name: String,
    ty: String,
    flags: String,
}

fn column_row(owning: &ModuleDef, table: &TableDef, col: &ColumnDef) -> ColumnRow {
    let is_primary_key = table.primary_key == Some(col.col_id);
    let is_unique = !is_primary_key
        && table.constraints.values().any(|constraint| {
            constraint
                .data
                .unique_columns()
                .is_some_and(|cols| cols.as_singleton() == Some(col.col_id))
        });
    let is_auto_inc = table.sequences.values().any(|seq| seq.column == col.col_id);
    let default = col.default_value.as_ref().map(|value| {
        let value = WithTypespace::new(owning.typespace(), &col.ty).with_value(value);
        format!("default: {}", value.to_satn())
    });

    let flags = [
        is_primary_key.then(|| "primary key".to_owned()),
        is_unique.then(|| "unique".to_owned()),
        is_auto_inc.then(|| "auto-increment".to_owned()),
        default,
    ]
    .into_iter()
    .flatten()
    .join(", ");

    ColumnRow {
        name: col.name.to_string(),
        ty: type_name(owning, &col.ty_for_generate).to_string(),
        flags,
    }
}

/// The name of column `col` of `table`, or its position if there is no such column.
fn column_name(table: &TableDef, col: ColId) -> String {
    table
        .get_column(col)
        .map_or_else(|| col.idx().to_string(), |col| col.name.to_string())
}

/// The width of `text` when padded with `format!("{:<width$}")`, which counts `char`s.
fn text_width(text: &str) -> usize {
    text.chars().count()
}

/// Writes `table`'s heading and subsections, starting at the writer's current indent.
fn write_table_block(
    w: &mut StyledWriter,
    prefix: &NamespacePath,
    owning: &ModuleDef,
    table: &TableDef,
) -> io::Result<()> {
    let access = match table.table_access {
        TableAccess::Public => "public",
        TableAccess::Private => "private",
    };
    let access = if table.is_event {
        format!("{access}, event")
    } else {
        access.to_owned()
    };
    w.write_indent()?;
    w.write_colored(&format!("{prefix}{}", table.name), Some(w.colors().table_name), true)?;
    w.write_plain(" (")?;
    w.write_colored(&access, Some(w.colors().access), false)?;
    w.write_plain(")\n")?;

    w.indent();

    write_subsection_label(w, "Columns:")?;
    let rows = table
        .columns
        .iter()
        .sorted_by_key(|col| col.col_id)
        .map(|col| column_row(owning, table, col))
        .collect_vec();
    let name_width = rows.iter().map(|row| text_width(&row.name)).max().unwrap_or(0) + 2;
    let type_width = rows.iter().map(|row| text_width(&row.ty)).max().unwrap_or(0) + 2;
    w.indent();
    for row in &rows {
        w.write_indent()?;
        w.write_plain(&format!("{:<name_width$}", row.name))?;
        w.write_colored(&row.ty, Some(w.colors().column_type), false)?;
        if !row.flags.is_empty() {
            w.write_plain(&" ".repeat(type_width - text_width(&row.ty)))?;
            w.write_plain(&row.flags)?;
        }
        w.write_plain("\n")?;
    }
    w.dedent();

    let multi_column_uniques = table
        .constraints
        .values()
        .filter_map(|constraint| {
            let cols = constraint.data.unique_columns().filter(|cols| cols.len() > 1)?;
            Some((&constraint.name, cols))
        })
        .sorted_by_key(|(name, _)| *name)
        .collect_vec();
    if !multi_column_uniques.is_empty() {
        write_subsection_label(w, "Unique constraints:")?;
        w.indent();
        for (_, cols) in multi_column_uniques {
            let cols = cols.iter().map(|col| column_name(table, col)).join(", ");
            w.write_line(format!("({cols})"))?;
        }
        w.dedent();
    }

    if !table.indexes.is_empty() {
        let indexes = table
            .indexes
            .values()
            .map(|index| {
                let algorithm = match &index.algorithm {
                    IndexAlgorithm::BTree(_) => "btree",
                    IndexAlgorithm::Hash(_) => "hash",
                    IndexAlgorithm::Direct(_) => "direct",
                };
                let cols = index
                    .algorithm
                    .columns()
                    .iter()
                    .map(|col| column_name(table, col))
                    .join(", ");
                (format!("{prefix}{}", index.name), format!("{algorithm} ({cols})"))
            })
            .sorted()
            .collect_vec();
        let name_width = indexes.iter().map(|(name, _)| text_width(name)).max().unwrap_or(0) + 2;
        write_subsection_label(w, "Indexes:")?;
        w.indent();
        for (name, algorithm) in indexes {
            w.write_line(format!("{name:<name_width$}{algorithm}"))?;
        }
        w.dedent();
    }

    if let Some(schedule) = &table.schedule {
        w.write_indent()?;
        w.write_colored("Schedule:", Some(w.colors().section_header), true)?;
        w.write_plain(&format!(" calls {} ", schedule.function_kind))?;
        w.write_colored(
            &format!("{prefix}{}", schedule.function_name),
            Some(w.colors().table_name),
            true,
        )?;
        w.write_plain("\n")?;
    }

    w.dedent();
    Ok(())
}

/// A named type, together with the qualified name `describe` shows for it.
pub struct NamedType<'a> {
    /// The type's scoped name joined with `.`, prefixed with the namespace path of its owning
    /// module (`lib.geo.Point`).
    pub qualified: String,
    /// The module that owns the type, against which its refs resolve.
    pub owning: &'a ModuleDef,
    /// The type's definition, whose `ty` refers into `owning`'s typespace.
    pub def: &'a TypeDef,
}

impl<'a> NamedType<'a> {
    fn new(prefix: &NamespacePath, owning: &'a ModuleDef, def: &'a TypeDef) -> Self {
        let qualified = format!("{prefix}{}", def.accessor_name.name_segments().format("."));
        Self { qualified, owning, def }
    }
}

/// The named types reachable from `roots`, including the types they refer to in turn, sorted by
/// qualified name.
///
/// Each root is a type use together with the namespace path and owning module it appears in.
/// Types that are a table's row type are skipped, because those already appear under Tables.
fn reachable_types<'a>(
    roots: impl IntoIterator<Item = (NamespacePath, &'a ModuleDef, &'a AlgebraicTypeUse)>,
) -> Vec<NamedType<'a>> {
    let mut work = roots.into_iter().collect_vec();
    let mut visited = HashSet::new();
    let mut found = Vec::new();

    while let Some((prefix, owning, ty)) = work.pop() {
        match ty {
            AlgebraicTypeUse::Array(inner) | AlgebraicTypeUse::Option(inner) => work.push((prefix, owning, inner)),
            AlgebraicTypeUse::Result { ok_ty, err_ty } => {
                work.push((prefix.clone(), owning, ok_ty));
                work.push((prefix, owning, err_ty));
            }
            AlgebraicTypeUse::Ref(r) => {
                if !visited.insert((prefix.clone(), *r)) {
                    continue;
                }
                if let Some((_, type_def)) = owning.type_def_from_ref(*r)
                    && !owning.tables().any(|table| table.product_type_ref == *r)
                {
                    found.push(NamedType::new(&prefix, owning, type_def));
                }
                match owning.typespace_for_generate().get(*r) {
                    Some(AlgebraicTypeDef::Product(product)) => {
                        work.extend(product.elements.iter().map(|(_, ty)| (prefix.clone(), owning, ty)))
                    }
                    Some(AlgebraicTypeDef::Sum(sum)) => {
                        work.extend(sum.variants.iter().map(|(_, ty)| (prefix.clone(), owning, ty)))
                    }
                    Some(AlgebraicTypeDef::PlainEnum(_)) | None => {}
                }
            }
            AlgebraicTypeUse::ScheduleAt
            | AlgebraicTypeUse::Identity
            | AlgebraicTypeUse::ConnectionId
            | AlgebraicTypeUse::Timestamp
            | AlgebraicTypeUse::TimeDuration
            | AlgebraicTypeUse::Uuid
            | AlgebraicTypeUse::Unit
            | AlgebraicTypeUse::Never
            | AlgebraicTypeUse::String
            | AlgebraicTypeUse::Primitive(_) => {}
        }
    }

    found.sort_by(|a, b| a.qualified.cmp(&b.qualified));
    found
}

/// Writes a named type's row: `Name = body`.
fn write_type_row(w: &mut StyledWriter, named: &NamedType<'_>) -> io::Result<()> {
    w.write_indent()?;
    w.write_colored(&named.qualified, Some(w.colors().table_name), true)?;
    w.write_plain(" = ")?;
    write_type_body(w, named.owning, named.def.ty)?;
    w.write_plain("\n")
}

/// The canonical names of the fields or variants of the product or sum type `r`.
///
/// `typespace_for_generate` keeps the names from the module source (`imageUrl`), while the
/// validated typespace has the canonical ones (`image_url`). Falls back to `source_names` if the
/// two disagree about the type's shape.
fn element_names<'a>(owning: &'a ModuleDef, r: AlgebraicTypeRef, source_names: Vec<&'a str>) -> Vec<&'a str> {
    let canonical: Option<Vec<&str>> = match owning.typespace().get(r) {
        Some(AlgebraicType::Product(product)) => product.elements.iter().map(|e| e.name().map(|n| &**n)).collect(),
        Some(AlgebraicType::Sum(sum)) => sum.variants.iter().map(|v| v.name().map(|n| &**n)).collect(),
        _ => None,
    };
    canonical
        .filter(|names| names.len() == source_names.len())
        .unwrap_or(source_names)
}

/// Writes the right-hand side of a `Name = ...` row in the Types section.
fn write_type_body(w: &mut StyledWriter, owning: &ModuleDef, r: AlgebraicTypeRef) -> io::Result<()> {
    match owning.typespace_for_generate().get(r) {
        Some(AlgebraicTypeDef::Product(product)) => {
            if product.elements.is_empty() {
                return w.write_plain("{}");
            }
            let names = element_names(owning, r, product.elements.iter().map(|(n, _)| &**n).collect());
            w.write_plain("{ ")?;
            for (i, (name, (_, ty))) in names.iter().zip(&product.elements).enumerate() {
                if i > 0 {
                    w.write_plain(", ")?;
                }
                w.write_plain(&format!("{name}: "))?;
                w.write_colored(&type_name(owning, ty).to_string(), Some(w.colors().column_type), false)?;
            }
            w.write_plain(" }")
        }
        Some(AlgebraicTypeDef::Sum(sum)) => {
            let names = element_names(owning, r, sum.variants.iter().map(|(n, _)| &**n).collect());
            for (i, (name, (_, ty))) in names.iter().zip(&sum.variants).enumerate() {
                if i > 0 {
                    w.write_plain(" | ")?;
                }
                w.write_plain(name)?;
                if !matches!(ty, AlgebraicTypeUse::Unit) {
                    w.write_plain("(")?;
                    w.write_colored(&type_name(owning, ty).to_string(), Some(w.colors().column_type), false)?;
                    w.write_plain(")")?;
                }
            }
            Ok(())
        }
        Some(AlgebraicTypeDef::PlainEnum(plain)) => {
            let names = element_names(owning, r, plain.variants.iter().map(|n| &**n).collect());
            w.write_plain(&names.join(" | "))
        }
        // Validation gives every named type a definition for generation, but if one is missing,
        // show its structure rather than nothing.
        None => match owning.typespace().get(r) {
            Some(ty) => w.write_colored(&fmt_algebraic_type(ty).to_string(), Some(w.colors().column_type), false),
            None => w.write_plain(&r.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifier::Identifier;
    use spacetimedb_lib::db::raw_def::v10::{
        CaseConversionPolicy, FunctionVisibility as RawFunctionVisibility, RawModuleDefV10Builder,
        RawModuleDefV10Section, RawSubmoduleV10,
    };
    use spacetimedb_lib::db::raw_def::v9::{btree, direct, hash};
    use spacetimedb_lib::{ProductType, ScheduleAt};
    use spacetimedb_sats::layout::PrimitiveType;
    use spacetimedb_sats::{AlgebraicValue, SumValue};
    use std::sync::Arc;

    fn create_module_def_v10(build_module: impl Fn(&mut RawModuleDefV10Builder)) -> ModuleDef {
        let mut builder = RawModuleDefV10Builder::new();
        build_module(&mut builder);
        builder
            .finish()
            .try_into()
            .expect("new_def should be a valid database definition")
    }

    fn empty_module() -> ModuleDef {
        create_module_def_v10(|_| {})
    }

    fn add_thumbnail(builder: &mut RawModuleDefV10Builder) -> AlgebraicTypeRef {
        let thumbnail = AlgebraicType::product([("url", AlgebraicType::String), ("width", AlgebraicType::U32)]);
        builder.add_algebraic_type([], "Thumbnail", thumbnail, true)
    }

    /// The type of parameter `param` of reducer `reducer`, as it appears in `params_for_generate`.
    fn param_ty<'a>(def: &'a ModuleDef, reducer: &str, param: &str) -> &'a AlgebraicTypeUse {
        let reducer = def.reducer(reducer).expect("reducer should exist");
        reducer
            .params_for_generate
            .elements
            .iter()
            .find(|(name, _)| &**name == param)
            .map(|(_, ty)| ty)
            .expect("param should exist")
    }

    fn name_of(def: &ModuleDef, ty: &AlgebraicTypeUse) -> String {
        type_name(def, ty).to_string()
    }

    #[test]
    fn primitives() {
        let def = empty_module();
        let cases = [
            (PrimitiveType::Bool, "Bool"),
            (PrimitiveType::I8, "I8"),
            (PrimitiveType::U8, "U8"),
            (PrimitiveType::I16, "I16"),
            (PrimitiveType::U16, "U16"),
            (PrimitiveType::I32, "I32"),
            (PrimitiveType::U32, "U32"),
            (PrimitiveType::I64, "I64"),
            (PrimitiveType::U64, "U64"),
            (PrimitiveType::I128, "I128"),
            (PrimitiveType::U128, "U128"),
            (PrimitiveType::I256, "I256"),
            (PrimitiveType::U256, "U256"),
            (PrimitiveType::F32, "F32"),
            (PrimitiveType::F64, "F64"),
        ];
        for (prim, expected) in cases {
            assert_eq!(name_of(&def, &AlgebraicTypeUse::Primitive(prim)), expected);
        }
    }

    #[test]
    fn special_and_builtin_types() {
        let def = empty_module();
        let cases = [
            (AlgebraicTypeUse::String, "String"),
            (AlgebraicTypeUse::Unit, "()"),
            (AlgebraicTypeUse::Never, "Never"),
            (AlgebraicTypeUse::ScheduleAt, "ScheduleAt"),
            (AlgebraicTypeUse::Identity, "Identity"),
            (AlgebraicTypeUse::ConnectionId, "ConnectionId"),
            (AlgebraicTypeUse::Timestamp, "Timestamp"),
            (AlgebraicTypeUse::TimeDuration, "TimeDuration"),
            (AlgebraicTypeUse::Uuid, "Uuid"),
        ];
        for (ty, expected) in cases {
            assert_eq!(name_of(&def, &ty), expected);
        }
    }

    #[test]
    fn containers() {
        let def = empty_module();
        let u32_ty = Arc::new(AlgebraicTypeUse::Primitive(PrimitiveType::U32));
        let string_ty = Arc::new(AlgebraicTypeUse::String);

        assert_eq!(name_of(&def, &AlgebraicTypeUse::Array(u32_ty.clone())), "Array<U32>");
        assert_eq!(
            name_of(&def, &AlgebraicTypeUse::Option(string_ty.clone())),
            "Option<String>"
        );
        assert_eq!(
            name_of(
                &def,
                &AlgebraicTypeUse::Result {
                    ok_ty: u32_ty,
                    err_ty: string_ty,
                }
            ),
            "Result<U32, String>"
        );
    }

    #[test]
    fn named_ref() {
        let def = create_module_def_v10(|builder| {
            let thumbnail = add_thumbnail(builder);
            builder.add_reducer(
                "set_thumb",
                ProductType::from([("thumb", AlgebraicType::Ref(thumbnail))]),
            );
        });
        assert_eq!(name_of(&def, param_ty(&def, "set_thumb", "thumb")), "Thumbnail");
    }

    #[test]
    fn nested_containers_with_ref() {
        let def = create_module_def_v10(|builder| {
            let thumbnail = add_thumbnail(builder);
            let thumbs = AlgebraicType::option(AlgebraicType::array(AlgebraicType::Ref(thumbnail)));
            builder.add_reducer("set_thumbs", ProductType::from([("thumbs", thumbs)]));
        });
        assert_eq!(
            name_of(&def, param_ty(&def, "set_thumbs", "thumbs")),
            "Option<Array<Thumbnail>>"
        );
    }

    #[test]
    fn scoped_ref() {
        let build = |policy: Option<CaseConversionPolicy>| {
            create_module_def_v10(move |builder| {
                if let Some(policy) = policy {
                    builder.set_case_conversion_policy(policy);
                }
                let point = AlgebraicType::product([("x", AlgebraicType::F32), ("y", AlgebraicType::F32)]);
                let point = builder.add_algebraic_type(["geo".into(), "shapes".into()], "Point", point, true);
                builder.add_reducer("move_to", ProductType::from([("point", AlgebraicType::Ref(point))]));
            })
        };

        // The default policy converts type names and their scope segments to PascalCase.
        let def = build(None);
        assert_eq!(name_of(&def, param_ty(&def, "move_to", "point")), "Geo.Shapes.Point");

        // With no conversion, the source names are kept, which also shows the separator is `.`.
        let def = build(Some(CaseConversionPolicy::None));
        assert_eq!(name_of(&def, param_ty(&def, "move_to", "point")), "geo.shapes.Point");
    }

    #[test]
    fn result_through_module() {
        let def = create_module_def_v10(|builder| {
            let thumbnail = add_thumbnail(builder);
            let outcome = AlgebraicType::result(AlgebraicType::U32, AlgebraicType::Ref(thumbnail));
            builder.add_reducer("try_thumb", ProductType::from([("outcome", outcome)]));
        });
        assert_eq!(
            name_of(&def, param_ty(&def, "try_thumb", "outcome")),
            "Result<U32, Thumbnail>"
        );
    }

    #[test]
    fn unresolvable_ref_falls_back() {
        // Validation gives every ref in a `ModuleDef` a name in its refmap, so the fallback from a
        // missing name to `fmt_algebraic_type` of the resolved type can't be reached through a
        // validated module. A ref that is out of range for the typespace is the reachable case:
        // it must print the raw ref rather than panic.
        let def = empty_module();
        let ty = AlgebraicTypeUse::Ref(AlgebraicTypeRef(u32::MAX));
        assert_eq!(name_of(&def, &ty), "&4294967295");
    }

    #[test]
    fn param_list() {
        let def = create_module_def_v10(|builder| {
            let thumbnail = add_thumbnail(builder);
            builder.add_reducer(
                "update",
                ProductType::from([
                    ("name", AlgebraicType::String),
                    ("count", AlgebraicType::U32),
                    (
                        "thumbs",
                        AlgebraicType::option(AlgebraicType::array(AlgebraicType::Ref(thumbnail))),
                    ),
                ]),
            );
            builder.add_reducer("tick", ProductType::unit());
        });

        let update = def.reducer("update").expect("reducer should exist");
        assert_eq!(
            super::param_list(&def, &update.params_for_generate).to_string(),
            "name: String, count: U32, thumbs: Option<Array<Thumbnail>>"
        );

        let tick = def.reducer("tick").expect("reducer should exist");
        assert_eq!(super::param_list(&def, &tick.params_for_generate).to_string(), "");
    }

    /// A module exercising every section of the describe output.
    fn describe_fixture() -> ModuleDef {
        let mut builder = RawModuleDefV10Builder::new();

        // A camelCase field, to check the canonical `image_url` is shown.
        let thumbnail = builder.add_algebraic_type(
            [],
            "Thumbnail",
            AlgebraicType::product([
                ("imageUrl", AlgebraicType::String),
                ("width", AlgebraicType::U32),
                ("height", AlgebraicType::U32),
            ]),
            true,
        );
        // Reachable only through `Shape`.
        let size = builder.add_algebraic_type(
            [],
            "Size",
            AlgebraicType::product([("width", AlgebraicType::F32), ("height", AlgebraicType::F32)]),
            true,
        );
        let shape = builder.add_algebraic_type(
            [],
            "Shape",
            AlgebraicType::sum([
                ("Point", AlgebraicType::unit()),
                ("Circle", AlgebraicType::F32),
                ("Rect", AlgebraicType::Ref(size)),
            ]),
            true,
        );
        let color = builder.add_algebraic_type(
            [],
            "Color",
            AlgebraicType::simple_enum(["Red", "Green", "Blue"].into_iter()),
            true,
        );
        // Declared but never used, so it must not be listed.
        builder.add_algebraic_type([], "Unused", AlgebraicType::product([("x", AlgebraicType::U8)]), true);
        // Reachable only through reducer `move_player`.
        let direction = builder.add_algebraic_type(
            [],
            "Direction",
            AlgebraicType::simple_enum(["North", "South", "East", "West"].into_iter()),
            true,
        );
        // Reachable only through procedure `player_stats`'s return type.
        let stats = builder.add_algebraic_type(
            [],
            "Stats",
            AlgebraicType::product([("wins", AlgebraicType::U32), ("losses", AlgebraicType::U32)]),
            true,
        );
        let schedule_at = builder.add_type::<ScheduleAt>();

        let player = builder
            .build_table_with_new_type(
                "player",
                ProductType::from([
                    ("id", AlgebraicType::U64),
                    ("name", AlgebraicType::String),
                    ("rank", AlgebraicType::U32),
                    ("nickname", AlgebraicType::option(AlgebraicType::String)),
                    ("joined_at", AlgebraicType::timestamp()),
                    ("avatar", AlgebraicType::Ref(thumbnail)),
                    ("color", AlgebraicType::Ref(color)),
                    ("level", AlgebraicType::U32),
                ]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index(btree(0), "player_id", "id")
            .with_index(hash(1), "player_name", "name")
            .with_unique_constraint(2)
            .with_index(direct(2), "player_rank", "rank")
            .with_default_column_value(6, AlgebraicValue::Sum(SumValue::new(0, ())))
            .with_default_column_value(7, AlgebraicValue::U32(1))
            .finish();

        builder
            .build_table_with_new_type(
                "match_result",
                ProductType::from([
                    ("player_id", AlgebraicType::U64),
                    ("round", AlgebraicType::U32),
                    ("scores", AlgebraicType::array(AlgebraicType::U32)),
                    ("shape", AlgebraicType::Ref(shape)),
                ]),
                true,
            )
            .with_access(TableAccess::Private)
            .with_unique_constraint([0, 1])
            .with_index(btree([0, 1]), "match_result_player_round", "player_round")
            .finish();

        builder
            .build_table_with_new_type(
                "player_joined",
                ProductType::from([("player_id", AlgebraicType::U64), ("name", AlgebraicType::String)]),
                true,
            )
            .with_event(true)
            .finish();

        let reminder = builder
            .build_table_with_new_type(
                "reminder",
                ProductType::from([
                    ("scheduled_id", AlgebraicType::U64),
                    ("scheduled_at", schedule_at),
                    ("message", AlgebraicType::String),
                ]),
                true,
            )
            .with_access(TableAccess::Private)
            .with_auto_inc_primary_key(0)
            .with_index(btree(0), "reminder_scheduled_id", "scheduled_id")
            .finish();
        builder.add_procedure(
            "send_reminder",
            ProductType::from([("job", AlgebraicType::Ref(reminder))]),
            AlgebraicType::unit(),
        );
        builder.add_schedule("reminder", 1, "send_reminder");

        builder.add_lifecycle_reducer(Lifecycle::Init, "init", ProductType::unit());
        builder.add_reducer(
            "move_player",
            ProductType::from([
                ("player_id", AlgebraicType::U64),
                ("direction", AlgebraicType::Ref(direction)),
            ]),
        );
        // Made private below, since the builder only makes lifecycle reducers private.
        builder.add_reducer("reset_ranks", ProductType::unit());
        builder.add_procedure(
            "player_stats",
            ProductType::from([("player_id", AlgebraicType::U64)]),
            AlgebraicType::option(AlgebraicType::Ref(stats)),
        );
        builder.add_view(
            "players_above_rank",
            0,
            true,
            false,
            ProductType::from([("min_rank", AlgebraicType::U32)]),
            AlgebraicType::array(AlgebraicType::Ref(player)),
        );
        builder.add_view(
            "top_player",
            0,
            true,
            true,
            ProductType::unit(),
            AlgebraicType::option(AlgebraicType::Ref(player)),
        );
        builder.add_http_handler("webhook");
        builder.add_http_handler("health");
        builder.add_http_route("webhook", MethodOrAny::Method(HttpMethod::Post), "/webhook");
        // Declared after `/webhook`, so the output shows declaration order is kept.
        builder.add_http_route("health", MethodOrAny::Any, "/health");
        builder.add_environment(vec![
            env_declaration("API_KEY", EnvVarType::String, false),
            env_declaration(
                "MODE",
                EnvVarType::Union(vec!["production".into(), "development".into()]),
                false,
            ),
            env_declaration("REGION", EnvVarType::StringLiteral("eu west".into()), true),
            env_declaration(
                "LOG_LEVEL",
                EnvVarType::Union(vec!["info".into(), "debug".into()]),
                true,
            ),
        ]);
        builder.add_row_level_security("SELECT * FROM player WHERE rank > 0");
        // Declared second but sorts first.
        builder.add_row_level_security("SELECT * FROM match_result WHERE round > 0");

        let mut lib = RawModuleDefV10Builder::new();
        lib.build_table_with_new_type(
            "session",
            ProductType::from([("id", AlgebraicType::U64), ("owner", AlgebraicType::identity())]),
            true,
        )
        .with_access(TableAccess::Private)
        .with_unique_constraint(0)
        .with_primary_key(0)
        .with_index(btree(0), "session_id", "id")
        .finish();
        // A submodule reducer, whose name must be shown qualified exactly once.
        lib.add_reducer("end_session", ProductType::from([("id", AlgebraicType::U64)]));
        // A submodule procedure, whose name must be shown qualified.
        lib.add_procedure("session_count", ProductType::unit(), AlgebraicType::U32);

        let mut raw = builder.finish();
        for section in &mut raw.sections {
            if let RawModuleDefV10Section::Reducers(reducers) = section {
                for reducer in reducers.iter_mut().filter(|r| &*r.source_name == "reset_ranks") {
                    reducer.visibility = RawFunctionVisibility::Private;
                }
            }
        }
        raw.sections
            .push(RawModuleDefV10Section::Submodules(vec![RawSubmoduleV10 {
                namespace: "lib".into(),
                module: lib.finish(),
            }]));
        raw.try_into()
            .expect("the describe fixture should be a valid module definition")
    }

    fn env_declaration(name: &str, ty: EnvVarType, optional: bool) -> EnvironmentDeclaration {
        EnvironmentDeclaration {
            name: name.into(),
            ty,
            optional,
        }
    }

    fn table_with_prefix<'a>(def: &'a ModuleDef, name: &str) -> (NamespacePath, &'a ModuleDef, &'a TableDef) {
        def.all_tables_with_prefix()
            .into_iter()
            .find(|(prefix, _, table)| format!("{prefix}{}", table.name) == name)
            .expect("table should exist")
    }

    #[test]
    fn describe_module_no_color() {
        let def = describe_fixture();
        insta::assert_snapshot!(
            "describe_module_no_color",
            describe_module(&def, PrettyPrintStyle::NoColor)
        );
    }

    #[test]
    fn describe_module_ansi() {
        let def = describe_fixture();
        insta::assert_snapshot!(
            "describe_module_ansi",
            describe_module(&def, PrettyPrintStyle::AnsiColor)
        );
    }

    #[test]
    fn describe_table_no_color() {
        let def = describe_fixture();
        let (prefix, owning, table) = table_with_prefix(&def, "player");
        insta::assert_snapshot!(
            "describe_table_no_color",
            describe_table(&prefix, owning, table, PrettyPrintStyle::NoColor)
        );
    }

    #[test]
    fn describe_reducer_no_color() {
        let def = describe_fixture();
        let (_, reducer, owning) = def.reducer_by_name_with_module("init").expect("reducer should exist");
        insta::assert_snapshot!(
            "describe_reducer_no_color",
            describe_reducer(owning, reducer, PrettyPrintStyle::NoColor)
        );
    }

    #[test]
    fn describe_reducer_qualifies_submodule_names() {
        let def = describe_fixture();
        let (_, reducer, owning) = def
            .reducer_by_name_with_module("lib.end_session")
            .expect("reducer should exist");
        assert_eq!(
            describe_reducer(owning, reducer, PrettyPrintStyle::NoColor),
            "lib.end_session(id: U64)\n"
        );
    }

    /// The body of section `title` in [`describe_module`] output, dedented back to column 0.
    fn module_section(module: &str, title: &str) -> String {
        let mut body = module
            .lines()
            .skip_while(|line| *line != title)
            .skip(1)
            .take_while(|line| line.is_empty() || line.starts_with("  "))
            .collect_vec();
        // The blank line separating this section from the next one isn't part of it.
        while body.last().is_some_and(|line| line.is_empty()) {
            body.pop();
        }
        body.iter()
            .map(|line| format!("{}\n", line.strip_prefix("  ").unwrap_or(line)))
            .collect()
    }

    #[test]
    fn describe_tables_matches_the_module_tables_section() {
        let def = describe_fixture();
        let module = describe_module(&def, PrettyPrintStyle::NoColor);
        let tables = describe_tables(&def, PrettyPrintStyle::NoColor);
        assert_eq!(tables, module_section(&module, "Tables"));
        // Submodule tables are included, under their qualified names.
        assert!(tables.starts_with("lib.session (private)\n"), "{tables}");
    }

    #[test]
    fn describe_reducers_matches_the_module_reducers_section() {
        let def = describe_fixture();
        let module = describe_module(&def, PrettyPrintStyle::NoColor);
        let reducers = describe_reducers(&def, PrettyPrintStyle::NoColor);
        assert_eq!(
            reducers,
            "init()  [lifecycle: init] [private]\n\
             lib.end_session(id: U64)\n\
             move_player(player_id: U64, direction: Direction)\n\
             reset_ranks()  [private]\n"
        );
        assert_eq!(reducers, module_section(&module, "Reducers"));
    }

    #[test]
    fn describe_procedure_qualifies_submodule_names() {
        let def = describe_fixture();
        let (prefix, owning, procedure) = def
            .all_procedures_with_prefix()
            .into_iter()
            .find(|(prefix, _, procedure)| format!("{prefix}{}", procedure.name) == "lib.session_count")
            .expect("procedure should exist");
        assert_eq!(
            describe_procedure(&prefix, owning, procedure, PrettyPrintStyle::NoColor),
            "lib.session_count() -> U32\n"
        );
    }

    #[test]
    fn describe_procedures_matches_the_module_procedures_section() {
        let def = describe_fixture();
        let module = describe_module(&def, PrettyPrintStyle::NoColor);
        let procedures = describe_procedures(&def, PrettyPrintStyle::NoColor);
        assert_eq!(
            procedures,
            "lib.session_count() -> U32\n\
             player_stats(player_id: U64) -> Option<Stats>\n\
             send_reminder(job: Reminder)  [private]\n"
        );
        assert_eq!(procedures, module_section(&module, "Procedures"));
    }

    #[test]
    fn describe_views_matches_the_module_views_section() {
        let def = describe_fixture();
        let module = describe_module(&def, PrettyPrintStyle::NoColor);
        let views = describe_views(&def, PrettyPrintStyle::NoColor);
        assert_eq!(
            views,
            "players_above_rank(min_rank: U32) -> Array<Player>  [public]\n\
             top_player() -> Option<Player>  [public] [anonymous]\n"
        );
        assert_eq!(views, module_section(&module, "Views"));
    }

    #[test]
    fn describe_view_renders_one_row() {
        let def = describe_fixture();
        let (prefix, owning, view) = sorted_views(&def)
            .into_iter()
            .find(|(_, _, view)| &*view.name == "top_player")
            .expect("view should exist");
        assert_eq!(
            describe_view(&prefix, owning, view, PrettyPrintStyle::NoColor),
            "top_player() -> Option<Player>  [public] [anonymous]\n"
        );
    }

    #[test]
    fn describe_http_routes_matches_the_module_http_routes_section() {
        let def = describe_fixture();
        let module = describe_module(&def, PrettyPrintStyle::NoColor);
        let routes = describe_http_routes(&def, PrettyPrintStyle::NoColor);
        assert_eq!(routes, "POST /webhook → webhook\nANY /health → health\n");
        assert_eq!(routes, module_section(&module, "HTTP routes"));
        assert_eq!(
            describe_http_route(&def.http_routes()[1], PrettyPrintStyle::NoColor),
            "ANY /health → health\n"
        );
    }

    #[test]
    fn describe_env_vars_matches_the_module_environment_section() {
        let def = describe_fixture();
        let module = describe_module(&def, PrettyPrintStyle::NoColor);
        let env_vars = describe_env_vars(&def, PrettyPrintStyle::NoColor);
        // Sorted by name, with union members sorted too.
        assert_eq!(
            env_vars,
            "API_KEY    String\n\
             LOG_LEVEL  \"debug\" | \"info\"              optional\n\
             MODE       \"development\" | \"production\"\n\
             REGION     \"eu west\"                     optional\n"
        );
        assert_eq!(env_vars, module_section(&module, "Environment variables"));
    }

    #[test]
    fn describe_env_var_aligns_on_its_own() {
        let def = describe_fixture();
        let region = def.environment().get("REGION").expect("declaration should exist");
        assert_eq!(
            describe_env_var(region, PrettyPrintStyle::NoColor),
            "REGION  \"eu west\"  optional\n"
        );
        let api_key = def.environment().get("API_KEY").expect("declaration should exist");
        assert_eq!(
            describe_env_var(api_key, PrettyPrintStyle::NoColor),
            "API_KEY  String\n"
        );
    }

    #[test]
    fn env_var_literals_are_quoted_and_escaped() {
        assert_eq!(env_var_type_name(&EnvVarType::StringLiteral(String::new())), "\"\"");
        assert_eq!(
            env_var_type_name(&EnvVarType::StringLiteral("say \"hi\"".into())),
            "\"say \\\"hi\\\"\""
        );
    }

    #[test]
    fn describe_types_matches_the_module_types_section() {
        let def = describe_fixture();
        let module = describe_module(&def, PrettyPrintStyle::NoColor);
        let types = describe_types(&def, PrettyPrintStyle::NoColor);
        assert_eq!(types, module_section(&module, "Types"));
        assert!(types.starts_with("Color = red | green | blue\n"), "{types}");
    }

    #[test]
    fn every_listed_type_can_be_found_by_its_listed_name() {
        let def = describe_fixture();
        let all = all_named_types(&def);
        for listed in sorted_types(&def) {
            let found = all
                .iter()
                .find(|named| named.qualified == listed.qualified)
                .unwrap_or_else(|| panic!("{} should be found", listed.qualified));
            assert!(
                std::ptr::eq(found.def, listed.def),
                "{} found the wrong type",
                listed.qualified
            );
        }
    }

    #[test]
    fn all_named_types_include_row_types_and_unused_types() {
        let def = describe_fixture();
        let all = all_named_types(&def);
        let find = |qualified: &str| {
            all.iter()
                .find(|named| named.qualified == qualified)
                .unwrap_or_else(|| panic!("{qualified} should be found"))
        };
        assert_eq!(
            describe_type(find("Unused"), PrettyPrintStyle::NoColor),
            "Unused = { x: U8 }\n"
        );

        // A submodule's row type is found under its qualified name, and only that name.
        let (_, lib, session) = table_with_prefix(&def, "lib.session");
        let (name, _) = lib
            .type_def_from_ref(session.product_type_ref)
            .expect("the row type should be named");
        let unqualified = name.name_segments().format(".").to_string();
        assert_eq!(
            describe_type(find(&format!("lib.{unqualified}")), PrettyPrintStyle::NoColor),
            format!("lib.{unqualified} = {{ id: U64, owner: Identity }}\n")
        );
        assert!(all.iter().all(|named| named.qualified != unqualified));
    }

    #[test]
    fn listings_of_an_empty_module_are_empty() {
        let def = empty_module();
        assert_eq!(describe_tables(&def, PrettyPrintStyle::NoColor), "");
        assert_eq!(describe_views(&def, PrettyPrintStyle::NoColor), "");
        assert_eq!(describe_reducers(&def, PrettyPrintStyle::NoColor), "");
        assert_eq!(describe_procedures(&def, PrettyPrintStyle::NoColor), "");
        assert_eq!(describe_http_routes(&def, PrettyPrintStyle::NoColor), "");
        assert_eq!(describe_env_vars(&def, PrettyPrintStyle::NoColor), "");
        assert_eq!(describe_types(&def, PrettyPrintStyle::NoColor), "");
    }

    #[test]
    fn function_signatures_are_type_roots() {
        let def = create_module_def_v10(|builder| {
            let direction = builder.add_algebraic_type(
                [],
                "Direction",
                AlgebraicType::simple_enum(["North", "South"].into_iter()),
                true,
            );
            let params = ProductType::from([("direction", AlgebraicType::Ref(direction))]);
            // Some module languages register a reducer's parameter list as a named type. It is
            // never referred to, so it must not be listed.
            builder.add_algebraic_type([], "Walk", AlgebraicType::Product(params.clone()), true);
            builder.add_reducer("walk", params);
        });
        let text = describe_module(&def, PrettyPrintStyle::NoColor);
        assert!(text.contains("\nTypes\n  Direction = north | south\n"), "{text}");
        assert!(!text.contains("Walk ="), "{text}");
    }

    #[test]
    fn describe_empty_module() {
        assert_eq!(describe_module(&empty_module(), PrettyPrintStyle::NoColor), "");
    }

    #[test]
    fn no_trailing_whitespace() {
        let text = describe_module(&describe_fixture(), PrettyPrintStyle::NoColor);
        for (i, line) in text.lines().enumerate() {
            assert_eq!(
                line,
                line.trim_end(),
                "line {} has trailing whitespace: {line:?}",
                i + 1
            );
        }
        assert!(text.ends_with('\n'), "output should end with a newline");
        assert!(!text.ends_with("\n\n"), "output should end with a single newline");
        assert!(
            !text.contains("\n\n\n"),
            "output should have at most one blank line in a row"
        );
    }

    #[test]
    fn reachable_types_skip_row_types_and_follow_refs() {
        let def = describe_fixture();
        let qualified = |ty: &AlgebraicTypeUse| {
            reachable_types([(NamespacePath::root(), &def, ty)])
                .into_iter()
                .map(|named| named.qualified)
                .collect_vec()
        };

        let (_, _, reminder) = table_with_prefix(&def, "reminder");
        assert!(qualified(&AlgebraicTypeUse::Ref(reminder.product_type_ref)).is_empty());

        let (_, _, match_result) = table_with_prefix(&def, "match_result");
        let shape = &match_result
            .get_column_by_name(&Identifier::for_test("shape"))
            .expect("column should exist")
            .ty_for_generate;
        assert_eq!(qualified(shape), ["Shape", "Size"]);
    }
}
