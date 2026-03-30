import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App.tsx';
import { ConnectedGuard } from './ConnectedGuard.tsx';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ConnectedGuard>
      <App />
    </ConnectedGuard>
  </StrictMode>
);
