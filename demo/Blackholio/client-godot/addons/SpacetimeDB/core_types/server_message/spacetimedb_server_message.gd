class_name SpacetimeDBServerMessage

# Server Message Tags (ensure these match protocol)
const INITIAL_SUBSCRIPTION      := 0x00
const TRANSACTION_UPDATE        := 0x01
const TRANSACTION_UPDATE_LIGHT  := 0x02 # Not currently handled in parse_packet
const IDENTITY_TOKEN            := 0x03
const ONE_OFF_QUERY_RESPONSE    := 0x04
const SUBSCRIBE_APPLIED         := 0x05
const UNSUBSCRIBE_APPLIED       := 0x06
const SUBSCRIPTION_ERROR        := 0x07
const SUBSCRIBE_MULTI_APPLIED   := 0x08
const UNSUBSCRIBE_MULTI_APPLIED := 0x09

static func get_resource_path(msg_type: int) -> String:
    match msg_type:
        INITIAL_SUBSCRIPTION:      return "res://addons/SpacetimeDB/core_types/server_message/initial_subscription.gd"
        TRANSACTION_UPDATE:        return "res://addons/SpacetimeDB/core_types/server_message/transaction_update.gd"
        IDENTITY_TOKEN:            return "res://addons/SpacetimeDB/core_types/server_message/identity_token.gd"
        ONE_OFF_QUERY_RESPONSE:    return "res://addons/SpacetimeDB/core_types/server_message/one_off_query_response.gd" # IMPLEMENT READER
        SUBSCRIBE_APPLIED:         return "res://addons/SpacetimeDB/core_types/server_message/subscribe_applied.gd"
        UNSUBSCRIBE_APPLIED:       return "res://addons/SpacetimeDB/core_types/server_message/unsubscribe_applied.gd"
        SUBSCRIPTION_ERROR:        return "res://addons/SpacetimeDB/core_types/server_message/subscription_error.gd" # Uses manual reader
        SUBSCRIBE_MULTI_APPLIED:   return "res://addons/SpacetimeDB/core_types/server_message/subscribe_multi_applied.gd"
        UNSUBSCRIBE_MULTI_APPLIED: return "res://addons/SpacetimeDB/core_types/server_message/unsubscribe_multi_applied.gd"
        # TRANSACTION_UPDATE_LIGHT (0x02) is not handled yet
        _:
            return ""
