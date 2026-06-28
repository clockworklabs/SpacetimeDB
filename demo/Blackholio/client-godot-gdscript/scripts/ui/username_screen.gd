extends CenterContainer

@onready var name_input: LineEdit = $VBox/NameInput
@onready var play_button: Button = $VBox/PlayButton


func _ready() -> void:
	play_button.pressed.connect(_on_play)
	name_input.text_submitted.connect(func(_text: String) -> void: _on_play())


func _on_play() -> void:
	var game: Node2D = get_tree().current_scene
	if game.has_method("on_enter_game"):
		game.on_enter_game(name_input.text)
