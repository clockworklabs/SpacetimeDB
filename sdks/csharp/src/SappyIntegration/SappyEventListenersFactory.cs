#if SAPPY
using System;
using UnityEngine;
using SpacetimeDB.EventHandling;

namespace SpacetimeDB.SappyIntegration
{
    public class SappyEventListenersFactory : IEventListenersFactory
    {
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.AfterAssembliesLoaded)]
        private static void AutoRegister()
        {
            // Hand this implementation back to the main assembly
            EventListenersProvider.SetFactory(new SappyEventListenersFactory());
        }

        public IEventListeners<T> Create<T>() where T : Delegate => new SappyEventListeners<T>();
    }
}
#endif