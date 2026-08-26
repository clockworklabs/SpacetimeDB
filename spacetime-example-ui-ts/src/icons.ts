export function googleIcon(): SVGSVGElement {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('aria-hidden', 'true');
  for (const [fill, d] of [
    [
      '#4285F4',
      'M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.76h3.56c2.08-1.92 3.28-4.74 3.28-8.09z',
    ],
    [
      '#34A853',
      'M12 23c2.97 0 5.46-.98 7.28-2.66l-3.56-2.76c-.98.66-2.24 1.06-3.72 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84A11 11 0 0 0 12 23z',
    ],
    [
      '#FBBC05',
      'M5.84 14.11A6.6 6.6 0 0 1 5.5 12c0-.73.13-1.44.34-2.11V7.05H2.18A11 11 0 0 0 1 12c0 1.78.43 3.46 1.18 4.95l3.66-2.84z',
    ],
    [
      '#EA4335',
      'M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1A11 11 0 0 0 2.18 7.05l3.66 2.84C6.71 7.3 9.14 5.38 12 5.38z',
    ],
  ]) {
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute('fill', fill);
    path.setAttribute('d', d);
    svg.append(path);
  }
  return svg;
}

export function githubIcon(): SVGSVGElement {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'currentColor');
  svg.setAttribute('aria-hidden', 'true');
  const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
  path.setAttribute(
    'd',
    'M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.1.79-.25.79-.56v-2c-3.2.7-3.87-1.36-3.87-1.36-.53-1.34-1.3-1.7-1.3-1.7-1.06-.72.08-.7.08-.7 1.17.08 1.79 1.2 1.79 1.2 1.04 1.78 2.73 1.26 3.4.96.11-.75.41-1.26.74-1.55-2.55-.29-5.24-1.28-5.24-5.7 0-1.26.45-2.29 1.19-3.1-.12-.29-.52-1.47.11-3.06 0 0 .97-.31 3.18 1.18a11 11 0 0 1 5.78 0c2.2-1.49 3.18-1.18 3.18-1.18.63 1.59.23 2.77.11 3.06.74.81 1.19 1.84 1.19 3.1 0 4.43-2.7 5.41-5.27 5.69.42.36.79 1.08.79 2.18v3.23c0 .31.21.67.8.56A11.51 11.51 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5z'
  );
  svg.append(path);
  return svg;
}

export function spacetimeMark(): SVGSVGElement {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 35 32');
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', 'SpacetimeDB');
  svg.classList.add('auth-logo');
  for (const [d, fillRule] of [
    [
      'M28.8002 15.317C28.5535 9.53226 29.63 6.19046 35 0L24.2649 10.9106C26.8343 14.2552 26.6051 19.0995 23.5774 22.1767C20.5498 25.2538 15.7834 25.4867 12.4925 22.8754L10.5042 24.8962L10.5116 24.9024L7.35285 28.1361C9.73784 26.7321 13.4208 27.1349 15.6425 27.3779C16.2579 27.4452 16.7611 27.5003 17.0937 27.5013C20.1371 27.6534 23.2301 26.5483 25.5544 24.186C27.9465 21.7549 29.0284 18.4965 28.8002 15.317Z',
      false,
    ],
    [
      'M17.9063 4.49871C18.2389 4.49971 18.7421 4.55476 19.3575 4.62207C21.5792 4.86508 25.2622 5.26792 27.6472 3.86395L24.4884 7.0976L24.4958 7.10383L22.5075 9.12462C19.2166 6.51328 14.4502 6.74618 11.4226 9.82332C8.3949 12.9005 8.16574 17.7448 10.7351 21.0894L0 32C5.36996 25.8095 6.44651 22.4677 6.1998 16.683C5.97163 13.5035 7.05355 10.2451 9.44557 7.81402C11.7699 5.45167 14.8629 4.34657 17.9063 4.49871Z',
      false,
    ],
    [
      'M24.7486 16C24.7486 20.0687 21.5033 23.367 17.5 23.367C13.4967 23.367 10.2514 20.0687 10.2514 16C10.2514 11.9313 13.4967 8.63292 17.5 8.63292C21.5033 8.63292 24.7486 11.9313 24.7486 16ZM17.5 21.6C20.5752 21.6 23.0682 19.0928 23.0682 16C23.0682 12.9072 20.5752 10.4 17.5 10.4C14.4248 10.4 11.9318 12.9072 11.9318 16C11.9318 19.0928 14.4248 21.6 17.5 21.6Z',
      true,
    ],
  ] as const) {
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute('d', d);
    path.setAttribute('fill', '#D7D8D9');
    if (fillRule) {
      path.setAttribute('fill-rule', 'evenodd');
      path.setAttribute('clip-rule', 'evenodd');
    }
    svg.append(path);
  }
  return svg;
}
