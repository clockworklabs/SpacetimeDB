using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Net.WebSockets;
using CsCheck;
using SpacetimeDB;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using SpacetimeDB.Types;

public class Tests
{
    [Fact]
    public static void DefaultEqualityComparerCheck()
    {
        // Sanity check on the behavior of the default EqualityComparer's Equals function w.r.t. spacetime types.
        var comparer = EqualityComparer<object>.Default;

        // Integers
        int integer = 5;
        int integerByValue = 5;
        int integerUnequalValue = 7;
        string integerAsDifferingType = "5";

        Assert.True(comparer.Equals(integer, integerByValue));
        Assert.False(comparer.Equals(integer, integerUnequalValue));
        // GenericEqualityComparer does not support to converting datatypes and will fail this test
        Assert.False(comparer.Equals(integer, integerAsDifferingType));

        // String
        string testString = "This is a test";
        string testStringByRef = testString;
        string testStringByValue = "This is a test";
        string testStringUnequalValue = "This is not the same string";

        Assert.True(comparer.Equals(testString, testStringByRef));
        Assert.True(comparer.Equals(testString, testStringByValue));
        Assert.False(comparer.Equals(testString, testStringUnequalValue));

        // Note: We are limited to only [SpacetimeDB.Type]

        // Identity and User
        Identity identity = Identity.From(Convert.FromBase64String("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY="));
        Identity identityByRef = identity;
        Identity identityByValue = Identity.From(Convert.FromBase64String("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY="));
        Identity identityUnequalValue = Identity.From(Convert.FromBase64String("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs="));

        User testUser = new User { Identity = identity, Name = "name", Online = false };
        User testUserByRef = testUser;
        User testUserByValue = new User { Identity = identity, Name = "name", Online = false };
        User testUserUnequalIdentityValue = new User { Identity = identityUnequalValue, Name = "name", Online = false };
        User testUserUnequalNameValue = new User { Identity = identity, Name = "unequalName", Online = false };
        User testUserUnequalOnlineValue = new User { Identity = identity, Name = "name", Online = true };

        Assert.True(comparer.Equals(identity, identityByRef));
        Assert.True(comparer.Equals(identity, identityByValue));
        Assert.False(comparer.Equals(identity, identityUnequalValue));

        Assert.True(comparer.Equals(testUser, testUserByRef));
        Assert.True(comparer.Equals(testUser, testUserByValue));
        Assert.False(comparer.Equals(testUser, testUserUnequalIdentityValue));
        Assert.False(comparer.Equals(testUser, testUserUnequalNameValue));
        Assert.False(comparer.Equals(testUser, testUserUnequalOnlineValue));

        // TaggedEnum using Status record
        Status statusCommitted = new Status.Committed(default);
        Status statusCommittedByRef = statusCommitted;
        Status statusCommittedByValue = new Status.Committed(default);
        Status statusFailed = new Status.Failed("Failed");
        Status statusFailedByValue = new Status.Failed("Failed");
        Status statusFailedUnequalValue = new Status.Failed("unequalFailed");
        Status statusOutOfEnergy = new Status.OutOfEnergy(default);

        Assert.True(comparer.Equals(statusCommitted, statusCommittedByRef));
        Assert.True(comparer.Equals(statusCommitted, statusCommittedByValue));
        Assert.False(comparer.Equals(statusCommitted, statusFailed));
        Assert.True(comparer.Equals(statusFailed, statusFailedByValue));
        Assert.False(comparer.Equals(statusFailed, statusFailedUnequalValue));
        Assert.False(comparer.Equals(statusCommitted, statusOutOfEnergy));
    }

    [Fact]
    public static void ListstreamWorks()
    {
        // Make sure ListStream behaves like MemoryStream.

        int listLength = 32;
        Gen.Select(Gen.Byte.List[listLength], Gen.Int[0, 10].SelectMany(n => Gen.Int[0, listLength + 5].List[n].Select(list =>
        {
            list.Sort();
            return list;
        })), (list, cuts) => (list, cuts)).Sample((listCuts) =>
        {
            var (list, cuts) = listCuts;
            var listStream = new ListStream(list);
            var memoryStream = new MemoryStream(list.ToArray());

            for (var i = 0; i < cuts.Count - 1; i++)
            {
                var start = cuts[i];
                var end = cuts[i + 1];

                var arr1 = new byte[end - start];
                Span<byte> span1 = arr1;

                var arr2 = new byte[end - start];
                Span<byte> span2 = arr2;

                var readList = listStream.Read(span1);
                var readMemory = memoryStream.Read(span2);
                Debug.Assert(readList == readMemory);
                Debug.Assert(span1.SequenceEqual(span2));
            }

            listStream = new ListStream(list);
            memoryStream = new MemoryStream(list.ToArray());

            for (var i = 0; i < cuts.Count - 1; i++)
            {
                var start = cuts[i];
                var end = cuts[i + 1];
                var len = end - start;

                var arr1 = new byte[len + 3];
                var arr2 = new byte[len + 3];

                // this is a janky way to choose the offset but I don't feel like plumbing in another randomized list
                var readList = listStream.Read(arr1, len % 3, len);
                var readMemory = memoryStream.Read(arr2, len % 3, len);
                Debug.Assert(readList == readMemory);
                Debug.Assert(arr1.SequenceEqual(arr2));
            }
        });
    }

