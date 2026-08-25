const STORAGE_KEYS = {
  cart: 'stdb.premiumStore.cart.v1',
};

const state = {
  catalog: [],
  cart: [],
  checkoutPending: false,
  animatedStepperKey: null,
};

const byId = id => document.getElementById(id);

const ui = {
  btnCart: byId('btnCart'),
  btnSettings: byId('btnSettings'),
  cartBackdrop: byId('cartBackdrop'),
  cartPopout: byId('cartPopout'),
  btnCartClose: byId('btnCartClose'),
  btnContinueShopping: byId('btnContinueShopping'),
  cartList: byId('cartList'),
  cartSummary: byId('cartSummary'),
  catalogGrid: byId('catalogGrid'),
  email: byId('email'),
  name: byId('name'),
  userId: byId('userId'),
  customerId: byId('customerId'),
  btnCheckoutCart: byId('btnCheckoutCart'),
  btnDevToolsClose: byId('btnDevToolsClose'),
  btnCustomer: byId('btnCustomer'),
  devToolsBackdrop: byId('devToolsBackdrop'),
  devToolsPopout: byId('devToolsPopout'),
  errorLine: byId('errorLine'),
  log: byId('log'),
  checkoutBanner: byId('checkoutBanner'),
  checkoutBannerText: byId('checkoutBannerText'),
  checkoutBannerClose: byId('checkoutBannerClose'),
};

let errorLineTimer = null;
function setError(message) {
  if (errorLineTimer) {
    clearTimeout(errorLineTimer);
    errorLineTimer = null;
  }
  if (!message) {
    ui.errorLine.innerHTML = '';
    return;
  }
  ui.errorLine.innerHTML = '';
  const pill = document.createElement('div');
  pill.className = 'error-line-msg';
  pill.textContent = message;
  pill.addEventListener('click', () => setError(''));
  ui.errorLine.appendChild(pill);
  errorLineTimer = setTimeout(() => setError(''), 6000);
}

let checkoutBannerTimer = null;
function showCheckoutBanner(kind, message) {
  if (!ui.checkoutBanner) return;
  ui.checkoutBanner.hidden = false;
  ui.checkoutBanner.classList.remove('success', 'canceled');
  ui.checkoutBanner.classList.add(kind);
  ui.checkoutBannerText.innerHTML = message;
  void ui.checkoutBanner.offsetWidth;
  ui.checkoutBanner.classList.add('is-visible');
  if (checkoutBannerTimer) clearTimeout(checkoutBannerTimer);
  checkoutBannerTimer = setTimeout(hideCheckoutBanner, 8000);
}
function hideCheckoutBanner() {
  if (!ui.checkoutBanner) return;
  ui.checkoutBanner.classList.remove('is-visible');
  if (checkoutBannerTimer) {
    clearTimeout(checkoutBannerTimer);
    checkoutBannerTimer = null;
  }
}
function consumePostCheckoutQueryFlags() {
  const url = new URL(window.location.href);
  const purchased = url.searchParams.get('purchased') === '1';
  const canceled = url.searchParams.get('canceled') === '1';
  if (!purchased && !canceled) return;

  if (purchased) {
    state.cart = [];
    state.animatedStepperKey = null;
    persistCart();
    showCheckoutBanner(
      'success',
      '<strong>Checkout complete.</strong> Stripe confirmed the session.'
    );
  } else {
    showCheckoutBanner(
      'canceled',
      '<strong>Checkout canceled.</strong> No charge was made. Your cart is still here.'
    );
  }
  url.searchParams.delete('purchased');
  url.searchParams.delete('canceled');
  window.history.replaceState({}, '', url.toString());
}

function writeLog(message) {
  const time = new Date().toLocaleTimeString();
  ui.log.value = `[${time}] ${message}\n\n` + ui.log.value;
}

function setButtonLoading(button, isLoading, loadingText = 'Loading...') {
  if (!button) return;
  if (isLoading) {
    if (!button.dataset.originalText) {
      button.dataset.originalText = button.textContent || '';
    }
    button.textContent = loadingText;
    button.disabled = true;
    return;
  }

  if (button.dataset.originalText) {
    button.textContent = button.dataset.originalText;
    delete button.dataset.originalText;
  }
  button.disabled = false;
}

