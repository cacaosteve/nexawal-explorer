// Mechanical catalog synchronization. Website translations are deliberately separate.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const catalog = JSON.parse(fs.readFileSync(path.join(root, 'nexawal-explorer/client-strings.json'), 'utf8'));
const iosPath = path.join(root, 'nexawal/nexawal/Localizable.xcstrings');
const gpuiPath = path.join(root, 'nexawal-gpui/assets/l10n.json');
const ios = JSON.parse(fs.readFileSync(iosPath, 'utf8'));
const gpui = JSON.parse(fs.readFileSync(gpuiPath, 'utf8'));
const names = ['details', 'enable', 'help', 'reported', 'disabled', 'missing', 'unavailable', 'node_stale', 'observed', 'mainnet', 'refresh', 'save_failed'];
assert.deepEqual(Object.keys(catalog).sort(), Object.keys(gpui).sort());
const escaped = s => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll("'", "\\'").replaceAll('"', '\\"');
const androidLocale = {en:'',fil:'tl',id:'in','pt-BR':'pt-rBR','zh-Hans':'zh-rCN','zh-Hant':'zh-rTW'};
for (const [locale, strings] of Object.entries(catalog)) {
  assert.equal(strings.length, names.length, locale);
  strings.forEach((value, index) => {
    assert.equal(typeof value, 'string'); assert.ok(value.trim());
    if (index === 2) for (const token of ['IP', 'I2P', 'HTTPS', 'explorer.nexawal.com']) assert.ok(value.includes(token), `${locale}: ${token}`);
    const key = catalog.en[index];
    ios.strings[key] ??= {extractionState:'manual', localizations:{}};
    ios.strings[key].localizations ??= {};
    // Existing shared labels retain their translations. New feature copy is authoritative here.
    if (index < 10 || !ios.strings[key].localizations[locale]) ios.strings[key].localizations[locale] = {stringUnit:{state:'translated', value}};
    if (index < 10 || !gpui[locale][key]) gpui[locale][key] = value;
  });
  const dir = path.join(root, 'nexawal-android/app/src/main/res', `values${locale === 'en' ? '' : '-'+(androidLocale[locale] ?? locale)}`);
  assert.ok(fs.statSync(dir).isDirectory(), dir);
  const rows = strings.map((s,i) => `    <string name="explorer_${names[i]}">${escaped(s)}</string>`);
  rows.push(`    <string name="explorer_observed_fmt">${escaped(strings[8])}: %1$s</string>`);
  const xml = `<?xml version="1.0" encoding="utf-8"?>\n<!-- Draft translations synchronized by nexawal-explorer/Scripts/sync-client-localizations.mjs. -->\n<resources>\n${rows.join('\n')}\n</resources>\n`;
  fs.writeFileSync(path.join(dir, 'explorer.xml'), xml);
}
fs.writeFileSync(iosPath, JSON.stringify(ios, null, 2)+'\n');
fs.writeFileSync(gpuiPath, JSON.stringify(gpui, null, 2)+'\n');
console.log(`Synchronized ${names.length} explorer strings across ${Object.keys(catalog).length} locales in all three clients.`);
