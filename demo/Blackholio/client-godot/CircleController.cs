using Godot;
using SpacetimeDB.Types;

public partial class CircleController : EntityController
{
	private static readonly Color[] ColorPalette =
	[
		//Yellow
		new(175 / 255.0f, 159 / 255.0f, 49 / 255.0f),
		new(175 / 255.0f, 116 / 255.0f, 49 / 255.0f),
		//Purple
		new(112 / 255.0f, 47 / 255.0f, 252 / 255.0f),
		new(51 / 255.0f, 91 / 255.0f, 252 / 255.0f),
		//Red
		new(176 / 255.0f, 54 / 255.0f, 54 / 255.0f),
		new(176 / 255.0f, 109 / 255.0f, 54 / 255.0f),
		new(141 / 255.0f, 43 / 255.0f, 99 / 255.0f),
		//Blue
		new(2 / 255.0f, 188 / 255.0f, 250 / 255.0f),
		new(7 / 255.0f, 50 / 255.0f, 251 / 255.0f),
		new(2 / 255.0f, 28 / 255.0f, 146 / 255.0f)
	];

	private static CanvasLayer _labelLayer;
	private static CanvasLayer LabelLayer
	{
		get
		{
			if (_labelLayer == null)
			{

				if (Engine.GetMainLoop() is not SceneTree sceneTree) return null;
				var root = sceneTree.Root;
				if (root == null) return null;
				_labelLayer = new CanvasLayer { Name = "CircleLabelLayer" };
				root.AddChild(_labelLayer);
			}
			return _labelLayer;
		}
	}
	private static Control _labelRoot;
	private static Control LabelRoot
	{
		get
		{
			if (_labelRoot == null)
			{
				_labelRoot = new Control
				{
					Name = "CircleLabelRoot",
					MouseFilter = Control.MouseFilterEnum.Ignore
				};
				_labelRoot.SetAnchorsPreset(Control.LayoutPreset.FullRect);

				LabelLayer.AddChild(_labelRoot);
			}
			return _labelRoot;
		}
	}

	private Label _label;
	private Label Label
	{
		get
		{
			if (_label == null)
			{
				_label = new Label
				{
					Name = $"{Name}_Label",
					TopLevel = false,
					MouseFilter = Control.MouseFilterEnum.Ignore,
					HorizontalAlignment = HorizontalAlignment.Center
				};
				_label.AddThemeFontSizeOverride("font_size", 13);
				_label.AddThemeColorOverride("font_color", Colors.White);
				_label.AddThemeColorOverride("font_shadow_color", new Color(0, 0, 0, 0.75f));
				_label.AddThemeConstantOverride("shadow_offset_x", 1);
				_label.AddThemeConstantOverride("shadow_offset_y", 1);
				LabelRoot.AddChild(_label);
			}
			return _label;
		}
	}

	private PlayerController OwnerPlayer { get; set; }
	
	public CircleController(Circle circle, PlayerController ownerPlayer) : base(circle.EntityId, ColorPalette[circle.PlayerId % ColorPalette.Length])
	{
		OwnerPlayer = ownerPlayer;
		Label.Text = ownerPlayer.Username;
		
		ownerPlayer.OnCircleSpawned(this);
	}

	public override void _Process(double delta)
	{
		base._Process(delta);
		Label.Text = OwnerPlayer?.Username ?? "";
		UpdateScreenLabelPosition();
	}

	public override void OnDelete()
	{
		base.OnDelete();

		if (IsInstanceValid(Label))
		{
			Label.QueueFree();
		}

		OwnerPlayer?.OnCircleDeleted(this);
	}

	public override void OnConsumed()
	{
		if (IsInstanceValid(Label))
		{
			Label.QueueFree();
		}

		OwnerPlayer?.OnCircleDeleted(this);
	}

	private void UpdateScreenLabelPosition()
	{
		if (!IsInstanceValid(Label)) return;

		Label.Size = Label.GetCombinedMinimumSize();
		var screenPosition = GetGlobalTransformWithCanvas().Origin;
		var offset = new Vector2(0.0f, Radius + 8.0f);
		Label.Position = screenPosition + offset - (Label.Size / 2.0f);
	}
}
