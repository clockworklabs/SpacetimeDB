using System.Text.Json;
ClientApp? app = null;
var input = Console.In.ReadLineAsync();
while (true)
{
    app?.Tick();
    if (input.IsCompleted)
    {
        var line = input.GetAwaiter().GetResult();
        if (line is null) break;
        using var document = JsonDocument.Parse(line);
        var request = document.RootElement;
        var id = request.GetProperty("id").GetInt32();
        try
        {
            switch (request.GetProperty("command").GetString())
            {
                case "start": app = new ClientApp(request.GetProperty("uri").GetString()!, request.GetProperty("database").GetString()!); break;
                case "add": app!.Add(request.GetProperty("delta").GetInt32()); break;
                case "stop": app!.Stop(); break;
                case "snapshot": break;
                default: throw new Exception("unknown driver command");
            }
            Console.WriteLine("EVAL:" + JsonSerializer.Serialize(new {
                id, ready = app!.Ready, outcome = app.Outcome, value = app.Value,
                changes = app.Changes, identity = app.Connection.Identity?.ToString(),
            }));
        }
        catch (Exception error) { Console.WriteLine("EVAL:" + JsonSerializer.Serialize(new { id, error = error.ToString() })); }
        input = Console.In.ReadLineAsync();
    }
    Thread.Sleep(5);
}
app?.Stop();
