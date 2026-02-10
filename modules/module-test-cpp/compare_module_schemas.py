#!/usr/bin/env python3
"""
Compare SpacetimeDB module schemas between Rust and C++ implementations.

This script compares the JSON schema outputs from:
- Rust module: rust-module-schema.json
- C++ module: cpp-module-schema.json or cpp-module-schema-latest.json

Usage:
    python3 compare_module_schemas.py
    
To regenerate schema files:
    spacetime describe module-test --json > rust-module-schema.json
    spacetime describe module-test-cpp --json > cpp-module-schema.json
"""

import json
import sys
import os

def load_schemas():
    """Load the schema files, trying both cpp-module-schema.json and cpp-module-schema-latest.json"""
    try:
        with open('rust-module-schema.json', 'r') as f:
            rust = json.load(f)
    except FileNotFoundError:
        print("Error: rust-module-schema.json not found")
        print("Generate it with: spacetime describe module-test --json > rust-module-schema.json")
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error parsing rust-module-schema.json: {e}")
        print("The file may be corrupted or incomplete. Try regenerating it.")
        sys.exit(1)
        
    # Try to load C++ schema, checking both possible filenames
    cpp = None
    for filename in ['cpp-module-schema-latest.json', 'cpp-module-schema.json']:
        if os.path.exists(filename):
            with open(filename, 'r') as f:
                cpp = json.load(f)
                print(f"Loaded C++ schema from {filename}")
                break
    
    if cpp is None:
        print("Error: No C++ schema file found")
        print("Generate it with: spacetime describe module-test-cpp --json > cpp-module-schema.json")
        sys.exit(1)
    
    return rust, cpp

def normalize_index(idx):
    """Normalize index representation for comparison"""
    name = idx.get('name', {})
    if isinstance(name, dict):
        name = name.get('some', None)
    
    accessor = idx.get('accessor_name', {})
    if isinstance(accessor, dict):
        accessor = accessor.get('some', None)
    
    algo = idx.get('algorithm', {})
    cols = []
    if isinstance(algo, dict) and 'BTree' in algo:
        cols = algo['BTree']
    
    return {'name': name, 'accessor': accessor, 'columns': cols}

def compare_tables(rust_tables, cpp_tables):
    """Compare table definitions between Rust and C++"""
    print("\n=== TABLE COMPARISON ===")
    print(f"Rust tables: {len(rust_tables)}")
    print(f"C++ tables: {len(cpp_tables)}")
    
    rust_names = set(rust_tables.keys())
    cpp_names = set(cpp_tables.keys())
    
    if rust_names != cpp_names:
        missing = rust_names - cpp_names
        extra = cpp_names - rust_names
        if missing:
            print(f"❌ Missing in C++: {missing}")
        if extra:
            print(f"❌ Extra in C++: {extra}")
    else:
        print("✅ All table names match!")
    
    return rust_names & cpp_names

def compare_indexes(rust_tables, cpp_tables, common_tables):
    """Compare index definitions for common tables"""
    print("\n=== INDEX COMPARISON ===")
    
    differences = []
    for table_name in sorted(common_tables):
        rust_table = rust_tables[table_name]
        cpp_table = cpp_tables[table_name]
        
        rust_indexes = [normalize_index(idx) for idx in rust_table.get('indexes', [])]
        cpp_indexes = [normalize_index(idx) for idx in cpp_table.get('indexes', [])]
        
        if rust_indexes != cpp_indexes:
            differences.append((table_name, rust_indexes, cpp_indexes))
    
    if not differences:
        print("✅ All table indexes match perfectly!")
    else:
        print(f"Found index differences in {len(differences)} table(s):")
        for table_name, rust_idx, cpp_idx in differences:
            print(f"\n{table_name}:")
            for r in rust_idx:
                name_str = str(r['name']) if r['name'] is not None else "None"
                accessor_str = str(r['accessor']) if r['accessor'] is not None else "None"
                print(f"  Rust:  name={name_str:<30} accessor={accessor_str:<20} cols={r['columns']}")
            for c in cpp_idx:
                name_str = str(c['name']) if c['name'] is not None else "None"
                accessor_str = str(c['accessor']) if c['accessor'] is not None else "None"
                print(f"  C++:   name={name_str:<30} accessor={accessor_str:<20} cols={c['columns']}")

def compare_reducers(rust_reducers, cpp_reducers):
    """Compare reducer definitions between Rust and C++"""
    print("\n=== REDUCER COMPARISON ===")
    
    rust_names = {r['name'] for r in rust_reducers}
    cpp_names = {r['name'] for r in cpp_reducers}
    
    print(f"Rust reducers: {len(rust_names)}")
    print(f"C++ reducers: {len(cpp_names)}")
    
    if rust_names != cpp_names:
        missing = rust_names - cpp_names
        extra = cpp_names - rust_names
        if missing:
            print(f"❌ Missing in C++: {missing}")
        if extra:
            print(f"❌ Extra in C++: {extra}")
    else:
        print("✅ All reducer names match!")

def check_testf_enum(rust, cpp):
    """Check the TestF enum specifically since it's known to have issues"""
    print("\n=== TESTF ENUM COMPARISON ===")
    
    for schema, lang in [(rust, 'Rust'), (cpp, 'C++')]:
        for reducer in schema['reducers']:
            if reducer['name'] == 'test':
                params = reducer.get('params', [])
                if len(params) >= 4:
                    param_type = params[3].get('algebraic_type', {})
                    if 'Sum' in param_type:
                        variants = param_type['Sum'].get('variants', [])
                        print(f"\n{lang} TestF enum:")
                        for v in variants:
                            name = v.get('name', 'unnamed')
                            alg_type = v.get('algebraic_type', {})
                            if 'Product' in alg_type and alg_type['Product'].get('fields'):
                                fields = alg_type['Product']['fields']
                                field_info = []
                                for f in fields:
                                    field_name = f.get('name', 'unnamed')
                                    field_info.append(field_name)
                                print(f"  - {name}: has payload with fields {field_info}")
                            else:
                                print(f"  - {name}: unit variant (no payload)")
                break

def main():
    print("=" * 60)
    print("SpacetimeDB Module Schema Comparison")
    print("Comparing Rust vs C++ module implementations")
    print("=" * 60)
    
    # Load schemas
    rust, cpp = load_schemas()
    
    # Extract tables and reducers
    rust_tables = {t['name']: t for t in rust['tables']}
    cpp_tables = {t['name']: t for t in cpp['tables']}
    rust_reducers = rust['reducers']
    cpp_reducers = cpp['reducers']
    
    # Run comparisons
    common_tables = compare_tables(rust_tables, cpp_tables)
    compare_indexes(rust_tables, cpp_tables, common_tables)
    compare_reducers(rust_reducers, cpp_reducers)
    check_testf_enum(rust, cpp)
    
    # Summary
    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    
    perfect_match = (
        len(rust_tables) == len(cpp_tables) and
        len(rust_reducers) == len(cpp_reducers) and
        set(rust_tables.keys()) == set(cpp_tables.keys()) and
        {r['name'] for r in rust_reducers} == {r['name'] for r in cpp_reducers}
    )
    
    if perfect_match:
        print("✅ Schema structure matches (tables and reducers)")
        print("⚠️  Note: There may be minor differences in:")
        print("   - Index naming conventions (especially multi-column indexes)")
        print("   - TestF enum payload support (C++ currently simplified)")
    else:
        print("❌ Schema structure has differences - review details above")

if __name__ == "__main__":
    main()