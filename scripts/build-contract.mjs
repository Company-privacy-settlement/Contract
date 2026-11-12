import { mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
function run(command, args) {
  const result = spawnSync(command, args, { stdio: 'inherit' });
  if (result.status !== 0) process.exit(result.status || 1);
}
mkdirSync('artifacts', { recursive: true });
const fixtures = process.argv.includes('--test-fixtures');
if (!fixtures) rmSync('artifacts/build-manifest.json', { force: true });
const contracts = {};
// Factory embeds the finalized pool bytes, so the order is part of the build.
for (const name of (fixtures ? ['test-ft'] : ['pool', 'factory', 'shared-vault', 'forest'])) {
  const stem = name.replaceAll('-', '_');
  run('cargo', ['build', '--locked', '--manifest-path', `contracts/${name}/Cargo.toml`, '--target', 'wasm32-unknown-unknown', '--release', '--target-dir', 'target']);
  const output = `artifacts/core_${stem}.wasm`;
  run('node_modules/.bin/wasm-opt', [
    `target/wasm32-unknown-unknown/release/core_${stem}.wasm`,
    '-o', output, '-Oz', '--enable-bulk-memory', '--enable-reference-types',
    '--enable-multivalue', '--enable-nontrapping-float-to-int',
    '--llvm-memory-copy-fill-lowering', '--llvm-nontrapping-fptoint-lowering', '--signext-lowering', '--strip-debug', '--strip-producers',
  ]);
  const size = statSync(output).size;
  if (size > 1_400_000) throw new Error(`Contract too large for normal deploy transaction: ${size}`);
  contracts[stem] = { wasm: `core_${stem}.wasm`, bytes: size, sha256: createHash('sha256').update(readFileSync(output)).digest('hex') };
  console.log(`NEAR-compatible optimized contract: ${output} (${size} bytes)`);
}
if (!fixtures) writeFileSync('artifacts/build-manifest.json', JSON.stringify({ protocol: 'core-build-v1', contracts, factoryEmbeddedVaultSha256: contracts.pool.sha256, forestEmbeddedVaultSha256: contracts.shared_vault.sha256 }, null, 2));