function closeDevTools() {
  ui.devToolsBackdrop.hidden = true;
  ui.devToolsPopout.hidden = true;
}

function openDevTools() {
  ui.devToolsBackdrop.hidden = false;
  ui.devToolsPopout.hidden = false;
}

function openCart() {
  ui.cartBackdrop.hidden = false;
  ui.cartPopout.hidden = false;
}

function missingPriceMessage(productName) {
  return `Missing Stripe price ID for ${productName}. Set STRIPE_SYNC_PRICES=1 and restart the example server.`;
}

function closeCart() {
  ui.cartBackdrop.hidden = true;
  ui.cartPopout.hidden = true;
}

function parsePositiveInteger(value, fallback = 1) {
  const parsed = Number.parseInt(String(value ?? ''), 10);
  if (!Number.isFinite(parsed) || parsed <= 0) return fallback;
  return parsed;
}

function parseAmountFromPriceLabel(priceLabel) {
  const text = String(priceLabel ?? '');
  const match = text.match(/-?\d[\d,]*(?:\.\d{1,2})?/);
  if (!match) return 0;
  const normalized = match[0].replace(/,/g, '');
  const parsed = Number.parseFloat(normalized);
  return Number.isFinite(parsed) ? parsed : 0;
}

function getMerchandising(item) {
  const byId = {
    'orbital-starter-pack': {
      badge: 'Best Seller',
      rating: 4.8,
      reviews: 214,
    },
    'warp-pass': {
      badge: 'Limited Offer',
      compareAt: 12,
      rating: 4.6,
      reviews: 129,
    },
    'fleet-command-bundle': { rating: 4.7, reviews: 88 },
  };
  return byId[item.id] || null;
}

function formatUsd(amount) {
  try {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(amount);
  } catch {
    return `$${amount.toFixed(2)}`;
  }
}

async function api(path, payload) {
  const response = await fetch(path, {
    method: payload ? 'POST' : 'GET',
    headers: payload ? { 'content-type': 'application/json' } : undefined,
    body: payload ? JSON.stringify(payload) : undefined,
  });
  const data = await response.json();
  if (!response.ok || data.ok === false) {
    throw new Error(data.error || `Request failed (${response.status})`);
  }
  return data;
}

function buildPostCheckoutUrl(flag) {
  const url = new URL(window.location.origin);
  url.searchParams.set(flag, '1');
  return url.toString();
}

function makeDefaultUserId() {
  return `pilot_${Math.random().toString(36).slice(2, 8)}`;
}

function seedDefaultBuyerDetails() {
  ui.email.value = 'pilot@spacetime.dev';
  ui.name.value = 'Orbital Pilot';
  ui.userId.value = makeDefaultUserId();
}

function hydrateCart() {
  try {
    const raw = localStorage.getItem(STORAGE_KEYS.cart);
    if (!raw) return;
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return;

    state.cart = parsed
      .map(item => ({
        id: String(item?.id || ''),
        name: String(item?.name || ''),
        priceLabel: String(item?.priceLabel || ''),
        mode: item?.mode === 'subscription' ? 'subscription' : 'payment',
        priceId: String(item?.priceId || ''),
        quantity: parsePositiveInteger(item?.quantity, 1),
      }))
      .filter(item => item.id && item.name && item.priceId);
  } catch {
    state.cart = [];
  }
}

function persistCart() {
  try {
    localStorage.setItem(STORAGE_KEYS.cart, JSON.stringify(state.cart));
  } catch {
    // Storage may be unavailable in private windows.
  }
}

function getCartEntry(itemId, priceId) {
  return state.cart.find(
    entry => entry.id === itemId && entry.priceId === priceId
  );
}

function getCartQuantity(itemId, priceId) {
  const entry = getCartEntry(itemId, priceId);
  return entry ? entry.quantity : 0;
}

function getCartKey(itemId, priceId) {
  return `${itemId}::${priceId}`;
}

