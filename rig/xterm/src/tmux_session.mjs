/**
 * Isolated `tmux -L <socket>` session manager and CLI entrypoint running
 * probe binaries inside a private tmux server attached to browser xterm.js.
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { VALID_TMUX_CONFIGS } from './args.mjs';
import { isDirectCliExecution, runHarnessCliMain } from './harness.mjs';
import { isTmux33Version } from './interactive_scenario.mjs';
import { buildSanitizedPtyEnv, startPtySession } from './pty_session.mjs';

const MODULE_FILE_PATH = fileURLToPath(import.meta.url);
const MODULE_DIRECTORY = path.dirname(MODULE_FILE_PATH);
const REPOSITORY_ROOT = path.resolve(MODULE_DIRECTORY, '..', '..', '..');
const RULE_5_ERROR_MESSAGE =
  'Rule 5 violation: cannot spawn child process before browser terminal is listening';

const ACTIVE_TMUX_SERVERS = new Map();
let sessionSequenceCounter = 0;
let hasRegisteredExitCleanup = false;
let hasRegisteredSignalHandlers = false;

function handleTerminationSignal(signalCode) {
  cleanupAllActiveTmuxServers();
  process.exit(signalCode);
}

const onSigInt = () => handleTerminationSignal(130);
const onSigTerm = () => handleTerminationSignal(143);

function syncSignalCleanupListeners() {
  if (ACTIVE_TMUX_SERVERS.size > 0 && !hasRegisteredSignalHandlers) {
    process.on('SIGINT', onSigInt);
    process.on('SIGTERM', onSigTerm);
    hasRegisteredSignalHandlers = true;
  } else if (ACTIVE_TMUX_SERVERS.size === 0 && hasRegisteredSignalHandlers) {
    process.off('SIGINT', onSigInt);
    process.off('SIGTERM', onSigTerm);
    hasRegisteredSignalHandlers = false;
  }
}

function removeResidualSocketFiles(socketName, sanitizedEnv) {
  if (typeof process.getuid !== 'function') {
    return;
  }
  const uid = process.getuid();
  const candidateRoots = ['/tmp', sanitizedEnv?.TMUX_TMPDIR, sanitizedEnv?.TMPDIR, os.tmpdir()];
  const baseDirectories = new Set(
    candidateRoots
      .filter(Boolean)
      .map((rootDirectory) => rootDirectory.replace(/\/+$/, '') || '/'),
  );
  baseDirectories.forEach((baseDirectory) => {
    const candidateSocketPath = path.join(baseDirectory, `tmux-${uid}`, socketName);
    try {
      fs.rmSync(candidateSocketPath, { force: true });
    } catch {
      // Ignore cleanup race if tmux already unlinked its socket.
    }
  });
}

function killTmuxServer(socketName, sanitizedEnv) {
  ACTIVE_TMUX_SERVERS.delete(socketName);
  syncSignalCleanupListeners();
  spawnSync('tmux', ['-L', socketName, 'kill-server'], { stdio: 'ignore', env: sanitizedEnv });
  removeResidualSocketFiles(socketName, sanitizedEnv);
}

function cleanupAllActiveTmuxServers() {
  [...ACTIVE_TMUX_SERVERS.entries()].forEach(([socketName, sanitizedEnv]) => {
    killTmuxServer(socketName, sanitizedEnv);
  });
}

function registerActiveTmuxServer(socketName, sanitizedEnv) {
  if (!hasRegisteredExitCleanup) {
    process.once('exit', cleanupAllActiveTmuxServers);
    hasRegisteredExitCleanup = true;
  }
  ACTIVE_TMUX_SERVERS.set(socketName, sanitizedEnv);
  syncSignalCleanupListeners();
}

/**
 * Generates a unique `tmux -L` socket name short enough for Unix `sun_path`
 * limits so parallel tmux sessions never collide with user servers.
 */
export function buildUniqueTmuxSocketName() {
  sessionSequenceCounter += 1;
  return `tuikit_xterm_${process.pid}_${Date.now().toString(36)}_${sessionSequenceCounter}`;
}

