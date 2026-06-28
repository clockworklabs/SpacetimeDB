## Abstract base class for all generated SpacetimeDB table row types.
##
## Every codegen'd table row type (e.g. [code]WorldPawnStatsRow[/code]) extends
## this class. The [code]_ModuleTable[/code] and [code]LocalDatabase[/code]
## store and return rows typed as [_ModuleTableType].
class_name _ModuleTableType
extends Resource
