using System;
using Godot;

public enum CircleVisualStyle
{
	Player,
	Food
}

public abstract partial class Circle2D : Node2D
{
	private float _radius = 10.0f;
	[Export]
	public float Radius
	{
		get => _radius;
		set
		{
			if (Mathf.IsEqualApprox(_radius, value)) return;

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
			if (_color == value) return;

			_color = value;
			QueueRedraw();
		}
	}

	[Export]
	public CircleVisualStyle VisualStyle { get; set; } = CircleVisualStyle.Player;

	[Export]
	public float AnimationSeed { get; set; }
	
	public override void _Draw()
	{
		if (Radius <= 0.01f) return;

		switch (VisualStyle)
		{
			case CircleVisualStyle.Player:
				DrawPlayerCircle();
				break;
			case CircleVisualStyle.Food:
				DrawFood();
				break;
			default:
				throw new ArgumentOutOfRangeException();
		}
	}

	protected void RedrawAnimatedVisuals() => QueueRedraw();

	private void DrawPlayerCircle()
	{
		var time = Time.GetTicksMsec() / 1000.0f;
		var pulse = 0.5f + 0.5f * Mathf.Sin(time * 2.2f + AnimationSeed);
		DrawCircle(Vector2.Zero, Radius * (1.16f + pulse * 0.04f), WithAlpha(Color, 0.14f));
		DrawCircle(Vector2.Zero, Radius, Shade(Color, 0.58f));
		DrawCircle(Vector2.Zero, Radius * 0.82f, Color);
		DrawCircle(new Vector2(-Radius * 0.22f, -Radius * 0.24f), Radius * 0.34f, WithAlpha(Shade(Color, 1.42f), 0.72f));

		var outline = new Vector2[73];
		for (var i = 0; i < outline.Length; i++)
		{
			var angle = Mathf.Tau * i / (outline.Length - 1);
			var wave = Mathf.Sin(angle * 7.0f + time * 3.0f + AnimationSeed) * 0.035f;
			outline[i] = Vector2.FromAngle(angle) * Radius * (1.015f + wave);
		}

		DrawPolyline(outline, WithAlpha(Shade(Color, 1.55f), 0.88f), Mathf.Clamp(Radius * 0.085f, 1.5f, 5.0f), true);
	}

	private void DrawFood()
	{
		var time = Time.GetTicksMsec() / 1000.0f;
		var pulse = 0.5f + 0.5f * Mathf.Sin(time * 5.0f + AnimationSeed);
		DrawCircle(Vector2.Zero, Radius * (1.32f + pulse * 0.09f), WithAlpha(Color, 0.1f));
		DrawCircle(Vector2.Zero, Radius, Shade(Color, 0.72f));
		DrawCircle(Vector2.Zero, Radius * 0.64f, Color);
		DrawCircle(Vector2.Zero, Radius * 0.24f, WithAlpha(Shade(Color, 1.55f), 0.86f));
	}

	private static Color Shade(Color color, float multiplier) => new Color(
		Mathf.Clamp(color.R * multiplier, 0.0f, 1.0f),
		Mathf.Clamp(color.G * multiplier, 0.0f, 1.0f),
		Mathf.Clamp(color.B * multiplier, 0.0f, 1.0f),
		color.A
	);

	private static Color WithAlpha(Color color, float alpha) => new(color.R, color.G, color.B, alpha);
}
