import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// Exercise live expert switching against the bundled Pi, without model calls.
const entry = resolve(process.env.AGENTCABIN_RUNTIME_ROOT || 'runtime-build', 'pi/node_modules/@earendil-works/pi-coding-agent/dist/bundle/index.js');
const { createAgentSession, DefaultResourceLoader, SessionManager } = await import(pathToFileURL(entry).href);
const expertId = 'agent-plugin--expert.fixture--mcp--echo';
const connectorId = 'agent-plugin--connector.fixture--mcp--echo';
const savedEnv = { ...process.env };
const savedArgv = process.argv[1];
const calls = [];
const server = createServer(async (req, res) => {
  if (req.method !== 'POST') { res.writeHead(405); res.end(); return; }
  assert.equal(req.headers.authorization, 'Bearer fixture-token');
  if (req.url !== '/global') {
    assert.match(req.url, /^\/internal\/(code|work)\/mcp\//);
    assert([expertId, connectorId].includes(decodeURIComponent(req.url.split('/').at(-1))));
  }
  const label = req.url === '/global' ? 'global' : decodeURIComponent(req.url).includes('connector.fixture') ? 'connector' : 'expert';
  const chunks = []; for await (const chunk of req) chunks.push(chunk);
  const request = JSON.parse(Buffer.concat(chunks).toString());
  if (request.id === undefined) { res.writeHead(202); res.end(); return; }
  let result;
  if (request.method === 'initialize') {
    result = { protocolVersion: '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'expert-host', version: '1' } };
  } else if (request.method === 'tools/list') {
    result = { tools: [{ name: 'echo', description: `Echo ${label} fixture text`, inputSchema: { type: 'object', properties: { text: { type: 'string' } }, required: ['text'] } }] };
  } else if (request.method === 'tools/call') {
    calls.push(req.url);
    result = { content: [{ type: 'text', text: `Echo: ${request.params.arguments.text}` }] };
  } else { throw new Error(`Unexpected method ${request.method}`); }
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }));
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
try {
  for (const mode of ['code', 'work']) {
    const dir = mkdtempSync(join(tmpdir(), `agentcabin-expert-${mode}-`));
    let session;
    try {
      for (const key of Object.keys(process.env)) {
        if (key.startsWith('AGENTCABIN_WORK_')) delete process.env[key];
      }
      Object.assign(process.env, {
        AGENTCABIN_PI_CODING_AGENT_ENTRY: entry,
        AGENTCABIN_CODE_CONNECTOR_BRIDGE_PORT: String(server.address().port),
        AGENTCABIN_CODE_CONNECTOR_BRIDGE_TOKEN: 'fixture-token',
      });
      process.argv[1] = join(dirname(entry), 'cli.js');
      mkdirSync(join(dir, 'extensions'));
      writeFileSync(join(dir, 'expert-extension.mjs'), readFileSync(resolve('src-tauri/src/agent/pi_expert_extension.mjs')));
      writeFileSync(join(dir, 'code-agent-plugin-mcp.mjs'), readFileSync(resolve('src-tauri/src/work/pi_code_agent_plugin_mcp.mjs')));
      writeFileSync(join(dir, 'extensions', 'agentcabin-work-mcp-adapter.mjs'), readFileSync(resolve('src-tauri/src/work/pi_mcp_adapter.mjs')));
      writeFileSync(join(dir, 'mcp.json'), JSON.stringify({ mcpServers: mode === 'code' ? { global: {
        type: 'http', url: `http://127.0.0.1:${server.address().port}/global`, headers: { Authorization: 'Bearer fixture-token' }, exposure: 'deferred',
      } } : {} }));
      writeFileSync(join(dir, 'agent-plugin-mcp.json'), JSON.stringify({ mcpServers: mode === 'code' ? {
        [connectorId]: { command: 'NEVER_EXECUTE', env: { TOKEN: 'HOST_ONLY_SECRET' } },
      } : {} }));
      const update = selected => writeFileSync(join(dir, 'expert-context.json'), JSON.stringify({
        expert: selected ? { id: 'expert.fixture' } : null,
        systemPrompt: selected ? 'EXPERT_FIXTURE_ROLE' : null,
        skills: [], members: {}, mcpServers: selected ? { [expertId]: {} } : {},
      }));
      update(false);
      const paths = [join(dir, 'expert-extension.mjs')];
      if (mode === 'work') {
        Object.assign(process.env, {
          AGENTCABIN_WORK_PROFILE_DIR: dir, AGENTCABIN_WORK_MCP_CONFIG: join(dir, 'mcp.json'),
          AGENTCABIN_WORK_BRIDGE_PORT: String(server.address().port), AGENTCABIN_WORK_BRIDGE_TOKEN: 'fixture-token',
        });
        paths.unshift(join(dir, 'extensions', 'agentcabin-work-mcp-adapter.mjs'));
      }
      const loader = new DefaultResourceLoader({ cwd: dir, agentDir: dir, noExtensions: true, noSkills: true,
        additionalExtensionPaths: paths, extensionFactories: [] });
      await loader.reload();
      const result = await createAgentSession({ cwd: dir, agentDir: dir, resourceLoader: loader, sessionManager: SessionManager.inMemory(dir) });
      assert.deepEqual(result.extensionsResult.errors, []);
      session = result.session;
      const runner = session.extensionRunner;

      const execute = async (name, input) => {
        const blocked = await runner.emitToolCall({ type: 'tool_call', toolName: name, toolCallId: 'fixture-call', input });
        assert(!blocked?.block, JSON.stringify(blocked));
        return runner.getToolDefinition(name).execute('fixture-call', input, undefined, undefined, runner.createToolContext('fixture-call'));
      };
      await runner.emit({ type: 'session_start' });
      if (mode === 'code') assert(session.getActiveToolNames().includes('tool_search'), 'Code discovery must be active before the first prompt');
      assert.equal(runner.getRegisteredCommands().filter(command => command.name === 'mcp').length, 1);
      assert(!session.getActiveToolNames().some(name => name.startsWith('mcp__')));
      if (mode === 'code') {
        for (const label of ['global', 'connector']) {
          const found = await execute('tool_search', { query: `echo ${label} fixture`, limit: 1 });
          assert(!found.isError, JSON.stringify(found));
          const name = found.details.loaded[0];
          assert(name, `${label}: ${JSON.stringify(found)}`);
          const echo = await execute(name, { text: label });
          assert.equal(echo.content[0].text, `Echo: ${label}`);
        }
        console.log('✓ code: shared global MCP and Host connector MCP remain callable');
      }
      update(true);
      await runner.emitInput('Use the selected expert', undefined, 'interactive');
      const found = await execute('tool_search', { query: 'echo expert fixture', limit: 1 });
      assert(!found.isError, JSON.stringify(found));
      const tool = found.details.loaded[0];
      assert(tool?.startsWith('mcp__') && session.getActiveToolNames().includes(tool), `${mode}: selected expert echo was not activated`);
      const echoed = await execute(tool, { text: 'AgentCabin MCP works' });
      assert(!echoed.isError, JSON.stringify(echoed));
      assert.equal(echoed.content[0].text, 'Echo: AgentCabin MCP works');
      update(false);
      await runner.emitInput('Exit expert', undefined, 'interactive');
      assert(!session.getActiveToolNames().includes(tool), `${mode}: expert MCP remained active after exit`);
      console.log(`✓ ${mode}: one MCP consumer, expert selection, Host echo, and removal on exit`);
    } finally {
      if (session) { await session.extensionRunner.emit({ type: 'session_shutdown' }); session.dispose(); }
      rmSync(dir, { recursive: true, force: true });
    }
  }
  assert.equal(calls.length, 4);
} finally {
  process.argv[1] = savedArgv;
  for (const key of Object.keys(process.env)) if (!(key in savedEnv)) delete process.env[key];
  Object.assign(process.env, savedEnv);
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
}
