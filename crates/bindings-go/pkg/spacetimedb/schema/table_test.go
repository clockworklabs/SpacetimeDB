package schema

import (
	"encoding/json"
	"testing"
)

func TestTableInfo_Validate(t *testing.T) {
	tests := []struct {
		name    string
		table   *TableInfo
		wantErr bool
		errMsg  string
	}{
		{
			name: "valid table",
			table: &TableInfo{
				Name: "users",
				Columns: []Column{
					{Name: "id", Type: TypeU32, PrimaryKey: true},
					{Name: "name", Type: TypeString},
				},
			},
			wantErr: false,
		},
		{
			name: "empty table name",
			table: &TableInfo{
				Name: "",
				Columns: []Column{
					{Name: "id", Type: TypeU32},
				},
			},
			wantErr: true,
			errMsg:  "table name cannot be empty",
		},
		{
			name: "invalid table name",
			table: &TableInfo{
				Name: "123invalid",
				Columns: []Column{
					{Name: "id", Type: TypeU32},
				},
			},
			wantErr: true,
			errMsg:  "not a valid identifier",
		},
		{
			name: "no columns",
			table: &TableInfo{
				Name:    "empty",
				Columns: []Column{},
			},
			wantErr: true,
			errMsg:  "must have at least one column",
		},
		{
			name: "duplicate column names",
			table: &TableInfo{
				Name: "users",
				Columns: []Column{
					{Name: "id", Type: TypeU32},
					{Name: "id", Type: TypeString},
				},
			},
			wantErr: true,
			errMsg:  "duplicate column name",
		},
		{
			name: "multiple primary keys",
			table: &TableInfo{
				Name: "users",
				Columns: []Column{
					{Name: "id", Type: TypeU32, PrimaryKey: true},
					{Name: "email", Type: TypeString, PrimaryKey: true},
				},
			},
			wantErr: true,
			errMsg:  "at most one primary key",
		},
		{
			name: "invalid index",
			table: &TableInfo{
				Name: "users",
				Columns: []Column{
					{Name: "id", Type: TypeU32},
				},
				Indexes: []Index{
					{Name: "idx_missing", Type: IndexTypeBTree, Columns: []string{"missing"}},
				},
			},
			wantErr: true,
			errMsg:  "does not exist in table",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.table.Validate()
			if tt.wantErr {
				if err == nil {
					t.Errorf("expected error containing '%s', got nil", tt.errMsg)
				} else if err.Error() != "" && tt.errMsg != "" {
					// Check if error contains expected message
					if !contains(err.Error(), tt.errMsg) {
						t.Errorf("expected error containing '%s', got '%s'", tt.errMsg, err.Error())
					}
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error: %v", err)
				}
			}
		})
	}
}

func TestColumn_Validate(t *testing.T) {
	tests := []struct {
		name    string
		column  Column
		wantErr bool
		errMsg  string
	}{
		{
			name:    "valid column",
			column:  Column{Name: "id", Type: TypeU32},
			wantErr: false,
		},
		{
			name:    "empty name",
			column:  Column{Name: "", Type: TypeU32},
			wantErr: true,
			errMsg:  "column name cannot be empty",
		},
		{
			name:    "invalid name",
			column:  Column{Name: "123invalid", Type: TypeU32},
			wantErr: true,
			errMsg:  "not a valid identifier",
		},
		{
			name:    "empty type",
			column:  Column{Name: "id", Type: ""},
			wantErr: true,
			errMsg:  "column type cannot be empty",
		},
		{
			name:    "invalid type",
			column:  Column{Name: "id", Type: "invalid_type"},
			wantErr: true,
			errMsg:  "not a valid SpacetimeDB type",
		},
		{
			name:    "auto-inc without primary key",
			column:  Column{Name: "id", Type: TypeU32, AutoInc: true},
			wantErr: true,
			errMsg:  "auto-increment columns must be primary keys",
		},
		{
			name:    "auto-inc with non-integer type",
			column:  Column{Name: "id", Type: TypeString, PrimaryKey: true, AutoInc: true},
			wantErr: true,
			errMsg:  "auto-increment columns must be integer types",
		},
		{
			name:    "valid auto-inc column",
			column:  Column{Name: "id", Type: TypeU32, PrimaryKey: true, AutoInc: true},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.column.Validate()
			if tt.wantErr {
				if err == nil {
					t.Errorf("expected error containing '%s', got nil", tt.errMsg)
				} else if !contains(err.Error(), tt.errMsg) {
					t.Errorf("expected error containing '%s', got '%s'", tt.errMsg, err.Error())
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error: %v", err)
				}
			}
		})
	}
}

