import { ScheduleAt, Timestamp } from 'spacetimedb';
import { schema, SenderError, t, table } from 'spacetimedb/server';

const ACTIVE_DURATION_MICROS = 60_000_000n;
const RESULTS_DURATION_MICROS = 10_000_000n;
const MAX_INCORRECT_GUESSES = 6;

type WordEntry = {
  word: string;
  difficulty: 'easy' | 'medium' | 'hard';
};

const WORDS: readonly WordEntry[] = [
  { word: 'APPLE', difficulty: 'easy' },
  { word: 'CLOUD', difficulty: 'easy' },
  { word: 'RIVER', difficulty: 'easy' },
  { word: 'MOUSE', difficulty: 'easy' },
  { word: 'PLANT', difficulty: 'easy' },
  { word: 'SMILE', difficulty: 'easy' },
  { word: 'TRAIN', difficulty: 'easy' },
  { word: 'SOCKET', difficulty: 'medium' },
  { word: 'BROWSER', difficulty: 'medium' },
  { word: 'REDUCER', difficulty: 'medium' },
  { word: 'NETWORK', difficulty: 'medium' },
  { word: 'CONSOLE', difficulty: 'medium' },
  { word: 'REQUEST', difficulty: 'medium' },
  { word: 'FUNCTION', difficulty: 'medium' },
  { word: 'DATABASE', difficulty: 'hard' },
  { word: 'TYPESCRIPT', difficulty: 'hard' },
  { word: 'SYNCHRONIZE', difficulty: 'hard' },
  { word: 'REPLICATION', difficulty: 'hard' },
  { word: 'SUBSCRIPTION', difficulty: 'hard' },
  { word: 'AUTHENTICATION', difficulty: 'hard' },
];

const RoundPhase = t.enum('RoundPhase', ['Active', 'Results']);
const TransitionKind = t.enum('TransitionKind', ['Close', 'Start']);

const player = table(
  { name: 'player', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
  }
);

const currentRound = table(
  { name: 'current_round', public: true },
  {
    id: t.u8().primaryKey(),
    round_number: t.u64(),
    phase: RoundPhase,
    difficulty: t.string(),
    word_length: t.u32(),
    started_at: t.timestamp(),
    phase_ends_at: t.timestamp(),
    answer: t.string().optional(),
  }
);

const roundResult = table(
  { name: 'round_result', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    rank: t.u32(),
    solved: t.bool(),
    solve_time_micros: t.u64().optional(),
    incorrect_guesses: t.u8(),
    revealed_letters: t.u32(),
  }
);

const roundSecret = table(
  { name: 'round_secret' },
  {
    id: t.u8().primaryKey(),
    round_number: t.u64(),
    answer: t.string(),
  }
);

const playerProgress = table(
  { name: 'player_progress' },
  {
    identity: t.identity().primaryKey(),
    round_number: t.u64(),
    guessed_letters: t.string(),
    incorrect_guesses: t.u8(),
    solved: t.bool(),
    failed: t.bool(),
    solved_at: t.timestamp().optional(),
  }
);

const transitionTimer = table(
  { name: 'transition_timer', scheduled: (): any => run_transition },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    round_number: t.u64(),
    kind: TransitionKind,
  }
);

const spacetimedb = schema({
  player,
  currentRound,
  roundResult,
  roundSecret,
  playerProgress,
  transitionTimer,
});
export default spacetimedb;

const myProgressRow = t.row('PlayerBoard', {
  round_number: t.u64(),
  masked_word: t.string(),
  guessed_letters: t.string(),
  incorrect_guesses: t.u8(),
  solved: t.bool(),
  failed: t.bool(),
});

function deadlineAfter(timestampMicros: bigint, durationMicros: bigint) {
  return ScheduleAt.time(timestampMicros + durationMicros);
}

function chooseWord(ctx: {
  random: { integerInRange: (min: number, max: number) => number };
}) {
  return WORDS[ctx.random.integerInRange(0, WORDS.length - 1)]!;
}

function maskWord(answer: string, guessedLetters: string) {
  return answer
    .split('')
    .map(letter => (guessedLetters.includes(letter) ? letter : '_'))
    .join(' ');
}

function revealedLetterCount(answer: string, guessedLetters: string) {
  return answer.split('').filter(letter => guessedLetters.includes(letter))
    .length;
}

