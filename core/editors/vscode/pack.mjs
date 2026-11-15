// Pack a VSIX that needs no toolchain to try.
//
// The kernel binaries are copied into the package, because the whole point of installing this is
// to see a model without first building Rust. Only the platform the pack ran on is included: a
// VSIX carrying four platforms is a download nobody wants, and a claim that nothing tested.
//
// The staged copies go to `bin/<platform>-<arch>/`, which is what the extension looks for after
// its settings and before the build a clone produces.

import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const platform = `${process.platform}-${process.arch}`;
const exe = process.platform === 'win32' ? '.exe' : '';
const release = path.resolve(here, '..', '..', 'target', 'release');
const stage = path.join(here, 'bin', platform);
const library = path.resolve(here, '..', '..', 'stdlib');
if (!existsSync(path.join(library, 'MANIFEST.sha256'))) {
  throw new Error(`missing versioned standard library: ${library}`);
}
for (const line of readFileSync(path.join(library, 'MANIFEST.sha256'), 'utf8').trim().split(/\r?\n/)) {
  const entry = line.match(/^([a-f0-9]{64})  (.+)$/);
  if (!entry) throw new Error('invalid standard-library checksum entry');
  const [, expected, relative] = entry;
  const file = path.resolve(library, relative);
  if (!file.startsWith(library + path.sep)) throw new Error(`invalid standard-library path: ${relative}`);
  const actual = createHash('sha256').update(readFileSync(file)).digest('hex');
  if (actual !== expected) throw new Error(`standard-library checksum mismatch: ${relative}`);
}
cpSync(library, path.join(stage, 'stdlib'), { recursive: true });

mkdirSync(stage, { recursive: true });
for (const name of ['sysml', 'sysml-lsp']) {
  const from = path.join(release, name + exe);
  if (!existsSync(from)) {
    console.error(
      `missing ${from}\n` +
        'build it first: cargo build --release --manifest-path core/Cargo.toml\n' +
        'on Windows: cmd //c core\\build.bat build --release',
    );
    process.exit(1);
  }
  copyFileSync(from, path.join(stage, name + exe));
}

mkdirSync(path.join(here, 'dist'), { recursive: true });
execSync(
  `npx --yes @vscode/vsce package --target ${platform} --out dist/sysml-v2-${platform}.vsix`,
  { cwd: here, stdio: 'inherit' },
);
