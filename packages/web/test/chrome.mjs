// Headless Chrome for the browser checks: load a page, wait for it to settle, and return
// what it wrote into <pre id="result"> as JSON.
// CHROME=/path/to/chrome overrides where Chrome is looked for.
import { execFile } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';

export { isDeepStrictEqual as same } from 'node:util';

const onPath = (name) => (process.env.PATH ?? '').split(delimiter).map((dir) => join(dir, name));
const chrome = [
  process.env.CHROME,
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ...['google-chrome', 'chromium', 'chromium-browser'].flatMap(onPath),
  'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
].find((path) => path && existsSync(path));

// What Chrome said on stderr: usually the reason a check fails (no GPU, sandbox, a page error).
let said = '';
const tail = () => `--- Chrome stderr (last lines) ---\n${said.split('\n').slice(-25).join('\n')}`;

/** What the page at `url` measured. Exits the process when there is no Chrome to ask. */
export async function measure(url) {
  if (!chrome) {
    // A gate that skips silently is not a gate: skipping has to be asked for.
    if (process.env.ZURI_SKIP_BROWSER) {
      console.log('browser checks skipped (ZURI_SKIP_BROWSER is set)');
      process.exit(0);
    }
    console.error('Chrome not found: set CHROME=/path/to/chrome, or ZURI_SKIP_BROWSER=1 to skip the browser checks on purpose');
    process.exit(1);
  }
  const profile = mkdtempSync(join(tmpdir(), 'zuri-chrome-'));
  const { stdout, stderr } = await new Promise((done) =>
    execFile(
      chrome,
      // --enable-unsafe-swiftshader lets WebGL fall back to software where there is no GPU (CI).
      // --autoplay-policy lets a page play sound without a person having touched it (no one will).
      ['--headless=new', '--no-first-run', '--window-size=900,1400', '--virtual-time-budget=20000', '--enable-unsafe-swiftshader', '--autoplay-policy=no-user-gesture-required', `--user-data-dir=${profile}`, '--dump-dom', url],
      // A hung Chrome fails here in two minutes, not when the CI job times out.
      { maxBuffer: 64 << 20, timeout: 120_000 },
      (error, stdout, stderr) => done({ stdout, stderr: `${stderr}\n${error?.message ?? ''}` }),
    ),
  );
  // Allowed to fail: on Windows Chrome's helpers can still hold files for a moment.
  try {
    rmSync(profile, { recursive: true, force: true, maxRetries: 5 });
  } catch {}
  said = stderr;
  const result = /<pre id="result">(.*?)<\/pre>/s.exec(stdout)?.[1];
  if (!result) throw new Error(`the page produced no result\n${tail()}`);
  return JSON.parse(result.replaceAll('&quot;', '"').replaceAll('&lt;', '<').replaceAll('&gt;', '>').replaceAll('&amp;', '&'));
}

/** Check `condition`, saying what was checked; `r` is printed when it does not hold. */
export function checker(r) {
  return (condition, what) => {
    if (!condition) {
      // No stack: what failed, what the page measured and what Chrome said are the report.
      console.error(`FAILED: ${what}\n${JSON.stringify(r, null, 1)}\n${tail()}`);
      process.exit(1);
    }
    console.log('ok  ', what);
  };
}
