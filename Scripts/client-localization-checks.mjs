import assert from 'node:assert/strict';

export const names = ['details','enable','help','reported','disabled','missing','unavailable','node_stale','observed','mainnet','refresh','save_failed'];
export const androidLocale = {en:'',fil:'tl',id:'in','pt-BR':'pt-rBR','zh-Hans':'zh-rCN','zh-Hant':'zh-rTW'};
export const escapeAndroid = s => s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll("'", "\\'").replaceAll('"', '\\"');

// These generated resources contain plain escaped text, not markup/plurals.
// Check values too: key presence alone misses English fallbacks or privacy-copy drift.
export function assertAndroidExplorerCopy(xml, values, locale) {
  const entries = new Map();
  for (const [, name, value] of xml.matchAll(/<string\s+name="([^"]+)"\s*>([\s\S]*?)<\/string>/g)) {
    assert.ok(!entries.has(name), `Android ${locale}: duplicate ${name}`);
    entries.set(name, value);
  }
  assert.equal(entries.size, names.length + 1, `Android ${locale}: resource count`);
  names.forEach((name, i) => assert.equal(entries.get(`explorer_${name}`), escapeAndroid(values[i]), `Android ${locale}: ${name} translation`));
  assert.equal(entries.get('explorer_observed_fmt'), `${escapeAndroid(values[8])}: %1$s`, `Android ${locale}: observed format`);
}

export function assertAndroidAdvancedCopy(xml, names, values, locale) {
  assert.equal(names.length, 25);
  assert.equal(values.length, names.length);
  const entries = new Map();
  for (const [, name, value] of xml.matchAll(/<string\s+name="([^"]+)"\s*>([\s\S]*?)<\/string>/g)) {
    assert.ok(!entries.has(name), `Android ${locale}: duplicate ${name}`);
    entries.set(name, value);
  }
  assert.equal(entries.size, names.length, `Android ${locale}: advanced resource count`);
  names.forEach((name, i) => assert.equal(entries.get(`explorer_${name}`), escapeAndroid(values[i]), `Android ${locale}: advanced ${name}`));
}
