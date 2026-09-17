using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;
using System.Threading.Tasks;
using SpacetimeDB.ClientApi;

namespace SpacetimeDB
{
    public readonly struct NextReconnect
    {
        public int Attempt { get; }
        public TimeSpan Delay { get; }

        internal NextReconnect(int attempt, TimeSpan delay)
        {
            Attempt = attempt;
            Delay = delay;
        }
    }

    public class UnknownResultException : SpacetimeDBException
    {
        public UnknownResultException() : base("Connection lost before the result was received. The operation may have executed.") { }
    }

    internal class ConnectionProtocolException : SpacetimeDBException
    {
        internal ConnectionProtocolException(string message, Exception? inner = null) : base(message, inner) { }
    }

    internal static class ReconnectPolicy
    {
        internal static TimeSpan Delay(int attempt, double random) => TimeSpan.FromMilliseconds(
            Math.Min(30000, Math.Min(30000, 1000 * Math.Pow(2, Math.Min(30, Math.Max(0, attempt - 1)))) * (0.5 + random)));

        [DataContract]
#if UNITY_5_3_OR_NEWER
        [UnityEngine.Scripting.Preserve]
#endif
        private class Claims
        {
            [DataMember(Name = "exp")]
            public double? Exp { get; set; }
            [DataMember(Name = "iat")]
            public double? Iat { get; set; }
        }

        internal static bool TokenNeedsRefresh(string? token, DateTimeOffset now)
        {
            try
            {
                var payload = token!.Split('.')[1].Replace('-', '+').Replace('_', '/');
                payload = payload.PadRight((payload.Length + 3) / 4 * 4, '=');
                using var stream = new MemoryStream(Convert.FromBase64String(payload));
                var claims = (Claims)new DataContractJsonSerializer(typeof(Claims)).ReadObject(stream)!;
                if (claims.Exp is not double exp || double.IsNaN(exp) || double.IsInfinity(exp)) return true;
                var margin = Math.Max(30, claims.Iat is double iat ? (exp - iat) * 0.05 : 0);
                return exp - now.ToUnixTimeMilliseconds() / 1000.0 <= margin;
            }
            catch
            {
                return true;
            }
        }
    }

    public abstract partial class DbConnectionBase<DbConnection, Tables, Reducer>
    {
        private bool automaticReconnect;
        private Func<Task<string>>? tokenProvider;
        private string? retainedToken;
        private readonly ConnectionId sessionId = ConnectionId.Random();
        private (string Uri, string Database, Compression Compression, bool Light, bool? Confirmed) connectionOptions;
        private bool hasEverConnected;
        private bool preparingReplay;
        private bool forceTokenRefresh;
        private bool usedFreshToken;
        private int reconnectAttempt;
        private double? reconnectAt;
        private Task<string>? tokenTask;
        private volatile int socketGeneration;
        private uint? replayRequestId;
        private HashSet<uint>? replayQueryIds;
        private readonly Random reconnectRandom = new();
        private readonly Dictionary<uint, string[]> subscriptionQueries = new();
        private readonly HashSet<uint> unsubscribeRequested = new();
        private event Action<Exception, NextReconnect?>? onConnectError;
        private event Action<Exception?, NextReconnect?>? onDisconnect;

        internal Func<double> ReconnectClock = () => Stopwatch.GetTimestamp() / (double)Stopwatch.Frequency;
        internal Func<WebSocket> SocketFactory = () => new WebSocket(new WebSocket.ConnectOptions { Protocol = "v2.bsatn.spacetimedb" });

        bool IDbConnection.AutomaticReconnectEnabled => automaticReconnect;

        public bool IsReconnecting => automaticReconnect && hasEverConnected && !onConnectInvoked && !isClosing;

        void IDbConnection.ConfigureReconnect(bool enabled, Func<Task<string>>? provider)
        {
            automaticReconnect = enabled;
            tokenProvider = provider;
        }

        private WebSocket CreateWebSocket()
        {
            var socket = SocketFactory();
            var generation = socketGeneration;
            socket.OnMessage += (bytes, timestamp) => EnqueueMessage(bytes, timestamp, generation);
            socket.OnClose += error => EnqueueSocketAction(() => HandleSocketFailure(error), generation);
            socket.OnConnectError += error => EnqueueSocketAction(() => HandleSocketFailure(error), generation);
            socket.OnSendError += error => EnqueueSocketAction(() =>
            {
                onSendError?.Invoke(error);
                HandleSocketFailure(error);
            }, generation);
            return socket;
        }

        private void EnqueueMessage(byte[] bytes, DateTime timestamp, int generation)
        {
            if (isClosing || generation != socketGeneration) return;
            _parseQueue.Add(new UnparsedMessage
            {
                bytes = bytes,
                timestamp = timestamp,
                generation = generation,
                parseQueueTrackerId = stats.ParseMessageQueueTracker.StartTrackingRequest()
            });
        }

