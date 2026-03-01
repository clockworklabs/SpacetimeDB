@tool
class_name UpdateStatusData extends Resource

enum StatusType {
    COMMITTED,
    FAILED,
    OUT_OF_ENERGY
}

@export var status_type: StatusType = StatusType.COMMITTED
# Only valid if status_type is COMMITTED
@export var committed_update: DatabaseUpdateData
# Only valid if status_type is FAILED
@export var failure_message: String = ""
