namespace SpacetimeDB.Testing;

/// <summary>A fixture clock starting at the Unix epoch.</summary>
public sealed class TestClock
{
    private readonly DatabaseBackend _backend;
    private Timestamp _now;

    internal TestClock(DatabaseBackend backend) => this._backend = backend;

    public Timestamp Now
    {
        get
        {
            _backend.CheckActive();
            return _now;
        }
    }

    public void Set(Timestamp value)
    {
        _backend.CheckActive();
        _now = value;
    }

    public void Advance(TimeDuration duration) => Set(Now + duration);
}
