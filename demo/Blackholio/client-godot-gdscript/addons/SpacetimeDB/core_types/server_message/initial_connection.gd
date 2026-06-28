## Server message received immediately after a WebSocket connection is established.
##
## Carries the client's [member identity], a unique [member connection_id] for
## this session, and an authentication [member token] that can be persisted
## for reconnection.
@tool
class_name IdentityTokenMessage
extends SpacetimeDBServerMessage

## BSATN type hints used by the SDK's binary deserializer.
const BSATN_TYPES: Dictionary[StringName, StringName] = { &"identity": &"identity", &"connection_id": &"connection_id", &"token": &"string" }

## The 32-byte identity of the connected client.
@export var identity: PackedByteArray
## A unique connection id for this WebSocket session.
@export var connection_id: PackedByteArray
## JWT-style token for re-authenticating on reconnect.
@export var token: String
