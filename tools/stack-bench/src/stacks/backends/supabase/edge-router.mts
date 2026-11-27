// Stack Bench Edge Functions router. Serves <app>/supabase/functions/<name> at
// /functions/v1/<name>. It has no remote imports. A worker is reused until a file
// under supabase/functions changes, so an edited function is live on the next
// request. A new worker per request held memory until its timeout, and a burst
// of requests exhausted the service. The module cache keeps npm and jsr
// downloads between workers.
const FUNCTIONS = '/home/deno/app/supabase/functions';
const loaded = new Map<string, string>();

async function filesSignature(directory: string): Promise<string> {
  const parts: string[] = [];
  try {
    for await (const entry of Deno.readDir(directory)) {
      const path = `${directory}/${entry.name}`;
      if (entry.isDirectory) parts.push(await filesSignature(path));
      else {
        const { mtime, size } = await Deno.lstat(path);
        parts.push(`${path}:${mtime?.getTime()}:${size}`);
      }
    }
  } catch (error) {
    if (!(error instanceof Deno.errors.NotFound)) throw error;
  }
  return parts.sort().join('\n');
}

Deno.serve(async (req: Request) => {
  const name = new URL(req.url).pathname.split('/')[1];
  if (name === '_health') return new Response('ok');
  if (!name || !/^[a-z0-9][a-z0-9_-]*$/.test(name)) {
    return Response.json({ msg: 'missing function name in request' }, { status: 400 });
  }
  const envVars = Object.entries(Deno.env.toObject());
  try {
    const signature = await filesSignature(FUNCTIONS);
    const forceCreate = loaded.get(name) !== signature;
    loaded.set(name, signature);
    for (;;) {
      // @ts-expect-error EdgeRuntime is supplied by the edge-runtime image.
      const worker = await EdgeRuntime.userWorkers.create({ servicePath: `${FUNCTIONS}/${name}`,
        memoryLimitMb: 150, workerTimeoutMs: 60_000, noModuleCache: false, forceCreate, envVars });
      try {
        return await worker.fetch(req);
      } catch (error) {
        // A worker past its time limit takes no new requests; the next create starts one.
        // @ts-expect-error WorkerAlreadyRetired is supplied by the edge-runtime image.
        if (!(error instanceof Deno.errors.WorkerAlreadyRetired)) throw error;
      }
    }
  } catch (error) {
    return Response.json({ msg: String(error) }, { status: 500 });
  }
});