function solvedWord(answer: string, guessedLetters: string) {
  return answer.split('').every(letter => guessedLetters.includes(letter));
}

function compareText(left: string, right: string) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function clearRows<T>(rows: Iterable<T>, remove: (row: T) => boolean) {
  for (const row of rows) remove(row);
}

function startRound(ctx: any, roundNumber: bigint) {
  clearRows(ctx.db.roundResult.iter(), (row: any) =>
    ctx.db.roundResult.delete(row)
  );
  clearRows(ctx.db.playerProgress.iter(), (row: any) =>
    ctx.db.playerProgress.delete(row)
  );

  const selection = chooseWord(ctx);
  const phaseEndsAt =
    ctx.timestamp.microsSinceUnixEpoch + ACTIVE_DURATION_MICROS;
  const oldRound = ctx.db.currentRound.id.find(0);
  const publicRound = {
    id: 0,
    round_number: roundNumber,
    phase: { tag: 'Active' },
    difficulty: selection.difficulty,
    word_length: selection.word.length,
    started_at: ctx.timestamp,
    phase_ends_at: new Timestamp(phaseEndsAt),
    answer: undefined,
  };
  if (oldRound) {
    ctx.db.currentRound.id.update(publicRound);
  } else {
    ctx.db.currentRound.insert(publicRound);
  }

  const oldSecret = ctx.db.roundSecret.id.find(0);
  const secret = {
    id: 0,
    round_number: roundNumber,
    answer: selection.word,
  };
  if (oldSecret) {
    ctx.db.roundSecret.id.update(secret);
  } else {
    ctx.db.roundSecret.insert(secret);
  }

  ctx.db.transitionTimer.insert({
    scheduled_id: 0n,
    scheduled_at: deadlineAfter(
      ctx.timestamp.microsSinceUnixEpoch,
      ACTIVE_DURATION_MICROS
    ),
    round_number: roundNumber,
    kind: { tag: 'Close' },
  });
}

export const my_progress = spacetimedb.view(
  { name: 'my_progress', public: true },
  myProgressRow.optional(),
  ctx => {
    if (!ctx.db.player.identity.find(ctx.sender)) return undefined;

    const round = ctx.db.currentRound.id.find(0);
    const secret = ctx.db.roundSecret.id.find(0);
    if (!round || !secret || round.phase.tag !== 'Active') return undefined;

    const progress = ctx.db.playerProgress.identity.find(ctx.sender);
    const guessedLetters =
      progress?.round_number === round.round_number
        ? progress.guessed_letters
        : '';

    return {
      round_number: round.round_number,
      masked_word: maskWord(secret.answer, guessedLetters),
      guessed_letters: guessedLetters,
      incorrect_guesses:
        progress?.round_number === round.round_number
          ? progress.incorrect_guesses
          : 0,
      solved:
        progress?.round_number === round.round_number ? progress.solved : false,
      failed:
        progress?.round_number === round.round_number ? progress.failed : false,
    };
  }
);

export const set_name = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    const trimmedName = name.trim();
    if (trimmedName.length === 0 || trimmedName.length > 20) {
      throw new SenderError('Names must be between 1 and 20 characters');
    }

    const existing = ctx.db.player.identity.find(ctx.sender);
    if (existing) {
      ctx.db.player.identity.update({ ...existing, name: trimmedName });
    } else {
      ctx.db.player.insert({ identity: ctx.sender, name: trimmedName });
    }
  }
);

