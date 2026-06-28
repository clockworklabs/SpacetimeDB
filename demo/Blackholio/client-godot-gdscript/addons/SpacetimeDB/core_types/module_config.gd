## Configuration resource for a single SpacetimeDB module.
##
## Stores the module's identity (name/alias) and codegen preferences.
## One of these is created per module and stored in [SpacetimeDBPluginConfig].
## The [member unparsed_module_schema] holds the raw JSON schema string
## returned by the SpacetimeDB server, used by the codegen pipeline to
## generate typed table classes, reducer wrappers, and BSATN bindings.
extends Resource

class_name SpacetimeDBModuleConfig

## The module's registered name on the SpacetimeDB server.
@export var name: String
## An optional short alias used for display or namespacing in the Godot editor.
@export var alias: String
@export_category("Codegen Config")
## If [code]true[/code], codegen skips scheduled (cron/timer) reducers.
@export var hide_scheduled_reducers: bool = true
## If [code]true[/code], codegen skips tables flagged as private.
@export var hide_private_tables: bool = true

## Raw JSON schema string from the server's [code]/database/schema[/code] endpoint.
## Parsed by [SpacetimeDBSchema] during code generation.
@export var unparsed_module_schema: String
