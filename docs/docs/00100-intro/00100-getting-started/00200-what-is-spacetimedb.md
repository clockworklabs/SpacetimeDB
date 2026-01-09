---
title: What is SpacetimeDB?
slug: /intro/what-is-spacetimedb
---


SpacetimeDB is a database that is also a server.

SpacetimeDB is a full-featured relational database system that lets you run your application logic **inside** the database. You no longer need to deploy a separate web or game server. [Several programming languages](/intro/language-support) are supported, including C# and Rust. You can still write authorization logic, just like you would in a traditional server.

This means that you can write your entire application in a single language and deploy it as a single binary. No more microservices, no more containers, no more Kubernetes, no more Docker, no more VMs, no more DevOps, no more infrastructure, no more ops, no more servers.

<figure>
  <img
    src="/docs/images/basic-architecture-diagram.png"
    alt="SpacetimeDB Architecture"
    style={{ width: '100%' }}
  />
  <figcaption style={{ marginTop: '10px', textAlign: 'center' }} align="center">
    <b align="center">SpacetimeDB application architecture</b>
    <span style={{ fontSize: '14px' }}>
      {' '}
      (elements in white are provided by SpacetimeDB)
    </span>
  </figcaption>
</figure>

In fact, it's so fast that we've been able to write the entire backend of our MMORPG [BitCraft Online](https://bitcraftonline.com) as a single SpacetimeDB database. Everything in the game -- chat messages, items, resources, terrain, and player locations -- is stored and processed by the database. SpacetimeDB [automatically mirrors](#state-mirroring) relevant state to connected players in real-time.

SpacetimeDB is optimized for maximum speed and minimum latency, rather than batch processing or analytical workloads. It is designed for real-time applications like games, chat, and collaboration tools.

Speed and latency is achieved by holding all of your application state in memory, while persisting data to a commit log which is used to recover data after restarts and system crashes.

## Application Workflow Preview

<figure>
  <img
    src="/docs/images/workflow-preview-diagram.png"
    alt="SpacetimeDB Application Workflow Preview"
    style={{ width: '100%' }}
  />
  <figcaption style={{ marginTop: '10px', textAlign: 'center' }} align="center">
    <b align="center">SpacetimeDB Application Workflow Preview</b>
  </figcaption>
</figure>

The above illustrates the workflow when using SpacetimeDB.

- All client-side reads happen with the data view that is cached locally.

- Client-side subscriptions tell the server what data client cares about and wants to be synced within its data view. Changes to data will be pushed by the server to the client cache.

- RLS filters restrict the data view server-side before subscriptions are evaluated. These filters can be used for access control or client scoping.

- Reducers are effectively async RPC's. The request is sent off and if the results of that reducer makes changes to data, it will be written to the database directly. As a result of that, if those changes make it through the two layers above, then the client will see the result when it queries its local cache.

## State Mirroring

SpacetimeDB can generate client code in a [variety of languages](/intro/language-support). This creates a client library custom-designed to talk to your database. It provides easy-to-use interfaces for connecting to the database and submitting requests. It can also **automatically mirror state** from your database to client applications.

You write SQL queries specifying what information a client is interested in -- for instance, the terrain and items near a player's avatar. SpacetimeDB will generate types in your client language for the relevant tables, and feed clients a stream of live updates whenever the database state changes. Note that this is a **read-only** mirror -- the only way to change the database is to submit requests, which are validated on the server.
