import { docs } from 'fumadocs-mdx:collections/server';
import { loader } from 'fumadocs-core/source';
import { createElement } from 'react';

function SparkleIcon() {
  return createElement('svg', {
    width: 16,
    height: 16,
    viewBox: '0 0 24 24',
    fill: 'currentColor',
    style: { color: 'rgba(182, 192, 207, 0.7)' },
  }, createElement('path', {
    d: 'M12 3l1.912 5.813a2 2 0 0 0 1.275 1.275L21 12l-5.813 1.912a2 2 0 0 0-1.275 1.275L12 21l-1.912-5.813a2 2 0 0 0-1.275-1.275L3 12l5.813-1.912a2 2 0 0 0 1.275-1.275L12 3z',
  }));
}

export const source = loader({
  baseUrl: '/',
  source: docs.toFumadocsSource(),
  icon(icon) {
    if (icon === 'sparkle') {
      return createElement(SparkleIcon);
    }
    return undefined;
  },
});
