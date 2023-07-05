using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.WebSockets;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using ClientApi;
using Newtonsoft.Json;
using SpacetimeDB.SATS;

namespace SpacetimeDB
{
    public class SpacetimeDBClient
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

        public struct DbEvent
        {
            public ClientCache.TableCache table;
            public TableOp op;
            public object newValue;
            public object oldValue;
            public byte[] deletedPk;
            public byte[] insertedPk;
        }

        public delegate void RowUpdate(string tableName, TableOp op, object oldValue, object newValue, SpacetimeDB.ReducerEventBase dbEvent);

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
        public event Action onSubscriptionApplied;

        /// <summary>
        /// Called when we receive an identity from the server
        /// </summary>
        public event Action<string, Identity> onIdentityReceived;

        /// <summary>
        /// Invoked when an event message is received or at the end of a transaction update.
        /// </summary>
        public event Action<ClientApi.Event> onEvent;

        private SpacetimeDB.WebSocket webSocket;
        private bool connectionClosed;
        public static ClientCache clientDB; 
        public static Dictionary<string, Func<ClientApi.Event, bool>> reducerEventCache = new Dictionary<string, Func<ClientApi.Event, bool>>();
        public static Dictionary<string, Action<ClientApi.Event>> deserializeEventCache = new Dictionary<string, Action<ClientApi.Event>>();

        private Thread messageProcessThread;

        public static SpacetimeDBClient instance;

        public ISpacetimeDBLogger Logger => logger;
        private ISpacetimeDBLogger logger;        

        public static void CreateInstance(ISpacetimeDBLogger loggerToUse)
        {
            if (instance == null)
            {
                new SpacetimeDBClient(loggerToUse);
            }
            else
            {
                loggerToUse.LogError($"Instance already created.");
            }
        }

        public Type FindReducerType()
        {
            // Get all loaded assemblies
            Assembly[] assemblies = AppDomain.CurrentDomain.GetAssemblies();

            // Iterate over each assembly and search for the type
            foreach (Assembly assembly in assemblies)
            {
                // Get all types in the assembly
                Type[] types = assembly.GetTypes();

                // Search for the type by name and namespace
                Type targetType = types.FirstOrDefault(t =>
                    t.Name == "Reducer" &&
                    t.Namespace == "SpacetimeDB");

                // If the type is found, return it
                if (targetType != null)
                {
                    return targetType;
                }
            }

            // If the type is not found in any assembly, return null or throw an exception
            return null;
        }


        protected SpacetimeDBClient(ISpacetimeDBLogger loggerToUse)
        {
            if (instance != null)
            {
                loggerToUse.LogError($"There is more than one {GetType()}");
                return;
            }

            instance = this;

            logger = loggerToUse;

            var options = new SpacetimeDB.ConnectOptions
            {
                //v1.bin.spacetimedb
                //v1.text.spacetimedb
                Protocol = "v1.bin.spacetimedb",
            };
            webSocket = new SpacetimeDB.WebSocket(logger, options);
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

            var reducerType = FindReducerType();
            if (reducerType != null)
            {
            // cache all our reducer events by their function name 
            foreach (var methodInfo in reducerType.GetMethods())
            {
                if (methodInfo.GetCustomAttribute<ReducerCallbackAttribute>() is
                    { } reducerEvent)
                {
                    reducerEventCache.Add(reducerEvent.FunctionName, (Func<ClientApi.Event, bool>)methodInfo.CreateDelegate(typeof(Func<ClientApi.Event, bool>)));
                }

                if (methodInfo.GetCustomAttribute<DeserializeEventAttribute>() is
                    { } deserializeEvent)
                {
                    deserializeEventCache.Add(deserializeEvent.FunctionName, (Action<ClientApi.Event>)methodInfo.CreateDelegate(typeof(Action<ClientApi.Event>)));
                }
                }
            }
            else
            {
                loggerToUse.LogError($"Could not find reducer type. Have you run spacetime generate?");
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
        private readonly BlockingCollection<ProcessedMessage> _nextMessageQueue = new BlockingCollection<ProcessedMessage>(new ConcurrentQueue<ProcessedMessage>());

        void ProcessMessages()
        {
            while (true)
            {
                var bytes = _messageQueue.Take();

                var (m, events) = PreProcessMessage(bytes);
                var processedMessage = new ProcessedMessage
                {
                    message = m,
                    events = events,
                };
                _nextMessageQueue.Add(processedMessage);
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
                                logger.LogError($"Unknown table name: {tableName}");
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
                                        if(algebraicValue == null)
                                        {
                                            throw new Exception("Failed to deserialize row");
                                        }
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

                        var updateEvent = new DbEvent
                        {
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
                        logger.LogWarning(
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


                if (message.TypeCase == Message.TypeOneofCase.TransactionUpdate && 
                    deserializeEventCache.TryGetValue(message.TransactionUpdate.Event.FunctionCall.Reducer, out var deserializer))
                {
                    deserializer.Invoke(message.TransactionUpdate.Event);
                }

                return (message, dbEvents);
            }
        }

        public void Close()
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
        public void Connect(string token, string host, string addressOrName, bool sslEnabled = true)
        {
            logger.Log($"SpacetimeDBClient: Connecting to {host} {addressOrName}");
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
                        logger.Log("Connection closed gracefully.");
                        return;
                    }

                    logger.LogException(e);
                }
            });
        }

