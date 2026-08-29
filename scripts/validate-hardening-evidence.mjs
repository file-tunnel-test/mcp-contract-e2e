import assert from 'node:assert/strict';
import fs from 'node:fs';

const readJson = (relativePath) =>
  JSON.parse(fs.readFileSync(new URL(`../${relativePath}`, import.meta.url), 'utf8'));

const evidence = readJson('fixtures/mcp-hardening-contract.json');
const pins = readJson('source-pins.json');
const plan = readJson('test-plan.json');
const pinned = pins.sources[evidence.source.fullName];
const planned = plan.sources.find((source) => source.fullName === evidence.source.fullName);

assert.equal(evidence.schemaVersion, 1);
assert.match(evidence.source.sha, /^[0-9a-f]{40}$/);
assert.deepEqual(evidence.source, {
  fullName: 'file-tunnel/ftnl-mcp-server.rs',
  branch: 'den-3384-mcp-server-hardening',
  sha: evidence.source.sha,
});
assert.equal(pinned.sha, evidence.source.sha);
assert.equal(pinned.branch, evidence.source.branch);
assert.equal(planned.sha, evidence.source.sha);
assert.equal(planned.branch, evidence.source.branch);

assert.deepEqual(evidence.transport, {
  kind: 'stdio',
  remoteListener: false,
  stdoutWireOnly: true,
  maxInboundFrameBytes: 1024 * 1024,
  rejectedFrameBehavior: 'recover-next-line',
  rejectedFramePayloadLogged: false,
});
assert.deepEqual(evidence.buildTools, {
  environmentVariable: 'FTNL_ENABLE_BUILD_TOOLS',
  enabledValue: '1',
  enabledByDefault: false,
  mayExecuteRepositoryCode: true,
});
assert.equal(evidence.toolContract.toolCount, 28);
assert.ok(evidence.toolContract.parameterizedToolSchemas >= 15);
assert.equal(evidence.toolContract.unknownArguments, 'reject');
assert.equal(evidence.resourceBounds.maxLocalRepos, 64);
assert.equal(evidence.resourceBounds.maxLocalEntriesInspected, 256);

assert.deepEqual(evidence.identityBoundary, {
  localTrustBoundary: 'spawning-process-and-os-account',
  bluetoothIsIdentity: false,
  remoteTransportRequiresSharedAuth: true,
  productAuthorizationSeparate: true,
});

const countedTests = [
  evidence.verifiedTests.unit,
  evidence.verifiedTests.boundedProcess,
  evidence.verifiedTests.httpHardening,
  evidence.verifiedTests.runtimeArchitecture,
  evidence.verifiedTests.stdioIntegration,
].reduce((sum, count) => sum + count, 0);
assert.equal(evidence.verifiedTests.total, countedTests);
assert.ok(evidence.verifiedTests.total >= 88);
assert.equal(evidence.verifiedTests.repositoryGate, 'nix develop --command agent-check');

const serialized = JSON.stringify(evidence);
assert.ok(!/ghp_[A-Za-z0-9]+|github_pat_[A-Za-z0-9_]+|lin_api_[A-Za-z0-9]+/.test(serialized));
console.log(`validated MCP hardening evidence for ${evidence.source.sha}`);