func TestIndex_Validate(t *testing.T) {
	availableColumns := map[string]bool{
		"id":   true,
		"name": true,
		"age":  true,
	}

	tests := []struct {
		name    string
		index   Index
		wantErr bool
		errMsg  string
	}{
		{
			name:    "valid index",
			index:   Index{Name: "idx_id", Type: IndexTypeBTree, Columns: []string{"id"}},
			wantErr: false,
		},
		{
			name:    "empty name",
			index:   Index{Name: "", Type: IndexTypeBTree, Columns: []string{"id"}},
			wantErr: true,
			errMsg:  "index name cannot be empty",
		},
		{
			name:    "invalid name",
			index:   Index{Name: "123invalid", Type: IndexTypeBTree, Columns: []string{"id"}},
			wantErr: true,
			errMsg:  "not a valid identifier",
		},
		{
			name:    "no columns",
			index:   Index{Name: "idx_empty", Type: IndexTypeBTree, Columns: []string{}},
			wantErr: true,
			errMsg:  "must have at least one column",
		},
		{
			name:    "invalid type",
			index:   Index{Name: "idx_invalid", Type: "invalid", Columns: []string{"id"}},
			wantErr: true,
			errMsg:  "index type 'invalid' is not valid",
		},
		{
			name:    "missing column",
			index:   Index{Name: "idx_missing", Type: IndexTypeBTree, Columns: []string{"missing"}},
			wantErr: true,
			errMsg:  "does not exist in table",
		},
		{
			name:    "duplicate columns",
			index:   Index{Name: "idx_dup", Type: IndexTypeBTree, Columns: []string{"id", "id"}},
			wantErr: true,
			errMsg:  "duplicate column 'id' in index",
		},
		{
			name:    "multi-column index",
			index:   Index{Name: "idx_multi", Type: IndexTypeBTree, Columns: []string{"name", "age"}},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.index.Validate(availableColumns)
			if tt.wantErr {
				if err == nil {
					t.Errorf("expected error containing '%s', got nil", tt.errMsg)
				} else if !contains(err.Error(), tt.errMsg) {
					t.Errorf("expected error containing '%s', got '%s'", tt.errMsg, err.Error())
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error: %v", err)
				}
			}
		})
	}
}

func TestTableInfo_Methods(t *testing.T) {
	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
			{Name: "name", Type: TypeString},
			{Name: "email", Type: TypeString, Unique: true},
		},
		Indexes: []Index{
			{Name: "idx_name", Type: IndexTypeBTree, Columns: []string{"name"}},
			{Name: "idx_email", Type: IndexTypeHash, Columns: []string{"email"}, Unique: true},
		},
	}

	// Test GetPrimaryKeyColumn
	pk := table.GetPrimaryKeyColumn()
	if pk == nil || pk.Name != "id" {
		t.Errorf("expected primary key column 'id', got %v", pk)
	}

	// Test GetColumn
	nameCol := table.GetColumn("name")
	if nameCol == nil || nameCol.Type != TypeString {
		t.Errorf("expected name column with type string, got %v", nameCol)
	}

	missingCol := table.GetColumn("missing")
	if missingCol != nil {
		t.Errorf("expected nil for missing column, got %v", missingCol)
	}

	// Test GetIndex
	nameIdx := table.GetIndex("idx_name")
	if nameIdx == nil || nameIdx.Type != IndexTypeBTree {
		t.Errorf("expected name index with btree type, got %v", nameIdx)
	}

	missingIdx := table.GetIndex("missing")
	if missingIdx != nil {
		t.Errorf("expected nil for missing index, got %v", missingIdx)
	}

	// Test HasColumn
	if !table.HasColumn("name") {
		t.Error("expected table to have 'name' column")
	}
	if table.HasColumn("missing") {
		t.Error("expected table not to have 'missing' column")
	}

	// Test HasIndex
	if !table.HasIndex("idx_name") {
		t.Error("expected table to have 'idx_name' index")
	}
	if table.HasIndex("missing") {
		t.Error("expected table not to have 'missing' index")
	}

	// Test counts
	if table.ColumnCount() != 3 {
		t.Errorf("expected 3 columns, got %d", table.ColumnCount())
	}
	if table.IndexCount() != 2 {
		t.Errorf("expected 2 indexes, got %d", table.IndexCount())
	}
}

func TestConstructors(t *testing.T) {
	// Test NewTableInfo
	table := NewTableInfo("test")
	if table.Name != "test" || !table.PublicRead || len(table.Columns) != 0 {
		t.Errorf("NewTableInfo failed: %+v", table)
	}

	// Test NewColumn
	col := NewColumn("name", TypeString)
	if col.Name != "name" || col.Type != TypeString {
		t.Errorf("NewColumn failed: %+v", col)
	}

	// Test NewPrimaryKeyColumn
	pkCol := NewPrimaryKeyColumn("id", TypeU32)
	if !pkCol.PrimaryKey || !pkCol.NotNull {
		t.Errorf("NewPrimaryKeyColumn failed: %+v", pkCol)
	}

	// Test NewAutoIncColumn
	autoCol := NewAutoIncColumn("id", TypeU32)
	if !autoCol.PrimaryKey || !autoCol.AutoInc || !autoCol.NotNull {
		t.Errorf("NewAutoIncColumn failed: %+v", autoCol)
	}

	// Test NewIndex
	idx := NewIndex("idx_test", IndexTypeBTree, []string{"name"})
	if idx.Name != "idx_test" || idx.Type != IndexTypeBTree {
		t.Errorf("NewIndex failed: %+v", idx)
	}

	// Test NewBTreeIndex
	btreeIdx := NewBTreeIndex("idx_btree", []string{"name"})
	if btreeIdx.Type != IndexTypeBTree {
		t.Errorf("NewBTreeIndex failed: %+v", btreeIdx)
	}

	// Test NewUniqueIndex
	uniqueIdx := NewUniqueIndex("idx_unique", IndexTypeHash, []string{"email"})
	if !uniqueIdx.Unique {
		t.Errorf("NewUniqueIndex failed: %+v", uniqueIdx)
	}
}

