# SpacetimeDB Quickstart in TypeScript

This is a Vite+React+TypeScript project that demonstrates how to use SpacetimeDB in a web application.

## Getting started

1. `npm install`(or `pnpm install`) to install the dependencies.
2. `npm run dev` to start the development server.
3. Open `http://localhost:5173` in your browser.

### Run the Spacetime DB Server

1. Open a terminal, `cd` to this folder, and type `npm run spacetime:start`. This will start spacetime DB server on `http://localhost:3000`.
2. Open another terminal, run `npm run spacetime:publish:local`. This will build the Spacetime rust files and publish them to the local server, with the database name `chat`.
3. Run `npm run spacetime:generate-bindings` to generate the TypeScript bindings for the Spacetime database.

That should be it! You should now be able to run the app and see the chat messages being sent and received.
