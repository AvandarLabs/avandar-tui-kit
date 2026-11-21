/**
 * Rule 5 PTY coordinator (`docs/testing.md`): refuses to spawn a child process until
 * the browser terminal is already listening, and attaches output/input handlers synchronously.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import pty from 'node-pty';

import { startBrowserBridge } from './bridge_server.mjs';

const MODULE_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const NODE_PTY_ROOT = path.resolve(MODULE_DIRECTORY, '..', 'node_modules', 'node-pty');

const RULE_5_ERROR_MESSAGE =
  'Rule 5 violation: cannot spawn child process before browser terminal is listening';

function ensureMacOsSpawnHelperExecutable() {
  if (process.platform !== 'darwin') {
    return;
  }
  const candidatePaths = [
    path.join(NODE_PTY_ROOT, 'prebuilds', `darwin-${process.arch}`, 'spawn-helper'),
    path.join(NODE_PTY_ROOT, 'build', 'Release', 'spawn-helper'),
  ];
  for (const helperPath of candidatePaths) {
    if (fs.existsSync(helperPath)) {
      const currentMode = fs.statSync(helperPath).mode;
      if ((currentMode & 0o111) === 0) {
        fs.chmodSync(helperPath, 0o755);
      }
    }
  }
}

function buildSanitizedPtyEnv(rawEnv) {
  const sourceEnv = rawEnv ?? process.env;
  const sanitizedEnv = {};
  for (const [envKey, envValue] of Object.entries(sourceEnv)) {
    if (envValue !== undefined && envValue !== null) {
      sanitizedEnv[envKey] = String(envValue);
    }
  }
  if (!sanitizedEnv.TERM) {
    sanitizedEnv.TERM = 'xterm-256color';
  }
  return sanitizedEnv;
}

function assertBridgeReady(bridge) {
  if (!bridge || bridge.isTerminalReady !== true) {
    throw new Error(RULE_5_ERROR_MESSAGE);
  }
}

function raceExitWithTimeout(exitPromise, timeoutMs) {
  return new Promise((resolve, reject) => {
    const failureTimer = setTimeout(() => {
      reject(new Error(`Timed out after ${timeoutMs}ms waiting for PTY child exit`));
    }, timeoutMs);
    exitPromise.then(
      (exitOutcome) => {
        clearTimeout(failureTimer);
        resolve(exitOutcome);
      },
      (exitError) => {
        clearTimeout(failureTimer);
        reject(exitError);
      },
    );
  });
}

function wirePtyStreams(ptyProcess, bridge) {
  let hasExited = false;
  let resolveExitPromise = null;
  const exitPromise = new Promise((resolve) => {
    resolveExitPromise = resolve;
  });
  const unsubscribeInput = bridge.onTerminalInput((inputData) => {
    if (!hasExited) {
      ptyProcess.write(inputData);
    }
  });
  ptyProcess.onData((outputChunk) => {
    if (bridge.isOpen) {
      bridge.writeToTerminal(outputChunk);
    }
  });
  ptyProcess.onExit(({ exitCode, signal }) => {
    hasExited = true;
    unsubscribeInput();
    resolveExitPromise({ exitCode, signal: signal ?? 0 });
  });
  return {
    exitPromise,
    isRunning: () => !hasExited,
    unsubscribeInput,
  };
}

function killPtyQuietly(ptyProcess) {
  try {
    ptyProcess.kill();
  } catch {
    // Process may have exited concurrently.
  }
}

/**
 * Spawns `command` inside `node-pty` attached to an already-listening `bridge`.
 * Throws synchronously if `bridge.isTerminalReady !== true` (Rule 5).
 */
export function startPtySession(options) {
  const {
    command,
    args = [],
    cwd = process.cwd(),
    env = process.env,
    cols = 100,
    rows = 30,
    bridge,
  } = options ?? {};
  assertBridgeReady(bridge);
  ensureMacOsSpawnHelperExecutable();

  const ptyProcess = pty.spawn(command, args, {
    name: 'xterm-256color',
    cols,
    rows,
    cwd,
    env: buildSanitizedPtyEnv(env),
  });
  const wired = wirePtyStreams(ptyProcess, bridge);

  return {
    pid: ptyProcess.pid,
    exitPromise: wired.exitPromise,
    async waitForExit(timeoutMs = 10000) {
      const exitOutcome = await raceExitWithTimeout(wired.exitPromise, timeoutMs);
      if (bridge.isOpen) {
        await bridge.flushTerminalWrites(timeoutMs);
      }
      return exitOutcome;
    },
    dispose() {
      wired.unsubscribeInput();
      if (wired.isRunning()) {
        killPtyQuietly(ptyProcess);
      }
    },
  };
}

/**
 * Coordinates browser bridge readiness and PTY child startup (Rule 5).
 * When `bridge` is supplied, asserts `bridge.isTerminalReady === true`; when omitted,
 * launches a managed headless browser bridge and waits for `terminal_ready` before spawning.
 */
export async function startBridgeAndPtySession({
  command,
  args = [],
  cwd = process.cwd(),
  env = process.env,
  cols = 100,
  rows = 30,
  headless = true,
  timeoutMs = 15000,
  bridge,
}) {
  const isManagedBridge = bridge === undefined;
  const activeBridge = isManagedBridge
    ? await startBrowserBridge({ cols, rows, headless, timeoutMs })
    : bridge;

  try {
    const ptySession = startPtySession({ command, args, cwd, env, cols, rows, bridge: activeBridge });
    return {
      bridge: activeBridge,
      ptySession,
      exitPromise: ptySession.exitPromise,
      waitForExit: (exitTimeoutMs = timeoutMs) => ptySession.waitForExit(exitTimeoutMs),
      async dispose() {
        ptySession.dispose();
        if (isManagedBridge) {
          await activeBridge.close();
        }
      },
    };
  } catch (startupError) {
    if (isManagedBridge) {
      await activeBridge.close();
    }
    throw startupError;
  }
}
