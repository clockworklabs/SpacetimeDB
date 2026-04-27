import { stdbLogger } from './logger.ts';

export const V2_WS_PROTOCOL = 'v2.bsatn.spacetimedb';
export const V3_WS_PROTOCOL = 'v3.bsatn.spacetimedb';
export const PREFERRED_WS_PROTOCOLS = [V3_WS_PROTOCOL, V2_WS_PROTOCOL] as const;

export type NegotiatedWsProtocol =
  | typeof V2_WS_PROTOCOL
  | typeof V3_WS_PROTOCOL;

export function normalizeWsProtocol(protocol: string): NegotiatedWsProtocol {
  if (protocol === V3_WS_PROTOCOL) {
    return V3_WS_PROTOCOL;
  }
  // We treat an empty negotiated subprotocol as legacy v2 for compatibility.
  if (protocol === '' || protocol === V2_WS_PROTOCOL) {
    return V2_WS_PROTOCOL;
  }

  stdbLogger(
    'warn',
    `Unexpected websocket subprotocol "${protocol}", falling back to ${V2_WS_PROTOCOL}.`
  );
  return V2_WS_PROTOCOL;
}
