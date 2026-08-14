export const DOCKER_HOST_ALIAS = 'host.docker.internal';

export function dockerHostServiceAddress(networkMode = 'bridge') {
  if (networkMode === 'host') return '127.0.0.1';
  if (networkMode === 'bridge') return DOCKER_HOST_ALIAS;
  throw new Error(`unsupported Docker network mode ${networkMode}`);
}

export function dockerHostGatewayArguments(networkMode = 'bridge') {
  if (networkMode === 'host') return [];
  if (networkMode !== 'bridge') throw new Error(`unsupported Docker network mode ${networkMode}`);
  return ['--add-host', `${DOCKER_HOST_ALIAS}:host-gateway`];
}
