import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function cargoVersion(manifest) {
  const section = manifest.split(/^\[package\]\s*$/m)[1]?.split(/^\[/m)[0];
  return section?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
}

// The generated parser embeds tree-sitter.json's version, so the three
// manifests must carry the same version.
export function versionsMatch(version, packageJson, treeSitterJson) {
  assert.equal(
    JSON.parse(packageJson).version,
    version,
    `package.json version must match Cargo.toml version ${version}`,
  );
  assert.equal(
    JSON.parse(treeSitterJson).metadata?.version,
    version,
    `tree-sitter.json metadata.version must match Cargo.toml version ${version}`,
  );
}

export function checkVersions(root = pathToFileURL(process.cwd() + '/')) {
  const read = (name) => readFileSync(new URL(name, root), 'utf8');
  const version = cargoVersion(read('Cargo.toml'));
  assert(version, 'Cargo.toml has no [package] version');
  versionsMatch(version, read('package.json'), read('tree-sitter.json'));
  return version;
}

// A workflow naming a command this script does not have must fail, not pass doing nothing.
export function runCommand(command) {
  if (command !== 'versions') {
    throw new Error(`release-policy: unknown command ${JSON.stringify(command)}`);
  }
  const version = checkVersions();
  console.log(`Cargo.toml, package.json, and tree-sitter.json all carry ${version}`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  runCommand(process.argv[2]);
}
