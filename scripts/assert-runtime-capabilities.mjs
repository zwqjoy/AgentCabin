import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

export function assertRuntimeCapabilities(runtimeRoot, extensionsRoot) {
  const hostPath = join(runtimeRoot, 'pi/node_modules/@earendil-works/pi-coding-agent/package.json');
  const host = JSON.parse(readFileSync(hostPath, 'utf8'));
  if (host.version !== '1.0.2') throw new Error(`Installed Pi Host version mismatch: ${host.version}`);
  const forbidden = new Set(['agent-browser','pi-agent-browser-native','pi-computer-use','agentcabin-computer-use']);
  function visit(dir) {
    if (!existsSync(dir)) return;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (forbidden.has(entry.name)) throw new Error(`Removed automation resource still bundled: ${join(dir,entry.name)}`);
      if (!entry.isDirectory()) continue;
      const child = join(dir,entry.name);
      if (entry.name === 'pi-coding-agent' && resolve(child) !== resolve(hostPath, '..')) {
        throw new Error(`Unexpected additional Pi Host: ${child}`);
      }
      visit(child);
    }
  }
  visit(runtimeRoot);
  if (!existsSync(join(extensionsRoot,'package.json'))) throw new Error(`Missing extension closure: ${extensionsRoot}`);
  visit(extensionsRoot);
  const manifest = JSON.parse(readFileSync(join(extensionsRoot,'package.json'),'utf8'));
  for (const name of Object.keys(manifest.dependencies || {})) {
    if (forbidden.has(name.split('/').at(-1))) throw new Error(`Removed automation dependency: ${name}`);
  }
  console.log('✓ Installed Pi 1.0.2; no automation resources or additional Pi Host in closure');
}
