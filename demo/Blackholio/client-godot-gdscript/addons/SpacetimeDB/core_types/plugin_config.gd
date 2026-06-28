## Top-level configuration resource for the SpacetimeDB Godot plugin.
##
## Persisted as a [code].tres[/code] file in the addon folder. Stores the server
## URI, the name used when registering the autoload singleton, and a dictionary
## of per-module configurations keyed by module name.
extends Resource

class_name SpacetimeDBPluginConfig

## Name of the autoload singleton registered in Project Settings.
@export var autoload_name: String = "SpacetimeDB"
## Base URI of the SpacetimeDB server (e.g. [code]http://127.0.0.1:3000[/code]).
@export var uri: String = "http://127.0.0.1:3000"
## Per-module configurations, keyed by module name.
@export var module_configs: Dictionary[String, SpacetimeDBModuleConfig] = { }
