import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { cargoVersion, checkVersions, runCommand } from './release-policy.mjs';

const writeManifests = (root, cargo, npm = cargo, treeSitter = cargo) => {
  writeFileSync(join(root, 'Cargo.toml'), `[package]\nname = "example"\nversion = "${cargo}"\n`);
  writeFileSync(join(root, 'package.json'), `{"name":"example","version":"${npm}"}`);
  writeFileSync(join(root, 'tree-sitter.json'), `{"metadata":{"version":"${treeSitter}"}}`);
};

test('the Cargo version is read from [package], not from another table', () => {
  const manifest = '[workspace]\nversion = "9.9.9"\n[package]\nversion = "0.1.0"\n[dependencies]\n';
  assert.equal(cargoVersion(manifest), '0.1.0');
  assert.equal(cargoVersion('[workspace]\nmembers = []\n'), undefined);
});

test('package.json and tree-sitter.json must carry the Cargo version', () => {
  const root = mkdtempSync(join(tmpdir(), 'mssql-versions-'));
  const url = pathToFileURL(root + '/');
  try {
    writeManifests(root, '0.1.2');
    assert.equal(checkVersions(url), '0.1.2');
    writeManifests(root, '0.1.2', '0.1.1');
    assert.throws(() => checkVersions(url), /package\.json/);
    writeManifests(root, '0.1.2', '0.1.2', '0.1.1');
    assert.throws(() => checkVersions(url), /tree-sitter\.json/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('an unknown command fails instead of passing without doing anything', () => {
  assert.throws(() => runCommand('resolve'), /unknown command "resolve"/);
  assert.throws(() => runCommand(undefined), /unknown command/);
});
