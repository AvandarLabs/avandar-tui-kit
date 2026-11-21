/**
 * Optional CI artifact archiving for `probe.jsonl` session logs and structured harness reports
 * when `RIG_XTERM_LOG_DIR` is set in the environment.
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

/**
 * Builds a deterministic session archive label from `runtime`, `managedArgs`, and optional `explicitLabel`.
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
 * Resets the in-process session archive sequence counter for deterministic unit tests.
 */
export function resetArchiveSessionSequence() {
  archivedSessionSequence = 0;
}

/**
 * Copies `logPath` into `RIG_XTERM_LOG_DIR` (if set and `logPath` exists) before scratch cleanup.
 */
export function archiveSessionProbeLog({
  logPath,
  label,
  tmuxMode = null,
  env = process.env,
} = {}) {
  const archiveDir = buildArchiveDirectoryPath(env);
  if (archiveDir === null || !logPath || !fs.existsSync(logPath)) {
    return null;
  }
  archivedSessionSequence += 1;
  const sequenceToken = String(archivedSessionSequence).padStart(2, '0');
  const filePrefix = buildArchivePrefix(tmuxMode);
  const safeLabel = buildArchiveToken(label, 'session');
  const destinationPath = path.join(archiveDir, `${filePrefix}-${sequenceToken}-${safeLabel}.jsonl`);
  fs.mkdirSync(archiveDir, { recursive: true });
  fs.copyFileSync(logPath, destinationPath);
  return destinationPath;
}

function writeFormattedJsonFile(archiveDir, fileName, payloadObject) {
  fs.mkdirSync(archiveDir, { recursive: true });
  const destinationPath = path.join(archiveDir, fileName);
  fs.writeFileSync(destinationPath, `${JSON.stringify(payloadObject, null, 2)}\n`, 'utf8');
  return destinationPath;
}

/**
 * Writes `<prefix>-<program>-<scenario>-report.json` to `RIG_XTERM_LOG_DIR` when a run succeeds.
 */
export function archiveHarnessReport({ report, parsedArgs, env = process.env } = {}) {
  const archiveDir = buildArchiveDirectoryPath(env);
  if (archiveDir === null || !report || !parsedArgs) {
    return null;
  }
  const filePrefix = buildArchivePrefix(parsedArgs.tmuxConfig);
  const programToken = buildArchiveToken(parsedArgs.program, 'program');
  const scenarioToken = buildArchiveToken(parsedArgs.scenario, 'scenario');
  const fileName = `${filePrefix}-${programToken}-${scenarioToken}-report.json`;
  return writeFormattedJsonFile(archiveDir, fileName, report);
}

/**
 * Writes `<prefix>-<program>-<scenario>-error.json` to `RIG_XTERM_LOG_DIR` when `runScenario` throws.
 */
export function archiveHarnessError({ error, parsedArgs, env = process.env } = {}) {
  const archiveDir = buildArchiveDirectoryPath(env);
  if (archiveDir === null || !error || !parsedArgs) {
    return null;
  }
  const filePrefix = buildArchivePrefix(parsedArgs.tmuxConfig);
  const programToken = buildArchiveToken(parsedArgs.program, 'program');
  const scenarioToken = buildArchiveToken(parsedArgs.scenario, 'scenario');
  const fileName = `${filePrefix}-${programToken}-${scenarioToken}-error.json`;
  const errorPayload = {
    error: error?.message ?? String(error),
    stack: typeof error?.stack === 'string' ? error.stack : '',
    timestamp: new Date().toISOString(),
  };
  return writeFormattedJsonFile(archiveDir, fileName, errorPayload);
}
