namespace SpacetimeDB.Testing;

/// <summary>A fixture clock starting at the Unix epoch.</summary>
public sealed class TestClock
{
    private readonly DatabaseBackend backend;
    private Timestamp now;

    internal TestClock(DatabaseBackend backend) => this.backend = backend;

    public Timestamp Now
    {
        get
        {
            backend.CheckActive();
            return now;
        }
    }

    public void Set(Timestamp value)
    {
        backend.CheckActive();
        now = value;
    }

    public void Advance(TimeDuration duration) => Set(Now + duration);
}
