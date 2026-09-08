import { FunctionVisibility as RawFunctionVisibility } from '../lib/autogen/types';

/** Internal functions require verified internal authority. Private functions also
 * admit the owner. Public functions admit any authenticated client. */
export type FunctionVisibility = 'public' | 'private' | 'internal';

export function rawVisibility(
  visibility: FunctionVisibility | undefined
): RawFunctionVisibility {
  switch (visibility) {
    case undefined:
      // Preserve V10's existing context-dependent default, including scheduled
      // private functions, without changing the raw definition's field layout.
      return RawFunctionVisibility.ClientCallable;
    case 'public':
      return RawFunctionVisibility.ExplicitClientCallable;
    case 'private':
      return RawFunctionVisibility.Private;
    case 'internal':
      return RawFunctionVisibility.Internal;
    default:
      throw new TypeError('Invalid function visibility');
  }
}
