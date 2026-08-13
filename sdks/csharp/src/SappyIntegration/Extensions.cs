#if SAPPY
using System;
using SpacetimeDB.EventHandling;
using Sappy;

namespace SpacetimeDB.SappyIntegration
{
    public static class Extensions
    {
        public static bool AddSapTarget<T>(this IEventListeners<T> listeners, SapTarget<T> value) where T : Delegate
        {
            if (listeners is not SappyEventListeners<T> sappyEventListeners)
            {
                throw new InvalidOperationException(
                    "Cannot add a SapTarget because this listener collection is not backed by Sappy. " +
                    "Ensure the Sappy integration assembly registered before this table handle was created."
                );
            }

            sappyEventListeners.Add(value);
            return true;
        }

        public static bool RemoveSapTarget<T>(this IEventListeners<T> listeners, SapTarget<T> value) where T : Delegate
        {
            if (listeners is not SappyEventListeners<T> sappyEventListeners)
            {
                throw new InvalidOperationException(
                    "Cannot remove a SapTarget because this listener collection is not backed by Sappy. " +
                    "Ensure the Sappy integration assembly registered before this table handle was created."
                );
            }

            sappyEventListeners.Remove(value);
            return true;
        }
    }
}
#endif
