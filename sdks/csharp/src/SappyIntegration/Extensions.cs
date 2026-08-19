#if SAPPY
using System;
using SpacetimeDB.EventHandling;
using Sappy;

namespace SpacetimeDB.SappyIntegration
{
    public static class Extensions
    {
        public static void AddSapTarget<T>(this IEventListeners<T> listeners, SapTarget<T> value) where T : Delegate
        {
            if (listeners is SappyEventListeners<T> sappyEventListeners)
            {
                sappyEventListeners.Add(value);
            }
            else
            {
                listeners.Add(value.Callback);
            }
        }

        public static void RemoveSapTarget<T>(this IEventListeners<T> listeners, SapTarget<T> value) where T : Delegate
        {
            if (listeners is SappyEventListeners<T> sappyEventListeners)
            {
                sappyEventListeners.Remove(value);
            }
            else
            {
                listeners.Add(value.Callback);
            }
        }
    }
}
#endif