type LabelFilter = Record<string, string>;

export async function fetchMetrics(url: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(
      `metrics GET ${url} failed: ${res.status} ${res.statusText}`,
    );
  }
  return await res.text();
}

export function parseMetricCounter(
  body: string,
  metricName: string,
  labels: LabelFilter,
): bigint | null {
  const lines = body.split('\n');

  for (const line of lines) {
    if (!line.startsWith(metricName)) continue;

    // spacetime_num_txns_total{committed="true",reducer="transfer",txn_type="Reducer"} 619241
    const m = line.match(/^([^{\s]+)(\{([^}]*)\})?\s+([0-9.eE+-]+)$/);
    if (!m) continue;

    const [, , , rawLabelStr = '', rawValue] = m;
    const labelStr = rawLabelStr.trim();

    const parsedLabels: Record<string, string> = {};
    if (labelStr.length > 0) {
      for (const kv of labelStr.split(',')) {
        const trimmed = kv.trim();
        if (!trimmed) continue;
        const idx = trimmed.indexOf('=');
        if (idx === -1) continue;
        const key = trimmed.slice(0, idx).trim();
        const rawVal = trimmed.slice(idx + 1).trim();
        const value = rawVal.replace(/^"|"$/g, '');
        parsedLabels[key] = value;
      }
    }

    let match = true;
    for (const [k, v] of Object.entries(labels)) {
      if (parsedLabels[k] !== v) {
        match = false;
        break;
      }
    }
    if (!match) continue;

    const integerPart = rawValue.split('.')[0];
    try {
      return BigInt(integerPart);
    } catch {
      continue;
    }
  }

  return null;
}

export async function getSpacetimeCommittedTransfers(): Promise<bigint | null> {
  const url =
    process.env.STDB_METRICS_URL ?? 'http://127.0.0.1:3000/v1/metrics';

  const labels: LabelFilter = {
    committed: 'true',
    reducer: 'transfer',
    txn_type: 'Reducer',
  };

  const body = await fetchMetrics(url);
  return parseMetricCounter(body, 'spacetime_num_txns_total', labels);
}