        private void EnqueueSocketAction(Action action, int generation)
        {
            if (!isClosing && generation == socketGeneration)
                _parseQueue.Add(new UnparsedMessage { action = action, generation = generation });
        }

        private void StartSocket()
        {
            if (isClosing) return;
            connectionClosed = false;
            onConnectInvoked = false;
            initialConnectionId = null;
            if (hasEverConnected) ConnectionId = ConnectionId.Random();
            socketGeneration++;
            webSocket.Abort();
            webSocket = CreateWebSocket();
            Log.Info($"SpacetimeDBClient: Connecting to {connectionOptions.Uri} {connectionOptions.Database}");
            if (IsTesting) return;
            var socket = webSocket;
            var connectionId = ConnectionId;
            var generation = socketGeneration;
            async Task ConnectSocket()
            {
                try
                {
                    await socket.Connect(retainedToken, connectionOptions.Uri, connectionOptions.Database,
                        connectionId, connectionOptions.Compression, connectionOptions.Light, connectionOptions.Confirmed,
                        automaticReconnect ? sessionId : null);
                }
                catch (Exception error)
                {
                    EnqueueSocketAction(() => HandleSocketFailure(error), generation);
                }
            }
#if UNITY_WEBGL && !UNITY_EDITOR
            _ = ConnectSocket();
#else
            _ = Task.Run(ConnectSocket);
#endif
        }

        private void HandleInitialConnection(InitialConnection initial)
        {
            if (automaticReconnect && (onConnectInvoked || initial.ConnectionId != ConnectionId ||
                (Identity is Identity identity && identity != initial.Identity)))
            {
                HandleSocketFailure(new ConnectionProtocolException("Unexpected identity or connection ID in InitialConnection."));
                return;
            }
            if (!automaticReconnect && ((Identity.HasValue && Identity.Value != initial.Identity) ||
                (initialConnectionId.HasValue && initialConnectionId.Value != initial.ConnectionId)))
            {
                Log.Error("Received InitialConnection with an unexpected identity or connection ID.");
                return;
            }
            if (onConnectInvoked) return;
            var reconnect = hasEverConnected;
            Identity = initial.Identity;
            initialConnectionId = initial.ConnectionId;
            ConnectionId = initial.ConnectionId;
            if (string.IsNullOrEmpty(retainedToken)) retainedToken = initial.Token;
            hasEverConnected = true;
            onConnectInvoked = true;
            reconnectAttempt = 0;
            reconnectAt = null;
            try
            {
                onConnect?.Invoke(initial.Identity, automaticReconnect ? retainedToken! : initial.Token);
            }
            catch (Exception error)
            {
                Log.Exception(error);
            }
            finally
            {
                if (!automaticReconnect) onConnect = null;
                if (!isClosing && automaticReconnect)
                {
                    if (reconnect) ReplaySubscriptions();
                    else
                    {
                        preparingReplay = false;
                        foreach (var id in subscriptions.Keys.ToArray()) SendSubscription(id);
                    }
                }
            }
        }

        private void HandleSocketFailure(Exception? error)
        {
            if (isClosing || connectionClosed) return;
            var established = onConnectInvoked;
            connectionClosed = true;
            onConnectInvoked = false;
            socketGeneration++;
            webSocket.Abort();
            preparingReplay = automaticReconnect;
            replayRequestId = null;
            replayQueryIds = null;
            tokenTask = null;
            var authError = error is WebSocket.ConnectException { StatusCode: 400 or 401 or 403 };
            var terminal = error is ConnectionProtocolException ||
                (authError && (tokenProvider == null || usedFreshToken));
            NextReconnect? next = null;
            if (automaticReconnect && hasEverConnected && !terminal)
            {
                if (authError) forceTokenRefresh = true;
                var sessionBusy = !established && error is WebSocket.CloseException { Code: 4000 };
                if (!sessionBusy) reconnectAttempt = reconnectAttempt == int.MaxValue ? int.MaxValue : reconnectAttempt + 1;
                var delay = ReconnectPolicy.Delay(sessionBusy ? 1 : reconnectAttempt, reconnectRandom.NextDouble());
                reconnectAt = ReconnectClock() + delay.TotalSeconds;
                next = new NextReconnect(reconnectAttempt, delay);
            }
            else
            {
                EndConnection();
            }
            FailPendingOperations(automaticReconnect ? new UnknownResultException() : new OperationCanceledException("Connection closed."));
            foreach (var id in unsubscribeRequested.ToArray()) EndSubscription(id);
            if (isClosing && next != null) return;
            if (established) onDisconnect?.Invoke(error, next);
            else onConnectError?.Invoke(error ?? new SpacetimeDBException("Connection closed before InitialConnection."), next);
        }

