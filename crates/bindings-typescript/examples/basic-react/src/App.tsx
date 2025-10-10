import { useState } from 'react'

function App() {
  const [name, setName] = useState('')
  const [people, setPeople] = useState<Array<{ id: number; name: string }>>([])

  const addPerson = (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return

    // TODO: Call add_person reducer through SpacetimeDB connection
    // After generating bindings with: spacetime generate --lang typescript
    // conn.reducers.addPerson(name)

    // For now, add locally for demo purposes
    setPeople([...people, { id: Date.now(), name }])
    setName('')
  }

  return (
    <div style={{ padding: '2rem' }}>
      <h1>SpacetimeDB React App</h1>

      <form onSubmit={addPerson} style={{ marginBottom: '2rem' }}>
        <input
          type="text"
          placeholder="Enter name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          style={{ padding: '0.5rem', marginRight: '0.5rem' }}
        />
        <button type="submit" style={{ padding: '0.5rem 1rem' }}>
          Add Person
        </button>
      </form>

      <div>
        <h2>People ({people.length})</h2>
        {people.length === 0 ? (
          <p>No people yet. Add someone above!</p>
        ) : (
          <ul>
            {people.map((person) => (
              <li key={person.id}>{person.name}</li>
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}

export default App
