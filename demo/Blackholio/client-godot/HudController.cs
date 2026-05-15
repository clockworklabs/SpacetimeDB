using System.Collections.Generic;
using System.Linq;
using Godot;

public partial class HudController : CanvasLayer
{
	private const int MaxLeaderboardRows = 11;

	private readonly string _defaultUsername;
	private readonly List<LeaderboardRowControls> _leaderboardRows = new();

	private Label _massLabel;
	private Label _circlesLabel;
	private Control _usernameOverlay;
	private LineEdit _usernameInput;
	private Control _deathOverlay;

	public static HudController Instance { get; private set; }

	public HudController(string defaultUsername)
	{
		_defaultUsername = defaultUsername;
		Layer = 32;
		Name = "HUD";
	}

	public override void _EnterTree()
	{
		Instance = this;
	}

	public override void _ExitTree()
	{
		if (Instance == this)
		{
			Instance = null;
		}
	}

	public override void _Ready()
	{
		var root = new Control
		{
			Name = "HUDRoot",
			MouseFilter = Control.MouseFilterEnum.Ignore
		};
		root.SetAnchorsPreset(Control.LayoutPreset.FullRect);
		AddChild(root);

		BuildStatusPanel(root);
		BuildLeaderboard(root);
		BuildUsernameChooser(root);
		BuildDeathOverlay(root);
	}

	public override void _Process(double delta)
	{
		UpdateStatus();
		UpdateLeaderboard();
	}

	public void ShowUsernameChooser(bool visible)
	{
		if (_usernameOverlay == null) return;

		_usernameOverlay.Visible = visible;
		if (visible)
		{
			_usernameInput.Text = _defaultUsername;
			_usernameInput.SelectAll();
			_usernameInput.GrabFocus();
		}
	}

	public void ShowDeathScreen(bool visible)
	{
		if (_deathOverlay == null) return;

		_deathOverlay.Visible = visible;
	}

	private void BuildStatusPanel(Control root)
	{
		var panel = CreatePanel("StatusPanel", new Color(0.025f, 0.035f, 0.07f, 0.78f));
		panel.SetAnchorsPreset(Control.LayoutPreset.TopLeft);
		panel.OffsetLeft = 16;
		panel.OffsetTop = 16;
		panel.OffsetRight = 230;
		panel.OffsetBottom = 92;
		root.AddChild(panel);

		var box = new VBoxContainer();
		box.AddThemeConstantOverride("separation", 4);
		panel.AddChild(box);

		_massLabel = CreateLabel("Mass: 0", 18, Colors.White);
		_circlesLabel = CreateLabel("Circles: 0", 14, new Color(0.78f, 0.84f, 0.94f));
		box.AddChild(_massLabel);
		box.AddChild(_circlesLabel);
	}

	private void BuildLeaderboard(Control root)
	{
		var panel = CreatePanel("Leaderboard", new Color(0.025f, 0.035f, 0.07f, 0.82f));
		panel.AnchorLeft = 1;
		panel.AnchorRight = 1;
		panel.OffsetLeft = -284;
		panel.OffsetTop = 16;
		panel.OffsetRight = -16;
		panel.OffsetBottom = 374;
		root.AddChild(panel);

		var box = new VBoxContainer();
		box.AddThemeConstantOverride("separation", 5);
		panel.AddChild(box);

		var title = CreateLabel("Leaderboard", 18, Colors.White);
		box.AddChild(title);

		for (var i = 0; i < MaxLeaderboardRows; i++)
		{
			var row = new HBoxContainer
			{
				Visible = false,
				CustomMinimumSize = new Vector2(0, 22)
			};
			row.AddThemeConstantOverride("separation", 8);

			var rank = CreateLabel("", 13, new Color(0.6f, 0.72f, 0.92f));
			rank.CustomMinimumSize = new Vector2(28, 0);
			var username = CreateLabel("", 13, Colors.White);
			username.SizeFlagsHorizontal = Control.SizeFlags.ExpandFill;
			var mass = CreateLabel("", 13, new Color(0.7f, 1.0f, 0.78f));
			mass.HorizontalAlignment = HorizontalAlignment.Right;
			mass.CustomMinimumSize = new Vector2(54, 0);

			row.AddChild(rank);
			row.AddChild(username);
			row.AddChild(mass);
			box.AddChild(row);
			_leaderboardRows.Add(new LeaderboardRowControls(row, rank, username, mass));
		}
	}

	private void BuildUsernameChooser(Control root)
	{
		_usernameOverlay = CreateModalOverlay("UsernameOverlay");
		root.AddChild(_usernameOverlay);

		var center = new CenterContainer();
		center.SetAnchorsPreset(Control.LayoutPreset.FullRect);
		_usernameOverlay.AddChild(center);

		var panel = CreatePanel("UsernamePanel", new Color(0.04f, 0.055f, 0.1f, 0.96f));
		panel.CustomMinimumSize = new Vector2(380, 188);
		center.AddChild(panel);

		var box = new VBoxContainer();
		box.AddThemeConstantOverride("separation", 12);
		panel.AddChild(box);

		var title = CreateLabel("Choose Username", 24, Colors.White);
		title.HorizontalAlignment = HorizontalAlignment.Center;
		box.AddChild(title);

		_usernameInput = new LineEdit
		{
			Text = _defaultUsername,
			PlaceholderText = "Username",
			MaxLength = 24
		};
		_usernameInput.TextSubmitted += _ => SubmitUsername();
		box.AddChild(_usernameInput);

		var button = new Button
		{
			Text = "Play"
		};
		button.Pressed += SubmitUsername;
		box.AddChild(button);
	}

