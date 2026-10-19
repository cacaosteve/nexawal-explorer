import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const files = [
  'nexawal-explorer/tests/fixtures/client_advanced_page0.json',
  'nexawal/Packages/NexaWalLogic/Tests/NexaWalLogicTests/Fixtures/explorer_advanced_page0.json',
  'nexawal-android/app/src/test/resources/explorer_advanced_page0.json',
  'nexawal-gpui/tests/fixtures/explorer_advanced_page0.json',
];
const canonical = fs.readFileSync(path.join(root,files[0]),'utf8');
for (const file of files) assert.equal(fs.readFileSync(path.join(root,file),'utf8'),canonical,file);
console.log('API contract and all three clients use identical synthetic advanced-page fixtures.');
