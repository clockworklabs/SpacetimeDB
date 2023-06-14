using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.WebSockets;
using System.Reflection;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using ClientApi;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using Newtonsoft.Json;
using SpacetimeDB;
using SpacetimeDB.SATS;
using UnityEngine;
using UnityEngine.Rendering;
using Event = ClientApi.Event;

namespace SpacetimeDB
{
    public class NetworkManager : MonoBehaviour
    {
        public enum TableOp
        {
            Insert,
            Delete,
            Update
        }

        public class ReducerCallRequest
        {
            public string fn;
            public object[] args;
        }

        public class SubscriptionRequest
        {
            public string subscriptionQuery;
        }

        private struct DbEvent
        {
            public ClientCache.TableCache table;
            public TableOp op;
            public object newValue;
            public object oldValue;
            public byte[] deletedPk;
            public byte[] insertedPk;
        }

        public delegate void RowUpdate(string tableName, TableOp op, object oldValue, object newValue, ClientApi.Event dbEvent);

        /// <summary>
        /// Called when a connection is established to a spacetimedb instance.
        /// </summary>
        public event Action onConnect;

        /// <summary>
        /// Called when a connection attempt fails.
        /// </summary>
        public event Action<WebSocketError?> onConnectError;

        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        public event Action<Exception> onSendError;

        /// <summary>
        /// Called when a connection that was established has disconnected.
        /// </summary>
        public event Action<WebSocketCloseStatus?, WebSocketError?> onDisconnect;

        /// <summary>
        /// Invoked on each row update to each table.
        /// </summary>
        public event RowUpdate onRowUpdate;

        /// <summary>
        /// Invoked when the local client cache is updated as a result of changes made to the subscription queries.
        /// </summary>
        public event Action onSubscriptionUpdate;

        /// <summary>
        /// Called when we receive an identity from the server
        /// </summary>
        public event Action<Identity> onIdentityReceived;

        /// <summary>
        /// Invoked when an event message is received or at the end of a transaction update.
        /// </summary>
        public event Action<ClientApi.Event> onEvent;

        private SpacetimeDB.WebSocket webSocket;
        private bool connectionClosed;
        public static ClientCache clientDB;
        public static Dictionary<string, MethodInfo> reducerEventCache = new Dictionary<string, MethodInfo>();
        public static Dictionary<string, MethodInfo> deserializeEventCache = new Dictionary<string, MethodInfo>();

        private Thread messageProcessThread;

        public static NetworkManager instance;

        public string TokenKey
        {
            get { return GetTokenKey(); }
        }

        protected void Awake()
        {
            if (instance != null)
            {
                Debug.LogError($"There is more than one {GetType()}");
                return;
            }

            instance = this;

            var options = new SpacetimeDB.ConnectOptions
            {
                //v1.bin.spacetimedb
                //v1.text.spacetimedb
                Protocol = "v1.bin.spacetimedb",
            };
            webSocket = new SpacetimeDB.WebSocket(options);
            webSocket.OnMessage += OnMessageReceived;
            webSocket.OnClose += (code, error) => onDisconnect?.Invoke(code, error);
            webSocket.OnConnect += () => onConnect?.Invoke();
            webSocket.OnConnectError += a => onConnectError?.Invoke(a);
            webSocket.OnSendError += a => onSendError?.Invoke(a);

            clientDB = new ClientCache();

            var type = typeof(IDatabaseTable);
            var types = AppDomain.CurrentDomain.GetAssemblies().SelectMany(s => s.GetTypes())
                                 .Where(p => type.IsAssignableFrom(p));
            foreach (var @class in types)
            {
                if (!@class.IsClass)
                {
                    continue;
                }

                var algebraicTypeFunc = @class.GetMethod("GetAlgebraicType", BindingFlags.Static | BindingFlags.Public);
                var algebraicValue = algebraicTypeFunc!.Invoke(null, null) as AlgebraicType;
                var conversionFunc = @class.GetMethods()
                                           .FirstOrDefault(a => a.Name == "op_Explicit" &&
                                                                a.GetParameters().Length > 0 &&
                                                                a.GetParameters()[0].ParameterType ==
                                                                typeof(AlgebraicValue));
                clientDB.AddTable(@class, algebraicValue,
                                  a => { return conversionFunc!.Invoke(null, new object[] { a }); });
            }

            // cache all our reducer events by their function name 
            foreach (var methodInfo in typeof(SpacetimeDB.Reducer).GetMethods())
            {
                if (methodInfo.GetCustomAttribute<ReducerEvent>() is
                    { } reducerEvent)
                {
                    reducerEventCache.Add(reducerEvent.FunctionName, methodInfo);
                }

                if (methodInfo.GetCustomAttribute<DeserializeEvent>() is
                    { } deserializeEvent)
                {
                    deserializeEventCache.Add(deserializeEvent.FunctionName, methodInfo);
                }
            }

            messageProcessThread = new Thread(ProcessMessages);
            messageProcessThread.Start();
        }

