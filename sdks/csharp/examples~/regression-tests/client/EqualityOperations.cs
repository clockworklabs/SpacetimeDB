using SpacetimeDB.Types;

public static class EqualityOperations
{
    public static bool Equals(this Player lh, Player rh)
    {
        return lh.Id == rh.Id &&
               lh.Identity == rh.Identity &&
               lh.Name == rh.Name;
    }

    public static bool Equals(this PlayerLevel lh, PlayerLevel rh)
    {
        return lh.Level == rh.Level &&
               lh.PlayerId == rh.PlayerId;
    }

    public static bool Equals(this PlayerAndLevel lh, PlayerAndLevel rh)
    {
        return lh.Id == rh.Id &&
               lh.Identity == rh.Identity &&
               lh.Name == rh.Name &&
               lh.Level == rh.Level;
    }
}