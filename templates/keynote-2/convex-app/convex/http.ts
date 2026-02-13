import { httpRouter } from "convex/server";
import { httpAction } from "./_generated/server";
import { api } from "./_generated/api";
import { seed_range } from './seed';

const http = httpRouter();

http.route({
  path: "/seed",
  method: "POST",
  handler: httpAction(async (ctx, req) => {
    const { start, count, initial } = await req.json();
    await ctx.runMutation(api.seed.seed_range, { start: Number(start), count: Number(count), initial: Number(initial) });
    return new Response("ok");
  }),
});

http.route({
  path: "/transfer",
  method: "POST",
  handler: httpAction(async (ctx, req) => {
    const { from, to, amount } = await req.json();
    await ctx.runMutation(api.transfer.transfer, { from_id: Number(from), to_id: Number(to), amount: Number(amount) });
    return new Response("ok");
  }),
});

export default http;