        struct ProcessedMessage
        {
            public Message message;
            public IList<DbEvent> events;
        }

        private readonly BlockingCollection<byte[]> _messageQueue = new BlockingCollection<byte[]>(new ConcurrentQueue<byte[]>());
        private ProcessedMessage? nextMessage;

        void ProcessMessages()
        {
            while (true)
            {
                var bytes = _messageQueue.Take();
                // Wait for the main thread to consume the message we digested for them
                while (nextMessage.HasValue)
                {
                    Thread.Sleep(1);
                }

                var (m, events) = PreProcessMessage(bytes);
                nextMessage = new ProcessedMessage
                {
                    message = m,
                    events = events,
                };
            }

            (Message, List<DbEvent>) PreProcessMessage(byte[] bytes)
            {
                var dbEvents = new List<DbEvent>();
                var message = ClientApi.Message.Parser.ParseFrom(bytes);
                using var stream = new MemoryStream();
                using var reader = new BinaryReader(stream);

                SubscriptionUpdate subscriptionUpdate = null;
                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                        subscriptionUpdate = message.SubscriptionUpdate;
                        break;
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                        break;
                }

                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        // First apply all of the state
                        foreach (var update in subscriptionUpdate.TableUpdates)
                        {
                            var tableName = update.TableName;
                            var table = clientDB.GetTable(tableName);
                            if (table == null)
                            {
                                Debug.LogError($"Unknown table name: {tableName}");
                                continue;
                            }

                            foreach (var row in update.TableRowOperations)
                            {
                                var rowPk = row.RowPk.ToByteArray();
                                var rowValue = row.Row.ToByteArray();
                                stream.Position = 0;
                                stream.Write(rowValue, 0, rowValue.Length);
                                stream.Position = 0;
                                stream.SetLength(rowValue.Length);

                                switch (row.Op)
                                {
                                    case TableRowOperation.Types.OperationType.Delete:
                                        // If we don't already have this row, we should skip this delete
                                        if (!table.entries.ContainsKey(rowPk))
                                        {
                                            if (update.TableRowOperations.Any(
                                                    a => a.RowPk.ToByteArray().SequenceEqual(rowPk)))
                                            {
                                                // Debug.LogWarning("We are deleting and inserting the same row in the same TX!");
                                            }
                                            else
                                            {
                                                Debug.LogWarning(
                                                    $"We received a delete for a row we don't even subscribe to! table={table.Name}");
                                            }
                                            continue;
                                        }

                                        dbEvents.Add(new DbEvent
                                        {
                                            table = table,
                                            deletedPk = rowPk,
                                            op = TableOp.Delete,
                                            newValue = null,
                                            // We cannot grab the old value here because there might be other
                                            // pending operations that will execute before us. We should only
                                            // set this value on the main thread where we know there are no other
                                            // operations which could remove this value.
                                            oldValue = null,
                                        });
                                        break;
                                    case TableRowOperation.Types.OperationType.Insert:
                                        // If we already have this row, we can ignore it
                                        if (table.entries.ContainsKey(rowPk))
                                        {
                                            continue;
                                        }

                                        var algebraicValue = AlgebraicValue.Deserialize(table.RowSchema, reader);
                                        Debug.Assert(algebraicValue != null);
                                        table.SetDecodedValue(rowPk, algebraicValue, out var obj);
                                        dbEvents.Add(new DbEvent
                                        {
                                            table = table,
                                            insertedPk = rowPk,
                                            op = TableOp.Insert,
                                            newValue = obj,
                                            oldValue = null,
                                        });
                                        break;
                                }
                            }
                        }

                        break;
                    case ClientApi.Message.TypeOneofCase.IdentityToken:
                        break;
                    case ClientApi.Message.TypeOneofCase.Event:
                        break;
                }
                
                // Factor out any insert/deletes into updates
                for (var x = 0; x < dbEvents.Count; x++)
                {
                    var insertEvent = dbEvents[x];
                    if (insertEvent.op != TableOp.Insert)
                    {
                        continue;
                    }

                    for (var y = 0; y < dbEvents.Count; y++)
                    {
                        var deleteEvent = dbEvents[y];
                        if (deleteEvent.op != TableOp.Delete || deleteEvent.table != insertEvent.table
                            || !insertEvent.table.ComparePrimaryKey(insertEvent.insertedPk, deleteEvent.deletedPk))
                        {
                            continue;
                        }

                        var updateEvent = new DbEvent { 
                            deletedPk = deleteEvent.deletedPk,
                            insertedPk = insertEvent.insertedPk,
                            op = TableOp.Update,
                            table = insertEvent.table,
                        };
                        dbEvents[x] = updateEvent;
                        dbEvents.RemoveAt(y);
                        break;
                    }
                }

