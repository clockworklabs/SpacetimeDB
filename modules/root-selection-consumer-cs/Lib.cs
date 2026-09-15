using SpacetimeDB;

public static partial class Consumer
{
    [Reducer]
    public static void ConsumerEntry(ReducerContext ctx)
    {
        if (Dependency.Answer() != 42)
        {
            throw new InvalidOperationException("Referenced assembly returned the wrong result.");
        }
        Log.Info("consumer published as root");
    }
}
