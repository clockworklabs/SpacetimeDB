#if SAPPY
using System;
using Sappy;
using SpacetimeDB.EventHandling;
using System.Runtime.CompilerServices;

namespace SpacetimeDB.SappyIntegration
{
    public class SappyEventListeners<T> : IEventListeners<T> where T : Delegate
    {
        private EventListeners<T>? _eventListeners;
        private SapDelegate<T>? _sapDelegate;

        public int Count
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get => (_eventListeners?.Count ?? 0) + (_sapDelegate?.Count ?? 0);
        }
        
        public T this[int index]
        {
            get
            {
                var eventListeners = _eventListeners;
                var eventListenersCount = eventListeners?.Count ?? 0;

                if ((uint)index < (uint)eventListenersCount)
                {
                    return eventListeners![index];
                }

                var sapDelegate = _sapDelegate;
                var sapIndex = index - eventListenersCount;

                if (sapDelegate != null && (uint)sapIndex < (uint)sapDelegate.Count)
                {
                    return sapDelegate[sapIndex];
                }

                throw new IndexOutOfRangeException();
            }
        }

        public void Add(SapTarget<T> listener)
        {
            if (listener == null) return;
            (_sapDelegate ??= new SapDelegate<T>()).Add(listener);
        }

        public void Remove(SapTarget<T> listener)
        {
            if (listener == null || _sapDelegate == null) return;
            _sapDelegate.Remove(listener);
        }

        public void Add(T listener)
        {
            if (listener == null) return;
            (_eventListeners ??= new EventListeners<T>()).Add(listener);
        }

        public void Remove(T listener)
        {
            if (listener == null || _eventListeners == null) return;
            _eventListeners.Remove(listener);
        }
    }
}
#endif