/**
 * Validates `tmuxMode` (`'stock'` or `'full'`) and returns the absolute path to
 * its committed `.conf` file, throwing with a `recoveryHint` if invalid.
 */
export function resolveTmuxConfigPath(tmuxMode) {
  if (!VALID_TMUX_CONFIGS.includes(tmuxMode)) {
    const invalidModeError = new Error(
      `Invalid tmux configuration '${tmuxMode}'; expected 'stock' or 'full'.`,
    );
    invalidModeError.recoveryHint = "Pass 'stock' or 'full' as the tmux configuration name.";
    throw invalidModeError;
  }
  const configPath = path.join(REPOSITORY_ROOT, 'rig', 'tmux', `${tmuxMode}.conf`);
  if (!fs.existsSync(configPath)) {
    const missingConfigError = new Error(`tmux configuration file not found at '${configPath}'.`);
    missingConfigError.recoveryHint = `Verify 'rig/tmux/${tmuxMode}.conf' exists in '${REPOSITORY_ROOT}'.`;
    throw missingConfigError;
  }
  return configPath;
}

function buildPosixShellCommand(tokens) {
  return tokens.map((token) => `'${String(token).replaceAll("'", "'\\''")}'`).join(' ');
}

function writePaneWrapperScript(scratchDir) {
  const wrapperPath = path.join(scratchDir, 'run-pane.sh');
  const paneExitTemporaryPath = path.join(scratchDir, 'pane.exit.tmp');
  const paneExitPath = path.join(scratchDir, 'pane.exit');
  const scriptLines = [
    '#!/bin/sh',
    'tmux_socket=$1; pane_exit_tmp=$2; pane_exit_file=$3; shift 3',
    'tmux -L "$tmux_socket" wait-for -L __rig_client_ready',
    'tmux -L "$tmux_socket" wait-for -U __rig_client_ready',
    '"$@"; pane_status=$?',
    'printf "%s\\n" "$pane_status" > "$pane_exit_tmp"',
    'mv "$pane_exit_tmp" "$pane_exit_file"',
    'exit "$pane_status"',
    '',
  ];
  fs.writeFileSync(wrapperPath, scriptLines.join('\n'), { encoding: 'utf8', mode: 0o755 });
  fs.chmodSync(wrapperPath, 0o755);
  return { wrapperPath, paneExitTemporaryPath, paneExitPath };
}

function startDetachedKeeperSession({ socketName, configPath, cols, rows, sanitizedEnv }) {
  const keeperArgs = [
    '-L', socketName, '-f', configPath,
    'new-session', '-d', '-s', '__rig_keeper', '-x', String(cols), '-y', String(rows), 'cat',
  ];
  const keeperOutcome = spawnSync('tmux', keeperArgs, { env: sanitizedEnv, encoding: 'utf8' });
  if (keeperOutcome.error) {
    killTmuxServer(socketName, sanitizedEnv);
    const missingTmuxError = new Error(`Failed to invoke tmux: ${keeperOutcome.error.message}`);
    missingTmuxError.recoveryHint =
      "Install tmux (>= 3.3) and ensure 'tmux' is in PATH, then retry.";
    throw missingTmuxError;
  }
  if (keeperOutcome.status !== 0) {
    killTmuxServer(socketName, sanitizedEnv);
    const startupError = new Error(
      `Failed to start private tmux server '${socketName}': ${(keeperOutcome.stderr ?? '').trim()}`,
    );
    startupError.recoveryHint = `Check tmux version (tmux -V >= 3.3) and config '${configPath}'.`;
    throw startupError;
  }
}

function readRecordedPaneExitCode(paneExitPath, fallbackExitCode) {
  if (!fs.existsSync(paneExitPath)) {
    return fallbackExitCode;
  }
  const parsedExitCode = Number.parseInt(fs.readFileSync(paneExitPath, 'utf8').trim(), 10);
  return Number.isInteger(parsedExitCode) ? parsedExitCode : fallbackExitCode;
}

