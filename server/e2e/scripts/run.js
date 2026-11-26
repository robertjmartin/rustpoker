// Test runner for the e2e suite.
//
//   node scripts/run.js dev         # fast loop, < 10s target
//   node scripts/run.js checkin     # dev + broader checks
//   node scripts/run.js full   # checkin + the full-depth tests
//
// Tags are frequency tiers, not areas. Higher tiers include lower ones, and
// package.json has one explicit script per tag (test:dev, test:checkin,
// test:full), so adding a tag is a deliberate two-place change.
//
// The runner validates the tag, builds the server binary, then imports the
// test files once in collection mode to find which files contain a selected
// test. Only those files are handed to `node --test`: a file with no selected
// tests would otherwise appear as an empty passing file. Each test file runs
// in its own process, so servers started inside them run side by side.
import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { TAG_LEVELS, collectionRegistry, tagsUpTo } from '../src/framework/test.js';

const E2E_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const SERVER_DIR = path.resolve(E2E_DIR, '..');
const TEST_DIR = path.join(E2E_DIR, 'src', 'tests');

const requested = process.argv.slice(2);
if (requested.length !== 1) {
  console.error('usage: node scripts/run.js <tag>');
  console.error(`available tags: ${TAG_LEVELS.join(', ')}`);
  process.exit(1);
}
const level = requested[0];

let selectedTags;
try {
  selectedTags = tagsUpTo(level);
} catch (error) {
  console.error(error.message);
  process.exit(1);
}

console.log('\n▶ building pokerserver (debug)');
const build = spawnSync('cargo', ['build', '-p', 'pokerserver'], {
  cwd: SERVER_DIR,
  stdio: 'inherit',
});
if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

const testFiles = readdirSync(TEST_DIR)
  .filter((file) => file.endsWith('.test.js'))
  .sort()
  .map((file) => path.join(TEST_DIR, file));

// Import each file with node:test disabled so e2eTest records names and tags.
// Importing is side-effect free, so the real run in the child process is
// unaffected.
process.env.E2E_COLLECT = '1';
const selectedFiles = [];
for (const file of testFiles) {
  collectionRegistry.length = 0;
  await import(pathToFileURL(file).href);
  const matches = collectionRegistry.some((test) => selectedTags.includes(test.tag));
  if (matches) selectedFiles.push(file);
}
delete process.env.E2E_COLLECT;

if (selectedFiles.length === 0) {
  console.log(`\nno tests tagged ${level}`);
  process.exit(0);
}

console.log(
  `\n▶ e2e run: ${level} (includes ${selectedTags.join(', ')}) ` +
    `— ${selectedFiles.length} of ${testFiles.length} files\n`,
);
const tests = spawnSync(process.execPath, ['--test', ...selectedFiles], {
  cwd: E2E_DIR,
  stdio: 'inherit',
  env: { ...process.env, E2E_TAG: level },
});
process.exit(tests.status ?? 1);
