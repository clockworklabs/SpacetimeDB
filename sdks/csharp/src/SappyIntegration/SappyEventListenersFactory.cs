#if SAPPY
using System;
using UnityEngine;
using SpacetimeDB.EventHandling;

namespace SpacetimeDB.SappyIntegration
{
    public class SappyEventListenersFactory : IEventListenersFactory
    {
        public IEventListeners<T> Create<T>() where T : Delegate => new SappyEventListeners<T>();
    }
}
#endif