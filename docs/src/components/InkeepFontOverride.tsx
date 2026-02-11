'use client';

import { useEffect } from 'react';

const FONT_OVERRIDE_STYLES = `
  * {
    font-family: 'Source Code Pro Variable', 'Source Code Pro', ui-monospace, monospace !important;
  }
  [class*="searchBar"] input,
  [class*="searchBar"] input::placeholder {
    color: #6F7987 !important;
  }
  [class*="searchBar"] svg {
    color: #6F7987 !important;
  }
  kbd {
    color: #6F7987 !important;
  }
  [class*="searchBar"] {
    border-color: #363840 !important;
  }
  [class*="searchBar"]:hover {
    border-color: #6F7987 !important;
  }
`;

function injectStylesIntoShadowRoot(host: Element) {
  const shadowRoot = host.shadowRoot;
  if (!shadowRoot) return;

  const existingStyle = shadowRoot.querySelector('[data-inkeep-font-override]');
  if (existingStyle) return;

  const style = document.createElement('style');
  style.setAttribute('data-inkeep-font-override', 'true');
  style.textContent = FONT_OVERRIDE_STYLES;
  shadowRoot.appendChild(style);
}

function processExistingElements() {
  const elements = document.querySelectorAll('[id^="inkeep-shadowradix"]');
  elements.forEach(injectStylesIntoShadowRoot);
}

export default function InkeepFontOverride() {
  useEffect(() => {
    processExistingElements();

    const observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        for (const node of mutation.addedNodes) {
          if (node instanceof Element) {
            if (node.id?.startsWith('inkeep-shadowradix')) {
              injectStylesIntoShadowRoot(node);
            }
            const descendants = node.querySelectorAll(
              '[id^="inkeep-shadowradix"]'
            );
            descendants.forEach(injectStylesIntoShadowRoot);
          }
        }
      }
    });

    observer.observe(document.body, {
      childList: true,
      subtree: true,
    });

    return () => observer.disconnect();
  }, []);

  return null;
}
