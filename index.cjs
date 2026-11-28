'use strict';
const {platform, arch} = process;
const target = {
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
  'darwin-x64': 'x86_64-apple-darwin',
  'darwin-arm64': 'aarch64-apple-darwin',
  'win32-x64': 'x86_64-pc-windows-msvc',
}[`${platform}-${arch}`];
if (!target) throw new Error(`Unsupported inference-engine platform: ${platform}-${arch}`);
if (platform === 'linux' && !process.report.getReport().header.glibcVersionRuntime) {
  throw new Error('inference-engine alpha requires glibc on Linux; musl is unsupported');
}
const native = require(`./inference-engine.${target}.node`);
module.exports = {
  profiles: native.profiles,
  version: native.version,
  compileProfile(selector) { return JSON.parse(native.compileProfile(selector)); },
};
