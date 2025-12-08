#!/usr/bin/env python3
import json
from collections import Counter

# Load both schemas
with open('rust-module-schema.json', 'r') as f:
    rust_schema = json.load(f)

with open('cpp-module-schema.json', 'r') as f:
    cpp_schema = json.load(f)

# Count actual tables
rust_tables = rust_schema.get('tables', [])
cpp_tables = cpp_schema.get('tables', [])

print(f'Actual table counts:')
print(f'  Rust: {len(rust_tables)} tables')
print(f'  C++: {len(cpp_tables)} tables')

# Count all duplicate table entries (tables can appear multiple times for different accessors)
rust_table_names = [t['name'] for t in rust_tables]
cpp_table_names = [t['name'] for t in cpp_tables]

rust_counts = Counter(rust_table_names)
cpp_counts = Counter(cpp_table_names)

print(f'\nTotal table entries (including duplicates):')
print(f'  Rust: {sum(rust_counts.values())} entries')
print(f'  C++: {sum(cpp_counts.values())} entries')

# Check for tables that appear different number of times
print(f'\nTables with different occurrence counts:')
all_tables = set(rust_counts.keys()) | set(cpp_counts.keys())
diff_found = False
for table in sorted(all_tables):
    rust_count = rust_counts.get(table, 0)
    cpp_count = cpp_counts.get(table, 0)
    if rust_count != cpp_count:
        print(f'  {table}: Rust={rust_count}, C++={cpp_count}, diff={cpp_count-rust_count}')
        diff_found = True

if not diff_found:
    print('  None - all tables appear the same number of times')

# Check unique table names
print(f'\nUnique table names:')
print(f'  Rust: {len(set(rust_table_names))} unique tables')
print(f'  C++: {len(set(cpp_table_names))} unique tables')

# Check if table counts suggest duplicates
print(f'\nTables appearing more than once:')
for table, count in rust_counts.items():
    if count > 1:
        print(f'  Rust: {table} appears {count} times')
        
for table, count in cpp_counts.items():
    if count > 1:
        print(f'  C++: {table} appears {count} times')