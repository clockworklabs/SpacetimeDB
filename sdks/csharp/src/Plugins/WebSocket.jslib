mergeInto(LibraryManager.library, {
    WebSocket_Init: function(openCallback, messageCallback, closeCallback, errorCallback) {
        this._webSocketManager = {
            instances: {},
            nextId: 1,
            callbacks: {
                open: null,
                message: null,
                close: null,
                error: null
            }
        };
        
        var manager = this._webSocketManager;
        manager.callbacks.open = openCallback;
        manager.callbacks.message = messageCallback;
        manager.callbacks.close = closeCallback;
        manager.callbacks.error = errorCallback;
    },

    WebSocket_Connect: async function(baseUriPtr, uriPtr, protocolPtr, authTokenPtr, callbackPtr) {
        try {
            var manager = this._webSocketManager;
            var host = UTF8ToString(baseUriPtr);
            var uri = UTF8ToString(uriPtr);
            var protocol = UTF8ToString(protocolPtr);
            // The C# WebGL bridge can only pass one string argument here, so
            // multiple offered subprotocols are marshalled as a comma-separated string.
            var offeredProtocols = protocol.indexOf(',') === -1 ? protocol : protocol.split(',');
            var authToken = UTF8ToString(authTokenPtr);
            if (authToken)
            {
                var tokenUrl = new URL('v1/identity/websocket-token', host);
                tokenUrl.protocol = host.startsWith("wss://") ? 'https:' : 'http:';
                var headers = new Headers();
                headers.set('Authorization', `Bearer ${authToken}`);

                var response = await fetch(tokenUrl, {
                    method: 'POST',
                    headers: headers
                });
                if (response.ok) {
                    const { token } = await response.json();
                    if (token) {
                        uri += `&token=${token}`;
                    }
                } else {
                    throw new Error(`Failed to verify token: ${response.statusText}`);
                }
            }

            var socket = new window.WebSocket(uri, offeredProtocols);
            socket.binaryType = "arraybuffer";

            var socketId = manager.nextId++;
            manager.instances[socketId] = socket;

            socket.onopen = function() {
                if (manager.callbacks.open) {
                    var protocolStr = socket.protocol || "";
                    // Marshal the negotiated subprotocol to C# just for the duration of
                    // this callback. We use stack allocation because the pointer only
                    // needs to remain valid while dynCall is executing synchronously.
                    var protocolLength = lengthBytesUTF8(protocolStr) + 1;
                    var stack = stackSave();
                    try {
                        var protocolPtr = stackAlloc(protocolLength);
                        // Write a temporary null-terminated UTF-8 string into the
                        // Emscripten stack frame so the C# callback can copy it.
                        stringToUTF8(protocolStr, protocolPtr, protocolLength);
                        dynCall('vii', manager.callbacks.open, [socketId, protocolPtr]);
                    } finally {
                        // Release the temporary stack allocation immediately after
                        // the callback returns; C# must not retain the pointer.
                        stackRestore(stack);
                    }
                }
            };

            socket.onmessage = function(event) {
                if (manager.callbacks.message && event.data instanceof ArrayBuffer) {
                    var buffer = _malloc(event.data.byteLength);
                    HEAPU8.set(new Uint8Array(event.data), buffer);
                    dynCall('viii', manager.callbacks.message, [socketId, buffer, event.data.byteLength]);
                    _free(buffer);
                }
            };
            socket.onclose = function(event) {
                if (manager.callbacks.close) {
                    var reasonStr = event.reason || "";
                    var reasonArray = intArrayFromString(reasonStr);
                    var reasonPtr = _malloc(reasonArray.length);
                    HEAP8.set(reasonArray, reasonPtr);
                    dynCall('viii', manager.callbacks.close, [socketId, event.code, reasonPtr]);
                    _free(reasonPtr);
                }
                delete manager.instances[socketId];
            };

            socket.onerror = function(error) {
                if (manager.callbacks.error) {
                    dynCall('vi', manager.callbacks.error, [socketId]);
                }
            };

            dynCall('vi', callbackPtr, [socketId]);
        } catch (e) {
            console.error("WebSocket connection error:", e);
            dynCall('vi', callbackPtr, [-1]);
        }
    },

    WebSocket_Send: function(socketId, dataPtr, length) {
        var manager = this._webSocketManager;
        var socket = manager.instances[socketId];
        if (!socket || socket.readyState !== socket.OPEN) return -1;

        try {
            var data = new Uint8Array(HEAPU8.buffer, dataPtr, length);
            socket.send(data);
            return 0;
        } catch (e) {
            console.error("WebSocket send error:", e);
            return -1;
        }
    },

    WebSocket_Close: function(socketId, code, reasonPtr) {
        var manager = this._webSocketManager;
        var socket = manager.instances[socketId];
        if (!socket) return;

        var reason = UTF8ToString(reasonPtr);
        socket.close(code, reason);
        delete manager.instances[socketId];
    }
});
