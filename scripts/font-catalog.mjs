// Writes apps/editor/src/fonts/catalog.json: the open-licence fonts the editor offers.
//   node scripts/font-catalog.mjs
//
// Nothing about a font is typed in here except which families to look up. Its licence,
// category and files are read from the family's own METADATA.pb in the Google Fonts
// repository at one commit, and a family whose licence is not one of LICENSES stops the
// script. Sizes and hashes come from that commit's file list, so the editor can check
// what it downloads. To move to a newer commit or to add a family, edit `commit` or
// FAMILIES below and run it again.
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const REPO = 'google/fonts';
const commit = 'cc39c4c2c304bc368c9b7c311ea2bcc6013299fe';
// METADATA.pb's `license` -> its SPDX identifier. All three allow commercial use.
const LICENSES = { OFL: 'OFL-1.1', APACHE2: 'Apache-2.0', UFL: 'Ubuntu-font-1.0' };
// Folder names under ofl/, apache/ or ufl/ in the repository.
const FAMILIES = `
  inter roboto opensans notosans worksans dmsans manrope montserrat nunito plusjakartasans figtree ibmplexsans
  sourcesans3 rubik spacegrotesk poppins lato raleway quicksand archivo oswald barlow kanit ubuntu fredoka orbitron outfit
  playfairdisplay merriweather lora sourceserif4 ebgaramond librebaskerville fraunces robotoslab dmserifdisplay crimsonpro
  bitter notoserif cinzel cormorantgaramond
  jetbrainsmono firacode robotomono sourcecodepro ibmplexmono spacemono inconsolata
  bebasneue anton archivoblack righteous lobster abrilfatface alfaslabone bungee comfortaa pressstart2p
  caveat pacifico dancingscript permanentmarker satisfy shadowsintolight indieflower greatvibes
`.split(/\s+/).filter(Boolean);

const get = async (url) => {
  const response = await fetch(url, { headers: { 'user-agent': 'zuri-font-catalog' } });
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return response;
};
const tree = await (await get(`https://api.github.com/repos/${REPO}/git/trees/${commit}?recursive=1`)).json();
if (tree.truncated) throw new Error('the file list came back truncated: the hashes cannot be trusted');
const files = new Map(tree.tree.map((entry) => [entry.path, entry]));

const field = (text, name) => new RegExp(`^\\s*${name}: "?([^"\\n]*)"?`, 'm').exec(text)?.[1];
const families = [];
for (const slug of FAMILIES) {
  const dir = ['ofl', 'apache', 'ufl'].map((licence) => `${licence}/${slug}`).find((dir) => files.has(`${dir}/METADATA.pb`));
  if (!dir) throw new Error(`${slug}: no such family in ${REPO}`);
  const meta = await (await get(`https://raw.githubusercontent.com/${REPO}/${commit}/${dir}/METADATA.pb`)).text();
  const license = LICENSES[field(meta, 'license')];
  if (!license) throw new Error(`${slug}: licence "${field(meta, 'license')}" is not one this catalogue takes`);
  const licenseFile = [...files.keys()].find((path) => path.startsWith(`${dir}/`) && /\/(OFL|UFL|LICENSE)\.txt$/.test(path));
  if (!licenseFile) throw new Error(`${slug}: no licence text beside the fonts`);
  const family = field(meta, 'name');
  const axes = [...meta.matchAll(/axes \{\s*tag: "(\w+)"/g)].map((match) => match[1]);
  let fonts = [...meta.matchAll(/fonts \{([^}]*)\}/g)].map((match) => match[1]);
  // A variable family is one or two files. Of a static one: regular and bold, upright and italic.
  if (!axes.length && fonts.some((font) => field(font, 'weight') === '400')) fonts = fonts.filter((font) => ['400', '700'].includes(field(font, 'weight')));
  const styles = fonts.map((font) => {
    const file = files.get(`${dir}/${field(font, 'filename')}`);
    if (!file) throw new Error(`${slug}: ${field(font, 'filename')} is not in the repository`);
    const named = field(font, 'full_name').replace(family, '').trim();
    const style = axes.length ? (field(font, 'style') === 'italic' ? 'Italic' : 'Regular') : named || 'Regular';
    return { style, path: file.path, bytes: file.size, sha: file.sha };
  });
  families.push({ family, category: field(meta, 'category').toLowerCase().replace('_', '-'), license, licenseFile, axes, styles });
  console.log(`${family}: ${license}, ${styles.map((style) => style.style).join(' / ')}${axes.length ? ` (variable: ${axes.join(', ')})` : ''}`);
}
families.sort((a, b) => a.family.localeCompare(b.family));
const out = fileURLToPath(new URL('../apps/editor/src/fonts/catalog.json', import.meta.url));
writeFileSync(out, JSON.stringify({ source: `https://github.com/${REPO}`, commit, families }, null, 1) + '\n');
console.log(`wrote ${out}: ${families.length} families`);
