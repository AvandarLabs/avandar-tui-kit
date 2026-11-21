/**
 * Condition-based JSONL log reader, `.ready` sentinel waiter, and live record condition waiter
 * (Rule 3 of docs/testing.md).
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

/**
 * Reads only newline-terminated JSON Lines records from `logPath`, ignoring any trailing
 * partial line that is still mid-write.
 */
export function readCompleteJsonLines(logPath) {
  if (!fs.existsSync(logPath)) {
    return [];
  }
  const rawContents = fs.readFileSync(logPath, 'utf8');
  const rawLines = rawContents.split('\n');
  const completedLines = rawContents.endsWith('\n') ? rawLines : rawLines.slice(0, -1);
  const records = [];
  for (const completedLine of completedLines) {
    const trimmedLine = completedLine.trim();
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
    const watcher = fs.watch(directoryPath, () => onFilesystemChange());
    watcher.on('error', () => watcher.close());
    return watcher;
  } catch {
    return null;
  }
}

function createEarlyExitError(earlyExitLabel, exitError, exitOutcome) {
  if (exitError) {
    return exitError;
  }
  const exitSummary = `exitCode=${exitOutcome?.exitCode ?? 'unknown'}, signal=${exitOutcome?.signal ?? 'none'}`;
  return new Error(`Process exited before ${earlyExitLabel}: ${exitSummary}`);
}

function registerConditionHandlers(options, settle) {
  const { directoryPath, evaluateConditionValue, timeoutMs, processExitPromise, earlyExitLabel } =
    options;
  const handles = { directoryWatcher: null, pollTimer: null, failureTimer: null };

  const checkCondition = () => {
    if (handles.directoryWatcher === null) {
      handles.directoryWatcher = attachDirectoryWatcher(directoryPath, checkCondition);
    }
    const matchedValue = evaluateConditionValue();
    if (matchedValue !== null && matchedValue !== undefined) {
      settle(null, matchedValue, handles);
      return true;
    }
    return false;
  };

  handles.directoryWatcher = attachDirectoryWatcher(directoryPath, checkCondition);
  handles.pollTimer = setInterval(checkCondition, CONDITION_POLL_INTERVAL_MS);
  handles.pollTimer.unref?.();
  handles.failureTimer = setTimeout(() => {
    if (!checkCondition()) {
      settle(new Error(`Timed out after ${timeoutMs}ms waiting for ${earlyExitLabel}`), null, handles);
    }
  }, timeoutMs);

  if (processExitPromise && typeof processExitPromise.then === 'function') {
    processExitPromise.then(
      (exitOutcome) => {
        if (!checkCondition()) {
          settle(createEarlyExitError(earlyExitLabel, null, exitOutcome), null, handles);
        }
      },
      (exitError) => {
        if (!checkCondition()) {
          settle(createEarlyExitError(earlyExitLabel, exitError, null), null, handles);
        }
      },
    );
  }
}

function waitForDirectoryCondition(options) {
  const immediateValue = options.evaluateConditionValue();
  if (immediateValue !== null && immediateValue !== undefined) {
    return Promise.resolve(immediateValue);
  }
  return new Promise((resolve, reject) => {
    let isSettled = false;
    const settleOnce = (failureError, resolvedValue, handles) => {
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
        resolve(resolvedValue);
      }
    };
    registerConditionHandlers(options, settleOnce);
  });
}

/**
 * Waits until `<logPath>.ready` exists and `logPath` contains a `{"kind":"ready"}` record,
 * failing fast if `processExitPromise` terminates beforehand or `timeoutMs` expires.
 */
export function waitForProbeReady(logPath, timeoutMs = 10000, processExitPromise = null) {
  const readySentinelPath = buildReadySentinelPath(logPath);
  return waitForDirectoryCondition({
    directoryPath: path.dirname(logPath),
    evaluateConditionValue: () => tryReadCompletedReadyRecords(logPath, readySentinelPath),
    timeoutMs,
    processExitPromise,
    earlyExitLabel: `writing ready sentinel (${readySentinelPath})`,
  });
}

/**
 * Waits until `selectMatchingValue(records)` over newline-terminated JSONL records in `logPath`
 * returns a non-nullish value, failing fast on early process exit or `timeoutMs` expiry.
 */
export function waitForLogRecords(
  logPath,
  selectMatchingValue,
  { timeoutMs = 10000, processExitPromise = null, description = 'JSONL log condition' } = {},
) {
  const evaluateLogSelector = () => {
    try {
      return selectMatchingValue(readCompleteJsonLines(logPath));
    } catch {
      return null;
    }
  };
  return waitForDirectoryCondition({
    directoryPath: path.dirname(logPath),
    evaluateConditionValue: evaluateLogSelector,
    timeoutMs,
    processExitPromise,
    earlyExitLabel: `${description} (${logPath})`,
  });
}
