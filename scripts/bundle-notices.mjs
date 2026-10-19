#!/usr/bin/env node
// Assemble attribution material; missing texts remain explicit review items.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const native = path.join(root, 'apps/desktop/src-tauri');
const destination = path.join(native, 'resources/notices');
const stage = `${destination}.stage-${process.pid}`;
const backup = `${destination}.previous-${process.pid}`;
const readme = fs.readFileSync(path.join(destination, 'README.txt'));
const target = process.env.CARGO_BUILD_TARGET || execFileSync('rustc', ['-vV'], {encoding: 'utf8'}).match(/^host: (.+)$/m)?.[1];
if (!target) throw new Error('Cannot determine the Cargo attribution target. Set CARGO_BUILD_TARGET.');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const inventory = {schema: 1, target, scope: 'Conservative non-dev Cargo dependency closure, including build dependencies; not a distribution clearance.', files: [], rust: [], unresolved: []};
const checkedBytes = (file, boundary) => {
  const real = fs.realpathSync(file), base = fs.realpathSync(boundary);
  if (real !== base && !real.startsWith(base + path.sep)) throw new Error(`Attribution source escapes its package: ${path.basename(file)}`);
  const stat = fs.statSync(real);
  if (!stat.isFile() || stat.size > 2 * 1024 * 1024) throw new Error(`Invalid attribution text: ${path.basename(file)}`);
  return fs.readFileSync(real);
};
const put = (relative, bytes) => {
  const file = path.join(stage, relative);
  fs.mkdirSync(path.dirname(file), {recursive: true}); fs.writeFileSync(file, bytes);
  inventory.files.push({path: relative.split(path.sep).join('/'), sha256: hash(bytes), bytes: bytes.length});
};
const copy = (file, relative, boundary) => put(relative, checkedBytes(file, boundary));
const licenseName = name => /^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)(?:[.\-_]|$)/i.test(name);
const licenseFiles = directory => {
  const result = [];
  for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
    if (entry.isFile() && licenseName(entry.name)) result.push(path.join(directory, entry.name));
    if (entry.isDirectory() && /^(licenses|licences)$/i.test(entry.name)) {
      for (const child of fs.readdirSync(path.join(directory, entry.name), {withFileTypes: true})) {
        if (child.isFile()) result.push(path.join(directory, entry.name, child.name));
      }
    }
  }
  return result;
};
try {
  fs.mkdirSync(stage); put('README.txt', readme);
  copy(path.join(root, 'LICENSE'), 'PWR-LICENSE.txt', root);
  const frontend = path.join(root, 'apps/desktop/dist/desktop/3rdpartylicenses.txt');
  copy(frontend, 'frontend/3rdpartylicenses.txt', path.dirname(frontend));
  const fonts = path.join(root, 'apps/desktop/public/fonts');
  for (const name of fs.readdirSync(fonts).sort()) if (/\.txt$/i.test(name)) copy(path.join(fonts, name), `fonts/${name}`, fonts);
  const packages = new Map();
  for (const [scope, manifest, entry] of [['core', path.join(root, 'Cargo.toml'), 'pwr-cli'], ['desktop', path.join(native, 'Cargo.toml'), 'pwr-desktop']]) {
    const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1', '--filter-platform', target, '--manifest-path', manifest], {encoding: 'utf8', maxBuffer: 32 * 1024 * 1024}));
    const byId = new Map(metadata.packages.map(p => [p.id, p]));
    const nodes = new Map(metadata.resolve.nodes.map(n => [n.id, n]));
    const pending = [metadata.packages.find(p => p.name === entry)?.id], seen = new Set();
    if (!pending[0]) throw new Error(`Missing Cargo package ${entry}`);
    while (pending.length) {
      const id = pending.pop(); if (seen.has(id)) continue; seen.add(id);
      const p = byId.get(id); if (!p || !nodes.has(id)) throw new Error('Incomplete Cargo dependency graph');
      const directory = path.dirname(p.manifest_path);
      if (p.source === null && directory !== root && !directory.startsWith(root + path.sep)) throw new Error(`Local dependency outside the product workspace: ${p.name}`);
      const key = `${p.name}@${p.version}:${p.source ?? `PWR-workspace:${path.relative(root, directory)}`}`;
      if (!packages.has(key)) packages.set(key, {metadata: p, scopes: new Set()});
      packages.get(key).scopes.add(scope);
      for (const dep of nodes.get(id).deps) if (dep.dep_kinds.some(kind => kind.kind !== 'dev')) pending.push(dep.pkg);
    }
  }
  for (const [key, value] of [...packages].sort(([a], [b]) => a.localeCompare(b))) {
    const p = value.metadata, directory = path.dirname(p.manifest_path);
    const prefix = `rust/${p.name}-${p.version}-${hash(key).slice(0, 12)}`;
    const ownLicense = p.source === null && p.license === 'Apache-2.0';
    const files = ownLicense ? [path.join(root, 'LICENSE')] : licenseFiles(directory);
    if (p.license_file) {
      const declared = path.resolve(directory, p.license_file);
      if (fs.existsSync(declared) && !files.includes(declared)) files.push(declared);
    }
    const texts = [];
    for (const file of files.sort()) {
      const relative = ownLicense ? 'PWR-LICENSE.txt' : `${prefix}/${path.relative(directory, file).split(path.sep).join('/')}`;
      if (!ownLicense) copy(file, relative, directory);
      texts.push(relative);
    }
    const record = {name: p.name, version: p.version, source: p.source, license: p.license, scopes: [...value.scopes].sort(), texts};
    inventory.rust.push(record);
    if (!p.license || texts.length === 0) inventory.unresolved.push({name: p.name, version: p.version, reason: !p.license ? 'No declared license expression' : 'No packaged license text found; source review required'});
  }
  const uv = path.join(native, 'resources/uv');
  if (process.platform === 'darwin') {
    if (!fs.existsSync(uv)) throw new Error('Run bundle-uv.sh before assembling macOS notices.');
    let licenses = process.env.PWR_UV_LICENSE_DIR;
    if (!licenses) {
      const executable = process.env.UV || execFileSync('which', ['uv'], {encoding: 'utf8'}).trim();
      licenses = path.resolve(path.dirname(fs.realpathSync(executable)), '..');
    }
    for (const name of ['LICENSE-MIT', 'LICENSE-APACHE']) copy(path.join(licenses, name), `uv/${name}`, licenses);
    inventory.uv = {version: execFileSync(uv, ['--version'], {encoding: 'utf8'}).trim(), sha256: hash(fs.readFileSync(uv)), scope: 'uv own license texts; transitive binary attribution still requires review'};
    inventory.unresolved.push({name: 'uv', reason: 'uv transitive binary attribution not established by its own license texts'});
  }
  inventory.files.sort((a,b) => a.path.localeCompare(b.path));
  fs.writeFileSync(path.join(stage, 'inventory.json'), JSON.stringify(inventory, null, 2) + '\n');
  // All inputs are checked before replacing the last complete generated set.
  fs.renameSync(destination, backup);
  try { fs.renameSync(stage, destination); } catch (error) { fs.renameSync(backup, destination); throw error; }
  fs.rmSync(backup, {recursive: true});
  console.log(`bundle-notices: ${inventory.rust.length} Cargo records, ${inventory.files.length} text files, ${inventory.unresolved.length} explicit review items (${target})`);
} finally { fs.rmSync(stage, {recursive: true, force: true}); }
