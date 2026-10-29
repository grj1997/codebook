import { build } from 'esbuild';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
const root = new URL('../', import.meta.url);
const outdir = new URL('browser-extension/dist/', root);
await mkdir(outdir, { recursive: true });
const identity = JSON.parse(await readFile(new URL('shared/extension-id.json', root), 'utf8'));
await build({ entryPoints: ['browser-extension/src/background.ts', 'browser-extension/src/content.ts'], outdir: 'browser-extension/dist', bundle: true, format: 'iife', platform: 'browser', target: 'chrome120', minify: false });
await writeFile(new URL('manifest.json', outdir), JSON.stringify({
  manifest_version: 3, name: 'Codebook · 本地账户填充', version: '0.1.0', minimum_chrome_version: '120',
  description: '连接本机 Codebook 密码库，在确认后填入当前网站的账户。', key: identity.key,
  permissions: ['nativeMessaging', 'tabs', 'webNavigation', 'alarms'],
  host_permissions: ['https://*/*'],
  background: { service_worker: 'background.js' },
  action: { default_title: '从本机 Codebook 选择账户' },
  content_scripts: [{ matches: ['https://*/*'], js: ['content.js'], run_at: 'document_idle', all_frames: false }],
  content_security_policy: { extension_pages: "script-src 'self'; object-src 'none'" },
}, null, 2));
console.log(`Extension ready: browser-extension/dist (ID: ${identity.id})`);
