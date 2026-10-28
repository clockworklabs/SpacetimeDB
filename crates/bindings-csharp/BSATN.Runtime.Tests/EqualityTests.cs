namespace SpacetimeDB;

using Xunit;

public static partial class EqualityTests
{
    [Type]
    public partial struct Cell
    {
        public int X;
        public int Y;
    }

    [Fact]
    public static void GeneratedStructObjectEquality()
    {
        var cell = new Cell { X = 1, Y = 2 };
        object equal = new Cell { X = 1, Y = 2 };
        object different = new Cell { X = 1, Y = 3 };
        Cell? nullable = new Cell { X = 1, Y = 2 };
        Cell? empty = null;

        Assert.True(cell.Equals(equal));
        Assert.False(cell.Equals(different));
        Assert.False(cell.Equals((object?)null));
        Assert.False(cell.Equals(new object()));
        Assert.True(object.Equals(cell, equal));
        Assert.False(object.Equals(cell, different));
        Assert.True(object.Equals(cell, nullable));
        Assert.False(object.Equals(cell, empty));
        Assert.True(EqualityComparer<object>.Default.Equals(cell, equal));
        Assert.False(EqualityComparer<object>.Default.Equals(cell, different));
        Assert.Contains(equal, new HashSet<object> { cell });
    }
}
