/**
 * Condition-based JSONL log reader and `.ready` sentinel waiter (Rule 3 of docs/testing.md).
 */

import fs from 'node:fs';
import path from 'node:path';

const CONDITION_POLL_INTERVAL_MS = 25;

/**
 * Builds the `<logPath>.ready` sentinel path paired with `logPath`.
 */
export function buildReadySentinelPath(logPath) {
  return `${logPath}.ready`;
}

/**
 * Reads and parses all non-empty JSON Lines records from `logPath`.
 */
export function readJsonLines(logPath) {
  const rawContents = fs.readFileSync(logPath, 'utf8');
  const records = [];
  for (const rawLine of rawContents.split('\n')) {
    const trimmedLine = rawLine.trim();
    if (trimmedLine.length > 0) {
      records.push(JSON.parse(trimmedLine));
    }
  }
  return records;
}

function tryReadCompletedReadyRecords(logPath, readySentinelPath) {
  if (!fs.existsSync(readySentinelPath) || !fs.existsSync(logPath)) {
    return null;
  }
  try {
    const parsedRecords = readJsonLines(logPath);
    const hasReadyKind = parsedRecords.some((record) => record && record.kind === 'ready');
    return hasReadyKind ? parsedRecords : null;
  } catch {
    return null;
  }
}

function attachDirectoryWatcher(directoryPath, onFilesystemChange) {
  if (!fs.existsSync(directoryPath)) {
    return null;
  }
  try {
    return fs.watch(directoryPath, () => onFilesystemChange());
  } catch {
    return null;
  }
}

function buildEarlyExitError(readySentinelPath, exitError, exitOutcome) {
  if (exitError) {
    return exitError;
  }
  const exitSummary = `exitCode=${exitOutcome?.exitCode ?? 'unknown'}, signal=${exitOutcome?.signal ?? 'none'}`;
  return new Error(
    `Process exited before writing ready sentinel (${readySentinelPath}): ${exitSummary}`,
  );
}

function registerWaitHandlers(logPath, readySentinelPath, timeoutMs, processExitPromise, settle) {
  const parentDirectory = path.dirname(logPath);
  const handles = { directoryWatcher: null, pollTimer: null, failureTimer: null };

  const evaluateCondition = () => {
    if (handles.directoryWatcher === null) {
      handles.directoryWatcher = attachDirectoryWatcher(parentDirectory, evaluateCondition);
    }
    const readyRecords = tryReadCompletedReadyRecords(logPath, readySentinelPath);
    if (readyRecords !== null) {
      settle(null, readyRecords, handles);
      return true;
    }
    return false;
  };

  handles.directoryWatcher = attachDirectoryWatcher(parentDirectory, evaluateCondition);
  handles.pollTimer = setInterval(evaluateCondition, CONDITION_POLL_INTERVAL_MS);
  handles.pollTimer.unref?.();
  handles.failureTimer = setTimeout(() => {
    if (!evaluateCondition()) {
      settle(new Error(`Timed out after ${timeoutMs}ms waiting for ${readySentinelPath}`), null, handles);
    }
  }, timeoutMs);

  if (processExitPromise && typeof processExitPromise.then === 'function') {
    processExitPromise.then(
      (exitOutcome) => {
        if (!evaluateCondition()) {
          settle(buildEarlyExitError(readySentinelPath, null, exitOutcome), null, handles);
        }
      },
      (exitError) => {
        if (!evaluateCondition()) {
          settle(buildEarlyExitError(readySentinelPath, exitError, null), null, handles);
        }
      },
    );
  }
}

/**
 * Waits until `<logPath>.ready` exists and `logPath` contains a `{"kind":"ready"}` record,
 * failing fast if `processExitPromise` terminates beforehand or `timeoutMs` expires.
 */
export function waitForProbeReady(logPath, timeoutMs = 10000, processExitPromise = null) {
  const readySentinelPath = buildReadySentinelPath(logPath);
  const immediateRecords = tryReadCompletedReadyRecords(logPath, readySentinelPath);
  if (immediateRecords !== null) {
    return Promise.resolve(immediateRecords);
  }

  return new Promise((resolve, reject) => {
    let isSettled = false;
    const settleOnce = (failureError, readyRecords, handles) => {
      if (isSettled) {
        return;
      }
      isSettled = true;
      clearTimeout(handles.failureTimer);
      clearInterval(handles.pollTimer);
      handles.directoryWatcher?.close();
      if (failureError) {
        reject(failureError);
      } else {
        resolve(readyRecords);
      }
    };
    registerWaitHandlers(logPath, readySentinelPath, timeoutMs, processExitPromise, settleOnce);
  });
}
