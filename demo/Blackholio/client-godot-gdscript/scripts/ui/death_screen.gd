extends CenterContainer

@onready var respawn_button: Button = $VBox/RespawnButton


func _ready() -> void:
	respawn_button.pressed.connect(_on_respawn)


func _on_respawn() -> void:
	var game: Node2D = get_tree().current_scene
	if game.has_method("on_respawn"):
		game.on_respawn()
