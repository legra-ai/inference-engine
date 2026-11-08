const {test} = require('node:test');
const assert = require('node:assert/strict');
const engine = require(process.env.INFERENCE_ENGINE_PACKAGE || '..');
test('native rules and explicit exclusions', () => {
  assert.ok(engine.profiles().includes('rdfs-core'));
  const profile = engine.compileProfile('rdfs-core');
  assert.equal(profile.program.rules.length, 14);
  assert.ok(profile.exclusions.length);
  assert.match(engine.version(), /^0\.1\.0-alpha\.[1-9][0-9]*$/);
});
test('unsupported full profile fails', () => {
  assert.throws(() => engine.compileProfile('owl2-rl'));
});
