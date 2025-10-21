import './App.css';
import { Link, Route, Routes } from 'react-router-dom';
import CounterPage from './pages/CounterPage';
import UserPage from './pages/UserPage';

function App() {
  return (
    <div>
      <nav style={{ marginBottom: '1rem' }}>
        <Link to="/">Counter</Link> | <Link to="/user">User</Link>
      </nav>

      <Routes>
        <Route path="/" element={<CounterPage />} />
        <Route path="/user" element={<UserPage />} />
      </Routes>
    </div>
  );
}

export default App;
