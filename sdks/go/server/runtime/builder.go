package runtime

import (
	"fmt"
	"reflect"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/moduledef"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// TableBuilder provides an explicit builder API for table registration
// when reflect-based schema discovery via RegisterTable is insufficient.
// This allows manual control over column types, constraints, and indexes.
type TableBuilder[T any] interface {
	WithAccess(access moduledef.TableAccess) TableBuilder[T]
	WithColumn(name string, algType types.AlgebraicType) TableBuilder[T]
	WithPrimaryKey(colIndex uint16) TableBuilder[T]
	WithAutoInc(colIndex uint16) TableBuilder[T]
	WithUnique(colIndex uint16) TableBuilder[T]
	WithBTreeIndex(colIndex uint16) TableBuilder[T]
	Build() TableRegistration
}

// NewTableBuilder creates a TableBuilder for explicit table configuration.
// T must be a struct type. The builder starts with no columns; all must be added
// manually via WithColumn.
func NewTableBuilder[T any](name string) TableBuilder[T] {
	var zero T
	t := reflect.TypeOf(zero)
	if t.Kind() == reflect.Ptr {
		t = t.Elem()
	}
	if t.Kind() != reflect.Struct {
		panic(fmt.Sprintf("runtime.NewTableBuilder: %s must be a struct type, got %v", name, t))
	}

	return &tableBuilder[T]{
		name:   name,
		goType: t,
		access: moduledef.TableAccessPublic,
	}
}

type tableBuilder[T any] struct {
	name     string
	goType   reflect.Type
	access   moduledef.TableAccess
	elements []types.ProductTypeElement
	columns  []columnMeta
}

func (b *tableBuilder[T]) WithAccess(access moduledef.TableAccess) TableBuilder[T] {
	b.access = access
	return b
}

func (b *tableBuilder[T]) WithColumn(name string, algType types.AlgebraicType) TableBuilder[T] {
	b.elements = append(b.elements, types.ProductTypeElement{
		Name:          name,
		AlgebraicType: algType,
	})
	b.columns = append(b.columns, columnMeta{})
	return b
}

func (b *tableBuilder[T]) WithPrimaryKey(colIndex uint16) TableBuilder[T] {
	if int(colIndex) < len(b.columns) {
		b.columns[colIndex].primaryKey = true
	}
	return b
}

func (b *tableBuilder[T]) WithAutoInc(colIndex uint16) TableBuilder[T] {
	if int(colIndex) < len(b.columns) {
		b.columns[colIndex].autoInc = true
	}
	return b
}

func (b *tableBuilder[T]) WithUnique(colIndex uint16) TableBuilder[T] {
	if int(colIndex) < len(b.columns) {
		b.columns[colIndex].unique = true
	}
	return b
}

func (b *tableBuilder[T]) WithBTreeIndex(colIndex uint16) TableBuilder[T] {
	if int(colIndex) < len(b.columns) {
		b.columns[colIndex].indexBTree = true
	}
	return b
}

func (b *tableBuilder[T]) Build() TableRegistration {
	schema := structSchema{
		productType: types.NewProductType(b.elements...),
		columns:     b.columns,
	}

	t := b.goType
	plan := buildStructPlan(t)
	reg := tableRegistration{
		name:   b.name,
		access: b.access,
		schema: schema,
		goType: t,
		plan:   plan,
		encodeFn: func(v any) []byte {
			return reflectEncode(v)
		},
		decodeFn: func(data []byte) (any, error) {
			return reflectDecode(t, data)
		},
		decodeReaderFn: func(r bsatn.Reader) (any, error) {
			rv := reflect.New(t).Elem()
			if err := reflectDecodeValue(r, rv); err != nil {
				return nil, err
			}
			return rv.Interface(), nil
		},
	}

	registeredTables = append(registeredTables, reg)
	return &registeredTables[len(registeredTables)-1]
}
