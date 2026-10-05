import assert from 'node:assert/strict';
import { buildWorkNativeMcpConfig } from '../src-tauri/src/work/pi_mcp_adapter.mjs';
import { createServer } from 'node:http';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

/** Exercises the real bundled Pi client, with a deterministic loopback bridge fixture. */
export async function smokeWorkNativeMcp(runtimeRoot = resolve('runtime-build')) {
  const entry = resolve(runtimeRoot, 'pi/node_modules/@earendil-works/pi-coding-agent/dist/index.js');
  const { createAgentSession, DefaultResourceLoader, SessionManager } = await import(pathToFileURL(entry).href);
  const packageFixture = process.env.AGENTCABIN_NATIVE_MCP_PACKAGE_FIXTURE === '1';
  const { createMcpToolName } = await import(pathToFileURL(join(dirname(entry), 'extensions/mcp/tools.js')).href);
  const logicalId = 'agent-plugin--github.fixture--mcp--github.enterprise';
  const expectedRoute = `/internal/work/mcp/${encodeURIComponent(packageFixture ? logicalId : 'fixture')}`;
  let nativeSearchTool = 'mcp__fixture__search_docs';
  const dir = mkdtempSync(join(tmpdir(), 'agentcabin-native-mcp-'));
  const saved = { ...process.env };
  const calls = [];
  const tools = [
    ['search_docs', 'Search documentation'], ['get_doc', 'Retrieve documentation'], ['create_item', 'Create an item'],
    ...Array.from({ length: 55 }, (_, i) => [`fixture_unrelated_${i}`, `Unrelated operation ${i}`]),
  ].map(([name, description]) => ({ name, description, inputSchema: { type: 'object', properties: {} } }));
  const server = createServer(async (req, res) => {
    if (req.method !== 'POST') { res.writeHead(405); res.end(); return; }
    if (req.headers.authorization !== 'Bearer run-scoped-fixture' || req.url !== expectedRoute) {
      res.writeHead(403); res.end(); return;
    }
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks).toString());
    if (body.id === undefined) { res.writeHead(202); res.end(); return; }
    let result;
    switch (body.method) {
      case 'initialize': result = { protocolVersion: '2024-11-05', capabilities: { tools: { listChanged: false } }, serverInfo: { name: 'Host-fixture', version: '1' } }; break;
      case 'tools/list': result = { tools }; break;
      case 'tools/call': calls.push(body.params); result = { content: [{ type: 'text', text: 'documentation fixture result' }] }; break;
      default: res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ jsonrpc: '2.0', id: body.id, error: { code: -32601, message: 'Method not found' } })); return;
    }
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ jsonrpc: '2.0', id: body.id, result }));
  });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  let session;
  try {
    const externalPort = process.env.AGENTCABIN_NATIVE_MCP_TEST_BRIDGE_PORT;
    Object.assign(process.env, {
      AGENTCABIN_PI_CODING_AGENT_ENTRY: entry, AGENTCABIN_WORK_PROFILE_DIR: dir,
      AGENTCABIN_WORK_BRIDGE_PORT: externalPort || String(server.address().port), AGENTCABIN_WORK_BRIDGE_TOKEN: process.env.AGENTCABIN_NATIVE_MCP_TEST_BRIDGE_TOKEN || 'run-scoped-fixture',
      AGENTCABIN_WORK_MCP_CONFIG: join(dir, 'mcp.json'), AGENTCABIN_WORK_PACKAGE_MCP_CONFIG: join(dir, 'connector-package-mcp.json'),
      AGENTCABIN_WORK_MCP_ENABLED: '1', AGENTCABIN_WORK_BROWSER_ENABLED: '0',
    });
    writeFileSync(join(dir, 'mcp.json'), JSON.stringify({ mcpServers: { fixture: {
      command: 'NEVER_EXECUTE', args: ['SECRET'], env: { API_KEY: 'SECRET' }, cwd: '/SECRET',
      url: 'https://SECRET.invalid', headers: { Authorization: 'SECRET' }, oauth: { token: 'SECRET' },
    } } }));
    if (packageFixture) {
      const packages = { mcpServers: { [logicalId]: { command: 'NEVER_EXECUTE', env: { TOKEN: 'SECRET' }, url: 'https://SECRET.invalid' } } };
      writeFileSync(join(dir, 'mcp.json'), JSON.stringify({ mcpServers: {} }));
      writeFileSync(join(dir, 'connector-package-mcp.json'), JSON.stringify(packages));
      const projected = buildWorkNativeMcpConfig({}, packages, { baseUrl: `http://127.0.0.1:${server.address().port}`, token: 'run-scoped-fixture' });
      assert(!JSON.stringify(projected).includes('SECRET'));
      nativeSearchTool = createMcpToolName(projected.servers[0].name, 'search_docs');
    }
    const start = async sessionManager => {
      const resourceLoader = new DefaultResourceLoader({ cwd: dir, agentDir: dir, noExtensions: true, noSkills: true, noPromptTemplates: true,
        additionalExtensionPaths: [resolve('src-tauri/src/work/pi_core_extension.mjs'), resolve('src-tauri/src/work/pi_mcp_adapter.mjs')] });
      await resourceLoader.reload();
      const result = await createAgentSession({ cwd: dir, agentDir: dir, resourceLoader, sessionManager });
      assert.deepEqual(result.extensionsResult.errors, []);
      session = result.session;
      await session.extensionRunner.emit({ type: 'session_start' });
      return { executeTool: async (name, input) => {
        const runner = session.extensionRunner;
        const blocked = await runner.emitToolCall({ type: 'tool_call', toolName: name, toolCallId: 'smoke-call', input });
        assert(!blocked?.block, JSON.stringify(blocked));
        return runner.getToolDefinition(name).execute('smoke-call', input, undefined, undefined, runner.createToolContext('smoke-call'));
      } };
    };
    let ctx = await start(SessionManager.create(dir, join(dir, 'sessions')));
    const initial = session.getActiveToolNames();
    assert(initial.includes('tool_search'));
    assert(!initial.includes('mcp'));
    assert(!initial.includes('codemode'));
    assert(!session.getAllTools().some(tool => tool.name === 'mcp'));
    assert(!initial.some(n => n.startsWith('mcp__')));
    const found = await ctx.executeTool('tool_search', { query: 'search documentation', limit: 2 });
    assert(!found.isError, JSON.stringify(found));
    assert(session.getActiveToolNames().includes(nativeSearchTool));
    assert(session.getActiveToolNames().filter(n => n.startsWith('mcp__')).length <= 2);
    assert(!session.getActiveToolNames().some(n => n.includes('mcp_resource')));
    await session.extensionRunner.emitBeforeAgentStart('documentation', undefined, {});
    assert(session.getActiveToolNames().includes(nativeSearchTool));
    const result = await ctx.executeTool(nativeSearchTool, {});
    assert(!result.isError, JSON.stringify(result));
    if (!externalPort) assert.equal(calls[0].name, 'search_docs');
    if (externalPort) {
      await ctx.executeTool('tool_search', { query: 'create item', limit: 1 });
      const mutation = await ctx.executeTool('mcp__fixture__create_item', {});
      assert(!mutation.isError, JSON.stringify(mutation));
    }
    // Pi resumes tool declarations from native system transcript messages.
    session.sessionManager.appendMessage({ role: 'system', content: session.systemPrompt, toolsAdded: session.agent.state.tools.map(({ name, description, parameters }) => ({ name, description, parameters })), timestamp: Date.now() });
    session.sessionManager.appendMessage({ role: 'user', content: 'Search documentation', timestamp: Date.now() });
    const file = session.sessionManager.getSessionFile();
    await session.extensionRunner.emit({ type: 'session_shutdown' }); session.dispose(); session = undefined;
    if (externalPort) process.env.AGENTCABIN_WORK_BRIDGE_TOKEN = process.env.AGENTCABIN_NATIVE_MCP_TEST_RESUME_TOKEN;
    ctx = await start(SessionManager.open(file, join(dir, 'sessions')));
    // Pi 1.0.2 SDK initializes the default loadout on open; re-discover through
    // native search rather than adding an AgentCabin persistence layer.
    assert(session.getActiveToolNames().includes('tool_search'));
    await ctx.executeTool('tool_search', { query: 'search documentation', limit: 2 });
    assert(session.getActiveToolNames().includes(nativeSearchTool));
    const resumed = await ctx.executeTool(nativeSearchTool, {});
    assert(!resumed.isError, JSON.stringify(resumed));
    if (!externalPort) assert.equal(calls.length, 2);
    console.log(`✓ Native Work MCP${packageFixture ? ' Claude package' : ''}: deferred 58 tools, search activation, core preservation, direct loopback call and resume`);
  } finally {
    if (session) { await session.extensionRunner.emit({ type: 'session_shutdown' }); session.dispose(); }
    for (const key of Object.keys(process.env)) if (!(key in saved)) delete process.env[key];
    Object.assign(process.env, saved);
    server.closeAllConnections(); await new Promise(r => server.close(r));
    rmSync(dir, { recursive: true, force: true });
  }
}
if (process.argv[1] === fileURLToPath(import.meta.url)) await smokeWorkNativeMcp(resolve(process.env.AGENTCABIN_RUNTIME_ROOT || 'runtime-build'));
