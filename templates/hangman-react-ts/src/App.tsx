import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { Identity, type Timestamp } from 'spacetimedb';
import { useReducer, useSpacetimeDB, useTable } from 'spacetimedb/react';
import './App.css';
import { reducers, tables } from './module_bindings';
import type * as Types from './module_bindings/types';

const LETTERS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('');
const MAX_INCORRECT_GUESSES = 6;

function messageFor(error: unknown) {
  return error instanceof Error ? error.message : 'The request failed.';
}

function formatDuration(milliseconds: number) {
  const seconds = Math.max(0, Math.ceil(milliseconds / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, '0')}`;
}

function Countdown({ deadline }: { deadline: Timestamp }) {
  const [remaining, setRemaining] = useState(
    deadline.toDate().getTime() - Date.now()
  );

  useEffect(() => {
    const update = () => setRemaining(deadline.toDate().getTime() - Date.now());
    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  }, [deadline]);

  return <span className="timer">{formatDuration(remaining)}</span>;
}

export function Gallows({ incorrectGuesses }: { incorrectGuesses: number }) {
  const count = Math.min(incorrectGuesses, MAX_INCORRECT_GUESSES);
  return (
    <svg
      className="gallows"
      viewBox="0 0 220 270"
      role="img"
      aria-label={`Incorrect guesses: ${count} of ${MAX_INCORRECT_GUESSES}`}
    >
      <path className="frame" d="M20 248h150M48 248V22h102v34M48 50l28-28" />
      {count >= 1 && <circle className="figure" cx="150" cy="80" r="24" />}
      {count >= 2 && <path className="figure" d="M150 104v70" />}
      {count >= 3 && <path className="figure" d="M150 123l-36 30" />}
      {count >= 4 && <path className="figure" d="M150 123l36 30" />}
      {count >= 5 && <path className="figure" d="M150 174l-34 45" />}
      {count >= 6 && <path className="figure" d="M150 174l34 45" />}
    </svg>
  );
}

export function MaskedWord({ maskedWord }: { maskedWord: string }) {
  return (
    <div className="masked-word" aria-label="word puzzle">
      {maskedWord.split(' ').map((letter, index) => (
        <span key={`${letter}-${index}`} className="letter-slot">
          {letter}
        </span>
      ))}
    </div>
  );
}

type KeyboardProps = {
  guessedLetters: string;
  disabled: boolean;
  onGuess: (letter: string) => void;
};

export function Keyboard({ guessedLetters, disabled, onGuess }: KeyboardProps) {
  return (
    <div className="keyboard" aria-label="letter keyboard">
      {LETTERS.map(letter => {
        const guessed = guessedLetters.includes(letter);
        return (
          <button
            className={guessed ? 'used' : undefined}
            key={letter}
            disabled={disabled || guessed}
            onClick={() => onGuess(letter)}
            type="button"
          >
            {letter}
          </button>
        );
      })}
    </div>
  );
}

function resultStatus(result: Types.RoundResult) {
  if (!result.solved) return `${result.revealedLetters} letters found`;
  const seconds = Number(result.solveTimeMicros ?? 0n) / 1_000_000;
  return `Solved in ${seconds.toFixed(1)}s`;
}

export function ResultsPanel({
  answer,
  results,
  identity,
}: {
  answer?: string;
  results: readonly Types.RoundResult[];
  identity?: Identity;
}) {
  const sorted = [...results].sort((left, right) => left.rank - right.rank);

  return (
    <section className="results panel">
      <div className="panel-title">
        <div>
          <p className="eyebrow">Round complete</p>
          <h2>The word was {answer}</h2>
        </div>
      </div>
      {sorted.length === 0 ? (
        <p className="empty-state">No guesses were made this round.</p>
      ) : (
        <ol className="standings">
          {sorted.map(result => (
            <li
              className={
                identity?.isEqual(result.identity) ? 'current-player' : ''
              }
              key={result.identity.toHexString()}
            >
              <span className="rank">#{result.rank}</span>
              <strong>{result.name}</strong>
              <span>{resultStatus(result)}</span>
              <span>{result.incorrectGuesses} misses</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

function NameForm({
  initialName = '',
  onSubmit,
  onCancel,
}: {
  initialName?: string;
  onSubmit: (name: string) => Promise<void>;
  onCancel?: () => void;
}) {
  const [name, setName] = useState(initialName);
  const [saving, setSaving] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim()) return;
    setSaving(true);
    try {
      await onSubmit(name);
    } finally {
      setSaving(false);
    }
  };

  return (
    <form className="name-form" onSubmit={submit}>
      <label htmlFor="player-name">Nickname</label>
      <div>
        <input
          autoFocus
          id="player-name"
          maxLength={20}
          onChange={event => setName(event.target.value)}
          placeholder="Choose a nickname"
          value={name}
        />
        <button disabled={saving || !name.trim()} type="submit">
          {saving ? 'Saving...' : 'Play'}
        </button>
        {onCancel && (
          <button className="secondary" onClick={onCancel} type="button">
            Cancel
          </button>
        )}
      </div>
    </form>
  );
}

function App() {
  const { identity, isActive: connected } = useSpacetimeDB();
  const [rounds] = useTable(tables.currentRound);
  const [players] = useTable(tables.player);
  const [boards] = useTable(tables.my_progress);
  const [results] = useTable(tables.roundResult);
  const setName = useReducer(reducers.setName);
  const guessLetter = useReducer(reducers.guessLetter);
  const [error, setError] = useState('');
  const [editingName, setEditingName] = useState(false);
  const [guessPending, setGuessPending] = useState(false);

  const player = players.find(row => identity?.isEqual(row.identity));
  const round = rounds[0];
  const board = boards[0];
  const isActive = round?.phase.tag === 'Active';

  const placeholderWord = useMemo(
    () => (round ? Array(round.wordLength).fill('_').join(' ') : ''),
    [round]
  );

  const saveName = async (name: string) => {
    setError('');
    try {
      await setName({ name });
      setEditingName(false);
    } catch (caught) {
      setError(messageFor(caught));
    }
  };

  const guess = async (letter: string) => {
    setError('');
    setGuessPending(true);
    try {
      await guessLetter({ letter });
    } catch (caught) {
      setError(messageFor(caught));
    } finally {
      setGuessPending(false);
    }
  };

  if (!connected || !identity || !round) {
    return (
      <main className="loading">
        <h1>Hangman</h1>
        <p>Connecting to the game...</p>
      </main>
    );
  }

  if (!player) {
    return (
      <main className="welcome">
        <section className="welcome-card">
          <p className="eyebrow">SpacetimeDB sample</p>
          <h1>Hangman</h1>
          <p>
            Race the other players to uncover the same word before the timer
            runs out or you miss six times.
          </p>
          {error && <p className="error">{error}</p>}
          <NameForm onSubmit={saveName} />
        </section>
      </main>
    );
  }

  const guessedLetters = board?.guessedLetters ?? '';
  const boardFinished = Boolean(board?.solved || board?.failed);
  const status = board?.solved
    ? 'Solved. Waiting for results.'
    : board?.failed
      ? 'Six misses. Waiting for results.'
      : 'Select a letter to make your guess.';

  return (
    <main className="app">
      <header className="game-header">
        <div>
          <p className="eyebrow">SpacetimeDB sample</p>
          <h1>Hangman</h1>
        </div>
        <div className="round-meta">
          <span>Round {round.roundNumber.toString()}</span>
          <span className={`difficulty ${round.difficulty}`}>
            {round.difficulty}
          </span>
          <Countdown deadline={round.phaseEndsAt} />
        </div>
        <div className="profile">
          {editingName ? (
            <NameForm
              initialName={player.name}
              onCancel={() => setEditingName(false)}
              onSubmit={saveName}
            />
          ) : (
            <>
              <span>{player.name}</span>
              <button
                className="secondary"
                onClick={() => setEditingName(true)}
                type="button"
              >
                Edit name
              </button>
            </>
          )}
        </div>
      </header>

      {error && (
        <p className="error game-error" role="alert">
          {error}
        </p>
      )}

      {isActive ? (
        <section className="play-area">
          <section className="gallows-panel panel">
            <Gallows incorrectGuesses={board?.incorrectGuesses ?? 0} />
            <p className="misses">
              {board?.incorrectGuesses ?? 0} / {MAX_INCORRECT_GUESSES} misses
            </p>
          </section>
          <section className="puzzle panel">
            <p className="eyebrow">Difficulty: {round.difficulty}</p>
            <MaskedWord maskedWord={board?.maskedWord ?? placeholderWord} />
            <p className="status">{status}</p>
            <Keyboard
              disabled={boardFinished || guessPending}
              guessedLetters={guessedLetters}
              onGuess={guess}
            />
          </section>
        </section>
      ) : (
        <ResultsPanel
          answer={round.answer}
          identity={identity}
          results={results}
        />
      )}
    </main>
  );
}

export default App;
