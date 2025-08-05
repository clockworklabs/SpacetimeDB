@tool
class_name Option extends Resource

@export var data: Array = [] :
    set(value):
        if value is Array:
            if value.size() > 0:
                _internal_data = value.slice(0, 1)
            else:
                _internal_data = []
        else:
            push_error("Optional data must be an Array.")
            _internal_data = []
    get():
        return _internal_data

var _internal_data: Array = []

static func some(value: Variant) -> Option:
    var result = Option.new()
    result.set_some(value)
    return result

static func none() -> Option:
    var result = Option.new()
    result.set_none()
    return result

func is_some() -> bool:
    return _internal_data.size() > 0

func is_none() -> bool:
    return _internal_data.is_empty()

func unwrap():
    if is_some():
        return _internal_data[0]
    else:
        push_error("Attempted to unwrap a None Optional value!")
        return null
        
func unwrap_or(default_value):
    if is_some():
        return _internal_data[0]
    else:
        return default_value

func unwrap_or_else(fn: Callable):
    if is_some():
        return _internal_data[0]
    else:
        return fn.call()

func expect(type: Variant.Type, err_msg: String = ""):
    if is_some():
        if typeof(_internal_data[0]) != type:
            err_msg = "Expected type %s, got %s" % [type, typeof(_internal_data[0])] if err_msg.is_empty() else err_msg
            push_error(err_msg)
            return null
        return _internal_data[0]
    else:
        err_msg = "Expected type %s, got None" % type if err_msg.is_empty() else err_msg
        push_error(err_msg)
        return null
        
func set_some(value):
    self.data = [value]
    
func set_none():
    self.data = []

func to_string() -> String:
    if is_some():
        return "Some(%s [type: %s])" % [_internal_data[0], typeof(_internal_data[0])]
    else:
        return "None"
