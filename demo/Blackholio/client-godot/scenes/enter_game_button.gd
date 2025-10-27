extends Button

@export var menu: Control
@export var camera: Camera2D
@export var menu_camera: Camera2D
@export var displayname_panel: Panel
@export var respawn_panel: Panel
@export var display_input: TextEdit

func _ready():
	pressed.connect(func ():
		if (!display_input.text):
			display_input.text = "<No Name>"
			return
		BlackholioModule.enter_game(display_input.text)
		GameManager.local_player.username = display_input.text
		menu.hide()
		camera.enabled = true
		menu_camera.enabled = false
		displayname_panel.hide()
		respawn_panel.show()
	)
	