function syncCartState() {
  const count = state.cart.length;
  const totalQty = state.cart.reduce((sum, item) => sum + item.quantity, 0);
  const oneTimeSubtotal = state.cart.reduce((sum, item) => {
    if (item.mode !== 'payment') return sum;
    return sum + parseAmountFromPriceLabel(item.priceLabel) * item.quantity;
  }, 0);
  const recurringMonthly = state.cart.reduce((sum, item) => {
    if (item.mode !== 'subscription') return sum;
    return sum + parseAmountFromPriceLabel(item.priceLabel) * item.quantity;
  }, 0);

  const cartModes = new Set(state.cart.map(entry => entry.mode));
  const isMixedMode = cartModes.size > 1;
  const hasMissingPrice = state.cart.some(entry => !entry.priceId);
  if (state.checkoutPending) {
    ui.btnCheckoutCart.disabled = true;
    ui.btnCheckoutCart.textContent = 'Redirecting to Stripe...';
  } else if (hasMissingPrice) {
    ui.btnCheckoutCart.disabled = true;
    ui.btnCheckoutCart.textContent = 'Sync Stripe prices first';
  } else if (isMixedMode) {
    ui.btnCheckoutCart.disabled = true;
    ui.btnCheckoutCart.textContent = "Can't mix one-time + subscription";
  } else {
    ui.btnCheckoutCart.disabled = count === 0;
    ui.btnCheckoutCart.textContent =
      totalQty > 0
        ? `Checkout with Stripe (${totalQty})`
        : 'Checkout with Stripe';
  }
  if (ui.btnCart) ui.btnCart.textContent = `Cart (${totalQty})`;
  if (ui.cartSummary) {
    ui.cartSummary.innerHTML =
      `<span>Subtotal: ${formatUsd(oneTimeSubtotal)}</span>` +
      `<span>Recurring: ${formatUsd(recurringMonthly)}/mo</span>`;
  }
  persistCart();
  renderCartList();
  if (state.catalog.length > 0) renderCatalog();
}

function renderCartList() {
  if (state.cart.length === 0) {
    ui.cartList.innerHTML = `
      <div class="cart-empty">
        <div class="cart-empty-title">Your cart is empty</div>
        <div class="cart-empty-copy">Add items from the storefront to start checkout.</div>
      </div>
    `;
    return;
  }

  ui.cartList.innerHTML = state.cart
    .map(item => {
      const unitPrice = parseAmountFromPriceLabel(item.priceLabel);
      const lineTotal = unitPrice * item.quantity;
      const cartKey = getCartKey(item.id, item.priceId);
      const stepperClass =
        state.animatedStepperKey === cartKey
          ? ' card-stepper bump cart-stepper'
          : ' card-stepper cart-stepper';
      return `
        <article class="cart-item">
          <div class="cart-item-top">
            <div class="cart-item-thumb" aria-hidden="true"></div>
            <div class="cart-item-content">
              <div class="cart-item-name">${item.name}</div>
              <div class="cart-item-meta">${item.mode === 'subscription' ? 'Subscription' : 'One-time payment'}</div>
              <div class="cart-item-sub">${formatUsd(unitPrice)} x ${item.quantity} = ${formatUsd(lineTotal)}${item.mode === 'subscription' ? ' /mo' : ''}</div>
            </div>
            <div class="cart-item-controls">
              <div class="${stepperClass.trim()}">
                <button class="stepper-btn js-cart-minus ${item.quantity === 1 ? 'is-trash' : ''}" data-product-id="${item.id}" data-price-id="${item.priceId}" aria-label="${item.quantity === 1 ? 'Remove item' : 'Decrease quantity'}">${item.quantity === 1 ? '<svg class="trash-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M3 6h18"/><path d="M8 6V4h8v2"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/></svg>' : '-'}</button>
                <div class="stepper-count">${item.quantity} in cart</div>
                <button class="stepper-btn js-cart-plus" data-product-id="${item.id}" data-price-id="${item.priceId}" aria-label="Increase quantity">+</button>
              </div>
            </div>
          </div>
        </article>
      `;
    })
    .join('');

  for (const button of ui.cartList.querySelectorAll('.js-cart-plus')) {
    button.addEventListener('click', () => {
      const productId = button.getAttribute('data-product-id') || '';
      const priceId = button.getAttribute('data-price-id') || '';
      const item = state.catalog.find(
        product => product.id === productId && product.priceId === priceId
      );
      if (!item) return;
      addItemToCart(item, 1);
    });
  }

  for (const button of ui.cartList.querySelectorAll('.js-cart-minus')) {
    button.addEventListener('click', () => {
      const productId = button.getAttribute('data-product-id') || '';
      const priceId = button.getAttribute('data-price-id') || '';
      const index = state.cart.findIndex(
        entry => entry.id === productId && entry.priceId === priceId
      );
      if (index < 0) return;
      state.cart[index].quantity -= 1;
      if (state.cart[index].quantity <= 0) {
        const removed = state.cart.splice(index, 1)[0];
        if (removed)
          writeLog(`cart_remove: ${removed.id} qty=${removed.quantity}`);
      }
      syncCartState();
    });
  }
}

