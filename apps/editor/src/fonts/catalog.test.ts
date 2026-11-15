// The font catalogue and the pure helpers around it.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { LICENSES, blobSha, fontUrl, groupInstalled, openFamilies, searchFamilies, verified } from './catalog.ts';

test('every catalogue font names a licence that allows commercial use, and a checkable file', () => {
  assert.ok(openFamilies.length >= 60);
  assert.deepEqual([...new Set(openFamilies.map((f) => f.category))].sort(), ['display', 'handwriting', 'monospace', 'sans-serif', 'serif']);
  for (const family of openFamilies) {
    assert.ok(family.license && family.license in LICENSES, `${family.family}: ${family.license}`);
    assert.match(family.licenseUrl ?? '', /^https:\/\/github\.com\/google\/fonts\/blob\/[0-9a-f]{40}\/(ofl|apache|ufl)\/[a-z0-9]+\/(OFL|UFL|LICENSE)\.txt$/);
    assert.ok(family.styles.length > 0 && family.styles[0].style === 'Regular', family.family);
    for (const style of family.styles) {
      assert.match(style.sha ?? '', /^[0-9a-f]{40}$/);
      assert.ok(style.path.endsWith('.ttf') && (style.bytes ?? 0) > 10_000, style.path);
    }
  }
  // The licences are named as SPDX names them, and each says what it asks.
  assert.deepEqual(Object.keys(LICENSES).sort(), ['Apache-2.0', 'OFL-1.1', 'Ubuntu-font-1.0']);
  assert.equal(openFamilies.find((f) => f.family === 'Ubuntu')?.license, 'Ubuntu-font-1.0');
  assert.ok(Object.values(LICENSES).every((license) => license.name && license.terms));
  const inter = openFamilies.find((f) => f.family === 'Inter')!;
  assert.deepEqual([inter.license, inter.axes, inter.styles.map((s) => s.style)], ['OFL-1.1', ['opsz', 'wght'], ['Regular', 'Italic']]);
  // Brackets and commas in a file name are escaped in its address.
  assert.match(fontUrl(inter.styles[0].path), /^https:\/\/raw\.githubusercontent\.com\/google\/fonts\/[0-9a-f]{40}\/ofl\/inter\/Inter%5Bopsz%2Cwght%5D\.ttf$/);
});

test('a download is checked against the hash the catalogue lists', async () => {
  const hello = new TextEncoder().encode('hello\n');
  // `git hash-object` of a file holding "hello\n".
  assert.equal(await blobSha(hello), 'ce013625030ba8dba906f756967f9e9ca394464a');
  const style = { style: 'Regular', path: 'x.ttf', sha: 'ce013625030ba8dba906f756967f9e9ca394464a' };
  assert.equal(await verified(style, hello), hello);
  await assert.rejects(verified(style, new TextEncoder().encode('hello')), /not the file the catalogue lists/);
  // An installed font has no hash to check.
  assert.equal(await verified({ style: 'Regular', path: '/fonts/x.ttf', index: 0 }, hello), hello);
});

test('installed fonts are grouped by family, and families are searched by name', () => {
  const families = groupInstalled([
    ['Menlo', 'Bold', '/f/Menlo.ttc', 1],
    ['Arial', 'Regular', '/f/Arial.ttf', 0],
    ['Menlo', 'Regular', '/f/Menlo.ttc', 0],
    ['Menlo', 'Thin', '/f/Menlo.ttc', 2],
  ]);
  assert.deepEqual(
    families.map((f) => [f.family, f.styles.map((s) => `${s.style}#${s.index}`)]),
    [
      ['Arial', ['Regular#0']],
      ['Menlo', ['Regular#0', 'Bold#1', 'Thin#2']],
    ],
  );
  assert.deepEqual(
    searchFamilies(families, ' men ').map((f) => f.family),
    ['Menlo'],
  );
  assert.deepEqual(
    searchFamilies(openFamilies, 'plex').map((f) => f.family),
    ['IBM Plex Mono', 'IBM Plex Sans'],
  );
  assert.ok(searchFamilies(openFamilies, '', 'monospace').every((f) => f.category === 'monospace'));
  assert.deepEqual(
    searchFamilies(openFamilies, 'plex', 'monospace').map((f) => f.family),
    ['IBM Plex Mono'],
  );
});
