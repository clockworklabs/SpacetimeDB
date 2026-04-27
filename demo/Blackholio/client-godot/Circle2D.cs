using Godot;

[Tool]
public partial class Circle2D : Node2D
{
	private float _radius = 10.0f;
	[Export]
	public float Radius
	{
		get => _radius;
		set
		{
			if (Mathf.IsEqualApprox(_radius, value))
			{
				return;
			}

			_radius = value;
			QueueRedraw();
		}
	}

	private Color _color = Colors.Brown;
	[Export]
	public Color Color
	{
		get => _color;
		set
		{
			if (_color == value)
			{
				return;
			}

			_color = value;
			QueueRedraw();
		}
	}
	
	public override void _Draw() => DrawCircle(Vector2.Zero, Radius, Color);
}