function renderCatalog() {
  if (state.catalog.length === 0) {
    ui.catalogGrid.innerHTML =
      '<div class="loading-card">No catalog items found.</div>';
    return;
  }

  const cards = state.catalog
    .map(item => {
      const merch = getMerchandising(item);
      const unitPrice = parseAmountFromPriceLabel(item.priceLabel);
      const compareAt =
        merch?.compareAt && merch.compareAt > unitPrice
          ? merch.compareAt
          : null;
      const purchaseType =
        item.mode === 'subscription' ? 'Subscription' : 'One-time';
      const rating = merch?.rating ?? 4.7;
      const reviews = merch?.reviews ?? 100;
      const stars = '&#9733;&#9733;&#9733;&#9733;&#9733;';
      const inCartQty = getCartQuantity(item.id, item.priceId);
      const cartKey = getCartKey(item.id, item.priceId);
      const stepperClass =
        state.animatedStepperKey === cartKey
          ? 'card-stepper bump'
          : 'card-stepper';
      const isMissingPrice = !item.priceId;
      return `
        <article class="product-card" data-product-id="${item.id}">
          <div class="product-row">
            <div class="product-media" aria-hidden="true"></div>
            <div class="product-head">
              <div class="badge-row">
                ${merch?.badge ? `<span class="merch-badge sale">${merch.badge}</span>` : ''}
                <span class="merch-badge mode">${purchaseType}</span>
              </div>
              <div class="product-copy">
                <h3 class="product-title">${item.name}</h3>
                <p class="product-description">${item.description}</p>
              </div>
              <div class="product-rating"><span>${rating.toFixed(1)}</span><span class="product-stars">${stars}</span><span>(${reviews})</span></div>
              <div class="card-top">
                <div class="price-stack">
                  <div class="price-label">${item.priceLabel}</div>
                  ${compareAt ? `<div class="price-compare">List: <s>${formatUsd(compareAt)}</s></div>` : ''}
                </div>
              </div>
              <div class="card-actions">
                ${
                  inCartQty > 0
                    ? `
                    <div class="${stepperClass}">
                      <button class="stepper-btn js-card-minus ${inCartQty === 1 ? 'is-trash' : ''}" data-product-id="${item.id}" data-price-id="${item.priceId}" aria-label="${inCartQty === 1 ? 'Remove item' : 'Decrease quantity'}">${inCartQty === 1 ? '<svg class="trash-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M3 6h18"/><path d="M8 6V4h8v2"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/></svg>' : '-'}</button>
                      <div class="stepper-count">${inCartQty} in cart</div>
                      <button class="stepper-btn js-card-plus" data-product-id="${item.id}" data-price-id="${item.priceId}" aria-label="Increase quantity">+</button>
                    </div>
                  `
                    : isMissingPrice
                      ? '<button class="btn add-cart" disabled title="Set STRIPE_SYNC_PRICES=1 and restart the example server.">Sync price first</button>'
                      : `<button class="btn add-cart js-add-to-cart" data-product-id="${item.id}">Add to cart</button>`
                }
              </div>
            </div>
          </div>
        </article>
      `;
    })
    .join('');

  ui.catalogGrid.innerHTML = cards;
  for (const button of ui.catalogGrid.querySelectorAll('.js-add-to-cart')) {
    button.addEventListener('click', async () => {
      const productId = button.getAttribute('data-product-id') || '';
      const item = state.catalog.find(product => product.id === productId);
      if (!item) return;
      setError('');
      addItemToCart(item, 1);
      await validatePriceForItem(item).catch(error => setError(error.message));
    });
  }

  for (const button of ui.catalogGrid.querySelectorAll('.js-card-plus')) {
    button.addEventListener('click', async () => {
      const productId = button.getAttribute('data-product-id') || '';
      const priceId = button.getAttribute('data-price-id') || '';
      const item = state.catalog.find(
        product => product.id === productId && product.priceId === priceId
      );
      if (!item) return;
      setError('');
      addItemToCart(item, 1);
      await validatePriceForItem(item).catch(error => setError(error.message));
    });
  }

  for (const button of ui.catalogGrid.querySelectorAll('.js-card-minus')) {
    button.addEventListener('click', () => {
      const productId = button.getAttribute('data-product-id') || '';
      const priceId = button.getAttribute('data-price-id') || '';
      const index = state.cart.findIndex(
        entry => entry.id === productId && entry.priceId === priceId
      );
      if (index < 0) return;
      state.cart[index].quantity -= 1;
      if (state.cart[index].quantity <= 0) {
        const removed = state.cart.splice(index, 1)[0];
        if (removed)
          writeLog(`cart_remove: ${removed.id} qty=${removed.quantity}`);
      }
      syncCartState();
    });
  }
}

