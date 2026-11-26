// Frequency tags.
//
// Test files group tests by area. A tag says *when* a test runs, not what it
// tests, and every test has exactly one tag:
//
//   dev        run constantly while developing; broad but fast, 5-10s total
//   checkin    run locally once before committing; at most 5 minutes
//   full       CI only, no time limit; includes checkin, i.e. everything
//
// Higher tiers include lower ones. Initially checkin holds every test and
// full has no extra tests; new slow/deep scenarios start at full as
// the suite grows. TAG_LEVELS is the single list of available tags: adding one
// means adding it here and adding an explicit npm script in package.json.
// Unknown tags fail at load, so a typo cannot silently select nothing.
import { test as nodeTest } from 'node:test';

export const TAGS = {
  DEV: 'dev',
  CHECKIN: 'checkin',
  FULL: 'full',
};

export const TAG_LEVELS = [TAGS.DEV, TAGS.CHECKIN, TAGS.FULL];

/** Every tag a run of `level` includes, fastest first. */
export function tagsUpTo(level) {
  const index = TAG_LEVELS.indexOf(level);
  if (index === -1) {
    throw new Error(`unknown test tag "${level}"; expected one of: ${TAG_LEVELS.join(', ')}`);
  }
  return TAG_LEVELS.slice(0, index + 1);
}

/**
 * Filled while the runner imports the test files to decide which files have at
 * least one selected test. Not used during a normal test run.
 */
export const collectionRegistry = [];

export function e2eTest(name, tag, fn, { timeout = 15_000 } = {}) {
  if (!TAG_LEVELS.includes(tag)) {
    throw new Error(
      `${name}: unknown test tag "${tag}"; expected one of: ${TAG_LEVELS.join(', ')}`,
    );
  }

  if (process.env.E2E_COLLECT === '1') {
    collectionRegistry.push({ name, tag });
    return;
  }

  const active = process.env.E2E_TAG || null;
  if (active !== null && !tagsUpTo(active).includes(tag)) {
    return; // Deselected: register nothing, print nothing.
  }

  nodeTest(name, { timeout }, fn);
}
