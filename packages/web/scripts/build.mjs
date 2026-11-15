// Builds the three wasm variants and compiles the TypeScript wrapper into dist/.
//   zuri-lite.wasm    shapes, animation, state machines, data binding (no text)
//   zuri.wasm         + text                                  (the default player)
//   zuri-editor.wasm  + JSON source compile / decompile / schema (authoring tools)
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { brotliCompressSync } from 'node:zlib';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkg = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(pkg, '..', '..');
const dist = join(pkg, 'dist');
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });

const LITE_LIMIT = 80_000; // bytes, brotli
const variants = [
  ['zuri-lite.wasm', ['--no-default-features']],
  ['zuri.wasm', []],
  ['zuri-editor.wasm', ['--features', 'json']],
];
// The wasm goes to npm: cargo-ship.sh keeps the build machine's paths out of it, and checks.
const ship = join(repo, 'scripts', 'cargo-ship.sh');
for (const [name, flags] of variants) {
  // A target dir per variant keeps the builds from invalidating each other.
  const target = join(repo, 'target', 'wasm-' + name.replace('.wasm', ''));
  execFileSync('bash', [ship, 'build', '-p', 'zuri-ffi', '--profile', 'wasm', '--target', 'wasm32-unknown-unknown', '--target-dir', target, ...flags], { cwd: repo, stdio: 'inherit' });
  const out = join(dist, name);
  copyFileSync(join(target, 'wasm32-unknown-unknown', 'wasm', 'zuri.wasm'), out);
  execFileSync('bash', [ship, '--check', out], { stdio: 'inherit' });
  const wasm = readFileSync(out);
  const brotli = brotliCompressSync(wasm).length;
  console.log(`${name}: ${wasm.length} bytes (${brotli} brotli)`);
  // The budget for the player is 100 KB. This limit sits just above what it measures, so
  // that a jump is a decision: one `str::parse::<f32>` and one `f32::clamp` with a computed
  // bound cost 21 KB between them. Raise it with the feature that needs the room, and say
  // so in memory/todo.md.
  if (name === 'zuri-lite.wasm' && brotli > LITE_LIMIT) {
    console.error(`zuri-lite.wasm is ${brotli} bytes brotli, over the limit of ${LITE_LIMIT}: find what grew (twiggy top) or raise LITE_LIMIT in ${import.meta.filename}`);
    process.exit(1);
  }
}
// Through node, not node_modules/.bin/tsc: that is a shell script, which Windows cannot spawn.
execFileSync(process.execPath, [join(pkg, 'node_modules', 'typescript', 'bin', 'tsc'), '-p', pkg], { cwd: pkg, stdio: 'inherit' });
console.log('built dist/');