function renderCatalogSkeleton(count = 6) {
  const cards = Array.from(
    { length: count },
    () => `
    <article class="skeleton-card" aria-hidden="true">
      <div class="skeleton-line skeleton-media"></div>
      <div class="skeleton-line badges"></div>
      <div class="skeleton-line title"></div>
      <div class="skeleton-line desc"></div>
      <div class="skeleton-line meta"></div>
      <div class="skeleton-line price"></div>
      <div class="skeleton-line actions"></div>
    </article>
  `
  ).join('');
  ui.catalogGrid.innerHTML = cards;
}

window.addEventListener('stdb:catalog', event => {
  const products = event.detail?.products ?? [];
  const sorted = [...products].sort(
    (a, b) => (a.sortOrder ?? 0) - (b.sortOrder ?? 0)
  );
  state.catalog = sorted;
  const catalogById = new Map(sorted.map(item => [item.id, item]));
  const previousCartCount = state.cart.length;
  state.cart = state.cart
    .filter(entry => {
      const item = catalogById.get(entry.id);
      return item && item.priceId && item.priceId === entry.priceId;
    })
    .map(entry => {
      const item = catalogById.get(entry.id);
      return {
        ...entry,
        name: item.name,
        priceLabel: item.priceLabel,
        mode: item.mode,
      };
    });
  if (state.cart.length !== previousCartCount) {
    setError(
      'Your saved cart contained outdated Stripe prices and was refreshed.'
    );
  }
  syncCartState();
});

window.addEventListener('stdb:connState', event => {
  const detail = event.detail || {};
  if (detail.state === 'connected') {
    writeLog('stdb: connected');
    return;
  }
  if (detail.state === 'error') {
    const message = detail.detail || 'SpacetimeDB connection failed.';
    setError(message);
    writeLog(`stdb: error: ${message}`);
    return;
  }
  writeLog('stdb: connecting');
});

async function validatePriceForItem(item) {
  if (!item?.priceId) {
    writeLog('price validation: missing price id');
    return;
  }
  if (!window.stdb) {
    writeLog('price validation: STDB not connected yet');
    return;
  }
  const validation = await window.stdb.validatePrice(item.priceId);
  if (validation.valid) {
    writeLog(`price valid: ${item.priceId} (active=${validation.active})`);
  } else {
    writeLog(`price invalid: ${validation.message || 'not found'}`);
  }
}

async function getOrCreateCustomer() {
  setError('');
  if (!window.stdb) {
    throw new Error('STDB not connected yet. Try again.');
  }
  const result = await window.stdb.getOrCreateCustomer({
    userId: ui.userId.value,
    email: ui.email.value || undefined,
    name: ui.name.value || undefined,
  });
  if (result.customerId) ui.customerId.value = result.customerId;
  writeLog(`get_or_create_customer: ${JSON.stringify(result)}`);
}

