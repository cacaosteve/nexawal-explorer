// Local-only Caddy outage regression. Requires Node 18+ and Docker.
// Uses synthetic identifiers, a loopback published port, and disposable containers.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { request } from 'node:http';
import { spawn, execFileSync } from 'node:child_process';

const image = 'caddy:2.10.2-alpine@sha256:4c6e91c6ed0e2fa03efd5b44747b625fec79bc9cd06ac5235a779726618e530d';
const example = await readFile(new URL('../deploy/Caddyfile.example', import.meta.url), 'utf8');
execFileSync('docker', ['pull', image], { stdio: 'inherit', timeout: 120_000 });

function get(port, host, path, headers = {}) {
  return new Promise((resolve, reject) => {
    const req = request({ hostname: '127.0.0.1', port, path,
      headers: { Host: host, ...headers }, timeout: 3000 }, response => {
      let body = '';
      response.setEncoding('utf8');
      response.on('data', chunk => { body += chunk; });
      response.on('end', () => resolve({ status: response.statusCode, headers: response.headers, body }));
    });
    req.on('timeout', () => req.destroy(new Error('request timeout')));
    req.on('error', reject);
    req.end();
  });
}

for (const handled of [true, false]) {
  const socket = createServer();
  await new Promise(resolve => socket.listen(0, '127.0.0.1', resolve));
  const port = socket.address().port;
  await new Promise(resolve => socket.close(resolve));
  const name = `nexawal-proxy-test-${process.pid}-${handled}`;
  let config = example.replace('{\n', '{\n    admin off\n    persist_config off\n')
    .replace('explorer.nexawal.com {', 'http://explorer.test:8080 {');
  if (!handled) config = config.replace(/    handle_errors \{[\s\S]*?\n    \}/, '');
  config += '\nhttp://unrelated.test:8080 {\n reverse_proxy 127.0.0.1:1\n}\n';
  let logs = '';
  const child = spawn('docker', ['run', '--rm', '-i', '--name', name,
    '-p', `127.0.0.1:${port}:8080`, image,
    'caddy', 'run', '--config', '/dev/stdin', '--adapter', 'caddyfile']);
  child.stdout.on('data', chunk => { logs += chunk; });
  child.stderr.on('data', chunk => { logs += chunk; });
  const exited = new Promise(resolve => child.on('exit', resolve));
  child.stdin.on('error', () => {});
  child.stdin.end(config);
  try {
    let response;
    for (let i = 0; i < 100; i++) {
      try {
        response = await get(port, 'explorer.test', `/tx/${'a'.repeat(64)}?probe=PRIVATE_QUERY_CANARY`,
          { 'X-Forwarded-For': '192.0.2.123', 'X-Test-Canary': 'PRIVATE_HEADER_CANARY' });
        break;
      } catch {
        if (child.exitCode !== null) throw new Error(`Caddy exited: ${logs}`);
        await new Promise(resolve => setTimeout(resolve, 100));
      }
    }
    assert.ok(response, `Caddy did not start: ${logs}`);
    assert.equal(response.status, handled ? 503 : 502, JSON.stringify(response));
    if (handled) {
      assert.equal(response.headers['referrer-policy'], 'no-referrer');
      assert.equal(response.headers['cache-control'], 'no-store');
      assert.match(response.headers['content-security-policy'], /default-src 'none'/);
      assert.match(response.body, /Explorer temporarily unavailable/);
    }
    const other = await get(port, 'unrelated.test', '/UNRELATED_LOG_CANARY');
    assert.equal(other.status, 502);
  } finally {
    try { execFileSync('docker', ['stop', '-t', '1', name], { stdio: 'ignore', timeout: 10_000 }); } catch {}
    await exited;
  }
  for (const secret of ['a'.repeat(64), 'PRIVATE_QUERY_CANARY', 'PRIVATE_HEADER_CANARY', '192.0.2.123']) {
    assert.ok(!logs.includes(secret), `Explorer request leaked into logs: ${secret}`);
  }
  assert.ok(logs.includes('UNRELATED_LOG_CANARY'), `Unrelated site's error log was lost: ${logs}`);
  console.log(`PASS: ${handled ? 'handled 503' : 'unhandled 502'} protects explorer logs; unrelated errors retained`);
}
