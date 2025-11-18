---
title: Overview
slug: /sdks
---

SpacetimeDB Client SDKs Overview

The SpacetimeDB Client SDKs provide a comprehensive interface to interact with the SpacetimeDB server engine from various programming languages. Currently, SDKs are available for

- [Rust](/sdks/rust) - [(Quickstart)](/sdks/rust/quickstart)
- [C#](/sdks/c-sharp) - [(Quickstart)](/sdks/c-sharp/quickstart)
- [TypeScript](/sdks/typescript) - [(Quickstart)](/sdks/typescript/quickstart)

## Key Features

The SpacetimeDB Client SDKs offer the following key functionalities:

### Connection Management

The SDKs handle the process of connecting and disconnecting from SpacetimeDB database servers, simplifying this process for the client applications.

### Authentication

The SDKs support authentication using an auth token, allowing clients to securely establish a session with the SpacetimeDB server.

### Local Database View

Each client can define a local view of the database via a subscription consisting of a set of queries. This local view is maintained by the server and populated into a local cache on the client side.

### Reducer Calls

The SDKs allow clients to call transactional functions (reducers) on the server.

### Callback Registrations

The SpacetimeDB Client SDKs offer powerful callback functionality that allow clients to monitor changes in their local database view. These callbacks come in two forms:

#### Connection and Subscription Callbacks

Clients can also register callbacks that trigger when the connection to the database server is established or lost, or when a subscription is updated. This allows clients to react to changes in the connection status.

#### Row Update Callbacks

Clients can register callbacks that trigger when any row in their local cache is updated by the server. These callbacks contain information about the reducer that triggered the change. This feature enables clients to react to changes in data that they're interested in.

#### Reducer Call Callbacks

Clients can also register callbacks that fire when a reducer call modifies something in the client's local view. This allows the client to know when a transactional function it has executed has had an effect on the data it cares about.

Additionally, when a client makes a reducer call that fails, the SDK triggers the registered reducer callback on the client that initiated the failed call with the error message that was returned from the server. This allows for appropriate error handling or user notifications.

## Choosing a Language

When selecting a language for your client application with SpacetimeDB, a variety of factors come into play. While the functionality of the SDKs remains consistent across different languages, the choice of language will often depend on the specific needs and context of your application. Here are a few considerations:

### Team Expertise

The familiarity of your development team with a particular language can greatly influence your choice. You might want to choose a language that your team is most comfortable with to increase productivity and reduce development time.

### Application Type

Different languages are often better suited to different types of applications. For instance, if you are developing a web-based application, you might opt for TypeScript due to its seamless integration with web technologies. On the other hand, if you're developing a desktop application, you might choose C#, depending on your requirements and platform.

### Performance

The performance characteristics of the different languages can also be a factor. If your application is performance-critical, you might opt for Rust, known for its speed and memory efficiency.

### Platform Support

The platform you're targeting can also influence your choice. For instance, if you're developing a game or a 3D application using the Unity engine, you'll want to choose the C# SDK, as Unity uses C# as its primary scripting language.

### Ecosystem and Libraries

Each language has its own ecosystem of libraries and tools that can help in developing your application. If there's a library in a particular language that you want to use, it may influence your choice.

Remember, the best language to use is the one that best fits your use case and the one you and your team are most comfortable with. It's worth noting that due to the consistent functionality across different SDKs, transitioning from one language to another should you need to in the future will primarily involve syntax changes rather than changes in the application's logic.

You may want to use multiple languages in your application. For instance, you might want to use C# in Unity for your game logic and TypeScript for a web-based administration panel. This is perfectly fine, as the SpacetimeDB server is completely client-agnostic.
