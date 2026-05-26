import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Identity } from 'spacetimedb';
import { describe, expect, it, vi } from 'vitest';
import { Gallows, Keyboard, MaskedWord, ResultsPanel } from './App';

describe('Hangman display components', () => {
  it('draws the figure according to incorrect guesses', () => {
    render(<Gallows incorrectGuesses={3} />);
    expect(
      screen.getByRole('img', { name: 'Incorrect guesses: 3 of 6' })
    ).toBeInTheDocument();
  });

  it('displays blanks and revealed letters', () => {
    render(<MaskedWord maskedWord="_ A _ A" />);
    expect(screen.getByLabelText('word puzzle')).toHaveTextContent('_A_A');
  });

  it('greys out guessed letters and submits a new guess', async () => {
    const onGuess = vi.fn();
    render(<Keyboard disabled={false} guessedLetters="AE" onGuess={onGuess} />);

    expect(screen.getByRole('button', { name: 'A' })).toBeDisabled();
    await userEvent.click(screen.getByRole('button', { name: 'B' }));
    expect(onGuess).toHaveBeenCalledWith('B');
  });

  it('shows ranked results and the revealed answer', () => {
    render(
      <ResultsPanel
        answer="CLOUD"
        results={[
          {
            identity: Identity.zero(),
            name: 'Ada',
            rank: 1,
            solved: true,
            solveTimeMicros: 2_500_000n,
            incorrectGuesses: 1,
            revealedLetters: 5,
          },
        ]}
      />
    );

    expect(screen.getByText('The word was CLOUD')).toBeInTheDocument();
    expect(screen.getByText('Solved in 2.5s')).toBeInTheDocument();
  });
});
