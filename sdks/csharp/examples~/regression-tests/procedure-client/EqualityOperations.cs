using SpacetimeDB.Types;

public static class EqualityOperations
{
    public static bool Equals(this MyTable lh, MyTable rh)
    {
        return lh.Field.A == rh.Field.A &&
               lh.Field.B == rh.Field.B;
    }
}