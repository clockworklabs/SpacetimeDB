namespace SpacetimeDB.Testing;

/// <summary>Seeds independent context RNGs, starting with root seed zero.</summary>
public sealed class TestRng
{
    private readonly DatabaseBackend backend;
    private Random source = new(0);

    internal TestRng(DatabaseBackend backend) => this.backend = backend;

    public void SetSeed(int seed)
    {
        backend.CheckActive();
        source = new Random(seed);
    }

    internal Random CreateRandom()
    {
        backend.CheckActive();
        return new Random(source.Next());
    }
}
