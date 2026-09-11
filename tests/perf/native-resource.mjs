import { readFile, writeFile } from 'node:fs/promises';
import { resolve as resolvePath } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execFileP = promisify(execFile);

function quoteSafe(p) {
  if (/["\r\n&|<>^]/.test(p)) throw new Error(`unsafe path: ${p}`);
  return p;
}

async function ps(script, args, duration) {
  const scriptPath = quoteSafe(resolvePath(script));
  const psArgs = ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', scriptPath, ...args];
  return execFileP('powershell.exe', psArgs, {
    windowsHide: true,
    timeout: (duration + 45) * 1000,
    maxBuffer: 64 * 1024 * 1024,
  });
}

export async function measureNative({
  page,
  identityFile,
  keeper,
  agentManifest,
  directory,
  mode = 'idle',
  duration = 600,
}) {
  if (!['idle', 'output'].includes(mode) || !Number.isInteger(duration) || duration < 1 || duration > 600) throw new Error('Invalid resource scenario');
  if (mode === 'output') {
    quoteSafe(process.execPath);
    quoteSafe(resolvePath('tests/perf/pty-workload.mjs'));
    const acquire = page.getByRole('button', { name: '제어권 가져오기', exact: true });
    if (await acquire.count()) await acquire.click();
    await page.getByText('제어권 해제').waitFor();
    await page.locator('.xterm-helper-textarea').focus();
    const command = `"${process.execPath}" "${resolvePath('tests/perf/pty-workload.mjs')}" output 102400 ${duration + 10}`;
    await page.keyboard.type(command);
    await page.keyboard.press('Enter');
    await page.waitForTimeout(1000);
  }

  const nativeDesc = resolvePath('tests/perf/native-descendants.ps1');
  const procManifest = resolvePath('tests/perf/process-manifest.ps1');
  const samplerScript = resolvePath('scripts/measure-resources.ps1');

  const nativeOut = resolvePath(directory, `${mode}-native-descendants.json`);
  await ps(nativeDesc, ['-Identity', identityFile, '-Output', nativeOut], duration);
  const nativeRaw = JSON.parse(await readFile(nativeOut, 'utf8'));
  if (!Array.isArray(nativeRaw)) throw new Error('Native manifest must be an array');
  const uiArray = nativeRaw;

  const existing = JSON.parse(await readFile(agentManifest, 'utf8'));
  if (!Array.isArray(existing)) throw new Error('Agent manifest must be an array');
  const productAgents = existing;

  const rootsOut = resolvePath(directory, `${mode}-roots.json`);
  const sessionsOut = resolvePath(directory, `${mode}-sessions.json`);
  const flag = 'wx';
  await writeFile(rootsOut, JSON.stringify([...productAgents, ...uiArray], null, 2), { flag });
  await writeFile(sessionsOut, JSON.stringify([keeper], null, 2), { flag });

  const manifestOut = resolvePath(directory, `${mode}-manifest.json`);
  await ps(procManifest, [
    '-AgentManifest', rootsOut,
    '-Sessions', sessionsOut,
    '-Output', manifestOut,
  ], duration);

  const rawOut = resolvePath(directory, `${mode}-raw.json`);
  await ps(samplerScript, [
    '-Manifest', manifestOut,
    '-Output', rawOut,
    '-DurationSeconds', String(duration),
    '-IntervalMs', '1000', '-IncludeGpu',
  ], duration);

  return {
    mode,
    duration,
    rawPath: rawOut,
    manifestPath: manifestOut,
    workload_scope: 'keeper CMD only; Node child excluded from this manifest',
  };
}
