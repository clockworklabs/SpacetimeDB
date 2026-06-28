## Identifier for a subscription query set in the SpacetimeDB BSATN WS protocol.
##
## Wraps a [code]u32[/code] id assigned by the server when a subscription is
## registered. Used to correlate [SubscribeAppliedMessage] and
## [DatabaseUpdateData] with the originating subscribe request.
@tool
class_name QueryIdData
extends RefCounted

## BSATN type hints used by the SDK's binary serializer.
const BSATN_TYPES: Dictionary[StringName, StringName] = { &"id": &"u32" }

## The server-assigned query set id ([code]u32[/code] on the wire).
@export var id: int


func _init(p_id: int = 0):
	id = p_id