                if (message.TypeCase == Message.TypeOneofCase.SubscriptionUpdate)
                {
                    // NOTE: We are going to calculate a local state diff here. This is kind of expensive to do,
                    // but keep in mind that we're still on the network thread so spending some extra time here
                    // is acceptable.
                    if (subscriptionUpdate!.TableUpdates.Any(
                            a => a.TableRowOperations.Any(b => b.Op == TableRowOperation.Types.OperationType.Delete)))
                    {
                        Debug.LogWarning(
                            "We see delete events in our subscription update, this is unexpected. Likely you should update your SpacetimeDBUnitySDK.");
                    }

                    foreach (var tableUpdate in subscriptionUpdate.TableUpdates)
                    {
                        var clientTable = clientDB.GetTable(tableUpdate.TableName);
                        var newPks = tableUpdate.TableRowOperations
                                                .Where(a => a.Op == TableRowOperation.Types.OperationType.Insert)
                                                .Select(b => b.RowPk.ToByteArray());
                        var existingPks = clientTable.entries.Select(a => a.Key);
                        dbEvents.AddRange(existingPks.Except(newPks, new ClientCache.TableCache.ByteArrayComparer())
                                                     .Select(a => new DbEvent
                                                     {
                                                         deletedPk = a,
                                                         newValue = null,
                                                         oldValue = clientTable.entries[a].Item2,
                                                         op = TableOp.Delete,
                                                         table = clientTable,
                                                     }));
                    }
                }

