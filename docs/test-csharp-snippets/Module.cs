#pragma warning disable STDB_UNSTABLE
using SpacetimeDB;
using System.Text.Json;

public static partial class Module
{
    // === Snippet 1: Defining Procedures ===
    [SpacetimeDB.Procedure]
    public static ulong AddTwoNumbers(ProcedureContext ctx, uint lhs, uint rhs)
    {
        return (ulong)lhs + (ulong)rhs;
    }

    // === Snippet 2: Accessing the database ===
    [SpacetimeDB.Table(Name = "MyTable")]
    public partial struct MyTable
    {
        public uint A;
        public string B;
    }

    [SpacetimeDB.Procedure]
    public static void InsertAValue(ProcedureContext ctx, uint a, string b)
    {
        ctx.WithTx(txCtx =>
        {
            txCtx.Db.MyTable.Insert(new MyTable { A = a, B = b });
            return 0;
        });
    }

    // === Snippet 3: Fallible database operations ===
    [SpacetimeDB.Procedure]
    public static void MaybeInsertAValue(ProcedureContext ctx, uint a, string b)
    {
        ctx.WithTx(txCtx =>
        {
            if (a < 10)
            {
                throw new Exception("a is less than 10!");
            }
            txCtx.Db.MyTable.Insert(new MyTable { A = a, B = b });
            return 0;
        });
    }

    // === Snippet 4: Reading values out of the database ===
    [SpacetimeDB.Table(Name = "Player")]
    public partial struct Player
    {
        public Identity Id;
        public uint Level;
    }

    [SpacetimeDB.Procedure]
    public static void FindHighestLevelPlayer(ProcedureContext ctx)
    {
        var highestLevelPlayer = ctx.WithTx(txCtx =>
        {
            Player? highest = null;
            foreach (var player in txCtx.Db.Player.Iter())
            {
                if (highest == null || player.Level > highest.Value.Level)
                {
                    highest = player;
                }
            }
            return highest;
        });

        if (highestLevelPlayer.HasValue)
        {
            Log.Info($"Congratulations to {highestLevelPlayer.Value.Id}");
        }
        else
        {
            Log.Warn("No players...");
        }
    }

    // === Snippet 5: HTTP Requests - Get ===
    [SpacetimeDB.Procedure]
    public static void GetRequest(ProcedureContext ctx)
    {
        var result = ctx.Http.Get("https://example.invalid");
        switch (result)
        {
            case Result<HttpResponse, HttpError>.OkR(var response):
                var body = response.Body.ToStringUtf8Lossy();
                Log.Info($"Got response with status {response.StatusCode} and body {body}");
                break;
            case Result<HttpResponse, HttpError>.ErrR(var e):
                Log.Error($"Request failed: {e.Message}");
                break;
        }
    }

    // === Snippet 6: HTTP Requests - Send ===
    [SpacetimeDB.Procedure]
    public static void PostRequest(ProcedureContext ctx)
    {
        var request = new HttpRequest
        {
            Method = SpacetimeDB.HttpMethod.Post,
            Uri = "https://example.invalid/upload",
            Headers = new List<HttpHeader>
            {
                new HttpHeader("Content-Type", "text/plain")
            },
            Body = HttpBody.FromString("This is the body of the HTTP request")
        };
        var result = ctx.Http.Send(request);
        switch (result)
        {
            case Result<HttpResponse, HttpError>.OkR(var response):
                var body = response.Body.ToStringUtf8Lossy();
                Log.Info($"Got response with status {response.StatusCode} and body {body}");
                break;
            case Result<HttpResponse, HttpError>.ErrR(var e):
                Log.Error($"Request failed: {e.Message}");
                break;
        }
    }

    // === Snippet 7: Calling Reducers from Procedures ===
    // Note: In C#, you can define helper methods that work with the transaction context
    // rather than calling reducers directly.
    private static void ProcessItemLogic(ulong itemId)
    {
        // ... item processing logic
    }

    [SpacetimeDB.Procedure]
    public static void FetchAndProcess(ProcedureContext ctx, string url)
    {
        // Fetch external data
        var result = ctx.Http.Get(url);
        var response = result.UnwrapOrThrow();
        var body = response.Body.ToStringUtf8Lossy();
        var itemId = ParseId(body);

        // Process within a transaction
        ctx.WithTx(txCtx =>
        {
            ProcessItemLogic(itemId);
            return 0;
        });
    }

    private static ulong ParseId(string body)
    {
        // Parse the ID from the response body
        return ulong.Parse(body);
    }

    // === Snippet 8: External AI API example ===
    [SpacetimeDB.Table(Name = "AiMessage", Public = true)]
    public partial struct AiMessage
    {
        public Identity User;
        public string Prompt;
        public string Response;
        public Timestamp CreatedAt;
    }

