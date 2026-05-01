import type { HttpMethod, MethodOrAny } from '../lib/http_types';
import type { HttpHandlerExport } from './http_handlers';

type RouteSpec = {
  handler: HttpHandlerExport<any>;
  method: MethodOrAny;
  path: string;
};

const ACCEPTABLE_ROUTE_PATH_CHARS_HUMAN_DESCRIPTION =
  'ASCII lowercase letters, digits and `-_~/`';

function characterIsAcceptableForRoutePath(c: string) {
  return (
    (c >= 'a' && c <= 'z') ||
    (c >= '0' && c <= '9') ||
    c === '-' ||
    c === '_' ||
    c === '~' ||
    c === '/'
  );
}

function assertValidPath(path: string) {
  if (path !== '' && !path.startsWith('/')) {
    throw new TypeError(`Route paths must start with \`/\`: ${path}`);
  }
  if (![...path].every(characterIsAcceptableForRoutePath)) {
    throw new TypeError(
      `Route paths may contain only ${ACCEPTABLE_ROUTE_PATH_CHARS_HUMAN_DESCRIPTION}: ${path}`
    );
  }
}

function routesOverlap(a: RouteSpec, b: RouteSpec) {
  const methodsMatch = (left: HttpMethod, right: HttpMethod) => {
    if (left.tag !== right.tag) {
      return false;
    }
    if (left.tag === 'Extension' && right.tag === 'Extension') {
      return left.value === right.value;
    }
    return true;
  };

  return (
    a.path === b.path &&
    (a.method.tag === 'Any' ||
      b.method.tag === 'Any' ||
      (a.method.tag === 'Method' &&
        b.method.tag === 'Method' &&
        methodsMatch(a.method.value, b.method.value)))
  );
}

function joinPaths(prefix: string, suffix: string) {
  if (prefix === '/') {
    return suffix;
  }
  if (suffix === '/') {
    return prefix;
  }
  const joinedPrefix = prefix.replace(/\/+$/, '');
  const joinedSuffix = suffix.replace(/^\/+/, '');
  return `${joinedPrefix}/${joinedSuffix}`;
}

export class Router {
  #routes: RouteSpec[];

  private constructor(routes: RouteSpec[] = []) {
    this.#routes = routes;
  }

  static new() {
    return new Router();
  }

  get(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Get' } },
      path,
      handler
    );
  }

  head(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Head' } },
      path,
      handler
    );
  }

  options(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Options' } },
      path,
      handler
    );
  }

  put(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Put' } },
      path,
      handler
    );
  }

  delete(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Delete' } },
      path,
      handler
    );
  }

  post(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Post' } },
      path,
      handler
    );
  }

  patch(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Patch' } },
      path,
      handler
    );
  }

  any(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute({ tag: 'Any' }, path, handler);
  }

  nest(path: string, subRouter: Router) {
    assertValidPath(path);
    if (this.#routes.some(route => route.path.startsWith(path))) {
      throw new TypeError(
        `Cannot nest router at \`${path}\`; existing routes overlap with nested path`
      );
    }

    let merged = new Router(this.#routes);
    for (const route of subRouter.#routes) {
      merged = merged.addRoute(
        route.method,
        joinPaths(path, route.path),
        route.handler
      );
    }
    return merged;
  }

  merge(otherRouter: Router) {
    let merged = new Router(this.#routes);
    for (const route of otherRouter.#routes) {
      merged = merged.addRoute(route.method, route.path, route.handler);
    }
    return merged;
  }

  intoRoutes() {
    return this.#routes.slice();
  }

  private addRoute(
    method: MethodOrAny,
    path: string,
    handler: HttpHandlerExport<any>
  ) {
    assertValidPath(path);
    const candidate = { method, path, handler };
    if (this.#routes.some(route => routesOverlap(route, candidate))) {
      throw new TypeError(`Route conflict for \`${path}\``);
    }
    return new Router([...this.#routes, candidate]);
  }
}
