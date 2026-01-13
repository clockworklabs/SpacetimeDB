import React, { useState } from 'react';
import { useAuth } from '../App';
import { useNavigate } from 'react-router-dom';

export default function Login() {
  const [username, setUsername] = useState('');
  const { login } = useAuth();
  const navigate = useNavigate();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await login(username);
      navigate('/');
    } catch (err) {
      alert('Login failed');
    }
  };

  return (
    <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', height: '100vh' }}>
      <form onSubmit={handleSubmit} style={{ background: 'var(--bg-secondary)', padding: 40, borderRadius: 5, width: 400 }}>
        <h2 style={{ textAlign: 'center', marginBottom: 20 }}>Welcome back!</h2>
        <div style={{ marginBottom: 20 }}>
          <label style={{ display: 'block', marginBottom: 8, fontSize: 12, fontWeight: 'bold', color: '#b5bac1' }}>USERNAME</label>
          <input 
            className="input" 
            style={{ width: '100%' }} 
            value={username} 
            onChange={e => setUsername(e.target.value)}
            required 
          />
        </div>
        <button className="btn btn-primary" style={{ width: '100%' }}>Log In</button>
      </form>
    </div>
  );
}
