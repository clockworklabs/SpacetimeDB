import { describe, expect, it } from 'vitest';
import { pointerDirection } from '../src/game/input';
import { leaderboardRows } from '../src/game/leaderboard';
import { cameraSize, centerOfMass, massToRadius } from '../src/game/math';
import { submittedUsername } from '../src/ui/UsernameChooser';

describe('Blackholio gameplay helpers', () => {
  it('converts server mass into a rendered radius', () => {
    expect(massToRadius(25)).toBe(5);
  });

  it('calculates a weighted local camera center', () => {
    expect(
      centerOfMass([
        { mass: 10, position: { x: 0, y: 10 } },
        { mass: 30, position: { x: 20, y: 10 } },
      ])
    ).toEqual({ x: 15, y: 10 });
    expect(centerOfMass([])).toBeUndefined();
  });

  it('matches the Unity camera size increase after splitting', () => {
    expect(cameraSize(15, 1)).toBe(53);
    expect(cameraSize(100, 1)).toBe(70);
    expect(cameraSize(100, 2)).toBe(100);
    expect(cameraSize(100, 5)).toBe(100);
  });

  it('normalizes pointer movement relative to viewport height', () => {
    expect(pointerDirection({ x: 800, y: 300 }, { x: 800, y: 600 })).toEqual({
      x: 2,
      y: 0,
    });
  });

  it('adds a living local player below the top-ten cutoff', () => {
    const rows = leaderboardRows([
      ...Array.from({ length: 11 }, (_, i) => ({
        id: i,
        name: `P${i}`,
        mass: 100 - i,
        local: false,
      })),
      { id: 99, name: 'Local', mass: 1, local: true },
    ]);
    expect(rows).toHaveLength(11);
    expect(rows.at(-1)?.id).toBe(99);
  });

  it('submits the same default username as the Unity chooser', () => {
    expect(submittedUsername('  Alice  ')).toBe('Alice');
    expect(submittedUsername('   ')).toBe('<No Name>');
  });
});
