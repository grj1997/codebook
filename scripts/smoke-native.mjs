// Exercises our own disposable app instance and the real Native Messaging bridge.
// It never registers a host or reads the user's real vault.
import { spawn } from 'node:child_process';
import { mkdir, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import assert from 'node:assert/strict';

const root = resolve(import.meta.dirname, '..');
const local = resolve(root, '.implementation', 'runtime-smoke', String(Date.now()));
await mkdir(local, { recursive: true });
const identity = JSON.parse(await readFile(resolve(root, 'shared/extension-id.json'), 'utf8'));
const executable = resolve(root, 'target/release/codebook.exe');
const hostPath = resolve(root, 'target/release/codebook-native-host.exe');
const children = [];
function run(path, args = [], env = process.env) {
  const child = spawn(path, args, { env, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  children.push(child); return child;
}
function frame(message) {
  const body = Buffer.from(JSON.stringify(message)), length = Buffer.alloc(4);
  length.writeUInt32LE(body.length); return Buffer.concat([length, body]);
}
function reply(child, timeout = 20_000) {
  return new Promise((accept, reject) => {
    let buffer = Buffer.alloc(0);
    const timer = setTimeout(() => reject(new Error('Native reply timeout')), timeout);
    child.stdout.on('data', bytes => {
      buffer = Buffer.concat([buffer, bytes]);
      if (buffer.length >= 4) {
        const size = buffer.readUInt32LE();
        if (size > 1024 * 1024) { clearTimeout(timer); reject(new Error('Oversized reply')); return; }
        if (buffer.length >= size + 4) {
          clearTimeout(timer); accept(JSON.parse(buffer.subarray(4, size + 4).toString('utf8')));
        }
      }
    });
    child.once('error', error => { clearTimeout(timer); reject(error); });
  });
}
try {
  const rejected = run(hostPath, ['chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/']);
  console.log('Checking invalid extension origin...');
  const denial = await reply(rejected);
  assert.equal(denial.type, 'error');
  assert.match(denial.message, /来源未获允许/);
  const app = run(executable, [], { ...process.env, LOCALAPPDATA: local });
  console.log('Checking isolated application startup...');
  await new Promise(r => setTimeout(r, 4000));
  assert.equal(app.exitCode, null, 'Disposable application failed to remain running');
  const second = run(executable, [], { ...process.env, LOCALAPPDATA: local });
  const secondExit = await new Promise((accept, reject) => {
    const timer = setTimeout(() => reject(new Error('Second application instance did not exit')), 10_000);
    second.once('exit', code => { clearTimeout(timer); accept(code); });
    second.once('error', error => { clearTimeout(timer); reject(error); });
  });
  assert.equal(secondExit, 0);
  assert.equal(app.exitCode, null, 'Original application should remain running');
  const host = run(hostPath, [`chrome-extension://${identity.id}/`]);
  console.log('Checking Native Messaging through the desktop pipe...');
  const statusPromise = reply(host);
  host.stdin.write(frame({ type: 'hello', browser: 'Google Chrome' }));
  const status = await statusPromise;
  assert.equal(status.type, 'status');
  assert.equal(status.paired, false, 'A fresh isolated vault must not authorize a browser');
  console.log('PASS: application startup, single instance, SID pipe, framed Native Messaging, origin rejection, unpaired browser denial');
} finally {
  for (const child of children.reverse()) if (child.exitCode === null) child.kill();
}
