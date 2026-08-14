using System;
using SpacetimeDB.EventHandling;
using Xunit;

public class EventListenersTests
{
    [Fact]
    public void BasicEventListenersDeduplicatesRemovesAndResubscribes()
    {
        var eventListeners = new BasicEventListeners<Action>();
        var callCount = 0;
        var listeners = new Action[12];

        for (var i = 0; i < listeners.Length; i++)
        {
            listeners[i] = new Listener(() => callCount++).Invoke;
            eventListeners.Add(listeners[i]);
        }

        eventListeners.Add(listeners[3]);
        Assert.Equal(listeners.Length, eventListeners.Count);

        InvokeAll(eventListeners);
        Assert.Equal(listeners.Length, callCount);

        eventListeners.Remove(listeners[3]);
        eventListeners.Remove(listeners[9]);
        eventListeners.Remove(listeners[3]);
        Assert.Equal(listeners.Length - 2, eventListeners.Count);

        callCount = 0;
        InvokeAll(eventListeners);
        Assert.Equal(listeners.Length - 2, callCount);

        eventListeners.Add(listeners[3]);
        eventListeners.Add(listeners[9]);
        Assert.Equal(listeners.Length, eventListeners.Count);
    }

    private static void InvokeAll(BasicEventListeners<Action> listeners)
    {
        for (var i = listeners.Count - 1; i >= 0; i--)
        {
            listeners[i]();
        }
    }

    private sealed class Listener
    {
        private readonly Action Callback;

        public Listener(Action callback)
        {
            Callback = callback;
        }

        public void Invoke()
        {
            Callback();
        }
    }
}
