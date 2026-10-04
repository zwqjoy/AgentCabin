import assert from 'node:assert/strict';
import test from 'node:test';
import { buildWorkNativeMcpConfig } from './pi_mcp_adapter.mjs';

test('native config exposes only authenticated loopback metadata', () => {
  const raw = { settings: { directTools: true }, mcpServers: { docs: {
    command: 'secret-command', args: ['SECRET'], cwd: '/SECRET', env: { API_KEY: 'SECRET' },
    url: 'https://SECRET.invalid', headers: { Authorization: 'SECRET' },
    auth: { provider: 'SECRET' }, oauth: { token: 'SECRET' }, description: 'API_KEY=SECRET',
  }, disabled: { disabled: true, command: 'SECRET' } } };
  const config = buildWorkNativeMcpConfig(raw, { mcpServers: { package: { url: 'https://SECRET.invalid' } } }, { baseUrl: 'http://127.0.0.1:49321', token: 'run-token' });
  assert.equal(JSON.stringify(config).includes('SECRET'), false);
  assert.equal(JSON.stringify(config).includes('secret-command'), false);
  assert.equal(config.autoEnableCodemode, false);
  assert.deepEqual(config.errors, []);
  for (const server of config.servers) {
    assert.equal(server.source, 'agentcabin-work-host'); assert.equal(server.scope, 'extension');
    assert.deepEqual(Object.keys(server.config).sort(), ['description','enabled','exposure','headers','timeout','type','url']);
    assert.equal(server.config.type, 'http'); assert.equal(server.config.exposure, 'deferred');
    assert.equal(server.config.timeout, 600);
    assert.match(server.config.url, /^http:\/\/127\.0\.0\.1:49321\/internal\/work\/mcp\//);
    assert.deepEqual(server.config.headers, { Authorization: 'Bearer run-token' });
  }
  assert.equal(config.servers.find(s => s.name === 'disabled').config.enabled, false);
});

test('projection fails closed on non-loopback targets and invalid tokens', () => {
  for (const baseUrl of ['https://127.0.0.1:1234','http://localhost:1234','http://remote.invalid:1234','http://user@127.0.0.1:1234','http://127.0.0.1:1234/path']) {
    assert.throws(() => buildWorkNativeMcpConfig({}, {}, { baseUrl, token: 'token' }));
  }
  for (const token of ['', 'x\n', 'x'.repeat(513)]) assert.throws(() => buildWorkNativeMcpConfig({}, {}, { baseUrl: 'http://127.0.0.1:1234', token }));
});

test('shim injects custom native loadConfig and refuses persistence', async () => {
  const fs = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const { default: workNativeMcp } = await import('./pi_mcp_adapter.mjs');
  const dir = fs.mkdtempSync(join(tmpdir(), 'native-mcp-config-'));
  const api = join(dir, 'native-api.mjs');
  fs.writeFileSync(api, `export const createToolSearchExtension = () => pi => pi.registerTool({name:'tool_search'});
export const createMcpExtension = options => pi => { globalThis.__nativeMcpOptions = options; };`);
  const raw = JSON.stringify({ mcpServers: { docs: { command: 'SECRET', env: { API_KEY: 'SECRET' } } } });
  fs.writeFileSync(join(dir, 'mcp.json'), raw);
  const env = {
    AGENTCABIN_WORK_PROFILE_DIR: dir, AGENTCABIN_WORK_MCP_CONFIG: join(dir, 'mcp.json'),
    AGENTCABIN_WORK_PACKAGE_MCP_CONFIG: join(dir, 'connector-package-mcp.json'),
    AGENTCABIN_WORK_BRIDGE_PORT: '43210', AGENTCABIN_WORK_BRIDGE_TOKEN: 'run-token',
    AGENTCABIN_PI_CODING_AGENT_ENTRY: api,
  };
  const old = Object.fromEntries(Object.keys(env).map(k => [k, process.env[k]]));
  try {
    Object.assign(process.env, env);
    const registered = [];
    await workNativeMcp({ registerTool: tool => registered.push(tool.name) });
    assert.deepEqual(registered, ['tool_search']);
    const options = globalThis.__nativeMcpOptions;
    assert.equal(JSON.stringify(options.loadConfig()).includes('SECRET'), false);
    assert.throws(() => options.updateConfig(), /Host-managed MCP config cannot be modified/);
    assert.equal(fs.readFileSync(join(dir, 'mcp.json'), 'utf8'), raw);
    assert.equal(fs.existsSync(join(dir, 'mcp-auth.json')), false);
  } finally {
    for (const [key, value] of Object.entries(old)) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
    delete globalThis.__nativeMcpOptions;
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