        private void TickReconnect()
        {
            if (isClosing) return;
            if (tokenTask != null)
            {
                if (!tokenTask.IsCompleted) return;
                var completed = tokenTask;
                tokenTask = null;
                try
                {
                    retainedToken = completed.GetAwaiter().GetResult();
                    if (string.IsNullOrEmpty(retainedToken)) throw new InvalidOperationException("Token provider returned an empty token.");
                    forceTokenRefresh = false;
                    usedFreshToken = true;
                    StartSocket();
                }
                catch (Exception error)
                {
                    connectionClosed = false;
                    HandleSocketFailure(error);
                }
                return;
            }
            if (reconnectAt is not double due || ReconnectClock() < due) return;
            reconnectAt = null;
            usedFreshToken = false;
            if (tokenProvider != null && (forceTokenRefresh || ReconnectPolicy.TokenNeedsRefresh(retainedToken, DateTimeOffset.UtcNow)))
            {
                try
                {
                    tokenTask = tokenProvider() ?? Task.FromException<string>(new InvalidOperationException("Token provider returned no task."));
                }
                catch (Exception error)
                {
                    connectionClosed = false;
                    HandleSocketFailure(error);
                }
            }
            else StartSocket();
        }

        private void EndConnection()
        {
            isClosing = true;
            connectionClosed = true;
            onConnectInvoked = false;
            reconnectAt = null;
            tokenTask = null;
            socketGeneration++;
            webSocket.Abort();
            _parseCancellationTokenSource.Cancel();
            while (_parseQueue.TryTake(out _)) { }
            while (_applyQueue.TryTake(out _)) { }
            FailPendingOperations(automaticReconnect ? new UnknownResultException() : new OperationCanceledException("Connection closed."));
#if UNITY_5_3_OR_NEWER
            SpacetimeDBNetworkManager._instance?.RemoveConnection(this);
#endif
        }

        private void SendSubscription(uint id)
        {
            webSocket.Send(new ClientMessage.Subscribe(new Subscribe(
                stats.SubscriptionRequestTracker.StartTrackingRequest(), new QuerySetId(id), subscriptionQueries[id].ToList())));
        }

        private void RemoveSubscription(uint id)
        {
            subscriptions.Remove(id);
            subscriptionQueries.Remove(id);
            unsubscribeRequested.Remove(id);
        }

        private void EndSubscription(uint id)
        {
            if (!subscriptions.TryGetValue(id, out var handle)) return;
            RemoveSubscription(id);
            try { handle.OnEnded(MakeSubscriptionEventContext()); }
            catch (Exception error) { Log.Exception(error); }
        }

        private void ReplaySubscriptions()
        {
            var entries = subscriptions.Select(entry => (entry.Value, subscriptionQueries[entry.Key])).ToArray();
            subscriptions.Clear();
            subscriptionQueries.Clear();
            var sets = new List<SubscribeSet>();
            foreach (var (handle, queries) in entries)
            {
                var id = querySetIdAllocator.Next();
                handle.RebindQuerySetId(new QuerySetId(id));
                subscriptions[id] = handle;
                subscriptionQueries[id] = queries;
                sets.Add(new SubscribeSet(new QuerySetId(id), queries.ToList()));
            }
            preparingReplay = false;
            replayRequestId = stats.SubscriptionRequestTracker.StartTrackingRequest();
            replayQueryIds = new HashSet<uint>(subscriptions.Keys);
            if (sets.Count == 0)
            {
                stats.SubscriptionRequestTracker.FinishTrackingRequest(replayRequestId.Value);
                ApplyReplayBatch(new SubscribeBatchApplied(replayRequestId.Value, new()), ParsedDatabaseUpdate.New());
                return;
            }
            webSocket.Send(new ClientMessage.SubscribeBatch(new SubscribeBatch(replayRequestId.Value, sets)));
        }

        private void ApplyReplayBatch(SubscribeBatchApplied batch, ParsedDatabaseUpdate update)
        {
            var ids = new HashSet<uint>(batch.Results.Select(result => result.QuerySetId.Id));
            if (batch.RequestId != replayRequestId || replayQueryIds == null ||
                ids.Count != batch.Results.Count || !ids.SetEquals(replayQueryIds))
            {
                HandleSocketFailure(new ConnectionProtocolException("Unexpected subscription replay response."));
                return;
            }
            replayRequestId = null;
            replayQueryIds = null;
            foreach (var table in Db.AllTables) table.AddSnapshotDeletes(update);
            ApplyUpdate(ToEventContext(new Event<Reducer>.SubscribeApplied()), update);
            foreach (var result in batch.Results)
            {
                if (!subscriptions.TryGetValue(result.QuerySetId.Id, out var handle)) continue;
                try
                {
                    if (result.Outcome is SubscribeSetOutcome.Error(var message))
                    {
                        RemoveSubscription(result.QuerySetId.Id);
                        handle.OnError(ToErrorContext(new SpacetimeDBException(message)));
                    }
                    else handle.OnApplied(MakeSubscriptionEventContext());
                }
                catch (Exception error)
                {
                    Log.Exception(error);
                }
            }
        }
    }
}
