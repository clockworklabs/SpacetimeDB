namespace SpacetimeDB.Testing;

/// <summary>Seeds independent context RNGs, starting with root seed zero.</summary>
public sealed class TestRng
{
    private readonly DatabaseBackend _backend;
    private Random _source = new(0);

    internal TestRng(DatabaseBackend backend) => this._backend = backend;

    public void SetSeed(int seed)
    {
        _backend.CheckActive();
        _source = new Random(seed);
    }

    internal Random CreateRandom()
    {
        _backend.CheckActive();
        return new Random(_source.Next());
    }
}