    [Fact]
    public static void V3BatchSizingCapsAt256KiB()
    {
        var messages = new[]
        {
            new byte[100_000],
            new byte[100_000],
            new byte[100_000],
        };

        Assert.Equal(2, WebSocketV3Frames.CountClientMessagesThatFitInFrame(messages));
        Assert.Equal(1, WebSocketV3Frames.CountClientMessagesThatFitInFrame(new[] { new byte[300_000] }));
        Assert.Equal(0, WebSocketV3Frames.CountClientMessagesThatFitInFrame(Array.Empty<byte[]>()));
    }

    [Fact]
    public static void V3ServerFrameDecodeHandlesSingleAndBatch()
    {
        static byte[] EncodeFrame(ServerFrame frame) =>
            IStructuralReadWrite.ToBytes(new ServerFrame.BSATN(), frame);

        var singlePayload = new byte[] { 1, 2, 3 };
        var single = WebSocketV3Frames.DecodeServerMessages(
            EncodeFrame(new ServerFrame.Single(singlePayload))
        );
        Assert.Single(single);
        Assert.Equal(singlePayload, single[0]);

        var batchPayloads = new[]
        {
            new byte[] { 4, 5 },
            new byte[] { 6, 7, 8 },
        };
        var batch = WebSocketV3Frames.DecodeServerMessages(
            EncodeFrame(new ServerFrame.Batch(batchPayloads))
        );
        Assert.Equal(2, batch.Length);
        Assert.Equal(batchPayloads[0], batch[0]);
        Assert.Equal(batchPayloads[1], batch[1]);
    }

    [Fact]
    public static async Task WebSocketFallsBackToV2WhenServerOnlyNegotiatesV2()
    {
        static int GetFreePort()
        {
            using var listener = new TcpListener(IPAddress.Loopback, 0);
            listener.Start();
            return ((IPEndPoint)listener.LocalEndpoint).Port;
        }

        static async Task WaitForAsync(Task task, SpacetimeDB.WebSocket ws, string error)
        {
            var deadline = DateTime.UtcNow.AddSeconds(5);
            while (!task.IsCompleted)
            {
                ws.Update();
                if (DateTime.UtcNow >= deadline)
                {
                    throw new TimeoutException(error);
                }
                await Task.Delay(10);
            }

            await task;
        }

        var port = GetFreePort();
        using var listener = new HttpListener();
        listener.Prefixes.Add($"http://127.0.0.1:{port}/");
        listener.Start();

        var serverObservedProtocols = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);

        var serverTask = Task.Run(async () =>
        {
            var context = await listener.GetContextAsync();
            serverObservedProtocols.TrySetResult(context.Request.Headers["Sec-WebSocket-Protocol"] ?? string.Empty);

            var webSocketContext = await context.AcceptWebSocketAsync(WebSocketProtocols.V2);
            await Task.Delay(100);
            await webSocketContext.WebSocket.CloseAsync(
                WebSocketCloseStatus.NormalClosure,
                "done",
                CancellationToken.None
            );
        });

        var ws = new SpacetimeDB.WebSocket(new SpacetimeDB.WebSocket.ConnectOptions
        {
            Protocols = WebSocketProtocols.Preferred,
        });

        var connected = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var closed = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);

        ws.OnConnect += () => connected.TrySetResult();
        ws.OnClose += _ => closed.TrySetResult();

        var clientTask = Task.Run(() => ws.Connect(
            "test-token",
            $"ws://127.0.0.1:{port}",
            "example",
            ConnectionId.Random(),
            Compression.None,
            false,
            null
        ));

        await WaitForAsync(connected.Task, ws, "Timed out waiting for websocket connection.");

        Assert.Equal(WebSocketProtocolVersion.V2, ws.ProtocolVersion);

        var offeredProtocols = await serverObservedProtocols.Task.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Contains(WebSocketProtocols.V3, offeredProtocols);
        Assert.Contains(WebSocketProtocols.V2, offeredProtocols);

        await WaitForAsync(closed.Task, ws, "Timed out waiting for websocket close.");
        await serverTask.WaitAsync(TimeSpan.FromSeconds(5));
        await clientTask.WaitAsync(TimeSpan.FromSeconds(5));
    }
}
