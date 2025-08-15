extends Button

@export var menu: Control
@export var camera: Camera2D
@export var menu_camera: Camera2D

func _ready():
	pressed.connect(func ():
		BlackholioModule.respawn()
		menu.hide()
		camera.enabled = true
		menu_camera.enabled = false
	)
	
	GameManager.died.connect(func():
		menu.show()
		camera.enabled = false
		menu_camera.enabled = true
	)
	
