import { defineApp } from "convex/server";
import shardedCounter from "@convex-dev/sharded-counter/convex.config.js";

const app = defineApp();

app.use(shardedCounter);

export default app;