using Godot;

namespace SpacetimeDB.Types
{
	public partial class DbVector2
	{
		public static implicit operator Vector2(DbVector2 vec) => new(vec.X, vec.Y);
		public static implicit operator DbVector2(Vector2 vec) => new(vec.X, vec.Y);
	}
}
