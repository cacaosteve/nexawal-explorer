import test from 'node:test';
import assert from 'node:assert/strict';
import { names, escapeAndroid, assertAndroidExplorerCopy, assertAndroidAdvancedCopy } from './client-localization-checks.mjs';
import fs from 'node:fs';

const values = names.map(name => `Français ${name} & l'adresse "test"`);
const rows = values.map((value, i) => `<string name="explorer_${names[i]}">${escapeAndroid(value)}</string>`);
rows.push(`<string name="explorer_observed_fmt">${escapeAndroid(values[8])}: %1$s</string>`);
const xml = `<resources>${rows.join('\n')}</resources>`;

test('compares every actual Android translation including escaped text', () => {
  assertAndroidExplorerCopy(xml, values, 'fr');
  for (let i = 0; i < names.length; i++) {
    assert.throws(() => assertAndroidExplorerCopy(xml.replace(rows[i], `<string name="explorer_${names[i]}">English fallback</string>`), values, 'fr'), /translation/);
  }
});
test('advanced copy validates all locales and catches every changed value', () => {
  const catalog = JSON.parse(fs.readFileSync(new URL('../client-advanced-strings.json', import.meta.url)));
  for (const [locale, values] of Object.entries(catalog.locales)) {
    const rows = values.map((value,i)=>`<string name="explorer_${catalog.names[i]}">${escapeAndroid(value)}</string>`);
    const xml = `<resources>${rows.join('\n')}</resources>`;
    assertAndroidAdvancedCopy(xml,catalog.names,values,locale);
    rows.forEach((row,i)=>assert.throws(()=>assertAndroidAdvancedCopy(xml.replace(row,`<string name="explorer_${catalog.names[i]}">WRONG</string>`),catalog.names,values,locale)));
  }
});
test('rejects missing, duplicate, empty and malformed-format resources', () => {
  for (const broken of [xml.replace(rows[0], ''), xml.replace(rows[0], rows[0] + rows[0]),
    xml.replace(escapeAndroid(values[0]), ''), xml.replace('%1$s', '%@')]) {
    assert.throws(() => assertAndroidExplorerCopy(broken, values, 'fr'));
  }
});
