import { FunctionVisibilityV11 } from '../lib/autogen/types';

/** Internal functions require verified internal authority. Private functions also
 * admit the owner. Public functions admit any authenticated client. */
export type FunctionVisibility = 'public' | 'private' | 'internal';

export function declaredVisibility(
  visibility: FunctionVisibility | undefined
): FunctionVisibilityV11 | undefined {
  switch (visibility) {
    case undefined:
      return undefined;
    case 'public':
      return FunctionVisibilityV11.ClientCallable;
    case 'private':
      return FunctionVisibilityV11.Private;
    case 'internal':
      return FunctionVisibilityV11.Internal;
    default:
      throw new TypeError('Invalid function visibility');
  }
}
