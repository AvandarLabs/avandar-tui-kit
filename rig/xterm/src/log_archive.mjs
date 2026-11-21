/**
 * Optional CI artifact archiving for `probe.jsonl` session logs and structured
 * harness JSON reports when `RIG_XTERM_LOG_DIR` is set in the environment.
 */

import fs from 'node:fs';
import path from 'node:path';

let archivedSessionSequence = 0;

function buildArchiveDirectoryPath(env = process.env) {
  const rawDirectory = (env?.RIG_XTERM_LOG_DIR ?? '').trim();
  return rawDirectory.length > 0 ? path.resolve(rawDirectory) : null;
}

function buildArchiveToken(rawToken, fallbackToken = 'session') {
  const sanitizedToken = String(rawToken ?? '')
    .replace(/[^A-Za-z0-9_.-]+/g, '_')
    .replace(/^_+|_+$/g, '');
  return sanitizedToken.length > 0 ? sanitizedToken : fallbackToken;
}

function buildArchivePrefix(tmuxMode) {
  if (!tmuxMode) {
    return 'xterm';
  }
  return `xterm-tmux-${buildArchiveToken(tmuxMode, 'tmux')}`;
}

function findMaxArchivedSequenceOnDisk(archiveDirectoryPath, filePrefix) {
  if (!fs.existsSync(archiveDirectoryPath)) {
    return 0;
  }
  const requiredPrefix = `${filePrefix}-`;
  return fs.readdirSync(archiveDirectoryPath).reduce((maxSequence, entryName) => {
    if (!entryName.startsWith(requiredPrefix) || !entryName.endsWith('.jsonl')) {
      return maxSequence;
    }
    const remainder = entryName.slice(requiredPrefix.length);
    const sequenceMatch = /^(\d+)-/.exec(remainder);
    if (sequenceMatch === null) {
      return maxSequence;
    }
    return Math.max(maxSequence, Number.parseInt(sequenceMatch[1], 10));
  }, 0);
}

/**
 * Builds a sanitized `<program>-<scenario>` label for `archiveSessionProbeLog`
 * from `runtime`, `managedArgs`, and optional `explicitLabel`.
 */
export function buildSessionArchiveLabel(runtime, managedArgs = [], explicitLabel = null) {
  const programName = buildArchiveToken(runtime?.program, 'program');
  if (explicitLabel) {
    return `${programName}-${buildArchiveToken(explicitLabel, 'session')}`;
  }
  const derivedScenario = managedArgs.includes('--push-kitty')
    ? 'interactive-kitty'
    : 'interactive';
  return `${programName}-${derivedScenario}`;
}

/**
 * Resets the in-process session archive sequence counter so unit tests start
 * numbering archived `.jsonl` files from `01`.
 */
export function resetArchiveSessionSequence() {
  archivedSessionSequence = 0;
}

/**
 * Copies `logPath` into `RIG_XTERM_LOG_DIR` with a collision-safe sequence
 * prefix before scratch cleanup, returning the destination path or `null`.
 */
export function archiveSessionProbeLog({
  logPath,
  label,
  tmuxMode = null,
  env = process.env,
} = {}) {
  const archiveDirectoryPath = buildArchiveDirectoryPath(env);
  if (archiveDirectoryPath === null || !logPath || !fs.existsSync(logPath)) {
    return null;
  }
  const filePrefix = buildArchivePrefix(tmuxMode);
  const safeLabel = buildArchiveToken(label, 'session');
  fs.mkdirSync(archiveDirectoryPath, { recursive: true });
  let candidateSequence = Math.max(
    archivedSessionSequence + 1,
    findMaxArchivedSequenceOnDisk(archiveDirectoryPath, filePrefix) + 1,
  );
  for (let attempt = 1; attempt <= 9999; attempt += 1) {
    const sequenceToken = String(candidateSequence).padStart(2, '0');
    const destinationPath = path.join(
      archiveDirectoryPath,
      `${filePrefix}-${sequenceToken}-${safeLabel}.jsonl`,
    );
    try {
      fs.copyFileSync(logPath, destinationPath, fs.constants.COPYFILE_EXCL);
      archivedSessionSequence = candidateSequence;
      return destinationPath;
    } catch (copyError) {
      if (copyError?.code !== 'EEXIST') {
        throw copyError;
      }
      candidateSequence += 1;
    }
  }
  throw new Error(`Exhausted unique session archive sequences for '${filePrefix}-${safeLabel}'.`);
}

function writeUniqueFormattedJsonFile(archiveDirectoryPath, baseStem, kindSuffix, payloadObject) {
  fs.mkdirSync(archiveDirectoryPath, { recursive: true });
  const serializedJson = `${JSON.stringify(payloadObject, null, 2)}\n`;
  for (let attempt = 1; attempt <= 9999; attempt += 1) {
    const candidateName =
      attempt === 1
        ? `${baseStem}-${kindSuffix}.json`
        : `${baseStem}-pid${process.pid}-${String(attempt).padStart(2, '0')}-${kindSuffix}.json`;
    const candidatePath = path.join(archiveDirectoryPath, candidateName);
    try {
      fs.writeFileSync(candidatePath, serializedJson, { encoding: 'utf8', flag: 'wx' });
      return candidatePath;
    } catch (writeError) {
      if (writeError?.code !== 'EEXIST') {
        throw writeError;
      }
    }
  }
  throw new Error(`Exhausted unique archive filenames for '${baseStem}-${kindSuffix}.json'.`);
}

function buildReportStem(parsedArgs) {
  const filePrefix = buildArchivePrefix(parsedArgs.tmuxConfig);
  const programToken = buildArchiveToken(parsedArgs.program, 'program');
  const scenarioToken = buildArchiveToken(parsedArgs.scenario, 'scenario');
  return `${filePrefix}-${programToken}-${scenarioToken}`;
}

/**
 * Writes `<prefix>-<program>-<scenario>-report.json` into `RIG_XTERM_LOG_DIR`
 * after a successful CLI run, returning the written file path or `null`.
 */
export function archiveHarnessReport({ report, parsedArgs, env = process.env } = {}) {
  const archiveDirectoryPath = buildArchiveDirectoryPath(env);
  if (archiveDirectoryPath === null || !report || !parsedArgs) {
    return null;
  }
  return writeUniqueFormattedJsonFile(
    archiveDirectoryPath,
    buildReportStem(parsedArgs),
    'report',
    report,
  );
}

/**
 * Writes `<prefix>-<program>-<scenario>-error.json` into `RIG_XTERM_LOG_DIR`
 * when scenario execution throws, returning the written file path or `null`.
 */
export function archiveHarnessError({ error, parsedArgs, env = process.env } = {}) {
  const archiveDirectoryPath = buildArchiveDirectoryPath(env);
  if (archiveDirectoryPath === null || !error || !parsedArgs) {
    return null;
  }
  const errorPayload = {
    error: error?.message ?? String(error),
    stack: typeof error?.stack === 'string' ? error.stack : '',
    timestamp: new Date().toISOString(),
  };
  return writeUniqueFormattedJsonFile(
    archiveDirectoryPath,
    buildReportStem(parsedArgs),
    'error',
    errorPayload,
  );
}
