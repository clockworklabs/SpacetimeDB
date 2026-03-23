import { NavLink } from 'react-router-dom'

const ACCENT = '#69b3ff'

export default function NavBar() {
  return (
    <nav
      className="sticky top-0 z-50 flex items-center gap-6 px-6 py-3 border-b"
      style={{ backgroundColor: '#0b0f14', borderColor: '#1e2a38' }}
    >
      {/* Logo / title */}
      <NavLink to="/" className="flex items-center gap-2 shrink-0">
        <span className="text-lg font-bold tracking-tight" style={{ color: ACCENT }}>
          SpacetimeDB
        </span>
        <span className="text-sm font-medium text-slate-400">LLM Benchmarks</span>
      </NavLink>

      <div className="flex-1" />

      {/* Nav links */}
      <div className="flex items-center gap-1">
        {[
          { to: '/', label: 'Leaderboard' },
          { to: '/trends', label: 'Trends' },
        ].map(({ to, label }) => (
          <NavLink
            key={to}
            to={to}
            end={to === '/'}
            className={({ isActive }) =>
              [
                'px-3 py-1.5 rounded text-sm font-medium transition-colors',
                isActive
                  ? 'text-white'
                  : 'text-slate-400 hover:text-slate-200',
              ].join(' ')
            }
            style={({ isActive }) =>
              isActive
                ? { backgroundColor: '#1e2a38', color: ACCENT }
                : {}
            }
          >
            {label}
          </NavLink>
        ))}
      </div>
    </nav>
  )
}
