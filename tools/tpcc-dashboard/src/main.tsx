import { createRoot } from 'react-dom/client';
import { ConnectedGuard } from './components/ConnectedGuard.tsx';
import { Provider } from 'react-redux';
import { store } from './store.ts';
import './style.css';
import { createBrowserRouter, RouterProvider } from 'react-router';
import DashboardPage from './pages/dashboard/DashboardPage.tsx';
import NodesPage from './pages/nodes/NodesPage.tsx';

const router = createBrowserRouter([
  {
    path: '/',
    Component: DashboardPage,
  },
  {
    path: '/nodes',
    Component: NodesPage,
  },
]);

createRoot(document.getElementById('root')!).render(
  <ConnectedGuard>
    <Provider store={store}>
      <img
        src="powered-by-spacetimedb.png"
        style={{
          position: 'absolute',
          top: 10,
          left: '50%',
          height: '60px',
          transform: 'translateX(-50%)',
        }}
      />
      <RouterProvider router={router} />
    </Provider>
  </ConnectedGuard>
);