	private void BuildDeathOverlay(Control root)
	{
		_deathOverlay = CreateModalOverlay("DeathOverlay");
		_deathOverlay.Visible = false;
		root.AddChild(_deathOverlay);

		var center = new CenterContainer();
		center.SetAnchorsPreset(Control.LayoutPreset.FullRect);
		_deathOverlay.AddChild(center);

		var panel = CreatePanel("DeathPanel", new Color(0.04f, 0.055f, 0.1f, 0.96f));
		panel.CustomMinimumSize = new Vector2(320, 148);
		center.AddChild(panel);

		var box = new VBoxContainer();
		box.AddThemeConstantOverride("separation", 14);
		panel.AddChild(box);

		var title = CreateLabel("Consumed", 24, Colors.White);
		title.HorizontalAlignment = HorizontalAlignment.Center;
		box.AddChild(title);

		var button = new Button
		{
			Text = "Respawn"
		};
		button.Pressed += Respawn;
		box.AddChild(button);
	}

	private void SubmitUsername()
	{
		if (!GameManager.IsConnected()) return;

		var name = _usernameInput.Text.Trim();
		if (string.IsNullOrEmpty(name))
		{
			name = "<No Name>";
		}

		GameManager.Conn.Reducers.EnterGame(name);
		ShowUsernameChooser(false);
	}

	private void Respawn()
	{
		if (!GameManager.IsConnected()) return;

		GameManager.Conn.Reducers.Respawn();
		ShowDeathScreen(false);
	}

	private void UpdateStatus()
	{
		var local = PlayerController.Local;
		var mass = local?.TotalMass() ?? 0;
		var circleCount = local?.NumberOfOwnedCircles ?? 0;
		_massLabel.Text = $"Mass: {mass}";
		_circlesLabel.Text = $"Circles: {circleCount}";
	}

	private void UpdateLeaderboard()
	{
		var players = Instantiator.PlayerControllers.Values
			.Select(player => (player, mass: player.TotalMass()))
			.Where(entry => entry.mass > 0)
			.OrderByDescending(entry => entry.mass)
			.Take(10)
			.ToList();

		var localPlayer = PlayerController.Local;
		if (localPlayer != null && localPlayer.NumberOfOwnedCircles > 0 && players.All(entry => entry.player != localPlayer))
		{
			players.Add((localPlayer, localPlayer.TotalMass()));
		}

		var rowIndex = 0;
		for (; rowIndex < players.Count && rowIndex < _leaderboardRows.Count; rowIndex++)
		{
			var row = _leaderboardRows[rowIndex];
			var player = players[rowIndex].player;
			var isLocal = player == localPlayer;
			row.Root.Visible = true;
			row.Rank.Text = $"{rowIndex + 1}.";
			row.Username.Text = player.Username;
			row.Mass.Text = players[rowIndex].mass.ToString();
			row.Username.AddThemeColorOverride("font_color", isLocal ? new Color(0.72f, 1.0f, 0.86f) : Colors.White);
		}

		for (; rowIndex < _leaderboardRows.Count; rowIndex++)
		{
			_leaderboardRows[rowIndex].Root.Visible = false;
		}
	}

	private static Control CreateModalOverlay(string name)
	{
		var overlay = new ColorRect
		{
			Name = name,
			Color = new Color(0.0f, 0.0f, 0.0f, 0.58f),
			MouseFilter = Control.MouseFilterEnum.Stop
		};
		overlay.SetAnchorsPreset(Control.LayoutPreset.FullRect);
		return overlay;
	}

	private static PanelContainer CreatePanel(string name, Color background)
	{
		var panel = new PanelContainer
		{
			Name = name,
			MouseFilter = Control.MouseFilterEnum.Stop
		};

		var style = new StyleBoxFlat
		{
			BgColor = background,
			BorderColor = new Color(0.25f, 0.46f, 0.72f, 0.55f),
			ContentMarginLeft = 14,
			ContentMarginTop = 12,
			ContentMarginRight = 14,
			ContentMarginBottom = 12
		};
		style.SetBorderWidthAll(1);
		style.SetCornerRadiusAll(8);
		panel.AddThemeStyleboxOverride("panel", style);
		return panel;
	}

	private static Label CreateLabel(string text, int fontSize, Color color)
	{
		var label = new Label
		{
			Text = text,
			ClipText = true
		};
		label.AddThemeFontSizeOverride("font_size", fontSize);
		label.AddThemeColorOverride("font_color", color);
		label.AddThemeColorOverride("font_shadow_color", new Color(0, 0, 0, 0.55f));
		label.AddThemeConstantOverride("shadow_offset_x", 1);
		label.AddThemeConstantOverride("shadow_offset_y", 1);
		return label;
	}

	private readonly record struct LeaderboardRowControls(
		HBoxContainer Root,
		Label Rank,
		Label Username,
		Label Mass
	);
}