export const guess_letter = spacetimedb.reducer(
  { letter: t.string() },
  (ctx, { letter }) => {
    if (!ctx.db.player.identity.find(ctx.sender)) {
      throw new SenderError('Choose a name before guessing');
    }

    const round = ctx.db.currentRound.id.find(0);
    const secret = ctx.db.roundSecret.id.find(0);
    if (
      !round ||
      !secret ||
      round.phase.tag !== 'Active' ||
      round.phase_ends_at.microsSinceUnixEpoch <=
        ctx.timestamp.microsSinceUnixEpoch
    ) {
      throw new SenderError('There is no active round');
    }

    const guess = letter.trim().toUpperCase();
    if (!/^[A-Z]$/.test(guess)) {
      throw new SenderError('Guess one letter from A to Z');
    }

    const previous = ctx.db.playerProgress.identity.find(ctx.sender);
    const progress =
      previous?.round_number === round.round_number
        ? previous
        : {
            identity: ctx.sender,
            round_number: round.round_number,
            guessed_letters: '',
            incorrect_guesses: 0,
            solved: false,
            failed: false,
            solved_at: undefined,
          };

    if (progress.solved || progress.failed) {
      throw new SenderError('Your attempt is already finished');
    }
    if (progress.guessed_letters.includes(guess)) {
      throw new SenderError('You already guessed that letter');
    }

    const guessedLetters = progress.guessed_letters + guess;
    const incorrectGuesses =
      progress.incorrect_guesses + (secret.answer.includes(guess) ? 0 : 1);
    const solved = solvedWord(secret.answer, guessedLetters);
    const failed = !solved && incorrectGuesses >= MAX_INCORRECT_GUESSES;
    const nextProgress = {
      ...progress,
      guessed_letters: guessedLetters,
      incorrect_guesses: incorrectGuesses,
      solved,
      failed,
      solved_at: solved ? ctx.timestamp : undefined,
    };

    if (previous?.round_number === round.round_number) {
      ctx.db.playerProgress.identity.update(nextProgress);
    } else {
      if (previous) ctx.db.playerProgress.identity.delete(ctx.sender);
      ctx.db.playerProgress.insert(nextProgress);
    }
  }
);

export const run_transition = spacetimedb.reducer(
  { arg: transitionTimer.rowType },
  (ctx, { arg }) => {
    if (!ctx.sender.isEqual(ctx.identity)) {
      throw new SenderError('Round transitions can only be scheduled');
    }

    const round = ctx.db.currentRound.id.find(0);
    if (!round || round.round_number !== arg.round_number) return;

    if (arg.kind.tag === 'Start' && round.phase.tag === 'Results') {
      startRound(ctx, round.round_number + 1n);
      return;
    }
    if (arg.kind.tag !== 'Close' || round.phase.tag !== 'Active') return;

    const secret = ctx.db.roundSecret.id.find(0);
    if (!secret || secret.round_number !== round.round_number) return;

    const results = Array.from(ctx.db.playerProgress.iter())
      .filter((progress: any) => progress.round_number === round.round_number)
      .map((progress: any) => ({
        progress,
        name:
          ctx.db.player.identity.find(progress.identity)?.name ??
          progress.identity.toHexString().slice(0, 8),
        revealed: revealedLetterCount(secret.answer, progress.guessed_letters),
        elapsed: progress.solved_at
          ? progress.solved_at.microsSinceUnixEpoch -
            round.started_at.microsSinceUnixEpoch
          : undefined,
      }))
      .sort((left: any, right: any) => {
        if (left.progress.solved !== right.progress.solved) {
          return left.progress.solved ? -1 : 1;
        }
        if (left.progress.solved && left.elapsed !== right.elapsed) {
          return left.elapsed < right.elapsed ? -1 : 1;
        }
        if (!left.progress.solved && left.revealed !== right.revealed) {
          return right.revealed - left.revealed;
        }
        if (
          left.progress.incorrect_guesses !== right.progress.incorrect_guesses
        ) {
          return (
            left.progress.incorrect_guesses - right.progress.incorrect_guesses
          );
        }
        const byName = compareText(left.name, right.name);
        return byName !== 0
          ? byName
          : compareText(
              left.progress.identity.toHexString(),
              right.progress.identity.toHexString()
            );
      });

    results.forEach((result: any, index: number) => {
      ctx.db.roundResult.insert({
        identity: result.progress.identity,
        name: result.name,
        rank: index + 1,
        solved: result.progress.solved,
        solve_time_micros: result.elapsed,
        incorrect_guesses: result.progress.incorrect_guesses,
        revealed_letters: result.revealed,
      });
    });

    ctx.db.currentRound.id.update({
      ...round,
      phase: { tag: 'Results' },
      phase_ends_at: new Timestamp(
        ctx.timestamp.microsSinceUnixEpoch + RESULTS_DURATION_MICROS
      ),
      answer: secret.answer,
    });
    ctx.db.transitionTimer.insert({
      scheduled_id: 0n,
      scheduled_at: deadlineAfter(
        ctx.timestamp.microsSinceUnixEpoch,
        RESULTS_DURATION_MICROS
      ),
      round_number: round.round_number,
      kind: { tag: 'Start' },
    });
  }
);

export const init = spacetimedb.init(ctx => {
  startRound(ctx, 1n);
});
