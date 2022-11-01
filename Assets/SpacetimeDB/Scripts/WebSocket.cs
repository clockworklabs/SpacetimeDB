using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Net.WebSockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

namespace WebSocketDispatch
{
    internal abstract class MainThreadDispatch
    {
        public abstract void Execute();
    }

    class OnConnectMessage : MainThreadDispatch
    {
        private WebSocketOpenEventHandler receiver;

        public OnConnectMessage(WebSocketOpenEventHandler receiver)
        {
            this.receiver = receiver;
        }

        public override void Execute()
        {
            receiver.Invoke();
        }
    }

    class OnDisconnectMessage : MainThreadDispatch
    {
        private WebSocketCloseEventHandler receiver;
        private WebSocketCloseStatus? status;

        public OnDisconnectMessage(WebSocketCloseEventHandler receiver, WebSocketCloseStatus? status)
        {
            this.receiver = receiver;
            this.status = status;
        }

        public override void Execute()
        {
            receiver.Invoke(status);
        }
    }

    class OnMessage : MainThreadDispatch
    {
        private WebSocketMessageEventHandler receiver;
        private byte[] message;

        public OnMessage(WebSocketMessageEventHandler receiver, byte[] message)
        {
            this.receiver = receiver;
            this.message = message;
        }

        public override void Execute()
        {
            receiver.Invoke(message);
        }
    }

    public delegate void WebSocketOpenEventHandler();

    public delegate void WebSocketMessageEventHandler(byte[] message);

    public delegate void WebSocketErrorEventHandler(string errorMsg);

    public delegate void WebSocketCloseEventHandler(WebSocketCloseStatus? closeCode);

    public struct ConnectOptions
    {
        public string Protocol;
    }


    public class WebSocket
    {
        // WebSocket buffer for incoming messages
        private static readonly int MAXMessageSize = 0x2000000; // 32MB

        // Connection parameters
        private readonly ConnectOptions _options;
        private readonly byte[] _receiveBuffer = new byte[MAXMessageSize];
        private readonly ConcurrentQueue<MainThreadDispatch> dispatchQueue = new ConcurrentQueue<MainThreadDispatch>();

        protected ClientWebSocket Ws;

        public WebSocket(ConnectOptions options)
        {
            Ws = new ClientWebSocket();
            _options = options;
        }

        public event WebSocketOpenEventHandler OnConnect;
        public event WebSocketMessageEventHandler OnMessage;
        public event WebSocketCloseEventHandler OnClose;

        public async Task Connect(string auth, string host, string nameOrAddress)
        {
            var url = new Uri($"ws://{host}/database/subscribe?name_or_address={nameOrAddress}");
            Ws.Options.AddSubProtocol(_options.Protocol);

            var source = new CancellationTokenSource(10000);
            if (!string.IsNullOrEmpty(auth))
            {
                var tokenBytes = Encoding.UTF8.GetBytes($"token:{auth}");
                var base64 = Convert.ToBase64String(tokenBytes);
                Ws.Options.SetRequestHeader("Authorization", "Basic " + base64);
            }
            else
            {
                Ws.Options.UseDefaultCredentials = true;
            }

            try
            {
                await Ws.ConnectAsync(url, source.Token);
                dispatchQueue.Enqueue(new OnConnectMessage(OnConnect));
            }
            catch (WebSocketException ex)
            {
                if (ex.WebSocketErrorCode == WebSocketError.UnsupportedProtocol)
                {
                    Debug.LogError("Unsupported protocol.");
                    return;
                }

                Debug.LogError("Error connecting: " + ex);
                return;
            }
            catch (Exception e)
            {
                Debug.LogError("Other error: " + e);
                return;
            }

            while (Ws.State == WebSocketState.Open)
            {
                try
                {
                    var receiveResult = await Ws.ReceiveAsync(new ArraySegment<byte>(_receiveBuffer),
                        CancellationToken.None);
                    if (receiveResult.MessageType == WebSocketMessageType.Close)
                    {
                        await Ws.CloseAsync(WebSocketCloseStatus.NormalClosure, string.Empty,
                            CancellationToken.None);
                        if (receiveResult.CloseStatus != WebSocketCloseStatus.NormalClosure)
                        {
                            Debug.LogError("Server closed connection abnormally.");
                            dispatchQueue.Enqueue(new OnDisconnectMessage(OnClose, receiveResult.CloseStatus));
                        }
                    }
                    else
                    {
                        var count = receiveResult.Count;
                        while (receiveResult.EndOfMessage == false)
                        {
                            if (count >= MAXMessageSize)
                            {
                                var closeMessage = $"Maximum message size: {MAXMessageSize} bytes.";
                                await Ws.CloseAsync(WebSocketCloseStatus.MessageTooBig, closeMessage,
                                    CancellationToken.None);
                                return;
                            }

                            receiveResult = await Ws.ReceiveAsync(
                                new ArraySegment<byte>(_receiveBuffer, count, MAXMessageSize - count),
                                CancellationToken.None);
                            count += receiveResult.Count;
                        }

                        var buffCopy = new byte[count];
                        for (var x = 0; x < count; x++)
                            buffCopy[x] = _receiveBuffer[x];
                        dispatchQueue.Enqueue(new OnMessage(OnMessage, buffCopy));
                    }
                }
                catch (WebSocketException ex)
                {
                    if (ex.WebSocketErrorCode == WebSocketError.ConnectionClosedPrematurely)
                    {
                        Debug.LogError("Server closed connection prematurely.");
                        dispatchQueue.Enqueue(new OnDisconnectMessage(OnClose, null));
                        break;
                    }

                    Debug.LogError(ex);
                }
            }
        }

        public Task Close(WebSocketCloseStatus code = WebSocketCloseStatus.NormalClosure, string reason = null)
        {
            Ws?.CloseAsync(code, "Disconnecting normally.", CancellationToken.None);
            Ws = null;

            return Task.CompletedTask;
        }

        private readonly object sendingLock = new object();
        private Task senderTask = null;
        private readonly ConcurrentQueue<byte[]> messageSendQueue = new ConcurrentQueue<byte[]>();

        /// <summary>
        /// This sender guarantees that that messages are sent out in the order they are received. Our websocket
        /// library only allows us to await one send call, so we have to wait until the current send call is complete
        /// before we start another one. This function is also thread safe, just in case.
        /// </summary>
        /// <param name="message">The message to send</param>
        public void Send(byte[] message)
        {
            lock (messageSendQueue)
            {
                messageSendQueue.Enqueue(message);
                if (senderTask == null)
                {
                    senderTask = Task.Run(async () => { await ProcessSendQueue(); });
                }
            }
        }


        private async Task ProcessSendQueue()
        {
            while (true)
            {
                byte[] message;
                lock (messageSendQueue)
                {
                    if (!messageSendQueue.TryDequeue(out message))
                    {
                        // We are out of messages to send
                        senderTask = null;
                        return;
                    }
                }

                await Ws!.SendAsync(new ArraySegment<byte>(message), WebSocketMessageType.Text, true,
                    CancellationToken.None);
            }
        }

        public WebSocketState GetState()
        {
            return Ws!.State;
        }

        public void Update()
        {
            while (dispatchQueue.TryDequeue(out var result))
            {
                result.Execute();
            }
        }
    }
}