/**
 * PTY session coordinator refusing to spawn a child process until the browser
 * terminal bridge is already listening and wiring bidirectional I/O streams.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import pty from 'node-pty';

import { startBrowserBridge } from './bridge_server.mjs';
import { createLifecycleDiagnostics } from './log_archive.mjs';

const MODULE_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const NODE_PTY_ROOT = path.resolve(MODULE_DIRECTORY, '..', 'node_modules', 'node-pty');

const RULE_5_ERROR_MESSAGE =
  'Rule 5 violation: cannot spawn child process before browser terminal is listening';

const STRIPPED_OUTER_ENV_KEYS = new Set([
  'COLUMNS',
  'ITERM_PROFILE',
  'ITERM_SESSION_ID',
  'LC_TERMINAL',
  'LC_TERMINAL_VERSION',
  'LINES',
  'ORCA_VERSION',
  'STY',
  'TERM',
  'TERMCAP',
  'TERM_PROGRAM',
  'TERM_PROGRAM_VERSION',
  'TMUX',
  'TMUX_PANE',
  'VSCODE_INJECTION',
  'WINDOW',
  'WINDOWID',
  'WT_SESSION',
]);

function ensureMacOsSpawnHelperExecutable(platform) {
  if (platform !== 'darwin') {
    return;
  }
  const candidatePaths = [
    path.join(NODE_PTY_ROOT, 'prebuilds', `darwin-${process.arch}`, 'spawn-helper'),
    path.join(NODE_PTY_ROOT, 'build', 'Release', 'spawn-helper'),
  ];
  candidatePaths.forEach((helperPath) => {
    if (fs.existsSync(helperPath)) {
      const currentMode = fs.statSync(helperPath).mode;
      if ((currentMode & 0o111) === 0) {
        fs.chmodSync(helperPath, 0o755);
      }
    }
  });
}

/**
 * Strips inherited outer-terminal and multiplexer variables from `baseEnv`,
 * sets `TERM=xterm-256color`, and applies `envOverrides` for PTY child spawns.
 */
export function buildSanitizedPtyEnv(baseEnv = process.env, envOverrides = {}) {
  const sourceEnv = baseEnv ?? process.env;
  const sanitizedEnv = Object.fromEntries(
    Object.entries(sourceEnv)
      .filter(
        ([envKey, envValue]) =>
          envValue !== undefined &&
          envValue !== null &&
          !STRIPPED_OUTER_ENV_KEYS.has(envKey.toUpperCase()),
      )
      .map(([envKey, envValue]) => [envKey, String(envValue)]),
  );
  sanitizedEnv.TERM = 'xterm-256color';
  if (envOverrides && typeof envOverrides === 'object') {
    Object.entries(envOverrides).forEach(([overrideKey, overrideValue]) => {
      if (overrideValue === undefined || overrideValue === null) {
        delete sanitizedEnv[overrideKey];
      } else {
        sanitizedEnv[overrideKey] = String(overrideValue);
      }
    });
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

function wirePtyStreams(ptyProcess, bridge, diagnostics) {
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
    diagnostics.record('pty-exit', 'return', { ptyPid: ptyProcess.pid, exitCode, signal: signal ?? 0 });
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

function disposePtyHandles(ptyProcess, isRunning, platform) {
  if (isRunning || platform === 'win32') {
    try {
      ptyProcess.kill();
    } catch {
      // Process may have exited concurrently.
    }
  }
  if (platform === 'win32') {
    try {
      ptyProcess._agent?._inSocket?.destroy?.();
    } catch {
      // ConPTY input socket may already be closed.
    }
  }
}

function normalizeSessionOptions(options) {
  return {
    diagnostics: options?.diagnostics ?? createLifecycleDiagnostics({ env: {} }),
    command: options?.command,
    args: options?.args ?? [],
    cwd: options?.cwd ?? process.cwd(),
    env: options?.env ?? process.env,
    envOverrides: options?.envOverrides ?? {},
    cols: options?.cols ?? 100,
    rows: options?.rows ?? 30,
    headless: options?.headless ?? true,
    timeoutMs: options?.timeoutMs ?? 15000,
    bridge: options?.bridge,
    platform: options?.platform ?? process.platform,
    ptySpawner: options?.ptySpawner ?? pty.spawn,
  };
}

/**
 * Spawns `options.command` inside `node-pty` wired to an already-ready browser
 * `bridge`, returning `{ pid, exitPromise, waitForExit, dispose }`.
 */
export function startPtySession(options) {
  const config = normalizeSessionOptions(options);
  assertBridgeReady(config.bridge);
  ensureMacOsSpawnHelperExecutable(config.platform);

  const ptyProcess = config.ptySpawner(config.command, config.args, {
    name: 'xterm-256color',
    cols: config.cols,
    rows: config.rows,
    cwd: config.cwd,
    env: buildSanitizedPtyEnv(config.env, config.envOverrides),
  });
  const diagnostics = config.diagnostics.withLabels({ ptyPid: ptyProcess.pid });
  const wired = wirePtyStreams(ptyProcess, config.bridge, diagnostics);
  let hasDisposed = false;

  return {
    pid: ptyProcess.pid,
    exitPromise: wired.exitPromise,
    async waitForExit(timeoutMs = 10000) {
      const exitOutcome = await diagnostics.run('pty-exit-wait', () => raceExitWithTimeout(wired.exitPromise, timeoutMs));
      if (config.bridge.isOpen) {
        await diagnostics.run('write-flush', () => config.bridge.flushTerminalWrites(timeoutMs));
      }
      return exitOutcome;
    },
    dispose() {
      if (hasDisposed) {
        return;
      }
      hasDisposed = true;
      wired.unsubscribeInput();
      disposePtyHandles(ptyProcess, wired.isRunning(), config.platform);
    },
  };
}

/**
 * Ensures a browser `bridge` has reached `terminal_ready` (starting a managed
 * bridge when omitted) before spawning `startPtySession`, returning a combined
 * `{ bridge, ptySession, exitPromise, waitForExit, dispose }` handle.
 */
export async function startBridgeAndPtySession(options) {
  const config = normalizeSessionOptions(options);
  const isManagedBridge = config.bridge === undefined;
  const activeBridge = isManagedBridge
    ? await startBrowserBridge({
        cols: config.cols,
        rows: config.rows,
        headless: config.headless,
        timeoutMs: config.timeoutMs,
      })
    : config.bridge;

  try {
    const ptySession = startPtySession({ ...config, bridge: activeBridge });
    return {
      bridge: activeBridge,
      ptySession,
      exitPromise: ptySession.exitPromise,
      waitForExit: (exitTimeoutMs = config.timeoutMs) => ptySession.waitForExit(exitTimeoutMs),
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
