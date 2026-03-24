import { Routes, Route } from 'react-router-dom'
import NavBar from './components/NavBar'
import Leaderboard from './pages/Leaderboard'
import ModelDetail from './pages/ModelDetail'
import CategoryView from './pages/CategoryView'
import Trends from './pages/Trends'

export default function App() {
  return (
    <div className="min-h-screen" style={{ backgroundColor: '#0d0d0e', color: '#e6e9f0' }}>
      <NavBar />
      <main>
        <Routes>
          <Route path="/" element={<Leaderboard />} />
          <Route path="/model/:name" element={<ModelDetail />} />
          <Route path="/category/:name" element={<CategoryView />} />
          <Route path="/trends" element={<Trends />} />
        </Routes>
      </main>
    </div>
  )
}
