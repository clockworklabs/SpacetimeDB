class_name SpacetimeDBConnectionOptions extends Resource

const CompressionPreference = SpacetimeDBConnection.CompressionPreference

var compression: CompressionPreference = CompressionPreference.NONE
var threading: bool = true
var one_time_token: bool = true
var token: String = ""
var debug_mode: bool = false
var inbound_buffer_size: int = 1024 * 1024 * 2 # 2MB
var outbound_buffer_size: int = 1024 * 1024 * 2 # 2MB

func set_all_buffer_size(size: int):
    inbound_buffer_size = size
    outbound_buffer_size = size
