package moduledef

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

type moduleDefBuilder struct {
	typespace         types.Typespace
	typeDefs          []TypeDef
	tables            []TableDef
	reducers          []ReducerDef
	schedules         []ScheduleDef
	lifecycleReducers []LifecycleReducerDef
	rlsFilters        []string
}

func (b *moduleDefBuilder) SetTypespace(ts types.Typespace) ModuleDefBuilder {
	b.typespace = ts
	return b
}

func (b *moduleDefBuilder) AddTypeDef(def TypeDef) ModuleDefBuilder {
	b.typeDefs = append(b.typeDefs, def)
	return b
}

func (b *moduleDefBuilder) AddTable(def TableDef) ModuleDefBuilder {
	b.tables = append(b.tables, def)
	return b
}

func (b *moduleDefBuilder) AddReducer(def ReducerDef) ModuleDefBuilder {
	b.reducers = append(b.reducers, def)
	return b
}

func (b *moduleDefBuilder) AddSchedule(def ScheduleDef) ModuleDefBuilder {
	b.schedules = append(b.schedules, def)
	return b
}

func (b *moduleDefBuilder) AddLifecycleReducer(def LifecycleReducerDef) ModuleDefBuilder {
	b.lifecycleReducers = append(b.lifecycleReducers, def)
	return b
}

func (b *moduleDefBuilder) AddRowLevelSecurity(sql string) ModuleDefBuilder {
	b.rlsFilters = append(b.rlsFilters, sql)
	return b
}

func (b *moduleDefBuilder) Build() ModuleDef {
	return &moduleDef{
		typespace:         b.typespace,
		typeDefs:          b.typeDefs,
		tables:            b.tables,
		reducers:          b.reducers,
		schedules:         b.schedules,
		lifecycleReducers: b.lifecycleReducers,
		rlsFilters:        b.rlsFilters,
	}
}

type moduleDef struct {
	typespace         types.Typespace
	typeDefs          []TypeDef
	tables            []TableDef
	reducers          []ReducerDef
	schedules         []ScheduleDef
	lifecycleReducers []LifecycleReducerDef
	rlsFilters        []string
}

// WriteBsatn encodes the module definition as BSATN.
//
// Outer encoding: RawModuleDef sum type, tag 2 = V10.
// V10 content: product with one field: sections (Vec<RawModuleDefV10Section>).
// Each section is a sum type with tags 0-10.
func (m *moduleDef) WriteBsatn(w bsatn.Writer) {
	// Outer: RawModuleDef sum type, tag 2 = V10
	w.PutSumTag(2)

	// V10 is a struct with one field: sections: Vec<Section>
	// Count the non-empty sections.
	sectionCount := uint32(0)
	if m.typespace != nil {
		sectionCount++
	}
	if len(m.typeDefs) > 0 {
		sectionCount++
	}
	if len(m.tables) > 0 {
		sectionCount++
	}
	if len(m.reducers) > 0 {
		sectionCount++
	}
	if len(m.schedules) > 0 {
		sectionCount++
	}
	if len(m.lifecycleReducers) > 0 {
		sectionCount++
	}
	if len(m.rlsFilters) > 0 {
		sectionCount++
	}

	w.PutArrayLen(sectionCount)

	// Section tag 0: Typespace
	if m.typespace != nil {
		w.PutSumTag(sectionTagTypespace)
		m.typespace.WriteBsatn(w)
	}

	// Section tag 1: Types
	if len(m.typeDefs) > 0 {
		w.PutSumTag(sectionTagTypes)
		w.PutArrayLen(uint32(len(m.typeDefs)))
		for _, td := range m.typeDefs {
			td.WriteBsatn(w)
		}
	}

	// Section tag 2: Tables
	if len(m.tables) > 0 {
		w.PutSumTag(sectionTagTables)
		w.PutArrayLen(uint32(len(m.tables)))
		for _, t := range m.tables {
			t.WriteBsatn(w)
		}
	}

	// Section tag 3: Reducers
	if len(m.reducers) > 0 {
		w.PutSumTag(sectionTagReducers)
		w.PutArrayLen(uint32(len(m.reducers)))
		for _, r := range m.reducers {
			r.WriteBsatn(w)
		}
	}

	// Section tag 6: Schedules
	if len(m.schedules) > 0 {
		w.PutSumTag(sectionTagSchedules)
		w.PutArrayLen(uint32(len(m.schedules)))
		for _, s := range m.schedules {
			s.WriteBsatn(w)
		}
	}

	// Section tag 7: LifeCycleReducers
	if len(m.lifecycleReducers) > 0 {
		w.PutSumTag(sectionTagLifeCycleReducers)
		w.PutArrayLen(uint32(len(m.lifecycleReducers)))
		for _, lr := range m.lifecycleReducers {
			lr.WriteBsatn(w)
		}
	}

	// Section tag 8: RowLevelSecurity
	if len(m.rlsFilters) > 0 {
		w.PutSumTag(sectionTagRowLevelSecurity)
		w.PutArrayLen(uint32(len(m.rlsFilters)))
		for _, sql := range m.rlsFilters {
			// Each RLS filter is a product type with one field: sql (a string).
			w.PutString(sql)
		}
	}

}

// Section tag constants matching RawModuleDefV10Section enum variants.
const (
	sectionTagTypespace          = 0
	sectionTagTypes              = 1
	sectionTagTables             = 2
	sectionTagReducers           = 3
	sectionTagProcedures         = 4
	sectionTagViews              = 5
	sectionTagSchedules          = 6
	sectionTagLifeCycleReducers  = 7
	sectionTagRowLevelSecurity   = 8
	sectionTagCaseConversion     = 9
	sectionTagExplicitNames      = 10
)