func TestString_Methods(t *testing.T) {
	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true, AutoInc: true},
			{Name: "name", Type: TypeString, NotNull: true},
		},
		Indexes: []Index{
			{Name: "idx_name", Type: IndexTypeBTree, Columns: []string{"name"}, Unique: true},
		},
	}

	// Test table string
	tableStr := table.String()
	if !contains(tableStr, "users") || !contains(tableStr, "columns=2") {
		t.Errorf("unexpected table string: %s", tableStr)
	}

	// Test column string
	colStr := table.Columns[0].String()
	if !contains(colStr, "id:u32") || !contains(colStr, "PK") || !contains(colStr, "AUTO") {
		t.Errorf("unexpected column string: %s", colStr)
	}

	// Test index string
	idxStr := table.Indexes[0].String()
	if !contains(idxStr, "idx_name") || !contains(idxStr, "UNIQUE") {
		t.Errorf("unexpected index string: %s", idxStr)
	}
}

func TestJSON_Serialization(t *testing.T) {
	table := &TableInfo{
		Name:       "users",
		PublicRead: true,
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
			{Name: "name", Type: TypeString},
		},
		Indexes: []Index{
			{Name: "idx_name", Type: IndexTypeBTree, Columns: []string{"name"}},
		},
	}

	// Test JSON marshaling
	data, err := json.Marshal(table)
	if err != nil {
		t.Fatalf("JSON marshal failed: %v", err)
	}

	// Verify JSON contains expected fields
	jsonStr := string(data)
	if !contains(jsonStr, "users") || !contains(jsonStr, "column_count") {
		t.Errorf("unexpected JSON: %s", jsonStr)
	}

	// Test JSON unmarshaling
	var decoded TableInfo
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("JSON unmarshal failed: %v", err)
	}

	if decoded.Name != table.Name || len(decoded.Columns) != len(table.Columns) {
		t.Errorf("JSON roundtrip failed: %+v", decoded)
	}
}

func TestValidation_Helpers(t *testing.T) {
	// Test isValidIdentifier
	tests := []struct {
		name  string
		input string
		want  bool
	}{
		{"valid", "valid_name", true},
		{"underscore", "_name", true},
		{"camelCase", "camelCase", true},
		{"empty", "", false},
		{"number start", "123invalid", false},
		{"special chars", "invalid-name", false},
		{"too long", string(make([]byte, 65)), false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := isValidIdentifier(tt.input); got != tt.want {
				t.Errorf("isValidIdentifier(%q) = %v, want %v", tt.input, got, tt.want)
			}
		})
	}

	// Test isValidType
	basicTypes := []string{TypeU32, TypeString, TypeBool, TypeIdentity}
	for _, typ := range basicTypes {
		if !isValidType(typ) {
			t.Errorf("expected %s to be valid type", typ)
		}
	}

	// Test isIntegerType
	intTypes := []string{TypeU8, TypeU32, TypeI64}
	for _, typ := range intTypes {
		if !isIntegerType(typ) {
			t.Errorf("expected %s to be integer type", typ)
		}
	}

	if isIntegerType(TypeString) {
		t.Error("expected string not to be integer type")
	}

	// Test isValidIndexType
	validIndexTypes := []IndexType{IndexTypeBTree, IndexTypeHash, IndexTypeDirect}
	for _, typ := range validIndexTypes {
		if !isValidIndexType(typ) {
			t.Errorf("expected %s to be valid index type", typ)
		}
	}

	if isValidIndexType("invalid") {
		t.Error("expected 'invalid' not to be valid index type")
	}
}

// Helper function to check if a string contains a substring
func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr ||
		(len(substr) > 0 &&
			(s[:len(substr)] == substr ||
				s[len(s)-len(substr):] == substr ||
				containsAt(s, substr))))
}

func containsAt(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

// Benchmark tests
func BenchmarkTableValidation(b *testing.B) {
	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
			{Name: "name", Type: TypeString},
			{Name: "email", Type: TypeString, Unique: true},
			{Name: "age", Type: TypeU32},
		},
		Indexes: []Index{
			{Name: "idx_name", Type: IndexTypeBTree, Columns: []string{"name"}},
			{Name: "idx_email", Type: IndexTypeHash, Columns: []string{"email"}},
		},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		table.Validate()
	}
}

func BenchmarkColumnLookup(b *testing.B) {
	table := &TableInfo{
		Name: "users",
		Columns: []Column{
			{Name: "id", Type: TypeU32, PrimaryKey: true},
			{Name: "name", Type: TypeString},
			{Name: "email", Type: TypeString},
		},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		table.GetColumn("name")
	}
}
