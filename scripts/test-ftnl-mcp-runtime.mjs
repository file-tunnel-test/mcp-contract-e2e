import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';

const MAX_MCP_FRAME_BYTES = 1024 * 1024;
const binary = process.argv[2];

if (!binary || !path.isAbsolute(binary) || !fs.existsSync(binary)) {
  throw new Error('usage: node scripts/test-ftnl-mcp-runtime.mjs /absolute/path/to/ftnl-mcp');
}

class McpProcess {
  constructor(environment = {}) {
    this.root = fs.mkdtempSync(path.join(os.tmpdir(), 'ftnl-mcp-e2e-'));
    this.nextId = 1;
    this.pending = new Map();
    this.stderr = '';
    this.child = childProcess.spawn(binary, [], {
      env: {
        ...process.env,
        FTNL_ROOT: this.root,
        ...environment,
        RUST_LOG: 'debug',
      },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    this.child.stderr.setEncoding('utf8');
    this.child.stderr.on('data', (chunk) => {
      this.stderr = `${this.stderr}${chunk}`.slice(-65_536);
    });
    this.child.on('exit', (code, signal) => {
      const error = new Error(`ftnl-mcp exited before the contract completed: code=${code} signal=${signal}`);
      for (const { reject } of this.pending.values()) reject(error);
      this.pending.clear();
    });
    readline.createInterface({ input: this.child.stdout }).on('line', (line) => {
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        throw new Error('stdout contained a non-JSON line');
      }
      if (message.id !== undefined && this.pending.has(message.id)) {
        const { resolve } = this.pending.get(message.id);
        this.pending.delete(message.id);
        resolve(message);
      }
    });
  }

  async writeLine(line) {
    if (!this.child.stdin.write(`${line}\n`)) {
      await new Promise((resolve) => this.child.stdin.once('drain', resolve));
    }
  }

  async request(method, params) {
    const id = this.nextId++;
    const response = new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`timed out waiting for ${method}`));
      }, 10_000);
      this.pending.set(id, {
        resolve: (message) => {
          clearTimeout(timeout);
          resolve(message);
        },
        reject: (error) => {
          clearTimeout(timeout);
          reject(error);
        },
      });
    });
    await this.writeLine(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
    return response;
  }

  async initialize() {
    const message = await this.request('initialize', {
      protocolVersion: '2025-06-18',
      capabilities: {},
      clientInfo: { name: 'file-tunnel-test', version: '1' },
    });
    assert.equal(message.error, undefined, JSON.stringify(message.error));
    await this.writeLine(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }));
  }

  async callTool(name, args) {
    return this.request('tools/call', { name, arguments: args });
  }

  close() {
    this.child.stdin.end();
    this.child.kill();
    fs.rmdirSync(this.root);
  }
}

function toolErrorText(message) {
  if (message.error) return JSON.stringify(message.error);
  assert.equal(message.result?.isError, true, `expected tool error: ${JSON.stringify(message)}`);
  return (message.result.content ?? []).map((item) => item.text ?? '').join('\n');
}

async function verifyBoundedTransportAndStrictTools() {
  const mcp = new McpProcess({ FTNL_ENABLE_BUILD_TOOLS: '' });
  try {
    await mcp.writeLine('x'.repeat(MAX_MCP_FRAME_BYTES + 1));
    await mcp.initialize();

    const listed = await mcp.request('tools/list', {});
    assert.equal(listed.error, undefined, JSON.stringify(listed.error));
    assert.equal(listed.result.tools.length, 28);
    const parameterized = listed.result.tools.filter(
      (tool) => Object.keys(tool.inputSchema?.properties ?? {}).length > 0,
    );
    assert.ok(parameterized.length >= 15);
    for (const tool of parameterized) {
      assert.equal(
        tool.inputSchema.additionalProperties,
        false,
        `${tool.name} accepts unadvertised arguments`,
      );
    }

    const privateMarker = 'private-invalid-log-marker';
    await mcp.writeLine(privateMarker);
    const afterInvalid = await mcp.request('tools/list', {});
    assert.equal(afterInvalid.result.tools.length, 28);
    await new Promise((resolve) => setImmediate(resolve));
    assert.ok(!mcp.stderr.includes(privateMarker), 'rejected frame contents reached stderr');

    const unknown = await mcp.callTool('repo_status', {
      repo: 'demo-repo',
      unadvertised: 'must-fail-closed',
    });
    assert.match(toolErrorText(unknown), /unknown field/i);

    const build = await mcp.callTool('cargo_check', { repo: 'demo-repo' });
    const buildText = toolErrorText(build);
    assert.match(buildText, /FTNL_ENABLE_BUILD_TOOLS=1/);
    assert.match(buildText, /build scripts and procedural macros/);
  } finally {
    mcp.close();
  }
}

async function verifyExplicitBuildOptInReachesRepositoryValidation() {
  const mcp = new McpProcess({ FTNL_ENABLE_BUILD_TOOLS: '1' });
  try {
    await mcp.initialize();
    const build = await mcp.callTool('cargo_check', { repo: 'demo-repo' });
    assert.match(toolErrorText(build), /no such local repo/i);
  } finally {
    mcp.close();
  }
}

await verifyBoundedTransportAndStrictTools();
await verifyExplicitBuildOptInReachesRepositoryValidation();
console.log('validated hardened ftnl-mcp runtime contract');
