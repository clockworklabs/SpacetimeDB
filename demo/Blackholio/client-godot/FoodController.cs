using Godot;
using SpacetimeDB.Types;

public partial class FoodController : EntityController
{
    private static readonly Color[] ColorPalette =
    [
        new(119 / 255.0f, 252 / 255.0f, 173 / 255.0f),
        new(76 / 255.0f, 250 / 255.0f, 146 / 255.0f),
        new(35 / 255.0f, 246 / 255.0f, 120 / 255.0f),
        new(119 / 255.0f, 251 / 255.0f, 201 / 255.0f),
        new(76 / 255.0f, 249 / 255.0f, 184 / 255.0f),
        new(35 / 255.0f, 245 / 255.0f, 165 / 255.0f),
    ];

    public FoodController(Food food) : base(food.EntityId, ColorPalette[food.EntityId % ColorPalette.Length])
    {
        VisualStyle = CircleVisualStyle.Food;
    }
}
