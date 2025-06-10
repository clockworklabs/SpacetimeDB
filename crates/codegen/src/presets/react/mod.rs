mod content;

use std::{ops::Deref, path::Path};

use content::{import_from, import_from_types};
use convert_case::{Case, Casing};
use spacetimedb_schema::{def::{ModuleDef, TableDef}, identifier::Identifier};

use crate::{code_indenter::{CodeIndenter, Indenter}, util::{print_auto_generated_file_comment, type_ref_name}};

use super::LangPreset;

const PREFIX_FOLDER: &str = "react";
const INDENT: &str = "  ";

const CONTEXT_FILE: &str = "context";
const USE_CONTEXT_FILE: &str = "useSpacetimeContext";


pub struct React;

impl React {
  fn generate_context(&self, _: &ModuleDef) -> Vec<(String, String)> {
    vec![
      (generate_path(String::from(CONTEXT_FILE) + ".tsx"), generate_react_spacetime_context_provider()),
      (generate_path(String::from(USE_CONTEXT_FILE) + ".ts"), generate_react_spacetime_context_hooks()),
    ]
  }

  fn generate_hooks(&self, module: &ModuleDef) -> Vec<(String, String)> {
    module.tables().map(|table| (table_module_hooks_path(&table.name), generate_react_hook_for_table(module, table))).collect()
  }

  fn generate_index(&self, module: &ModuleDef) -> Vec<(String, String)> {
    vec![
      (generate_path(String::from("index.ts")), generate_index_file(module)),
    ]
  }
}

// React need typescript to run
impl LangPreset for React {
    fn generate(&self, module: &ModuleDef) -> Vec<(String, String)> {
      itertools::chain!(
        self.generate_context(module),
        self.generate_hooks(module),
        self.generate_index(module),
      ).collect()
    }
}

fn table_module_hooks_name(table_name: &Identifier) -> String {
    format!("useSpacetime{}Store.tsx", table_name.deref().to_case(Case::Pascal).to_string())
}

fn table_module_hooks_path(table_name: &Identifier) -> String {
  generate_path(  table_module_hooks_name(table_name))
}

fn generate_path(filename: String) -> String {
  Path::new(PREFIX_FOLDER).join(filename).display().to_string()
}

fn generate_index_file(module: &ModuleDef) -> String {
  let mut output = CodeIndenter::new(String::new(), INDENT);
  let out = &mut output;

  print_file_header(out);

  writeln!(out, "export * from './{}';", CONTEXT_FILE);
  writeln!(out, "export * from './{}';", USE_CONTEXT_FILE);

  for table in module.tables() {
    writeln!(out, "export * from './{}';", table_module_hooks_name(&table.name));
  }

  output.into_inner()
}

fn generate_react_spacetime_context_provider() -> String {
  let mut output = CodeIndenter::new(String::new(), INDENT);
  let out = &mut output;
  
  print_file_header(out);

  writeln!(out, "{}", import_from_types(&[
    "PropsWithChildren",
  ], "react"));
  writeln!(out, "{}", import_from_types(&[
    "DbConnectionBuilder",
    "Identity",
  ], "@clockworklabs/spacetimedb-sdk"));
  out.newline();
  writeln!(out, "{}", import_from(&[
    "createContext",
    "useCallback",
    "useEffect",
    "useMemo",
    "useState",
  ], "react"));
  out.newline();
  writeln!(out, "{}", import_from(&[
    "DbConnection",
    "ErrorContext",
    "SubscriptionEventContext",
  ], "../"));

  out.newline();
  writeln!(out, "{}", content::REACT_SPACETIME_CONTEXT_PROVIDER_TYPE);
  writeln!(out, "{}", content::REACT_SPACETIME_CONTEXT_PROVIDER);

  output.into_inner()
}

fn generate_react_spacetime_context_hooks() -> String {
  let mut output = CodeIndenter::new(String::new(), INDENT);
  let out = &mut output;
  
  print_file_header(out);

  writeln!(out, "{}", import_from(&[
    "useContext",
  ], "react"));
  out.newline();
  writeln!(out, "{}", content::import_from(&[
    "SpacetimeContext",
  ], "./context"));
  
  writeln!(out, "{}", content::REACT_SPACETIME_CONTEXT_HOOKS);

  output.into_inner()
}

fn generate_react_hook_for_table(module: &ModuleDef, table: &TableDef) -> String {
  let mut output = CodeIndenter::new(String::new(), INDENT);
  let out = &mut output;

  let table_name = table.name.deref().to_case(Case::Lower);
  let table_type: String = type_ref_name(module, table.product_type_ref);


  print_file_header(out);
  writeln!(out, "{}", import_from_types(&[
    &table_type,
  ], "../"));
  out.newline();
  writeln!(out, "{}", import_from(&[
    "useRef",
    "useCallback",
    "useSyncExternalStore",
  ], "react"));
  writeln!(out, "{}", import_from(&[
    "useSpacetimeContext",
  ], &format!("./{}", USE_CONTEXT_FILE)));
  out.newline();

  let hooks_code = content::replace_reducer_store_hooks(content::REACT_SPACETIME_REDUCER_STORE_HOOKS, &table_name, &table_type);
  
  writeln!(out, "{}", hooks_code);

  output.into_inner()
}

fn print_file_header(output: &mut Indenter) {
  print_auto_generated_file_comment(output);
}