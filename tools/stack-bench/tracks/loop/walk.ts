import type { LintWalkContext } from '../../linter/lint.js';

export async function walk({ page, args, byStage, checkHook, results }: LintWalkContext): Promise<void> {
  await page.goto(args.url ?? '');
  for (const hook of byStage('landing')) await checkHook(page, hook, results);
}
