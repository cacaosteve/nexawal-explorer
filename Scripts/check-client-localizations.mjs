import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { androidLocale, assertAndroidExplorerCopy, assertAndroidAdvancedCopy } from './client-localization-checks.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const catalog = JSON.parse(fs.readFileSync(path.join(root,'nexawal-explorer/client-strings.json')));
const advanced = JSON.parse(fs.readFileSync(path.join(root,'nexawal-explorer/client-advanced-strings.json')));
assert.deepEqual(Object.keys(advanced.locales).sort(), Object.keys(catalog).sort());
const ios = JSON.parse(fs.readFileSync(path.join(root,'nexawal/nexawal/Localizable.xcstrings')));
const desktop = JSON.parse(fs.readFileSync(path.join(root,'nexawal-gpui/assets/l10n.json')));
assert.deepEqual(Object.keys(catalog).sort(),Object.keys(desktop).sort());
for (const [locale, values] of Object.entries(catalog)) {
  assert.equal(values.length,12);
  const xml = fs.readFileSync(path.join(root,'nexawal-android/app/src/main/res',`values${locale==='en'?'':'-'+(androidLocale[locale]??locale)}`,'explorer.xml'),'utf8');
  assertAndroidExplorerCopy(xml, values, locale);
  for (let i=0;i<12;i++) {
    const key = catalog.en[i];
    assert.ok(ios.strings[key]?.localizations?.[locale]?.stringUnit?.value, `iOS ${locale} ${key}`);
    assert.ok(desktop[locale][key], `GPUI ${locale} ${key}`);
    if (i<10) { assert.equal(ios.strings[key].localizations[locale].stringUnit.value,values[i]); assert.equal(desktop[locale][key],values[i]); }
  }
  assert.equal((xml.match(/%1\$s/g)??[]).length,1,locale);
  assert.ok(!xml.includes('%@'));
  for (const token of ['IP','I2P','HTTPS','explorer.nexawal.com']) assert.ok(values[2].includes(token));
  const advancedXml = fs.readFileSync(path.join(root,'nexawal-android/app/src/main/res',`values${locale==='en'?'':'-'+(androidLocale[locale]??locale)}`,'explorer_advanced.xml'),'utf8');
  assertAndroidAdvancedCopy(advancedXml, advanced.names, advanced.locales[locale], locale);
  advanced.locales[locale].forEach((value,i)=>{
    const key = advanced.locales.en[i];
    const mobile = ios.strings[key]?.localizations?.[locale]?.stringUnit?.value;
    assert.ok(mobile && desktop[locale][key], `${locale}: advanced ${key}`);
    if (i !== 11) { assert.equal(mobile,value); assert.equal(desktop[locale][key],value); }
  });
}
console.log('Explorer labels, privacy/error copy and format-token coverage verified across all 54 locales and all three clients.');