                return (message, dbEvents);
            }
        }

        private void OnDestroy()
        {
            connectionClosed = true;
            webSocket.Close();
            webSocket = null;
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="host">The host or IP address and the port to connect to. Example: spacetime.spacetimedb.net:3000</param>
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        /// <param name="sslEnabled">Should websocket use SSL</param>
        public void Connect(string host, string addressOrName, bool sslEnabled = true)
        {
            var token = PlayerPrefs.HasKey(GetTokenKey()) ? PlayerPrefs.GetString(GetTokenKey()) : null;

            Task.Run(async () =>
            {
                try
                {
                    await webSocket.Connect(token, host, addressOrName, sslEnabled);
                }
                catch (Exception e)
                {
                    if (connectionClosed)
                    {
                        Debug.Log("Connection closed gracefully.");
                        return;
                    }

                    Debug.LogException(e);
                }
            });
        }

        private void OnMessageProcessComplete(Message message, IList<DbEvent> events)
        {
            switch (message.TypeCase)
            {
                case Message.TypeOneofCase.SubscriptionUpdate:
                case Message.TypeOneofCase.TransactionUpdate:
                    // First apply all of the state
                    for (var i = 0; i < events.Count; i++)
                    {
                        // TODO: Reimplement updates when we add support for primary keys
                        var ev = events[i];
                        switch (ev.op)
                        {
                            case TableOp.Delete:
                                ev.oldValue = events[i].table.DeleteEntry(ev.deletedPk);
                                events[i] = ev;
                                break;
                            case TableOp.Insert:
                                ev.newValue = events[i].table.InsertEntry(ev.insertedPk);
                                events[i] = ev;
                                break;
                            case TableOp.Update:
                                ev.oldValue = events[i].table.DeleteEntry(ev.deletedPk);
                                ev.newValue = events[i].table.InsertEntry(ev.insertedPk);
                                events[i] = ev;
                                break;
                            default:
                                throw new ArgumentOutOfRangeException();
                        }
                    }

                    // Send out events
                    var eventCount = events.Count;
                    for (var i = 0; i < eventCount; i++)
                    {
                        var tableName = events[i].table.ClientTableType.Name;
                        var tableOp = events[i].op;
                        var oldValue = events[i].oldValue;
                        var newValue = events[i].newValue;

                        switch (tableOp)
                        {
                            case TableOp.Insert:
                                if (oldValue == null && newValue != null)
                                {
                                    try
                                    {
                                        if (events[i].table.InsertCallback != null)
                                        {
                                            events[i].table.InsertCallback.Invoke(newValue, message.Event);
                                        }
                                    }
                                    catch (Exception e)
                                    {
                                        Debug.LogException(e);
                                    }
                                    
                                    try
                                    {
                                        if (events[i].table.RowUpdatedCallback != null)
                                        {
                                            events[i].table.RowUpdatedCallback
                                                .Invoke(tableOp, null, newValue, message.Event);
                                        }
                                    }
                                    catch (Exception e)
                                    {
                                        Debug.LogException(e);
                                    }

                                }
                                else
                                {
                                    Debug.LogError("Failed to send callback: invalid insert!");
                                }

                                break;
                            case TableOp.Delete:
                                {
                                    if (oldValue != null && newValue == null)
                                    {
                                        if (events[i].table.DeleteCallback != null)
                                        {
                                            try
                                            {
                                                events[i].table.DeleteCallback.Invoke(oldValue, message.Event);
                                            }
                                            catch (Exception e)
                                            {
                                                Debug.LogException(e);
                                            }
                                        }

                                        if (events[i].table.RowUpdatedCallback != null)
                                        {
                                            try
                                            {
                                                events[i].table.RowUpdatedCallback
                                                     .Invoke(tableOp, oldValue, null, message.Event);
                                            }
                                            catch (Exception e)
                                            {
                                                Debug.LogException(e);
                                            }
                                        }
                                    }
                                    else
                                    {
                                        Debug.LogError("Failed to send callback: invalid delete");
                                    }

                                    break;
                                }
                            case TableOp.Update:
                                {
                                    if (oldValue != null && newValue != null)
                                    {
                                        try
                                        {
                                            if (events[i].table.UpdateCallback != null)
                                            {
                                                events[i].table.UpdateCallback.Invoke(oldValue, newValue, message.Event);
                                            }
                                        }
                                        catch (Exception e)
                                        {
                                            Debug.LogException(e);
                                        }
                                        
                                        try
                                        {
                                            if (events[i].table.RowUpdatedCallback != null)
                                            {
                                                events[i].table.RowUpdatedCallback
                                                         .Invoke(tableOp, oldValue, null, message.Event);
                                            }
                                        }
                                        catch (Exception e)
                                        {
                                            Debug.LogException(e);
                                        }
                                    }
                                    else
                                    {
                                        Debug.LogError("Failed to send callback: invalid update");
                                    }

                                    break;
                                }
                            default:
                                throw new ArgumentOutOfRangeException();
                        }

                        onRowUpdate?.Invoke(tableName, tableOp, oldValue, newValue, message.Event);
                    }

                    switch (message.TypeCase)
                    {
                        case Message.TypeOneofCase.SubscriptionUpdate:
                            onSubscriptionUpdate?.Invoke();
                            break;
                        case Message.TypeOneofCase.TransactionUpdate:
                            onEvent?.Invoke(message.TransactionUpdate.Event);

                            var functionName = message.TransactionUpdate.Event.FunctionCall.Reducer;
                            if (reducerEventCache.TryGetValue(functionName, out var value))
                            {
                                value.Invoke(null, new object[] { message.TransactionUpdate.Event });
                            }

                            break;
                        case Message.TypeOneofCase.None:
                            break;
                        case Message.TypeOneofCase.FunctionCall:
                            break;
                        case Message.TypeOneofCase.Event:
                            break;
                        case Message.TypeOneofCase.IdentityToken:
                            break;
                        default:
                            throw new ArgumentOutOfRangeException();
                    }

                    break;
                case Message.TypeOneofCase.IdentityToken:
                    onIdentityReceived?.Invoke(Identity.From(message.IdentityToken.Identity.ToByteArray()));
                    PlayerPrefs.SetString(GetTokenKey(), message.IdentityToken.Token);
                    break;
                case Message.TypeOneofCase.Event:
                    onEvent?.Invoke(message.Event);
                    break;
            }
        }

        private void OnMessageReceived(byte[] bytes) => _messageQueue.Add(bytes);

        private string GetTokenKey()
        {
            var key = "spacetimedb.identity_token";
#if UNITY_EDITOR
            // Different editors need different keys
            key += $" - {Application.dataPath}";
#endif
            return key;
        }

        internal void InternalCallReducer(string json)
        {
            webSocket.Send(Encoding.ASCII.GetBytes("{ \"call\": " + json + " }"));
        }

        public void Subscribe(List<string> queries)
        {
            var json = JsonConvert.SerializeObject(queries);
            webSocket.Send(Encoding.ASCII.GetBytes("{ \"subscribe\": { \"query_strings\": " + json + " }}"));
        }

        private void Update()
        {
            webSocket.Update();

            if (nextMessage != null)
            {
                OnMessageProcessComplete(nextMessage.Value.message, nextMessage.Value.events);
                nextMessage = null;
            }
        }
    }
}
