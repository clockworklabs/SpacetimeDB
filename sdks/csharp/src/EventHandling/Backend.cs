using System;

namespace SpacetimeDB.EventHandling
{
    public static class Backend
    {
        internal static bool UseNativeDispatch { get; private set; } = true;
        private static IEventListenersFactory? CustomFactory { get; set; }

        public static void UseNativeEvents()
        {
            UseNativeDispatch = true;
            CustomFactory = null;
        }

        public static void UseCustomListeners(IEventListenersFactory? factory = null)
        {
            UseNativeDispatch = false;
            CustomFactory = factory;
        }

        internal static IEventListeners<T> Create<T>() where T : Delegate => CustomFactory?.Create<T>() ?? new EventListeners<T>();
    }
}