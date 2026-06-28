## Abstract base class for all SpacetimeDB WebSocket protocol messages.
##
## Both [SpacetimeDBClientMessage] and [SpacetimeDBServerMessage] extend this.
## Not instantiated directly — serves as a common ancestor for type-checking
## and polymorphic handling of messages in the SDK's networking layer.
class_name SpacetimeDBMessage
extends RefCounted
