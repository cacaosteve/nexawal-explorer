// Mechanical catalog synchronization. Website translations are deliberately separate.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { names, androidLocale, escapeAndroid as escaped } from './client-localization-checks.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const catalog = JSON.parse(fs.readFileSync(path.join(root, 'nexawal-explorer/client-strings.json'), 'utf8'));
const advanced = JSON.parse(fs.readFileSync(path.join(root, 'nexawal-explorer/client-advanced-strings.json'), 'utf8'));
assert.deepEqual(Object.keys(advanced.locales).sort(), Object.keys(catalog).sort());
const iosPath = path.join(root, 'nexawal/nexawal/Localizable.xcstrings');
const gpuiPath = path.join(root, 'nexawal-gpui/assets/l10n.json');
const ios = JSON.parse(fs.readFileSync(iosPath, 'utf8'));
const gpuiText = fs.readFileSync(gpuiPath, 'utf8');
const gpui = JSON.parse(gpuiText);
// JSON.stringify otherwise moves integer-like keys ("0", "1") to the front.
// Preserve existing catalog order to avoid unrelated localization churn.
const order = {};
let language;
for (const line of gpuiText.split('\n')) {
  const locale = line.match(/^  ("(?:[^"\\]|\\.)+"): \{/);
  if (locale) { language = JSON.parse(locale[1]); order[language] = []; }
  const key = line.match(/^    ("(?:[^"\\]|\\.)*"): /);
  if (key && language) order[language].push(JSON.parse(key[1]));
}
assert.deepEqual(Object.keys(catalog).sort(), Object.keys(gpui).sort());
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
  const values = advanced.locales[locale];
  assert.equal(values.length, advanced.names.length, locale);
  values.forEach((value, i) => {
    assert.equal(typeof value, 'string'); assert.ok(value.trim(), `${locale}: ${i}`);
    const key = advanced.locales.en[i];
    ios.strings[key] ??= {extractionState:'manual', localizations:{}};
    ios.strings[key].localizations ??= {};
    // Next is an existing shared label; retain native shared translations.
    if (i !== 11 || !ios.strings[key].localizations[locale]) ios.strings[key].localizations[locale] = {stringUnit:{state:'translated',value}};
    if (i !== 11 || !gpui[locale][key]) gpui[locale][key] = value;
  });
  fs.writeFileSync(path.join(dir, 'explorer_advanced.xml'), `<?xml version="1.0" encoding="utf-8"?>\n<!-- Draft translations synchronized by nexawal-explorer/Scripts/sync-client-localizations.mjs. -->\n<resources>\n${values.map((value,i)=>`    <string name="explorer_${advanced.names[i]}">${escaped(value)}</string>`).join('\n')}\n</resources>\n`);
}
fs.writeFileSync(iosPath, JSON.stringify(ios, null, 2)+'\n');
const sections = Object.entries(gpui).map(([locale, strings]) => {
  const keys = [...order[locale], ...Object.keys(strings).filter(key => !order[locale].includes(key))];
  return `  ${JSON.stringify(locale)}: {\n${keys.map(key => `    ${JSON.stringify(key)}: ${JSON.stringify(strings[key])}`).join(',\n')}\n  }`;
});
fs.writeFileSync(gpuiPath, `{\n${sections.join(',\n')}\n}\n`);
console.log(`Synchronized ${names.length} summary and ${advanced.names.length} advanced explorer strings across ${Object.keys(catalog).length} locales in all three clients.`);
