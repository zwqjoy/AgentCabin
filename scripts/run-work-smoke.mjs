#!/usr/bin/env node

/**
 * Real Agent Smoke Runner V1 CLI wrapper.
 * Spawns the AgentCabin Desktop process with AGENTCABIN_WORK_SMOKE_CONFIG set.
 */

import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..');

function printUsage() {
  console.log(`
Usage: node scripts/run-work-smoke.mjs [options]

Options:
  --scenario <name>     Scenario name (default: core-contract-approval)
  --runtime <name>      Runtime to use (default: pi, only pi is supported)
  --model <name>        Explicit model name (required, e.g. "Qwen3.8-27B")
  --effort <level>      Reasoning effort (required): off, low, medium, high
  --timeout <seconds>   Timeout in seconds (default: 600)
  --keep-workspace      Do not delete fixture and workspace on success
  --report-dir <path>   Output report directory (default: reports/work-smoke)
  --help, -h            Show this help message
`);
}

function parseArgs(args) {
  const options = {
    scenario: 'core-contract-approval',
    runtime: 'pi',
    model: '',
    effort: '',
    timeout: 600,
    keepWorkspace: false,
    reportDir: path.join(REPO_ROOT, 'reports', 'work-smoke'),
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--help' || arg === '-h') {
      printUsage();
      process.exit(0);
    } else if (arg === '--scenario') {
      options.scenario = args[++i];
    } else if (arg === '--runtime') {
      options.runtime = args[++i];
    } else if (arg === '--model') {
      options.model = args[++i];
    } else if (arg === '--effort') {
      options.effort = args[++i];
    } else if (arg === '--timeout') {
      options.timeout = parseInt(args[++i], 10);
    } else if (arg === '--keep-workspace') {
      options.keepWorkspace = true;
    } else if (arg === '--report-dir') {
      options.reportDir = path.resolve(REPO_ROOT, args[++i]);
    } else {
      console.error(`Unknown option: ${arg}`);
      printUsage();
      process.exit(1);
    }
  }

  return options;
}

function validateOptions(opts) {
  if (!opts.model || opts.model.trim() === '') {
    console.error('Error: --model <name> is required and cannot be empty.');
    process.exit(1);
  }

  if (opts.runtime !== 'pi') {
    console.error(`Error: Only runtime "pi" is supported in V1, got "${opts.runtime}".`);
    process.exit(1);
  }

  if (!opts.effort || opts.effort.trim() === '') {
    console.error('Error: --effort <level> is required. Must be one of: off, low, medium, high');
    process.exit(1);
  }

  const validEfforts = ['off', 'low', 'medium', 'high'];
  if (!validEfforts.includes(opts.effort)) {
    console.error(`Error: Invalid effort "${opts.effort}". Must be one of: ${validEfforts.join(', ')}`);
    process.exit(1);
  }

  if (isNaN(opts.timeout) || opts.timeout <= 0) {
    console.error(`Error: Invalid timeout "${opts.timeout}". Must be a positive integer.`);
    process.exit(1);
  }

  if (opts.scenario !== 'core-contract-approval') {
    console.error(`Error: Unknown scenario "${opts.scenario}". Available: core-contract-approval`);
    process.exit(1);
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  validateOptions(options);

  // Ensure reportDir exists
  fs.mkdirSync(options.reportDir, { recursive: true });

  const tempConfigFile = path.join(
    '/tmp',
    `agentcabin-smoke-${Date.now()}-${randomUUID().slice(0, 8)}.json`
  );

  const configContent = {
    scenario: options.scenario,
    runtime: options.runtime,
    model: options.model,
    effort: options.effort,
    timeout_secs: options.timeout,
    keep_workspace: options.keepWorkspace,
    report_dir: options.reportDir,
  };

  fs.writeFileSync(tempConfigFile, JSON.stringify(configContent, null, 2), 'utf-8');

  console.log('====================================================');
  console.log(' AgentCabin Real Agent Smoke Runner V1');
  console.log('====================================================');
  console.log(`Scenario:        ${options.scenario}`);
  console.log(`Runtime:         ${options.runtime}`);
  console.log(`Explicit Model:  ${options.model}`);
  console.log(`Effort:          ${options.effort}`);
  console.log(`Timeout:         ${options.timeout}s`);
  console.log(`Keep Workspace:  ${options.keepWorkspace}`);
  console.log(`Report Dir:      ${options.reportDir}`);
  console.log(`Temp Config:     ${tempConfigFile}`);
  console.log('====================================================\n');

  const cargoArgs = [
    'run',
    '--manifest-path',
    path.join(REPO_ROOT, 'src-tauri', 'Cargo.toml'),
    '--bin',
    'AgentCabin',
    '--features',
    'work-smoke',
  ];

  const env = {
    ...process.env,
    AGENTCABIN_WORK_SMOKE_CONFIG: tempConfigFile,
    RUST_BACKTRACE: '1',
  };

  const child = spawn('cargo', cargoArgs, {
    cwd: REPO_ROOT,
    env,
    stdio: 'inherit',
  });

  const cleanup = () => {
    try {
      if (fs.existsSync(tempConfigFile)) {
        fs.unlinkSync(tempConfigFile);
      }
    } catch {
      // ignore
    }
  };

  process.on('SIGINT', () => {
    cleanup();
    child.kill('SIGINT');
  });

  process.on('SIGTERM', () => {
    cleanup();
    child.kill('SIGTERM');
  });

  child.on('close', (code) => {
    cleanup();
    console.log(`\n[run-work-smoke] Desktop process exited with code ${code}`);

    // Scan report directory for the newest JSON report
    try {
      const files = fs.readdirSync(options.reportDir)
        .filter(f => f.endsWith('.json'))
        .map(f => ({
          name: f,
          path: path.join(options.reportDir, f),
          mtime: fs.statSync(path.join(options.reportDir, f)).mtimeMs,
        }))
        .sort((a, b) => b.mtime - a.mtime);

      if (files.length > 0) {
        const latest = files[0];
        console.log(`\nLatest Smoke Report: ${latest.path}`);
        const mdPath = latest.path.replace(/\.json$/, '.md');
        if (fs.existsSync(mdPath)) {
          console.log(`Markdown Report:     ${mdPath}`);
        }
      }
    } catch (e) {
      console.warn(`[run-work-smoke] Failed to inspect report dir: ${e.message}`);
    }

    process.exit(code ?? 1);
  });
}

main().catch((err) => {
  console.error('[run-work-smoke] Unhandled error:', err);
  process.exit(1);
});
