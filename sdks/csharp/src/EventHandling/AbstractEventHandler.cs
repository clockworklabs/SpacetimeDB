using System;

namespace SpacetimeDB.EventHandling
{
    internal class AbstractEventHandler
    {
        private EventListeners<Action> Listeners { get; } = new();

        public void Invoke()
        {
            for (var i = Listeners.Count - 1; i >= 0; i--)
            {
                Listeners[i]?.Invoke();
            }
        }
        
        public void AddListener(Action listener) => Listeners.Add(listener);

        public void RemoveListener(Action listener) => Listeners.Remove(listener);
    }
    internal class AbstractEventHandler<T>
    {
        private EventListeners<Action<T>> Listeners { get; } = new();

        public void Invoke(T value)
        {
            for (var i = Listeners.Count - 1; i >= 0; i--)
            {
                Listeners[i]?.Invoke(value);
            }
        }
        
        public void AddListener(Action<T> listener) => Listeners.Add(listener);
        public void RemoveListener(Action<T> listener) => Listeners.Remove(listener);
    }
    
    internal class AbstractEventHandler<T1, T2>
    {
        private EventListeners<Action<T1, T2>> Listeners { get; } = new();

        public void Invoke(T1 v1, T2 v2)
        {
            for (var i = Listeners.Count - 1; i >= 0; i--)
            {
                Listeners[i]?.Invoke(v1, v2);
            }
        }
        
        public void AddListener(Action<T1, T2> listener) => Listeners.Add(listener);
        public void RemoveListener(Action<T1, T2> listener) => Listeners.Remove(listener);
    }
    
    internal class AbstractEventHandler<T1, T2, T3>
    {
        private EventListeners<Action<T1, T2, T3>> Listeners { get; } = new();

        public void Invoke(T1 v1, T2 v2, T3 v3)
        {
            for (var i = Listeners.Count - 1; i >= 0; i--)
            {
                Listeners[i]?.Invoke(v1, v2, v3);
            }
        }
        
        public void AddListener(Action<T1, T2, T3> listener) => Listeners.Add(listener);
        public void RemoveListener(Action<T1, T2, T3> listener) => Listeners.Remove(listener);
    }
    
    internal class AbstractEventHandler<T1, T2, T3, T4>
    {
        private EventListeners<Action<T1, T2, T3, T4>> Listeners { get; } = new();

        public void Invoke(T1 v1, T2 v2, T3 v3, T4 v4)
        {
            for (var i = Listeners.Count - 1; i >= 0; i--)
            {
                Listeners[i]?.Invoke(v1, v2, v3, v4);
            }
        }
        
        public void AddListener(Action<T1, T2, T3, T4> listener) => Listeners.Add(listener);
        public void RemoveListener(Action<T1, T2, T3, T4> listener) => Listeners.Remove(listener);
    }
    
    internal class AbstractEventHandler<T1, T2, T3, T4, T5>
    {
        private EventListeners<Action<T1, T2, T3, T4, T5>> Listeners { get; } = new();

        public void Invoke(T1 v1, T2 v2, T3 v3, T4 v4, T5 v5)
        {
            for (var i = Listeners.Count - 1; i >= 0; i--)
            {
                Listeners[i]?.Invoke(v1, v2, v3, v4, v5);
            }
        }
        
        public void AddListener(Action<T1, T2, T3, T4, T5> listener) => Listeners.Add(listener);
        public void RemoveListener(Action<T1, T2, T3, T4, T5> listener) => Listeners.Remove(listener);
    }
}