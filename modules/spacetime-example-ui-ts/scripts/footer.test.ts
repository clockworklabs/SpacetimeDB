// @vitest-environment jsdom

import { expect, it } from 'vitest';
import { mountFooter } from '../src/footer';

it('appends a Built on footer that links the wordmark to spacetimedb.com', () => {
  const footer = mountFooter(document.body);

  expect(document.body.lastElementChild).toBe(footer);
  expect(footer.querySelector('span')?.textContent).toBe('Built on');
  const link = footer.querySelector('a');
  expect(link?.href).toBe('https://spacetimedb.com/');
  expect(link?.rel).toBe('noopener noreferrer');
  expect(link?.querySelector('img')?.alt).toBe('SpacetimeDB');
});
