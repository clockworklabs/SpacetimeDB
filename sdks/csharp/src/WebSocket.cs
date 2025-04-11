using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;

using System;
using System.Collections.Concurrent;
using System.Linq;
using System.Net.Sockets;
using System.Net.WebSockets;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace SpacetimeDB
{
    internal class WebSocket
    {
        public delegate void OpenEventHandler();

        public delegate void MessageEventHandler(byte[] message, DateTime timestamp);

        public delegate void CloseEventHandler(Exception? e);

        public delegate void ConnectErrorEventHandler(Exception e);
        public delegate void SendErrorEventHandler(Exception e);

        public struct ConnectOptions
        {
            public string Protocol;
        }

        // WebSocket buffer for incoming messages
        private static readonly int MAXMessageSize = 0x4000000; // 64MB

        // Connection parameters
        private readonly ConnectOptions _options;
        private readonly byte[] _receiveBuffer = new byte[MAXMessageSize];
        private readonly ConcurrentQueue<Action> dispatchQueue = new();

        protected ClientWebSocket Ws = new();

        public WebSocket(ConnectOptions options)
        {
            _options = options;
#if UNITY_WEBGL && !UNITY_EDITOR
            InitializeWebGL();
#endif
        }

        public event OpenEventHandler? OnConnect;
        public event ConnectErrorEventHandler? OnConnectError;
        public event SendErrorEventHandler? OnSendError;
        public event MessageEventHandler? OnMessage;
        public event CloseEventHandler? OnClose;

#if UNITY_WEBGL && !UNITY_EDITOR
        private bool _isConnected = false;
        private bool _isConnecting = false;
        public bool IsConnected => _isConnected;
#else 
        public bool IsConnected { get { return Ws != null && Ws.State == WebSocketState.Open; } }
#endif
        
#if UNITY_WEBGL && !UNITY_EDITOR
[DllImport("__Internal")]
    private static extern void WebSocket_Init(
        IntPtr openCallback,
        IntPtr messageCallback,
        IntPtr closeCallback,
        IntPtr errorCallback
    );

    [DllImport("__Internal")]
    private static extern int WebSocket_Connect(string uri, string protocol, string authToken);

    [DllImport("__Internal")]
    private static extern int WebSocket_Send(int socketId, byte[] data, int length);

    [DllImport("__Internal")]
    private static extern void WebSocket_Close(int socketId, int code, string reason);

    [AOT.MonoPInvokeCallback(typeof(Action<int>))]
    private static void WebGLOnOpen(int socketId)
    {
        Instance?.HandleWebGLOpen(socketId);
    }

    [AOT.MonoPInvokeCallback(typeof(Action<int, IntPtr, int>))]
    private static void WebGLOnMessage(int socketId, IntPtr dataPtr, int length)
    {
        try {
            byte[] data = new byte[length];
            Marshal.Copy(dataPtr, data, 0, length);
            Instance?.HandleWebGLMessage(socketId, data);
        } catch (Exception e) {
            UnityEngine.Debug.LogError($"Error handling message: {e}");
        }
    }

    [AOT.MonoPInvokeCallback(typeof(Action<int, int, IntPtr>))]
    private static void WebGLOnClose(int socketId, int code, IntPtr reasonPtr)
    {
        try {
            string reason = Marshal.PtrToStringUTF8(reasonPtr);
            Instance?.HandleWebGLClose(socketId, code, reason);
        } catch (Exception e) {
            UnityEngine.Debug.LogError($"Error handling close: {e}");
        }
    }

    [AOT.MonoPInvokeCallback(typeof(Action<int>))]
    private static void WebGLOnError(int socketId)
    {
        Instance?.HandleWebGLError(socketId);
    }

    private static WebSocket Instance;
    private int _webglSocketId = -1;

    private void InitializeWebGL()
    {
        Instance = this;
        // Convert callbacks to function pointers
        var openPtr = Marshal.GetFunctionPointerForDelegate((Action<int>)WebGLOnOpen);
        var messagePtr = Marshal.GetFunctionPointerForDelegate((Action<int, IntPtr, int>)WebGLOnMessage);
        var closePtr = Marshal.GetFunctionPointerForDelegate((Action<int, int, IntPtr>)WebGLOnClose);
        var errorPtr = Marshal.GetFunctionPointerForDelegate((Action<int>)WebGLOnError);
        
        WebSocket_Init(openPtr, messagePtr, closePtr, errorPtr);
    }
#endif

        public async Task Connect(string? auth, string host, string nameOrAddress, ConnectionId connectionId, Compression compression, bool light)
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            if (_isConnecting || _isConnected) return;
    
            _isConnecting = true;
            try
            {
                var uri = $"{host}/v1/database/{nameOrAddress}/subscribe?connection_id={connectionId}&compression={compression}";
                if (light) uri += "&light=true";
        
                _webglSocketId = WebSocket_Connect(uri, _options.Protocol, auth);
                if (_webglSocketId == -1)
                {
                    dispatchQueue.Enqueue(() => OnConnectError?.Invoke(
                        new Exception("Failed to connect WebSocket")));
                }
            }
            catch (Exception e)
            {
                dispatchQueue.Enqueue(() => OnConnectError?.Invoke(e));
            }
            finally
            {
                _isConnecting = false;
            }
        // Events will be handled via UnitySendMessage callbacks
#else
            var uri = $"{host}/v1/database/{nameOrAddress}/subscribe?connection_id={connectionId}&compression={compression}";
            if (light)
            {
                uri += "&light=true";
            }
            var url = new Uri(uri);
            Ws.Options.AddSubProtocol(_options.Protocol);

            var source = new CancellationTokenSource(10000);
            if (!string.IsNullOrEmpty(auth))
            {
                Ws.Options.SetRequestHeader("Authorization", $"Bearer {auth}");
            }
            else
            {
                Ws.Options.UseDefaultCredentials = true;
            }

            try
            {
                await Ws.ConnectAsync(url, source.Token);
                if (Ws.State == WebSocketState.Open)
                {
                    if (OnConnect != null)
                    {
                        dispatchQueue.Enqueue(() => OnConnect());
                    }
                }
                else
                {
                    if (OnConnectError != null)
                    {
                        dispatchQueue.Enqueue(() => OnConnectError(
                            new Exception($"WebSocket connection failed. Current state: {Ws.State}")));
                    }
                    return;
                }
            }
            catch (WebSocketException ex) when (ex.WebSocketErrorCode == WebSocketError.Success)
            {
                // How can we get here:
                // - When you go to connect and the server isn't running (port closed) - target machine actively refused
                // - 404 - No module with at that module address instead of 101 upgrade
                // - 401? - When the identity received by SpacetimeDB wasn't signed by its signing key
                // - 400 - When the auth is malformed
                if (OnConnectError != null)
                {
                    // .net 6,7,8 has support for Ws.HttpStatusCode as long as you set
                    // ClientWebSocketOptions.CollectHttpResponseDetails = true
                    var message = "A WebSocketException occurred, even though the WebSocketErrorCode is \"Success\".\n"
                    + "This indicates that there was no native error information for the exception.\n"
                    + "Due to limitations in the .NET core version we do not have access to the HTTP status code returned by the request which would provide more info on the nature of the error.\n\n"
                    + "This error could arise for a number of reasons:\n"
                    + "1. The target machine actively refused the connection.\n"
                    + "2. The module you are trying to connect to does not exist (404 NOT FOUND).\n"
                    + "3. The auth token you sent to SpacetimeDB was not signed by the correct signing key (400 BAD REQUEST).\n"
                    + "4. The auth token is malformed (400 BAD REQUEST).\n"
                    + "5. You are not authorized (401 UNAUTHORIZED).\n\n"
                    + "Did you forget to start the server or publish your module?\n\n"
                    + "Here are some values that might help you debug:\n"
                    + $"Message: {ex.Message}\n"
                    + $"WebSocketErrorCode: {ex.WebSocketErrorCode}\n"
                    + $"ErrorCode: {ex.ErrorCode}\n"
                    + $"NativeErrorCode: {ex.NativeErrorCode}\n"
                    + $"InnerException Message: {ex.InnerException?.Message}\n"
                    + $"WebSocket CloseStatus: {Ws.CloseStatus}\n"
                    + $"WebSocket State: {Ws.State}\n"
                    + $"InnerException: {ex.InnerException}\n"
                    + $"Exception: {ex}"
                    ;
                    dispatchQueue.Enqueue(() => OnConnectError(new Exception(message)));
                }
            }
            catch (WebSocketException ex)
            {
                if (OnConnectError != null)
                {
                    var message = $"WebSocket connection failed: {ex.WebSocketErrorCode}\n"
                    + $"Exception message: {ex.Message}\n";
                    dispatchQueue.Enqueue(() => OnConnectError(new Exception(message)));
                }
            }
            catch (SocketException ex)
            {
                // This might occur if the server is unreachable or the DNS lookup fails.
                if (OnConnectError != null)
                {
                    dispatchQueue.Enqueue(() => OnConnectError(ex));
                }
            }
            catch (Exception ex)
            {
                if (OnConnectError != null)
                {
                    dispatchQueue.Enqueue(() => OnConnectError(ex));
                }
            }

            while (Ws.State == WebSocketState.Open)
            {
                try
                {
                    var receiveResult = await Ws.ReceiveAsync(new ArraySegment<byte>(_receiveBuffer),
                        CancellationToken.None);
                    if (receiveResult.MessageType == WebSocketMessageType.Close)
                    {
                        if (Ws.State != WebSocketState.Closed)
                        {
                            await Ws.CloseAsync(WebSocketCloseStatus.NormalClosure, string.Empty,
                            CancellationToken.None);
                        }
                        if (OnClose != null)
                        {
                            switch (receiveResult.CloseStatus)
                            {
                                case WebSocketCloseStatus.NormalClosure:
                                    dispatchQueue.Enqueue(() => OnClose(null));
                                    break;
                                case WebSocketCloseStatus.EndpointUnavailable:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1000) The connection has closed after the request was fulfilled.")));
                                    break;
                                case WebSocketCloseStatus.ProtocolError:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1002) The client or server is terminating the connection because of a protocol error.")));
                                    break;
                                case WebSocketCloseStatus.InvalidMessageType:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1003) The client or server is terminating the connection because it cannot accept the data type it received.")));
                                    break;
                                case WebSocketCloseStatus.Empty:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1005) No error specified.")));
                                    break;
                                case WebSocketCloseStatus.InvalidPayloadData:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1007) The client or server is terminating the connection because it has received data inconsistent with the message type.")));
                                    break;
                                case WebSocketCloseStatus.PolicyViolation:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1008) The connection will be closed because an endpoint has received a message that violates its policy.")));
                                    break;
                                case WebSocketCloseStatus.MessageTooBig:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1009) Message too big")));
                                    break;
                                case WebSocketCloseStatus.MandatoryExtension:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1010) The client is terminating the connection because it expected the server to negotiate an extension.")));
                                    break;
                                case WebSocketCloseStatus.InternalServerError:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("(1011) The connection will be closed by the server because of an error on the server.")));
                                    break;
                                default:
                                    dispatchQueue.Enqueue(() => OnClose(new Exception("Unknown error")));
                                    break;
                            }
                        }
                        return;
                    }

                    var startReceive = DateTime.UtcNow;
                    var count = receiveResult.Count;
                    while (receiveResult.EndOfMessage == false)
                    {
                        if (count >= MAXMessageSize)
                        {
                            // TODO: Improve this, we should allow clients to receive messages of whatever size
                            var closeMessage = $"Maximum message size: {MAXMessageSize} bytes.";
                            await Ws.CloseAsync(WebSocketCloseStatus.MessageTooBig, closeMessage,
                                CancellationToken.None);
                            if (OnClose != null)
                            {
                                dispatchQueue.Enqueue(() => OnClose(new Exception("(1009) Message too big")));
                            }
                            return;
                        }

                        receiveResult = await Ws.ReceiveAsync(
                            new ArraySegment<byte>(_receiveBuffer, count, MAXMessageSize - count),
                            CancellationToken.None);
                        count += receiveResult.Count;
                    }

                    if (OnMessage != null)
                    {
                        var message = _receiveBuffer.Take(count).ToArray();
                        dispatchQueue.Enqueue(() => OnMessage(message, startReceive));
                    }
                }
                catch (WebSocketException ex)
                {
                    if (OnClose != null) dispatchQueue.Enqueue(() => OnClose(ex));
                    return;
                }
            }
