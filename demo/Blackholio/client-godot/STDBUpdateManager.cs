#if GODOT
using System.Collections.Generic;
using Godot;

namespace SpacetimeDB
{
    public partial class STDBUpdateManager : Node
    {
        private const string SingletonNodeName = nameof(STDBUpdateManager);
        private static STDBUpdateManager _instance;
        private static STDBUpdateManager Instance => EnsureInstance();

        private List<IDbConnection> Connections { get; } = new();

        private static STDBUpdateManager EnsureInstance()
        {
            if (IsInstanceValid(_instance))
            {
                return _instance;
            }

            if (Engine.GetMainLoop() is not SceneTree sceneTree)
            {
                GD.PushWarning($"{SingletonNodeName} could not be created because the SceneTree is not available yet.");
                return null;
            }

            var root = sceneTree.Root;
            if (root == null)
            {
                GD.PushWarning($"{SingletonNodeName} could not be created because the scene root is not available yet.");
                return null;
            }

            var existing = root.GetNodeOrNull<STDBUpdateManager>(SingletonNodeName);
            if (existing != null)
            {
                _instance = existing;
                return _instance;
            }

            _instance = new STDBUpdateManager
            {
                Name = SingletonNodeName,
            };
            root.AddChild(_instance);
            return _instance;
        }

        public static bool Add(IDbConnection conn)
        {
            if (conn == null) return false;
            var connections = Instance?.Connections;
            if (connections == null || connections.Contains(conn)) return false;
            connections.Add(conn);
            return true;
        }

        public static bool Remove(IDbConnection conn, bool disconnect = false)
        {
            if (conn == null) return false;
            var connections = Instance?.Connections;
            if (connections != null && connections.Remove(conn))
            {
                if (disconnect)
                {
                    conn.Disconnect();
                }

                return true;
            }

            return false;
        }

        public override void _EnterTree()
        {
            if (_instance != null && _instance != this && IsInstanceValid(_instance))
            {
                QueueFree();
                return;
            }

            _instance = this;
        }

        public override void _ExitTree()
        {
            foreach (var conn in Connections)
            {
                conn?.Disconnect();
            }

            if (_instance == this)
            {
                _instance = null;
            }
        }

        public override void _Process(double delta)
        {
            foreach (var conn in Connections)
            {
                conn?.FrameTick();
            }
        }
    }
}
#endif
