using SpacetimeDB;

public class WebSocketTests
{
    [Fact]
    public void ReceiveBufferStartsSmallAndGrowsOnlyWhenFull()
    {
        Assert.Equal(64 * 1024, WebSocket.InitialReceiveBufferSize);
        Assert.Equal(
            WebSocket.InitialReceiveBufferSize,
            WebSocket.NextReceiveBufferSize(WebSocket.InitialReceiveBufferSize, 1024));
        Assert.Equal(
            2 * WebSocket.InitialReceiveBufferSize,
            WebSocket.NextReceiveBufferSize(
                WebSocket.InitialReceiveBufferSize,
                WebSocket.InitialReceiveBufferSize));
    }

    [Fact]
    public void ReceiveBufferGrowthStopsAtMessageLimit()
    {
        Assert.Equal(
            WebSocket.MaxMessageSize,
            WebSocket.NextReceiveBufferSize(WebSocket.MaxMessageSize / 2, WebSocket.MaxMessageSize / 2));
        Assert.Equal(
            WebSocket.MaxMessageSize,
            WebSocket.NextReceiveBufferSize(WebSocket.MaxMessageSize, WebSocket.MaxMessageSize));
    }
}
