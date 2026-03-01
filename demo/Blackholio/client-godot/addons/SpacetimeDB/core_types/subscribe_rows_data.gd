@tool
class_name SubscribeRowsData extends Resource

@export var table_id: int # u32 (TableId is likely u32)
@export var table_name: String
@export var table_rows: TableUpdateData

func _init():
    set_meta("bsatn_type_table_id", "u32")
