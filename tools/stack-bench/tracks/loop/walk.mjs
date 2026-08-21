export async function walk({ page, args, byStage, checkHook, results }) {
  await page.goto(args.url);
  for (const hook of byStage('landing')) await checkHook(page, hook, results);
}
