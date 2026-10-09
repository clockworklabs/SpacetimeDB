import { TimeDuration } from 'spacetimedb';

export interface Http {
  fetch(
    url: string,
    options?: {
      method?: string;
      headers?: Record<string, string>;
      body?: string;
      timeout?: TimeDuration;
    }
  ): { status: number; json(): unknown };
}

export class ProviderError extends Error {
  constructor(
    code: string,
    readonly status?: number
  ) {
    super(`daytona.${code}`);
  }
}

function object(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new ProviderError('invalid_response');
  }
  return value as Record<string, unknown>;
}

function text(value: unknown, maxLength = 4096): string {
  if (
    typeof value !== 'string' ||
    value.length === 0 ||
    value.length > maxLength
  ) {
    throw new ProviderError('invalid_response');
  }
  return value;
}

// Keep exit, exec, and shell options inside a child process so Daytona can
// record its exit status. Quote the whole command as one POSIX shell argument.
export function shellCommand(command: string): string {
  return `sh -c '${command.replace(/'/g, "'\\''")}'`;
}

function command(value: unknown) {
  const row = object(value);
  if (
    row.exitCode !== undefined &&
    (typeof row.exitCode !== 'number' ||
      !Number.isInteger(row.exitCode) ||
      row.exitCode < -2147483648 ||
      row.exitCode > 2147483647)
  ) {
    throw new ProviderError('invalid_response');
  }
  return {
    id: text(row.id),
    command: text(row.command, 5 * 4096 + 16),
    exitCode: row.exitCode,
  };
}

export interface RemoteSandbox {
  id: string;
  state: string;
  labels: Record<string, string>;
  toolboxProxyUrl?: string;
  autoDestroyAt?: string;
}

export function api(
  http: Http,
  key: string,
  toolboxOrigins = ['https://proxy.app.daytona.io']
) {
  if (!key.trim()) throw new ProviderError('missing_api_key');
  const control = 'https://app.daytona.io/api';

  function request(
    url: string,
    method = 'GET',
    body?: object,
    readBody = true
  ): unknown {
    let response: ReturnType<Http['fetch']>;
    try {
      response = http.fetch(url, {
        method,
        headers: {
          Authorization: `Bearer ${key}`,
          'Content-Type': 'application/json',
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        timeout: new TimeDuration(10_000_000n),
      });
    } catch {
      throw new ProviderError('transport');
    }
    if (response.status < 200 || response.status >= 300) {
      throw new ProviderError(`http_${response.status}`, response.status);
    }
    if (!readBody || method === 'DELETE' || response.status === 204)
      return undefined;
    try {
      return response.json();
    } catch {
      throw new ProviderError('invalid_response');
    }
  }

  function sandbox(value: unknown): RemoteSandbox {
    const row = object(value);
    const labels: Record<string, string> = {};
    for (const [key, value] of Object.entries(object(row.labels)))
      labels[key] = text(value);
    return {
      id: text(row.id),
      state: text(row.state),
      labels,
      toolboxProxyUrl:
        row.toolboxProxyUrl == null ? undefined : text(row.toolboxProxyUrl),
      autoDestroyAt:
        row.autoDestroyAt == null ? undefined : text(row.autoDestroyAt),
    };
  }

  function toolbox(base: string, path: string): string {
    // Module runtimes do not provide the browser URL constructor. Accept only
    // the provider's fixed /toolbox/{id} form on an approved HTTPS origin.
    const match =
      /^(https:\/\/[a-z0-9.-]+(?::[0-9]+)?)\/toolbox\/[a-zA-Z0-9_-]+\/?$/.exec(
        base
      );
    if (!match || !toolboxOrigins.includes(match[1])) {
      throw new ProviderError('untrusted_toolbox');
    }
    return `${base.replace(/\/$/, '')}${path}`;
  }

  return {
    create(
      name: string,
      labels: Record<string, string>,
      snapshot: string,
      ttlMinutes: number
    ) {
      return sandbox(
        request(`${control}/sandbox`, 'POST', {
          name,
          labels,
          snapshot,
          ttlMinutes,
          public: false,
          networkBlockAll: true,
          autoStopInterval: 0,
          autoDeleteInterval: 0,
        })
      );
    },
    sandbox(id: string) {
      return sandbox(request(`${control}/sandbox/${encodeURIComponent(id)}`));
    },
    delete(id: string) {
      request(`${control}/sandbox/${encodeURIComponent(id)}`, 'DELETE');
    },
    toolbox(row: RemoteSandbox) {
      const base =
        row.toolboxProxyUrl ??
        text(
          object(
            request(
              `${control}/sandbox/${encodeURIComponent(row.id)}/toolbox-proxy-url`
            )
          ).url
        );
      return toolbox(
        `${base.replace(/\/$/, '')}/${encodeURIComponent(row.id)}`,
        ''
      );
    },
    createSession(base: string, sessionId: string) {
      request(toolbox(base, '/process/session'), 'POST', { sessionId }, false);
    },
    session(base: string, sessionId: string) {
      const result = object(
        request(
          toolbox(base, `/process/session/${encodeURIComponent(sessionId)}`)
        )
      );
      if (result.sessionId !== sessionId || !Array.isArray(result.commands)) {
        throw new ProviderError('invalid_response');
      }
      return result.commands.map(command);
    },
    submit(base: string, sessionId: string, input: string) {
      return text(
        object(
          request(
            toolbox(
              base,
              `/process/session/${encodeURIComponent(sessionId)}/exec`
            ),
            'POST',
            { command: shellCommand(input), runAsync: true }
          )
        ).cmdId
      );
    },
    command(base: string, sessionId: string, commandId: string) {
      return command(
        request(
          toolbox(
            base,
            `/process/session/${encodeURIComponent(sessionId)}/command/${encodeURIComponent(commandId)}`
          )
        )
      );
    },
  };
}