        private void OnMessageProcessComplete(Message message, IList<DbEvent> events)
        {
            switch (message.TypeCase)
            {
                case Message.TypeOneofCase.SubscriptionUpdate:
                case Message.TypeOneofCase.TransactionUpdate:
                    // First trigger OnBeforeDelete
                    for (var i = 0; i < events.Count; i++)
                    {
                        // TODO: Reimplement updates when we add support for primary keys
                        var ev = events[i];
                        if (ev.op == TableOp.Delete && ev.table.TryGetValue(ev.deletedPk, out var oldVal))
                        {
                            try
                            {
                                ev.table.BeforeDeleteCallback?.Invoke(oldVal, message.TransactionUpdate?.Event);
                            }
                            catch (Exception e)
                            {
                                logger.LogException(e);
                            }
                        }
                    }

                    // Apply all of the state
                    for (var i = 0; i < events.Count; i++)
                    {
                        // TODO: Reimplement updates when we add support for primary keys
                        var ev = events[i];
                        switch (ev.op)
                        {
                            case TableOp.Delete:
                                ev.oldValue = events[i].table.DeleteEntry(ev.deletedPk);
                                if (ev.oldValue != null)
                                {
                                    ev.table.InternalValueDeletedCallback(ev.oldValue);
                                }
                                events[i] = ev;
                                break;
                            case TableOp.Insert:
                                ev.newValue = events[i].table.InsertEntry(ev.insertedPk);
                                ev.table.InternalValueInsertedCallback(ev.newValue);
                                events[i] = ev;
                                break;
                            case TableOp.Update:
                                ev.oldValue = events[i].table.DeleteEntry(ev.deletedPk);
                                ev.newValue = events[i].table.InsertEntry(ev.insertedPk);
                                if (ev.oldValue != null)
                                {
                                    ev.table.InternalValueDeletedCallback(ev.oldValue);
                                }
                                ev.table.InternalValueInsertedCallback(ev.newValue);
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
                                            events[i].table.InsertCallback.Invoke(newValue, message.TransactionUpdate?.Event);
                                        }
                                    }
                                    catch (Exception e)
                                    {
                                        logger.LogException(e);
                                    }
                                    
                                    try
                                    {
                                        if (events[i].table.RowUpdatedCallback != null)
                                        {
                                            events[i].table.RowUpdatedCallback
                                                .Invoke(tableOp, null, newValue, message.TransactionUpdate?.Event);
                                        }
                                    }
                                    catch (Exception e)
                                    {
                                        logger.LogException(e);
                                    }

                                }
                                else
                                {
                                    logger.LogError("Failed to send callback: invalid insert!");
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
                                                events[i].table.DeleteCallback.Invoke(oldValue, message.TransactionUpdate?.Event);
                                            }
                                            catch (Exception e)
                                            {
                                                logger.LogException(e);
                                            }
                                        }

                                        if (events[i].table.RowUpdatedCallback != null)
                                        {
                                            try
                                            {
                                                events[i].table.RowUpdatedCallback
                                                     .Invoke(tableOp, oldValue, null, message.TransactionUpdate?.Event);
                                            }
                                            catch (Exception e)
                                            {
                                                logger.LogException(e);
                                            }
                                        }
                                    }
                                    else
                                    {
                                        logger.LogError("Failed to send callback: invalid delete");
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
                                                events[i].table.UpdateCallback.Invoke(oldValue, newValue, message.TransactionUpdate?.Event);
                                            }
                                        }
                                        catch (Exception e)
                                        {
                                            logger.LogException(e);
                                        }
                                        
                                        try
                                        {
                                            if (events[i].table.RowUpdatedCallback != null)
                                            {
                                                events[i].table.RowUpdatedCallback
                                                         .Invoke(tableOp, oldValue, null, message.TransactionUpdate?.Event);
                                            }
                                        }
                                        catch (Exception e)
                                        {
                                            logger.LogException(e);
                                        }
                                    }
                                    else
                                    {
                                        logger.LogError("Failed to send callback: invalid update");
                                    }

                                    break;
                                }
                            default:
                                throw new ArgumentOutOfRangeException();
                        }

                        onRowUpdate?.Invoke(tableName, tableOp, oldValue, newValue, message.Event?.FunctionCall.CallInfo);
                    }

                    switch (message.TypeCase)
                    {
                        case Message.TypeOneofCase.SubscriptionUpdate:
                            onSubscriptionApplied?.Invoke();
                            break;
                        case Message.TypeOneofCase.TransactionUpdate:
                            onEvent?.Invoke(message.TransactionUpdate.Event);

                            var functionName = message.TransactionUpdate.Event.FunctionCall.Reducer;
                            if (reducerEventCache.TryGetValue(functionName, out var value))
                            {
                                value.Invoke(message.TransactionUpdate.Event);
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
                    onIdentityReceived?.Invoke(message.IdentityToken.Token, Identity.From(message.IdentityToken.Identity.ToByteArray()));
                    break;
                case Message.TypeOneofCase.Event:
                    onEvent?.Invoke(message.Event);
                    break;
            }
        }

        private void OnMessageReceived(byte[] bytes) => _messageQueue.Add(bytes);

        public void InternalCallReducer(string json)
        {
            if(!webSocket.IsConnected)
            {
                logger.LogError("Cannot call reducer, not connected to server!");
                return;
            }
            webSocket.Send(Encoding.ASCII.GetBytes("{ \"call\": " + json + " }"));
        }

        public void Subscribe(List<string> queries)
        {
            if(!webSocket.IsConnected)
            {
                logger.LogError("Cannot subscribe, not connected to server!");
                return;
            }
            var json = JsonConvert.SerializeObject(queries);
            webSocket.Send(Encoding.ASCII.GetBytes("{ \"subscribe\": { \"query_strings\": " + json + " }}"));
        }

        public void Update()
        {
            webSocket.Update();

            while (_nextMessageQueue.TryTake(out var nextMessage))
            {
                OnMessageProcessComplete(nextMessage.message, nextMessage.events);
            }
        }
    }
}