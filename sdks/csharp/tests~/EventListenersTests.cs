using System;
using System.Collections.Generic;
using SpacetimeDB.EventHandling;
using Xunit;

public class EventListenersTests
{
    [Fact]
    public void DelegateIndexHandlesHashCollisions()
    {
        var index = new DelegateIndex<Action, string>(0, new ConstantHashComparer<Action>());
        var listeners = new Action[12];

        for (var i = 0; i < listeners.Length; i++)
        {
            var id = i;
            listeners[i] = () => _ = id;
            Assert.True(index.Add(listeners[i], $"listener-{i}"));
        }

        Assert.False(index.Add(listeners[3], "duplicate"));
        Assert.Equal(listeners.Length, index.Count);

        Assert.True(index.Remove(listeners[3], out var removed));
        Assert.Equal("listener-3", removed);
        Assert.False(index.Remove(listeners[3], out _));

        Assert.True(index.Remove(listeners[9], out removed));
        Assert.Equal("listener-9", removed);
        Assert.Equal(listeners.Length - 2, index.Count);
    }

    private sealed class ConstantHashComparer<T> : IEqualityComparer<T>
    {
        public bool Equals(T? x, T? y) => EqualityComparer<T>.Default.Equals(x!, y!);

        public int GetHashCode(T obj) => 0;
    }
}
