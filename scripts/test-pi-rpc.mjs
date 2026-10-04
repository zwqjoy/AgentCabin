#!/usr/bin/env node
// Exercise the bundled RPC subprocess against an isolated deterministic provider.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';

const runtime = resolve(process.env.AGENTCABIN_RUNTIME_ROOT || 'runtime-build');
const home = mkdtempSync(join(tmpdir(), 'agentcabin-rpc-regression-'));
const events = [];
let requests = 0;
let hang = false;
const server = createServer(async (req, res) => {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const body = JSON.parse(Buffer.concat(chunks).toString());
  requests++;
  if (hang) return;
  res.writeHead(200, { 'Content-Type': 'text/event-stream' });
  const base = { id: 'smoke', object: 'chat.completion.chunk', created: 1, model: 'smoke' };
  const emit = (delta, finish_reason = null) => res.write(`data: ${JSON.stringify({ ...base, choices: [{ index: 0, delta, finish_reason }] })}\n\n`);
  emit({ role: 'assistant' });
  const called = body.messages.some(m => m.role === 'tool');
  if (!called && body.tools?.some(t => t.function.name === 'smoke_progress')) {
    emit({ tool_calls: [{ index: 0, id: 'smoke-tool', type: 'function', function: { name: 'smoke_progress', arguments: '{}' } }] });
    emit({}, 'tool_calls');
  } else {
    emit({ content: 'Verified deterministic RPC response and compacted session summary.' });
    emit({}, 'stop');
  }
  res.end('data: [DONE]\n\n');
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
writeFileSync(join(home, 'models.json'), JSON.stringify({ providers: { 'rpc-smoke': {
  baseUrl: `http://127.0.0.1:${server.address().port}/v1`, api: 'openai-completions', apiKey: 'local-test',
  models: [{ id: 'smoke', reasoning: true, contextWindow: 32000, maxTokens: 4096 }],
} } }));
writeFileSync(join(home, 'settings.json'), JSON.stringify({ compaction: { enabled: false, keepRecentTokens: 0, reserveTokens: 1000 } }));
const extension = join(home, 'progress.mjs');
writeFileSync(extension, `export default pi => pi.registerTool({name:'smoke_progress',label:'Progress',description:'RPC regression progress',parameters:{type:'object',properties:{}},async execute(id,p,s,onUpdate){onUpdate?.({content:[{type:'text',text:'working'}]});return {content:[{type:'text',text:'complete'}]};}});`);
let child;
const pending = new Map();
let sequence = 0;
let stderr = '';
const waitFor = async (predicate, label) => {
  const deadline = Date.now() + 15000;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error(`Timed out: ${label}; ${stderr}`);
    await new Promise(r => setTimeout(r, 20));
  }
};
function start(sessionFile) {
  const bin = resolve(runtime, process.platform === 'win32' ? 'pi/bin/pi.cmd' : 'pi/bin/pi');
  child = spawn(bin, ['--mode','rpc','--provider','rpc-smoke','--model','smoke','--no-skills','--no-prompt-templates','-e',extension,...(sessionFile ? ['--session',sessionFile] : [])], { cwd: home, env: { ...process.env, PI_CODING_AGENT_DIR: home }, stdio: ['pipe','pipe','pipe'] });
  let buffer = '';
  child.stderr.on('data', c => { stderr += c; });
  child.stdout.on('data', c => {
    buffer += c;
    let index;
    while ((index = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0,index); buffer = buffer.slice(index+1);
      let value; try { value = JSON.parse(line); } catch { continue; }
      events.push(value);
      if (pending.has(value.id) && value.type === 'response') { pending.get(value.id)(value); pending.delete(value.id); }
    }
  });
}
async function rpc(type, fields = {}) {
  const id = `regression-${++sequence}`;
  const response = new Promise(r => pending.set(id, r));
  child.stdin.write(JSON.stringify({id,type,...fields})+'\n');
  const result = await Promise.race([response, new Promise((_,reject) => { const timer = setTimeout(() => reject(new Error(`RPC timeout ${type}: ${stderr}`)),15000); timer.unref(); })]);
  assert.equal(result.success,true,`${type}: ${JSON.stringify(result)}`);
  return result.data;
}
async function idle() {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    const state = await rpc("get_state");
    if (!state.isStreaming && !state.pendingMessageCount) return;
    await new Promise(r => setTimeout(r, 20));
  }
  throw new Error("Pi did not become idle");
}
async function stop() {
  const current = child;
  if (!current || current.exitCode !== null) return;
  const exited = new Promise(r => current.once('exit',r));
  current.kill('SIGTERM'); await exited;
}
try {
  start();
  await rpc('get_state');
  await rpc('set_model',{provider:'rpc-smoke',modelId:'smoke'});
  await rpc('set_thinking_level',{level:'low'});
  await rpc('set_auto_retry',{enabled:false});
  const mark = events.length;
  await rpc('prompt',{message:'Exercise the progress tool.'});
  await rpc('steer',{message:'Keep it concise.'});
  await rpc('follow_up',{message:'Confirm completion.'});
  await waitFor(() => events.slice(mark).some(e => e.type === 'agent_end'), 'prompt completion');
  await waitFor(() => events.slice(mark).some(e => e.type === 'tool_execution_end'), 'tool completion');
  for (const type of ['tool_execution_start','tool_execution_update','tool_execution_end']) assert.ok(events.slice(mark).some(e => e.type === type),type);
  await waitFor(() => requests >= 2, 'provider response');
  await rpc('clear_queue');
  await waitFor(() => !events.length || events.at(-1)?.type !== 'agent_start', 'idle');
  await idle();
  const messages = await rpc('get_fork_messages');
  const forkable = messages.messages?.[0];
  assert.ok(forkable?.entryId,JSON.stringify(messages));
  await rpc('fork',{entryId:forkable.entryId});
  const forkMark = events.length;
  await rpc('prompt',{message:'Produce content for compaction. '+ 'Regression evidence. '.repeat(200)});
  await waitFor(() => events.slice(forkMark).some(e => e.type === 'agent_end'), 'fork prompt');
  await idle();
  await rpc('prompt',{message:'Summarize the prior evidence.'});
  await idle();
  await rpc('compact',{customInstructions:'Preserve the RPC regression result.'});
  const state = await rpc('get_state');
  assert.ok(state.sessionFile, 'persisted session');
  await stop();
  start(state.sessionFile);
  const resumed = await rpc('get_state');
  assert.equal(resumed.sessionFile,state.sessionFile);
  assert.equal(resumed.model.id,'smoke');
  hang = true;
  const before = requests;
  const abortedPrompt = rpc('prompt',{message:'Wait until aborted.'});
  await waitFor(() => requests > before,'abort request reached provider');
  await rpc('abort');
  await abortedPrompt;
  console.log('PASS: Pi RPC boot, prompt, steer, follow_up, abort, get_state, set_model, set_thinking_level, tool start/update/end, resume, fork, compaction');
} finally {
  await stop(); server.closeAllConnections(); await new Promise(r => server.close(r)); rmSync(home,{recursive:true,force:true});
}
