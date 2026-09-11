import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readFile, stat } from 'node:fs/promises';
import { resolve } from 'node:path';
import { performance } from 'node:perf_hooks';

const execFileP = promisify(execFile);

const helpersDir = resolve(process.cwd(), 'tests', 'perf');

async function readIdentity(identityFile) {
  const buf = await readFile(identityFile, 'utf8');
  return JSON.parse(buf);
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

export async function nativeCycle({
  exe,
  profile,
  directory,
  index,
  chromium,
  agentExe,
  keeper,
  first = false,
  beforeClose = async () => {},
  onPaired = () => {},
}) {
  const startedAt = performance.now();
  const identityFile = resolve(directory, `ui-${index}.json`);
  const port = 19400 + index;

  const startScript = resolve(helpersDir, 'native-start.ps1');
  const closeScript = resolve(helpersDir, 'native-close.ps1');
  const cdpGuardScript = resolve(helpersDir, 'native-cdp-owner.ps1');
  const cdpGuardOutput = resolve(directory, `cdp-children-${index}.json`);

  let psStart;
  let browser;
  let startCompleted = false;
  let measurement;
  const errors = [];
  let selectedPage;
  const pairTasks = [];
  let pairResponseHandler;

  try {
    psStart = execFileP(
      'powershell.exe',
      [
        '-NoProfile',
        '-ExecutionPolicy',
        'Bypass',
        '-File',
        startScript,
        '-Exe',
        exe,
        '-Profile',
        profile,
        '-Output',
        identityFile,
        '-Port',
        String(port),
      ],
      { windowsHide: true, timeout: 20000 },
    );
    await psStart;
    startCompleted = true;

    const identity = await readIdentity(identityFile);

    const deadline = Date.now() + 15000;
    let versionOk = false;
    while (Date.now() < deadline) {
      const ctl = new AbortController();
      const tid = setTimeout(() => ctl.abort(), 1000);
      try {
        const r = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: ctl.signal });
        if (r.ok) {
          versionOk = true;
          break;
        }
      } catch {
        // ignore
      } finally {
        clearTimeout(tid);
      }
      await sleep(100);
    }
    if (!versionOk) throw new Error(`CDP version endpoint not reachable on port ${port}`);

    await execFileP('powershell.exe', [
      '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', cdpGuardScript,
      '-Identity', identityFile, '-Port', String(port), '-Output', cdpGuardOutput,
    ], { windowsHide: true, timeout: 20000 });

    browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);

    let context = browser.contexts()[0];
    if (!context) {
      const cDeadline = Date.now() + 5000;
      while (Date.now() < cDeadline && !context) {
        await sleep(100);
        context = browser.contexts()[0];
      }
    }
    if (!context) throw new Error('No browser context available via CDP');

    let page = context.pages().find((candidate) => /^https?:\/\/tauri\.localhost\//.test(candidate.url()) || candidate.url().startsWith('tauri://localhost'));
    if (!page) {
      const pDeadline = Date.now() + 5000;
      while (Date.now() < pDeadline && !page) {
        await sleep(100);
        page = context.pages().find((candidate) => /^https?:\/\/tauri\.localhost\//.test(candidate.url()) || candidate.url().startsWith('tauri://localhost'));
      }
    }
    if (!page) throw new Error('No native Tauri page available');
    selectedPage = page;
    pairResponseHandler = (response) => {
      try {
        const url = new URL(response.url());
        if (url.pathname !== '/api/v1/auth/pair' || !response.ok()) return;
        pairTasks.push((async () => {
          try {
            const body = await response.json();
            if (body && typeof body.client_id === 'string' && body.client_id.length > 0) onPaired(body.client_id);
          } catch (error) { errors.push(error instanceof Error ? error : new Error(String(error))); }
        })());
      } catch (error) { errors.push(error instanceof Error ? error : new Error(String(error))); }
    };
    page.on('response', pairResponseHandler);

    if (first) {
      await page.getByRole('button', { name: '이 PC를 호스트로 사용' }).click();
      await page.getByRole('checkbox').check();
      await page.getByRole('button', { name: '이 PC에서 Agent 시작 또는 연결' }).click();
    } else {
      await page.getByRole('button', { name: '다른 PC에 접속' }).click();
      await page.getByRole('checkbox').check();
      await page.getByRole('button', { name: '등록된 기기로 연결' }).click();
    }

    await page.locator('.workspace').waitFor({ state: 'visible', timeout: 15000 });
    await page.locator('.xterm-screen').waitFor({ state: 'visible', timeout: 15000 });
    await page.locator('.pane-toolbar .status.live').waitFor({ state: 'visible', timeout: 15000 });

    const statusJson = await execFileP(agentExe, ['status'], {
      windowsHide: true,
      timeout: 20000,
      maxBuffer: 8 * 1024 * 1024,
    });
    const status = JSON.parse(statusJson.stdout);

    let sessionMatch = null;
    const sessions = Array.isArray(status.sessions) ? status.sessions : [];
    for (const s of sessions) {
      if (
        s &&
        s.session_id === keeper.session_id &&
        s.pid === keeper.pid &&
        s.process_created === keeper.process_created &&
        s.state === 'running' &&
        status.epoch === keeper.agent_epoch
      ) {
        sessionMatch = s;
        break;
      }
    }
    if (!sessionMatch) {
      throw new Error('Agent session matching keeper not found or not in expected state');
    }

    const elapsed_ms = performance.now() - startedAt;

    measurement = {
      index,
      elapsed_ms,
      identity,
      keeper: {
        session_match: true,
        epoch_match: true,
        running: true,
      },
      session: {
        session_id: sessionMatch.session_id,
        pid: sessionMatch.pid,
        process_created: sessionMatch.process_created,
        state: sessionMatch.state,
        epoch: sessionMatch.agent_epoch,
      },
    };

    await beforeClose({ page, identity, identityFile });
  } catch (mainErr) {
    errors.push(mainErr);
  } finally {
    if (selectedPage && pairResponseHandler) selectedPage.off('response', pairResponseHandler);
    try { await Promise.all(pairTasks); } catch (error) { errors.push(error instanceof Error ? error : new Error(String(error))); }
    if (startCompleted) {
      try {
        try {
          await stat(identityFile);
        } catch {
          // identity file missing; nothing to close via helper
        }
        await execFileP(
          'powershell.exe',
          [
            '-NoProfile',
            '-ExecutionPolicy',
            'Bypass',
            '-File',
            closeScript,
            '-Identity',
            identityFile,
          ],
          { windowsHide: true, timeout: 20000 },
        );
      } catch (e) {
        errors.push(e);
      }
    }

    if (browser) {
      try {
        await browser.close();
      } catch (e) {
        errors.push(e);
      }
    }

    if (errors.length) {
      throw new AggregateError(errors, 'nativeCycle failed');
    }
    return measurement;
  }
}