#endif
        }

        public Task Close(WebSocketCloseStatus code = WebSocketCloseStatus.NormalClosure)
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            if (_isConnected)
            {
                HandleWebGLClose(_webglSocketId, (int)code, "Disconnecting normally.");
                _isConnected = false;
            }
#else
            if (Ws?.State == WebSocketState.Open)
            {
                return Ws.CloseAsync(code, "Disconnecting normally.", CancellationToken.None);
            }
#endif
             return Task.CompletedTask;
         }

        private Task? senderTask;
        private readonly ConcurrentQueue<ClientMessage> messageSendQueue = new();

        /// <summary>
        /// This sender guarantees that that messages are sent out in the order they are received. Our websocket
        /// library only allows us to await one send call, so we have to wait until the current send call is complete
        /// before we start another one. This function is also thread safe, just in case.
        /// </summary>
        /// <param name="message">The message to send</param>
        public void Send(ClientMessage message)
        {
#if UNITY_WEBGL && !UNITY_EDITOR
            try
            {
                var messageBSATN = new ClientMessage.BSATN();
                var encodedMessage = IStructuralReadWrite.ToBytes(messageBSATN, message);
                WebSocket_Send(_webglSocketId, encodedMessage, encodedMessage.Length);
            }
            catch (Exception e)
            {
                UnityEngine.Debug.LogError($"WebSocket send error: {e}");
                dispatchQueue.Enqueue(() => OnSendError?.Invoke(e));
            }
#else
            lock (messageSendQueue)
            {
                messageSendQueue.Enqueue(message);
                senderTask ??= Task.Run(ProcessSendQueue);
            }
#endif
        }

        private async Task ProcessSendQueue()
        {
            try
            {
                while (true)
                {
                    ClientMessage message;

                    lock (messageSendQueue)
                    {
                        if (!messageSendQueue.TryDequeue(out message))
                        {
                            // We are out of messages to send
                            senderTask = null;
                            return;
                        }
                    }

                    var messageBSATN = new ClientMessage.BSATN();
                    var encodedMessage = IStructuralReadWrite.ToBytes(messageBSATN, message);
                    await Ws!.SendAsync(encodedMessage, WebSocketMessageType.Binary, true, CancellationToken.None);
                }
            }
            catch (Exception e)
            {
                senderTask = null;
                if (OnSendError != null) dispatchQueue.Enqueue(() => OnSendError(e));
            }
        }

        public WebSocketState GetState()
        {
            return Ws!.State;
        }
