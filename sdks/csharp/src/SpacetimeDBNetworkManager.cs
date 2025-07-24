#if UNITY_5_3_OR_NEWER
using System;
using System.Collections.Generic;
using SpacetimeDB;
using UnityEngine;

namespace SpacetimeDB
{
    // This class is only used in Unity projects.
    // Attach this to a GameObject in your scene to use SpacetimeDB.
    public class SpacetimeDBNetworkManager : MonoBehaviour
    {
        internal static SpacetimeDBNetworkManager? _instance;

        public void Awake()
        {
            // Ensure that users don't create several SpacetimeDBNetworkManager instances.
            // We're using a global (static) list of active connections and we don't want several instances to walk over it several times.
            if (_instance != null)
            {
                throw new InvalidOperationException("SpacetimeDBNetworkManager is a singleton and should only be attached once.");
            }
            else
            {
                _instance = this;
            }
        }

        private readonly List<IDbConnection> activeConnections = new();

        public bool AddConnection(IDbConnection conn)
        {
            if (activeConnections.Contains(conn))
            {
                return false;
            }
            activeConnections.Add(conn);
            return true;

        }

        public bool RemoveConnection(IDbConnection conn)
        {
            return activeConnections.Remove(conn);
        }
        
        private void ForEachConnection(Action<IDbConnection> action)
        {
            // It's common to call disconnect from Update, which will then modify the ActiveConnections collection,
            // therefore we must reverse-iterate the list of connections.
            for (var x = activeConnections.Count - 1; x >= 0; x--)
            {
                action(activeConnections[x]);
            }
        }

        private void Update() => ForEachConnection(conn => conn.FrameTick());
        private void OnDestroy() => ForEachConnection(conn => conn.Disconnect());
    }
}
#endif