function readTmuxPasteBuffer(socketName, sanitizedEnv) {
  const showOutcome = spawnSync('tmux', ['-L', socketName, 'show-buffer'], {
    env: sanitizedEnv,
    encoding: 'utf8',
  });
  if (showOutcome.status === 0) {
    return showOutcome.stdout;
  }
  if ((showOutcome.stderr ?? '').toLowerCase().includes('no buffers')) {
    return null;
  }
  throw new Error(
    `tmux show-buffer failed on '${socketName}': ${(showOutcome.stderr ?? '').trim()}`,
  );
}

function queryTmuxServerInfo(socketName, sanitizedEnv) {
  const versionOutcome = spawnSync('tmux', ['-V'], { env: sanitizedEnv, encoding: 'utf8' });
  const versionText = (versionOutcome.stdout ?? '').trim() || 'tmux';
  const mouseOutcome = spawnSync('tmux', ['-L', socketName, 'show-options', '-g', 'mouse'], {
    env: sanitizedEnv,
    encoding: 'utf8',
  });
  return Object.freeze({
    version: versionText,
    isMouseEnabled: (mouseOutcome.stdout ?? '').trim() === 'mouse on',
  });
}

function createTmuxSessionHandle({
  ptySession, socketName, tmuxServerInfo, sanitizedEnv, paneExitPath, ownedScratchDir,
}) {
  let hasDisposed = false;
  return {
    pid: ptySession.pid,
    socketName,
    tmuxServerInfo,
    exitPromise: ptySession.exitPromise,
    async waitForExit(timeoutMs = 10000) {
      const ptyExitOutcome = await ptySession.waitForExit(timeoutMs);
      return {
        exitCode: readRecordedPaneExitCode(paneExitPath, ptyExitOutcome.exitCode),
        signal: ptyExitOutcome.signal,
      };
    },
    readTmuxBuffer() {
      return readTmuxPasteBuffer(socketName, sanitizedEnv);
    },
    dispose() {
      if (hasDisposed) {
        return;
      }
      hasDisposed = true;
      ptySession.dispose();
      killTmuxServer(socketName, sanitizedEnv);
      if (ownedScratchDir !== null) {
        fs.rmSync(ownedScratchDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
      }
    },
  };
}

function preparePaneLaunchPlan(options, socketName) {
  const ownedScratchDir = options.scratchDir
    ? null
    : fs.mkdtempSync(path.join(os.tmpdir(), 'tuikit-tmux-'));
  const activeScratchDir = options.scratchDir ?? ownedScratchDir;
  const { wrapperPath, paneExitTemporaryPath, paneExitPath } =
    writePaneWrapperScript(activeScratchDir);
  const paneCommand = buildPosixShellCommand([
    '/bin/sh', wrapperPath, socketName, paneExitTemporaryPath, paneExitPath,
    options.programPath, ...(options.programArgs ?? []),
  ]);
  return { ownedScratchDir, paneExitPath, paneCommand };
}

function createHandshakeReplyWaiter(bridge, expectedReplyPrefix, timeoutMs) {
  let accumulatedInput = '';
  let resolveReply = null;
  let rejectReply = null;
  const replyPromise = new Promise((resolve, reject) => {
    resolveReply = resolve;
    rejectReply = reject;
  });
  const timer = setTimeout(() => {
    cleanup();
    rejectReply(new Error(`Timed out after ${timeoutMs}ms waiting for tmux client handshake`));
  }, timeoutMs);
  const unsubscribe = bridge.onTerminalInput((chunk) => {
    accumulatedInput += chunk;
    if (accumulatedInput.includes(expectedReplyPrefix)) {
      cleanup();
      resolveReply();
    }
  });
  function cleanup() {
    clearTimeout(timer);
    unsubscribe();
  }
  return { replyPromise, cleanup };
}

async function completeTmuxClientHandshake({
  bridge, ptySession, handshakeWaiter, socketName, sanitizedEnv, timeoutMs,
}) {
  const unexpectedExit = ptySession.exitPromise.then(({ exitCode }) => {
    throw new Error(`tmux client exited (${exitCode}) before outer handshake completed`);
  });
  try {
    await Promise.race([handshakeWaiter.replyPromise, unexpectedExit]);
    await bridge.flushTerminalWrites(timeoutMs);
    spawnSync('tmux', ['-L', socketName, 'wait-for', '-U', '__rig_client_ready'], {
      env: sanitizedEnv,
      stdio: 'ignore',
    });
  } finally {
    handshakeWaiter.cleanup();
  }
}

async function attachGatedTmuxClient({
  options, socketName, paneCommand, tmuxServerInfo, sanitizedEnv, cols, rows, timeoutMs,
}) {
  spawnSync('tmux', ['-L', socketName, 'wait-for', '-L', '__rig_client_ready'], {
    env: sanitizedEnv,
    stdio: 'ignore',
  });
  const ptySession = startPtySession({
    command: 'tmux',
    args: ['-L', socketName, 'new-session', '-s', 'probe', paneCommand],
    cols,
    rows,
    cwd: options.cwd ?? process.cwd(),
    env: options.env ?? process.env,
    envOverrides: options.envOverrides ?? {},
    bridge: options.bridge,
    diagnostics: options.diagnostics,
    timeoutMs,
  });
  const expectedPrefix = isTmux33Version(tmuxServerInfo.version) ? '\x1b[>' : '\x1b]11;rgb:';
  const handshakeWaiter = createHandshakeReplyWaiter(options.bridge, expectedPrefix, timeoutMs);
  try {
    await completeTmuxClientHandshake({
      bridge: options.bridge, ptySession, handshakeWaiter, socketName, sanitizedEnv, timeoutMs,
    });
    return ptySession;
  } catch (handshakeError) {
    ptySession.dispose();
    throw handshakeError;
  }
}

/**
 * Starts `options.programPath` inside a private `tmux -L <socket>` server
 * gated on the outer `options.bridge` handshake, returning a session handle.
 */
export async function createTmuxSession(options) {
  if (!options?.bridge || options.bridge.isTerminalReady !== true) {
    throw new Error(RULE_5_ERROR_MESSAGE);
  }
  const cols = options.cols ?? 100;
  const rows = options.rows ?? 30;
  const timeoutMs = options.timeoutMs ?? 20000;
  const configPath = resolveTmuxConfigPath(options.tmuxMode);
  const sanitizedEnv = buildSanitizedPtyEnv(options.env ?? process.env, options.envOverrides ?? {});
  const socketName = buildUniqueTmuxSocketName();
  const { ownedScratchDir, paneExitPath, paneCommand } = preparePaneLaunchPlan(options, socketName);
  try {
    registerActiveTmuxServer(socketName, sanitizedEnv);
    startDetachedKeeperSession({ socketName, configPath, cols, rows, sanitizedEnv });
    const tmuxServerInfo = queryTmuxServerInfo(socketName, sanitizedEnv);
    const ptySession = await attachGatedTmuxClient({
      options, socketName, paneCommand, tmuxServerInfo, sanitizedEnv, cols, rows, timeoutMs,
    });
    return createTmuxSessionHandle({
      ptySession, socketName, tmuxServerInfo, sanitizedEnv, paneExitPath, ownedScratchDir,
    });
  } catch (startupError) {
    killTmuxServer(socketName, sanitizedEnv);
    if (ownedScratchDir !== null) {
      fs.rmSync(ownedScratchDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
    }
    throw startupError;
  }
}

if (isDirectCliExecution(MODULE_FILE_PATH)) {
  const exitStatus = await runHarnessCliMain({
    scriptName: 'run-tmux.sh',
    requireTmuxConfig: true,
    observeProcessLifecycle: true,
    createSession: createTmuxSession,
  });
  process.exitCode = exitStatus;
}