    [SpacetimeDB.Procedure]
    public static string AskAi(ProcedureContext ctx, string prompt, string apiKey)
    {
        // Build the request to OpenAI's API
        var requestBody = JsonSerializer.Serialize(new
        {
            model = "gpt-4",
            messages = new[] { new { role = "user", content = prompt } }
        });

        var request = new HttpRequest
        {
            Method = SpacetimeDB.HttpMethod.Post,
            Uri = "https://api.openai.com/v1/chat/completions",
            Headers = new List<HttpHeader>
            {
                new HttpHeader("Content-Type", "application/json"),
                new HttpHeader("Authorization", $"Bearer {apiKey}")
            },
            Body = HttpBody.FromString(requestBody)
        };

        // Make the HTTP request
        var response = ctx.Http.Send(request).UnwrapOrThrow();

        if (response.StatusCode != 200)
        {
            throw new Exception($"API returned status {response.StatusCode}");
        }

        var bodyStr = response.Body.ToStringUtf8Lossy();

        // Parse the response
        var aiResponse = ExtractContent(bodyStr)
            ?? throw new Exception("Failed to parse AI response");

        // Store the conversation in the database
        ctx.WithTx(txCtx =>
        {
            txCtx.Db.AiMessage.Insert(new AiMessage
            {
                User = txCtx.Sender,
                Prompt = prompt,
                Response = aiResponse,
                CreatedAt = txCtx.Timestamp
            });
            return 0;
        });

        return aiResponse;
    }

    private static string? ExtractContent(string json)
    {
        // Simple extraction - in production, use proper JSON parsing
        var doc = JsonDocument.Parse(json);
        return doc.RootElement
            .GetProperty("choices")[0]
            .GetProperty("message")
            .GetProperty("content")
            .GetString();
    }

    // === Snippet 9: File Storage - S3 Upload ===
    [SpacetimeDB.Table(Name = "Document", Public = true)]
    public partial struct Document
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public Identity OwnerId;
        public string Filename;
        public string S3Key;
        public Timestamp UploadedAt;
    }

    // Upload file to S3 and register in database
    [SpacetimeDB.Procedure]
    public static string UploadToS3(
        ProcedureContext ctx,
        string filename,
        string contentType,
        List<byte> data,
        string s3Bucket,
        string s3Region)
    {
        // Generate a unique S3 key
        var timestamp = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var s3Key = $"uploads/{timestamp}-{filename}";
        var url = $"https://{s3Bucket}.s3.{s3Region}.amazonaws.com/{s3Key}";

        // Build the S3 PUT request (simplified - add AWS4 signature in production)
        var request = new HttpRequest
        {
            Uri = url,
            Method = SpacetimeDB.HttpMethod.Put,
            Headers = new List<HttpHeader>
            {
                new HttpHeader("Content-Type", contentType),
                new HttpHeader("x-amz-content-sha256", "UNSIGNED-PAYLOAD"),
                // Add Authorization header with AWS4 signature
            },
            Body = new HttpBody(data.ToArray()),
        };

        // Upload to S3
        var response = ctx.Http.Send(request).UnwrapOrThrow();

        if (response.StatusCode != 200)
        {
            throw new Exception($"S3 upload failed with status: {response.StatusCode}");
        }

        // Store metadata in database
        ctx.WithTx(txCtx =>
        {
            txCtx.Db.Document.Insert(new Document
            {
                Id = 0,
                OwnerId = txCtx.Sender,
                Filename = filename,
                S3Key = s3Key,
                UploadedAt = txCtx.Timestamp,
            });
            return 0;
        });

        return s3Key;
    }

    // === Snippet 10: Pre-signed URL Flow ===
    [SpacetimeDB.Type]
    public partial struct UploadInfo
    {
        public string UploadUrl;
        public string S3Key;
    }

    // Procedure returns a pre-signed URL for client-side upload
    [SpacetimeDB.Procedure]
    public static UploadInfo GetUploadUrl(
        ProcedureContext ctx,
        string filename,
        string contentType)
    {
        var timestamp = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var s3Key = $"uploads/{timestamp}-{filename}";

        // Generate pre-signed URL (requires AWS credentials and signing logic)
        var uploadUrl = GeneratePresignedUrl(s3Key, contentType);

        return new UploadInfo { UploadUrl = uploadUrl, S3Key = s3Key };
    }

    // Client uploads directly to S3 using the pre-signed URL, then calls:
    [SpacetimeDB.Reducer]
    public static void ConfirmUpload(ReducerContext ctx, string filename, string s3Key)
    {
        ctx.Db.Document.Insert(new Document
        {
            Id = 0,
            OwnerId = ctx.Sender,
            Filename = filename,
            S3Key = s3Key,
            UploadedAt = ctx.Timestamp,
        });
    }

    private static string GeneratePresignedUrl(string s3Key, string contentType)
    {
        // Implement AWS S3 pre-signed URL generation
        throw new NotImplementedException();
    }

    // === Snippet 11: Schedule Tables ===
    [SpacetimeDB.Table(Scheduled = "SendReminder", ScheduledAt = "ScheduleAt")]
    public partial struct Reminder
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public uint UserId;
        public string Message;
        public ScheduleAt ScheduleAt;
    }

    [SpacetimeDB.Reducer]
    public static void SendReminder(ReducerContext ctx, Reminder reminder)
    {
        // Process the scheduled reminder
    }
}
