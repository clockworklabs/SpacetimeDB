package moduledef

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// ModuleDefBuilder builds a RawModuleDefV10 for __describe_module__.
type ModuleDefBuilder interface {
	SetTypespace(ts types.Typespace) ModuleDefBuilder
	AddTypeDef(def TypeDef) ModuleDefBuilder
	AddTable(def TableDef) ModuleDefBuilder
	AddReducer(def ReducerDef) ModuleDefBuilder
	AddSchedule(def ScheduleDef) ModuleDefBuilder
	AddLifecycleReducer(def LifecycleReducerDef) ModuleDefBuilder
	AddRowLevelSecurity(sql string) ModuleDefBuilder
	Build() ModuleDef
}

// ModuleDef is a built module definition ready for BSATN encoding.
type ModuleDef interface {
	bsatn.Serializable
}

// NewModuleDefBuilder creates a new ModuleDefBuilder.
func NewModuleDefBuilder() ModuleDefBuilder {
	return &moduleDefBuilder{}
}
