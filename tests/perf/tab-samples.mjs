export async function measureTabSwitches(page, labels, count = 100) {
  if (!Array.isArray(labels) || labels.length !== 2) {
    throw new Error('labels must be an array of exactly 2 unique owned label strings');
  }
  if (labels.some((label) => typeof label !== 'string' || !label) || new Set(labels).size !== 2) {
    throw new Error('labels must be 2 unique strings');
  }
  if (!Number.isInteger(count) || count < 1 || count > 1000) {
    throw new Error('count must be an integer between 1 and 1000');
  }

  const samples = [];
  for (let i = 0; i < count; i++) {
    const label = labels[i % 2];
    const start = performance.now();
    await page.locator('.sidebar').getByRole('button', { name: label, exact: true }).click();
    await page
      .locator('.terminal-pane')
      .filter({ has: page.locator('.pane-title').filter({ hasText: label }) })
      .locator('.pane-toolbar .status.live')
      .waitFor({ state: 'visible', timeout: 5000 });
    samples.push(performance.now() - start);
  }
  return samples;
}
