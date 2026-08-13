export const DOCKER_HOST_ALIAS = 'host.docker.internal';

export function dockerHostGatewayArguments(networkMode = 'bridge') {
  if (networkMode === 'host') return [];
  if (networkMode !== 'bridge') throw new Error(`unsupported Docker network mode ${networkMode}`);
  return ['--add-host', `${DOCKER_HOST_ALIAS}:host-gateway`];
}
