// Stack Bench Edge Functions router. Serves <app>/supabase/functions/<name> at
// /functions/v1/<name>. It has no remote imports. forceCreate starts a new worker
// per request, so an edited function is live on the next request; the module
// cache keeps npm and jsr downloads between requests.
Deno.serve(async (req: Request) => {
  const name = new URL(req.url).pathname.split('/')[1];
  if (name === '_health') return new Response('ok');
  if (!name || !/^[a-z0-9][a-z0-9_-]*$/.test(name)) {
    return Response.json({ msg: 'missing function name in request' }, { status: 400 });
  }
  const envVars = Object.entries(Deno.env.toObject());
  try {
    // @ts-ignore EdgeRuntime is a global of the edge-runtime image.
    const worker = await EdgeRuntime.userWorkers.create({ servicePath: `/home/deno/app/supabase/functions/${name}`,
      memoryLimitMb: 150, workerTimeoutMs: 60_000, noModuleCache: false, forceCreate: true, envVars });
    return await worker.fetch(req);
  } catch (error) {
    return Response.json({ msg: String(error) }, { status: 500 });
  }
});
