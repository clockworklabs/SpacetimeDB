extends PanelContainer

const MAX_ROWS: int = 10

@onready var rows_container: VBoxContainer = $MarginContainer/VBox/Rows

var row_labels: Array[Label] = []


func _ready() -> void:
	for i in MAX_ROWS + 1: # +1 for local player if not in top 10
		var label := Label.new()
		label.add_theme_font_size_override("font_size", 14)
		rows_container.add_child(label)
		row_labels.append(label)
		label.visible = false


func update_leaderboard(game: Node2D) -> void:
	if not game.has_method("get_leaderboard_data"):
		return

	var entries: Array[Dictionary] = game.get_leaderboard_data()

	# Hide all first
	for label: Label in row_labels:
		label.visible = false

	# Show top 10
	var shown: int = 0
	var local_shown: bool = false
	for i in mini(entries.size(), MAX_ROWS):
		var entry: Dictionary = entries[i]
		row_labels[shown].text = "%d. %s - %d" % [i + 1, entry.name, entry.mass]
		row_labels[shown].add_theme_color_override(
			"font_color",
			Color.YELLOW if entry.is_local else Color.WHITE,
		)
		row_labels[shown].visible = true
		if entry.is_local:
			local_shown = true
		shown += 1

	# Show local player at bottom if not in top 10
	if not local_shown:
		for i in entries.size():
			if entries[i].is_local:
				row_labels[shown].text = "%d. %s - %d" % [i + 1, entries[i].name, entries[i].mass]
				row_labels[shown].add_theme_color_override("font_color", Color.YELLOW)
				row_labels[shown].visible = true
				break
