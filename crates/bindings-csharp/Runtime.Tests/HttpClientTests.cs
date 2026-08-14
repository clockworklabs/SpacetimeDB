namespace Runtime.Tests;

using System.Reflection;
using SpacetimeDB;

public class HttpClientTests
{
    [Fact]
    public void MaxTimeoutMatchesHostLimit()
    {
        var maxTimeout = typeof(HttpClient).GetField(
            "MaxTimeout",
            BindingFlags.NonPublic | BindingFlags.Static
        );

        Assert.NotNull(maxTimeout);
        Assert.Equal(TimeSpan.FromSeconds(180), maxTimeout.GetValue(null));
    }
}