#if UNITY_WEBGL && !UNITY_EDITOR
        public void HandleWebGLOpen(int socketId)
        {
            if (socketId == _webglSocketId)
            {
                _isConnected = true;
                if (OnConnect != null)
                    dispatchQueue.Enqueue(() => OnConnect());
            }
        }
        
        public void HandleWebGLMessage(int socketId, byte[] message)
        {
            UnityEngine.Debug.Log($"HandleWebGLMessage: {message.Length}");
            if (socketId == _webglSocketId && OnMessage != null)
            {
                dispatchQueue.Enqueue(() => OnMessage(message, DateTime.UtcNow));
            }
        }
        
        public void HandleWebGLClose(int socketId, int code, string reason)
        {
            UnityEngine.Debug.Log($"HandleWebGLClose: {code} {reason}");
            if (socketId == _webglSocketId && OnClose != null)
            {
                _isConnected = false;
                var ex = code != 1000 ? new Exception($"WebSocket closed with code {code}: {reason}") : null;
                dispatchQueue.Enqueue(() => OnClose?.Invoke(ex));
            }
        }
        
        public void HandleWebGLError(int socketId)
        {
            UnityEngine.Debug.Log($"HandleWebGLError: {socketId}");
            if (socketId == _webglSocketId && OnConnectError != null)
            {
                dispatchQueue.Enqueue(() => OnConnectError(new Exception($"Socket {socketId} error.")));
            }
        }
#endif

        public void Update()
        {
            while (dispatchQueue.TryDequeue(out var result))
            {
                result();
            }
        }
    }
}
