using System.Collections.Generic;
using Godot;

public partial class StarfieldBackground : Node2D
{
	private readonly float _worldSize;
	private readonly Color _backgroundColor;
	private readonly List<Star> _stars = new();
	private float _time;

	public StarfieldBackground(float worldSize, Color backgroundColor)
	{
		_worldSize = worldSize;
		_backgroundColor = backgroundColor;
		Name = "Starfield";
		ZIndex = -1000;
		GenerateStars();
	}

	public override void _Process(double delta)
	{
		_time += (float)delta;
		QueueRedraw();
	}

	public override void _Draw()
	{
		DrawRect(new Rect2(Vector2.Zero, new Vector2(_worldSize, _worldSize)), _backgroundColor);

		DrawCircle(new Vector2(_worldSize * 0.22f, _worldSize * 0.68f), _worldSize * 0.18f, new Color(0.15f, 0.32f, 0.58f, 0.08f));
		DrawCircle(new Vector2(_worldSize * 0.76f, _worldSize * 0.24f), _worldSize * 0.22f, new Color(0.45f, 0.16f, 0.52f, 0.07f));
		DrawCircle(new Vector2(_worldSize * 0.54f, _worldSize * 0.55f), _worldSize * 0.26f, new Color(0.0f, 0.62f, 0.72f, 0.045f));

		foreach (var star in _stars)
		{
			var pulse = 0.68f + 0.22f * Mathf.Sin(_time * star.TwinkleSpeed + star.Phase);
			var color = WithAlpha(star.Color, star.Color.A * pulse);
			DrawCircle(star.Position, star.Radius * (0.9f + pulse * 0.1f), color);
		}
	}

	private void GenerateStars()
	{
		var rng = new RandomNumberGenerator
		{
			Seed = 0xB1AC40E10
		};

		var count = Mathf.RoundToInt(_worldSize * 0.55f);
		for (var i = 0; i < count; i++)
		{
			var warmth = rng.RandfRange(0.0f, 1.0f);
			_stars.Add(new Star
			{
				Position = new Vector2(rng.RandfRange(0, _worldSize), rng.RandfRange(0, _worldSize)),
				Radius = rng.RandfRange(0.35f, 1.15f),
				Phase = rng.RandfRange(0, Mathf.Tau),
				TwinkleSpeed = rng.RandfRange(0.7f, 1.9f),
				Color = new Color(
					Mathf.Lerp(0.50f, 0.78f, warmth),
					Mathf.Lerp(0.56f, 0.78f, warmth),
					Mathf.Lerp(0.76f, 0.94f, warmth),
					rng.RandfRange(0.16f, 0.42f)
				)
			});
		}
	}

	private static Color WithAlpha(Color color, float alpha) => new(color.R, color.G, color.B, alpha);

	private struct Star
	{
		public Vector2 Position;
		public float Radius;
		public float Phase;
		public float TwinkleSpeed;
		public Color Color;
	}
}
