import { leaseFromEnv } from '../../runtime/backend-lease.js';

type ContainerImage = { reference: string; imageId: string | null | undefined };

// Record the database image and every platform service image, by role.
export function supabaseSetupMetadata({ helpers, env = process.env }: {
  env?: NodeJS.ProcessEnv;
  helpers: { containerImage: (container: string) => ContainerImage };
}) {
  const { lease } = leaseFromEnv(env, { backend: 'supabase', active: true });
  const { container, serviceContainers } = lease.resources;
  if (!container || !serviceContainers) throw new Error('Supabase setup requires its owned platform');
  return { spacetime: null, spacetimeBindings: null,
    database: helpers.containerImage(container.id),
    platformServices: Object.fromEntries(Object.entries(serviceContainers)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([role, service]) => [role, helpers.containerImage(service.id)])) };
}
