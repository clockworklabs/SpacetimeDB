// Override Inkeep's font to match the site's Value 3 style (Source Code Pro)
// This reaches into Inkeep's shadow DOM to inject custom styles

function injectInkeepFontOverride() {
  // Find all Inkeep shadow DOM containers
  const inkeepElements = document.querySelectorAll('[id^="inkeep-shadowradix"]');

  inkeepElements.forEach((element) => {
    const shadowRoot = element.shadowRoot;
    if (!shadowRoot) return;

    // Check if we already injected styles
    if (shadowRoot.querySelector('#spacetime-inkeep-font-override')) return;

    // Create and inject style element
    const style = document.createElement('style');
    style.id = 'spacetime-inkeep-font-override';
    // Search bar style: Value 3 with Neutral 4 color
    // Target Inkeep's specific data-part attributes for search bar
    style.textContent = `
      /* Search bar text */
      [data-part="search-bar__text"],
      input,
      input::placeholder {
        font-family: 'Source Code Pro Variable', 'Source Code Pro', monospace !important;
        font-size: 14px !important;
        font-style: normal !important;
        font-weight: 600 !important;
        line-height: 20px !important;
        color: #6F7987 !important;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      /* Magnifying glass icon */
      .ikp-search-bar__icon,
      [data-part="icon"] {
        color: #6F7987 !important;
      }

      /* Keyboard shortcut (⌘K) */
      .ikp-search-bar__kbd-wrapper,
      [data-part="search-bar__kbd-wrapper"],
      .ikp-search-bar__kbd-wrapper *,
      .ikp-search-bar__kbd-shortcut-key {
        color: #6F7987 !important;
      }

      /* Button border - n5 default, lighter on hover */
      [data-part="search-bar__button"],
      .ikp-search-bar__button {
        border-color: #363840 !important;
        transition: border-color 0.2s ease !important;
      }

      [data-part="search-bar__button"]:hover,
      .ikp-search-bar__button:hover {
        border-color: #6F7987 !important;
      }
    `;
    shadowRoot.appendChild(style);
  });
}

// Run on initial load
if (typeof window !== 'undefined') {
  // Wait for DOM to be ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      // Give Inkeep time to initialize
      setTimeout(injectInkeepFontOverride, 100);
    });
  } else {
    setTimeout(injectInkeepFontOverride, 100);
  }

  // Also observe for dynamically added Inkeep elements
  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (mutation.type === 'childList') {
        injectInkeepFontOverride();
      }
    }
  });

  // Start observing once DOM is ready
  const startObserving = () => {
    observer.observe(document.body, {
      childList: true,
      subtree: true,
    });
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', startObserving);
  } else {
    startObserving();
  }
}
