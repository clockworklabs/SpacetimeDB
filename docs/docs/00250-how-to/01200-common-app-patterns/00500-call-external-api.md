---
title: Call an external API from your module
slug: /how-to/app-patterns/call-external-api
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first. This guide uses [Open-Meteo](https://open-meteo.com), a weather API you can call without an API key. Its free API is for non-commercial use, so check its terms before you ship.

There's no C++ version of this guide yet.
:::

In this guide, you'll show live weather in your game world, from a weather API. Your module fetches the weather on a schedule and stores it in a public table, so every client gets it through a subscription, and you make one request per refresh, however many players are connected. You'll make the request from a [procedure](../../00200-core-concepts/00200-functions/00400-procedures.md), call it with a [schedule table](../../00200-core-concepts/00300-tables/00500-schedule-tables.md), and start it from the `init` [lifecycle reducer](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md).

## How it works

- A public `weather` table has a single row with the latest weather: the temperature, a weather code, and when it was fetched.
- A private `weather_refresh` schedule table calls the `refresh_weather` procedure every 15 minutes. The `init` reducer inserts its rows when the database is created.
- `refresh_weather` fetches the current weather from the API, then stores it in `weather`.
- If the request fails, `weather` keeps the last weather, and the next refresh tries again.
- Clients subscribe to `weather`, and update what they show when it changes.

The request is made from a procedure, not a reducer. Reducers can't make HTTP requests, because they run inside a transaction. A procedure makes the request outside of any transaction, then stores the result in a short one.

## Define the tables

Add two tables:

- `weather` holds the latest weather, in a single row with `id` 0. It's public, so clients can subscribe to it.
- `weather_refresh` is the schedule table. Its `scheduled_at` column says when to call `refresh_weather`.

The URL asks Open-Meteo for the current temperature and weather code at one place. Change `latitude` and `longitude` to your game's location.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt, TimeDuration } from 'spacetimedb';
import { schema, table, t, type InferSchema, type ProcedureCtx } from 'spacetimedb/server';

// The current weather in Paris.
const WEATHER_URL =
  'https://api.open-meteo.com/v1/forecast?latitude=48.85&longitude=2.35&current=temperature_2m,weather_code';

// Open-Meteo updates the current weather every 15 minutes.
const REFRESH_INTERVAL_MICROS = 15n * 60n * 1_000_000n;

const weather = table(
  { name: 'weather', public: true },
  {
    id: t.u8().primaryKey(),
    temperature: t.f64(),
    weatherCode: t.u8(),
    fetchedAt: t.timestamp(),
  }
);

const weatherRefresh = table(
  { name: 'weather_refresh' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const spacetimedb = schema({ weather, weatherRefresh });
export default spacetimedb;

type Ctx = ProcedureCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
#pragma warning disable STDB_UNSTABLE
using System.Text.Json.Nodes;
using SpacetimeDB;

public static partial class Module
{
    // The current weather in Paris.
    const string WeatherUrl =
        "https://api.open-meteo.com/v1/forecast?latitude=48.85&longitude=2.35&current=temperature_2m,weather_code";

    // Open-Meteo updates the current weather every 15 minutes.
    static readonly TimeSpan RefreshInterval = TimeSpan.FromMinutes(15);

    [SpacetimeDB.Table(Accessor = "Weather", Public = true)]
    public partial struct Weather
    {
        [SpacetimeDB.PrimaryKey]
        public byte Id;
        public double Temperature;
        public byte WeatherCode;
        public Timestamp FetchedAt;
    }

    [SpacetimeDB.Table(Accessor = "WeatherRefresh", Scheduled = "RefreshWeather", ScheduledAt = "ScheduledAt")]
    public partial struct WeatherRefresh
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::http::{Request, Timeout};
use spacetimedb::{procedure, reducer, table, ProcedureContext, ReducerContext, ScheduleAt, Table, Timestamp};
use std::time::Duration;

/// The current weather in Paris.
const WEATHER_URL: &str =
    "https://api.open-meteo.com/v1/forecast?latitude=48.85&longitude=2.35&current=temperature_2m,weather_code";

/// Open-Meteo updates the current weather every 15 minutes.
const REFRESH_INTERVAL: Duration = Duration::from_secs(15 * 60);

#[table(accessor = weather, public)]
pub struct Weather {
    #[primary_key]
    id: u8,
    temperature: f64,
    weather_code: u8,
    fetched_at: Timestamp,
}

#[table(accessor = weather_refresh, scheduled(refresh_weather))]
pub struct WeatherRefresh {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}
```

Add `serde_json` to your module's `Cargo.toml` to read JSON:

```toml
[dependencies]
serde_json = "1"
```

</TabItem>
</Tabs>

## Start the refresh

Insert two rows into `weather_refresh` from the `init` reducer:

- One that calls `refresh_weather` right away, so clients get the weather as soon as you publish. SpacetimeDB deletes it once it has run.
- One that calls `refresh_weather` every 15 minutes, until the row is deleted. Open-Meteo updates the current weather every 15 minutes, so refreshing more often wouldn't get newer weather.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const init = spacetimedb.init(ctx => {
  ctx.db.weatherRefresh.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch),
  });
  ctx.db.weatherRefresh.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(REFRESH_INTERVAL_MICROS),
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        ctx.Db.WeatherRefresh.Insert(new WeatherRefresh
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(ctx.Timestamp),
        });
        ctx.Db.WeatherRefresh.Insert(new WeatherRefresh
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Interval(RefreshInterval),
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.weather_refresh().insert(WeatherRefresh {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp),
    });
    ctx.db.weather_refresh().insert(WeatherRefresh {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(REFRESH_INTERVAL.into()),
    });
}
```

</TabItem>
</Tabs>

:::note
`init` only runs when the database is created. Publishing new code to an existing database doesn't run it again, so the refresh won't start there. To start it, either republish with `--delete-data`, which erases all data, or insert the rows from another reducer. Before inserting the interval row, check that `weather_refresh` doesn't have one already: with two of them, `refresh_weather` would run twice as often.
:::

## Fetch and store the weather

Add a `fetch_weather` function that requests the current weather, and returns the temperature and the weather code. It's the only code specific to Open-Meteo. It gives up on the request after 10 seconds, and fails if the status isn't 200, or if the reply doesn't have the values it expects.

Then add the `refresh_weather` procedure. SpacetimeDB calls it with each row of `weather_refresh` when the row's time comes. It calls `fetch_weather`, then stores the result in `weather`, in a short transaction. If `fetch_weather` fails, it logs the error and returns, so `weather` keeps the last weather.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Fetches the current weather. Throws if the request fails, or the reply isn't what it expects.
function fetchWeather(ctx: Ctx) {
  const response = ctx.http.fetch(WEATHER_URL, { timeout: TimeDuration.fromMillis(10_000) });
  if (response.status !== 200) {
    throw new Error(`The weather API returned status ${response.status}`);
  }
  const current = response.json().current;
  if (typeof current?.temperature_2m !== 'number' || !Number.isInteger(current?.weather_code)) {
    throw new Error('The weather API returned an unexpected reply');
  }
  return { temperature: current.temperature_2m as number, weatherCode: current.weather_code as number };
}

export const refreshWeather = spacetimedb.procedure(
  { onSchedule: weatherRefresh },
  { refresh: weatherRefresh.rowType },
  t.unit(),
  ctx => {
    let fetched: ReturnType<typeof fetchWeather>;
    try {
      fetched = fetchWeather(ctx);
    } catch (error) {
      console.error(`Could not refresh the weather: ${error}`);
      return {};
    }
    ctx.withTx(tx => {
      const row = { id: 0, ...fetched, fetchedAt: tx.timestamp };
      if (tx.db.weather.id.find(0)) {
        tx.db.weather.id.update(row);
      } else {
        tx.db.weather.insert(row);
      }
    });
    return {};
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Fetches the current weather. Throws if the request fails, or the reply isn't what it expects.
    static (double Temperature, byte WeatherCode) FetchWeather(ProcedureContext ctx)
    {
        var response = ctx.Http.Get(WeatherUrl, TimeSpan.FromSeconds(10)).UnwrapOrThrow();
        if (response.StatusCode != 200)
        {
            throw new Exception($"The weather API returned status {response.StatusCode}");
        }
        var current = JsonNode.Parse(response.Body.ToStringUtf8Lossy())?["current"];
        var temperature = current?["temperature_2m"]?.GetValue<double>();
        var weatherCode = current?["weather_code"]?.GetValue<byte>();
        if (temperature is null || weatherCode is null)
        {
            throw new Exception("The weather API returned an unexpected reply");
        }
        return (temperature.Value, weatherCode.Value);
    }

    [SpacetimeDB.Procedure]
    public static void RefreshWeather(ProcedureContext ctx, WeatherRefresh refresh)
    {
        (double Temperature, byte WeatherCode) fetched;
        try
        {
            fetched = FetchWeather(ctx);
        }
        catch (Exception error)
        {
            Log.Error($"Could not refresh the weather: {error.Message}");
            return;
        }
        ctx.WithTx(tx =>
        {
            var row = new Weather
            {
                Id = 0,
                Temperature = fetched.Temperature,
                WeatherCode = fetched.WeatherCode,
                FetchedAt = tx.Timestamp,
            };
            return tx.Db.Weather.Id.Find(0) is null ? tx.Db.Weather.Insert(row) : tx.Db.Weather.Id.Update(row);
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// Fetches the current weather: the temperature and the weather code.
fn fetch_weather(ctx: &mut ProcedureContext) -> Result<(f64, u8), String> {
    let request = Request::builder()
        .uri(WEATHER_URL)
        .extension(Timeout(Duration::from_secs(10).into()))
        .body(())
        .map_err(|e| e.to_string())?;
    let response = ctx.http.send(request).map_err(|e| format!("Request failed: {e:?}"))?;
    let (parts, body) = response.into_parts();
    if parts.status != 200 {
        return Err(format!("The weather API returned status {}", parts.status));
    }

    let reply: serde_json::Value = serde_json::from_slice(&body.into_bytes()).map_err(|e| e.to_string())?;
    let current = &reply["current"];
    let temperature = current["temperature_2m"].as_f64();
    let weather_code = current["weather_code"].as_u64().and_then(|code| u8::try_from(code).ok());
    match (temperature, weather_code) {
        (Some(temperature), Some(weather_code)) => Ok((temperature, weather_code)),
        _ => Err("The weather API returned an unexpected reply".to_string()),
    }
}

#[procedure]
pub fn refresh_weather(ctx: &mut ProcedureContext, _refresh: WeatherRefresh) {
    let (temperature, weather_code) = match fetch_weather(ctx) {
        Ok(fetched) => fetched,
        Err(error) => {
            log::error!("Could not refresh the weather: {error}");
            return;
        }
    };
    ctx.with_tx(|tx| {
        let row = Weather {
            id: 0,
            temperature,
            weather_code,
            fetched_at: tx.timestamp,
        };
        if tx.db.weather().id().find(0).is_some() {
            tx.db.weather().id().update(row);
        } else {
            tx.db.weather().insert(row);
        }
    });
}
```

</TabItem>
</Tabs>

Clients can't call `refresh_weather` themselves: scheduled procedures are private, so only the scheduler, the database owner and team collaborators can run them.

## Show the weather on the client

On the client, subscribe to `weather`, and show the weather when its row is inserted or updated. The weather code is a [WMO code](https://open-meteo.com/en/docs#weather_variable_documentation), which `describe` turns into a word.

`fetched_at` says when the weather was fetched. You can show it, or hide the weather when it's more than an hour old, in case the API has been failing.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';
import type { Weather } from './module_bindings/types';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.weather]);
  })
  .build();

conn.db.weather.onInsert((_ctx, weather) => showWeather(weather));
conn.db.weather.onUpdate((_ctx, _old, weather) => showWeather(weather));

// A short description of a WMO weather code.
function describe(code: number) {
  if (code === 0) return 'clear';
  if (code <= 2) return 'partly cloudy';
  if (code === 3) return 'cloudy';
  if (code <= 48) return 'foggy';
  if (code <= 67) return 'rainy';
  if (code <= 77) return 'snowy';
  if (code <= 82) return 'showers';
  if (code <= 86) return 'snow showers';
  return 'stormy';
}

function showWeather(weather: Weather) {
  console.log(`${weather.temperature.toFixed(1)} °C, ${describe(weather.weatherCode)}`);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using System;
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.Weather())
            .Subscribe();
    })
    .Build();

conn.Db.Weather.OnInsert += (ctx, weather) => ShowWeather(weather);
conn.Db.Weather.OnUpdate += (ctx, oldWeather, weather) => ShowWeather(weather);

// A short description of a WMO weather code.
string Describe(byte code) => code switch
{
    0 => "clear",
    <= 2 => "partly cloudy",
    3 => "cloudy",
    <= 48 => "foggy",
    <= 67 => "rainy",
    <= 77 => "snowy",
    <= 82 => "showers",
    <= 86 => "snow showers",
    _ => "stormy",
};

void ShowWeather(Weather weather) =>
    Console.WriteLine($"{weather.Temperature:F1} °C, {Describe(weather.WeatherCode)}");
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Table, TableWithPrimaryKey};

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder().add_query(|q| q.from.weather()).subscribe();
        })
        .build()
        .expect("failed to connect");

    conn.db().weather().on_insert(|_ctx, weather| show_weather(weather));
    conn.db().weather().on_update(|_ctx, _old, weather| show_weather(weather));

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

/// A short description of a WMO weather code.
fn describe(code: u8) -> &'static str {
    match code {
        0 => "clear",
        1..=2 => "partly cloudy",
        3 => "cloudy",
        4..=48 => "foggy",
        49..=67 => "rainy",
        68..=77 => "snowy",
        78..=82 => "showers",
        83..=86 => "snow showers",
        _ => "stormy",
    }
}

fn show_weather(weather: &Weather) {
    println!("{:.1} °C, {}", weather.temperature, describe(weather.weather_code));
}
```

</TabItem>
</Tabs>

## What's next?

You now bring live data from an external API into your game:

- A schedule table calls a procedure every 15 minutes, starting from `init`.
- The procedure fetches the data outside of any transaction, and stores it in a public table that clients subscribe to.
- When the API fails, the table keeps the last data, and the next refresh tries again.

To adapt it to your app:

- **APIs that need a key.** Store the key in an environment variable, as in [Call an AI model from your module](./00550-call-ai-model.md#store-the-api-key), and send it in a header.
- **Several locations.** Give `weather` one row per location, with its latitude and longitude. Open-Meteo accepts several comma-separated coordinates in one request, and returns a list, so one request still refreshes them all.
- **Rate limits.** Check the API's limits, and refresh no more often than its data changes. Each refresh is one request, however many players are connected.
- **Refresh on demand.** A reducer can't make the request, but it can insert a row into `weather_refresh` that runs right away. Limit how often players can do it, as the [usernames guide](../01100-social/00100-unique-usernames.md) does for name requests. In a TypeScript module, the [Rate Limit submodule](https://spacetimedb.com/submodules/rate-limit), a package you add to your module, can do it for you.
- **APIs that send you data.** Some APIs call your server when something changes, instead of waiting for you to ask. To receive those calls, add an [HTTP handler](../../00200-core-concepts/00200-functions/00600-HTTP-handlers.md) to your module.

To learn more about the features used here, see [Procedures](../../00200-core-concepts/00200-functions/00400-procedures.md), [Schedule Tables](../../00200-core-concepts/00300-tables/00500-schedule-tables.md) and [Lifecycle Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md).

Next, you can [call an AI model from your module](./00550-call-ai-model.md).