async function createCheckoutForCart(cartItems, triggerButton) {
  if (!cartItems || cartItems.length === 0) throw new Error('Cart is empty.');
  const missingPrice = cartItems.find(entry => !entry.priceId);
  if (missingPrice) {
    throw new Error(missingPriceMessage(missingPrice.name));
  }

  const modes = Array.from(new Set(cartItems.map(entry => entry.mode)));
  if (modes.length > 1) {
    throw new Error(
      'Stripe checkout cannot mix one-time and subscription items in the same session. ' +
        'Please remove one type and check out separately.'
    );
  }
  const mode = modes[0];

  const successUrl = buildPostCheckoutUrl('purchased');
  const cancelUrl = buildPostCheckoutUrl('canceled');

  state.checkoutPending = true;
  syncCartState();
  setButtonLoading(triggerButton, true, 'Redirecting...');

  try {
    if (!window.stdb) {
      throw new Error('STDB not connected yet. Try again.');
    }
    const result = await window.stdb.createCheckoutSession({
      items: cartItems.map(entry => ({
        priceId: entry.priceId,
        quantity: entry.quantity,
      })),
      customerId: ui.customerId.value || undefined,
      mode,
      successUrl,
      cancelUrl,
    });
    writeLog(`create_checkout_session: ${JSON.stringify(result)}`);

    if (!result.url) {
      throw new Error('Stripe checkout URL missing from session response.');
    }
    window.location.assign(result.url);
  } finally {
    setButtonLoading(triggerButton, false);
    state.checkoutPending = false;
    syncCartState();
  }
}

function addItemToCart(item, quantity) {
  if (!item) return;
  if (!item.priceId) {
    const message = missingPriceMessage(item.name);
    setError(message);
    writeLog(message);
    return;
  }
  state.animatedStepperKey = getCartKey(item.id, item.priceId);
  const existing = state.cart.find(
    entry => entry.id === item.id && entry.priceId === item.priceId
  );
  if (existing) {
    existing.quantity += quantity;
  } else {
    state.cart.push({
      id: item.id,
      name: item.name,
      priceLabel: item.priceLabel,
      mode: item.mode,
      priceId: item.priceId,
      quantity,
    });
  }
  syncCartState();
  writeLog(`cart_add: ${item.id} qty=${quantity}`);
  setTimeout(() => {
    if (state.animatedStepperKey === getCartKey(item.id, item.priceId)) {
      state.animatedStepperKey = null;
      if (state.catalog.length > 0) renderCatalog();
    }
  }, 260);
}

async function createCheckoutCartNext() {
  setError('');
  if (state.cart.length === 0) throw new Error('Cart is empty.');
  for (const entry of state.cart) {
    const item = state.catalog.find(product => product.id === entry.id);
    if (!item)
      throw new Error(`Cart item ${entry.id} is absent from the catalog.`);
  }
  await createCheckoutForCart(state.cart, ui.btnCheckoutCart);
}

function wireEvents() {
  if (ui.btnCart) ui.btnCart.addEventListener('click', openCart);
  ui.btnCartClose.addEventListener('click', closeCart);
  ui.btnContinueShopping.addEventListener('click', closeCart);
  ui.cartBackdrop.addEventListener('click', closeCart);

  if (ui.btnSettings) {
    ui.btnSettings.addEventListener('click', openDevTools);
  }

  ui.btnCustomer.addEventListener('click', () => {
    getOrCreateCustomer().catch(error => setError(error.message));
  });

  ui.btnCheckoutCart.addEventListener('click', () => {
    createCheckoutCartNext().catch(error => setError(error.message));
  });

  ui.btnDevToolsClose.addEventListener('click', closeDevTools);
  ui.devToolsBackdrop.addEventListener('click', closeDevTools);

  if (ui.checkoutBannerClose) {
    ui.checkoutBannerClose.addEventListener('click', hideCheckoutBanner);
  }

  document.addEventListener('keydown', event => {
    const key = String(event.key || '').toLowerCase();
    if ((event.ctrlKey || event.metaKey) && event.shiftKey && key === 'd') {
      event.preventDefault();
      openDevTools();
      return;
    }
    if (event.key === 'Escape' && !ui.cartPopout.hidden) {
      closeCart();
      return;
    }
    if (event.key === 'Escape' && !ui.devToolsPopout.hidden) {
      closeDevTools();
    }
  });
}

async function boot() {
  renderCatalogSkeleton();
  await api('/api/config');
  seedDefaultBuyerDetails();
  hydrateCart();
  wireEvents();
  consumePostCheckoutQueryFlags();
  syncCartState();
  if (state.catalog.length > 0) renderCatalog();
}

boot().catch(error => {
  setError(error.message);
  writeLog(`boot error: ${error.message}`);
});
