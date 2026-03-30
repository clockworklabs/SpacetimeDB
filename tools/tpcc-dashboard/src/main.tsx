import { createRoot } from 'react-dom/client';
import App from './App.tsx';
import { ConnectedGuard } from './ConnectedGuard.tsx';
import { Provider } from 'react-redux';
import { store } from './store.ts';
import './style.css';

createRoot(document.getElementById('root')!).render(
  <ConnectedGuard>
    <Provider store={store}>
      <App />
    </Provider>
  </ConnectedGuard>
);
