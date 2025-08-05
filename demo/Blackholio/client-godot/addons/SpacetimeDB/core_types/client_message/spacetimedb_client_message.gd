class_name SpacetimeDBClientMessage

# Client Message Variant Tags (ensure these match server/protocol)
const CALL_REDUCER      := 0x00
const SUBSCRIBE         := 0x01 # Legacy? Verify usage.
const ONEOFF_QUERY      := 0x02
const SUBSCRIBE_SINGLE  := 0x03
const SUBSCRIBE_MULTI   := 0x04
const UNSUBSCRIBE       := 0x05 # Single? Verify usage.
const UNSUBSCRIBE_MULTI := 0x06
