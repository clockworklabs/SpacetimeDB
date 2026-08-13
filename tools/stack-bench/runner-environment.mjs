export function controllerRunner({ env = process.env, platform = process.platform,
  architecture = process.arch } = {}) {
  return { schemaVersion: 1, mode: env.STACK_BENCH_APPLIANCE === '1' ? 'appliance' : 'local-controller',
    platform, architecture };
}
