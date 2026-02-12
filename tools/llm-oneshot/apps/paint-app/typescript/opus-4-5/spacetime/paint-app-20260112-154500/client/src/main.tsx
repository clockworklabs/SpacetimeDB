import { createRoot } from 'react-dom/client';
import App from './App';
import './styles.css';

// NO React.StrictMode - it breaks WebSocket connections
createRoot(document.getElementById('root')!).render(<App />);
