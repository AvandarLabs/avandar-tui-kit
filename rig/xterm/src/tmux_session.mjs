/**
 * Layer 4 private `tmux -L <socket>` session manager and CLI entrypoint for `rig/xterm/run-tmux.sh`.
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { VALID_TMUX_CONFIGS } from './args.mjs';
import { runHarnessCliMain } from './harness.mjs';
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
  const baseDirectories = new Set(['/tmp']);
  if (sanitizedEnv?.TMUX_TMPDIR) {
    baseDirectories.add(sanitizedEnv.TMUX_TMPDIR);
  }
  for (const baseDir of baseDirectories) {
    const candidateSocketPath = path.join(baseDir, `tmux-${uid}`, socketName);
    try {
      fs.rmSync(candidateSocketPath, { force: true });
    } catch {
      // Ignore cleanup race if tmux already unlinked its socket.
    }
  }
}

function killTmuxServer(socketName, sanitizedEnv) {
  ACTIVE_TMUX_SERVERS.delete(socketName);
  syncSignalCleanupListeners();
  spawnSync('tmux', ['-L', socketName, 'kill-server'], {
    stdio: 'ignore',
    env: sanitizedEnv,
  });
  removeResidualSocketFiles(socketName, sanitizedEnv);
}

function cleanupAllActiveTmuxServers() {
  for (const [socketName, sanitizedEnv] of [...ACTIVE_TMUX_SERVERS.entries()]) {
    killTmuxServer(socketName, sanitizedEnv);
  }
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
 * Builds a unique private `tmux -L` socket name short enough for Unix `sun_path` limits.
 */
export function buildUniqueTmuxSocketName() {
  sessionSequenceCounter += 1;
  const timestampBase36 = Date.now().toString(36);
  return `tuikit_xterm_${process.pid}_${timestampBase36}_${sessionSequenceCounter}`;
}

/**
 * Resolves `rig/tmux/<tmuxMode>.conf` without altering `stock.conf` or `full.conf`.
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

function buildPosixShellToken(rawToken) {
  return `'${String(rawToken).replaceAll("'", "'\\''")}'`;
}

function buildPosixShellCommand(tokens) {
  return tokens.map((token) => buildPosixShellToken(token)).join(' ');
}

function writePaneWrapperScript(scratchDir) {
  const wrapperPath = path.join(scratchDir, 'run-pane.sh');
  const paneExitTmpPath = path.join(scratchDir, 'pane.exit.tmp');
  const paneExitPath = path.join(scratchDir, 'pane.exit');
  const scriptLines = [
    '#!/bin/sh',
    'pane_exit_tmp=$1',
    'pane_exit_file=$2',
    'shift 2',
    '"$@"',
    'pane_status=$?',
    'printf "%s\\n" "$pane_status" > "$pane_exit_tmp"',
    'mv "$pane_exit_tmp" "$pane_exit_file"',
    'exit "$pane_status"',
    '',
  ];
  fs.writeFileSync(wrapperPath, scriptLines.join('\n'), { encoding: 'utf8', mode: 0o755 });
  fs.chmodSync(wrapperPath, 0o755);
  return { wrapperPath, paneExitTmpPath, paneExitPath };
}

function startDetachedKeeperSession({ socketName, configPath, cols, rows, sanitizedEnv }) {
  const keeperOutcome = spawnSync(
    'tmux',
    [
      '-L',
      socketName,
      '-f',
      configPath,
      'new-session',
      '-d',
      '-s',
      '__rig_keeper',
      '-x',
      String(cols),
      '-y',
      String(rows),
      'cat',
    ],
    { env: sanitizedEnv, encoding: 'utf8' },
  );
  if (keeperOutcome.error) {
    killTmuxServer(socketName, sanitizedEnv);
    const missingTmuxError = new Error(`Failed to invoke tmux: ${keeperOutcome.error.message}`);
    missingTmuxError.recoveryHint = "Install tmux (>= 3.3) and ensure 'tmux' is in PATH, then retry.";
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
  const rawContent = fs.readFileSync(paneExitPath, 'utf8').trim();
  const parsedExitCode = Number.parseInt(rawContent, 10);
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
  const stderrLower = (showOutcome.stderr ?? '').toLowerCase();
  if (stderrLower.includes('no buffers')) {
    return null;
  }
  throw new Error(
    `tmux show-buffer failed on '${socketName}': ${(showOutcome.stderr ?? '').trim()}`,
  );
}

function queryTmuxServerInfo(socketName, sanitizedEnv) {
  const versionOutcome = spawnSync('tmux', ['-V'], { env: sanitizedEnv, encoding: 'utf8' });
  const versionText = (versionOutcome.stdout ?? '').trim() || 'tmux';
  const mouseOptionOutcome = spawnSync('tmux', ['-L', socketName, 'show-options', '-g', 'mouse'], {
    env: sanitizedEnv,
    encoding: 'utf8',
  });
  const mouseLine = (mouseOptionOutcome.stdout ?? '').trim();
  return Object.freeze({
    version: versionText,
    isMouseEnabled: mouseLine === 'mouse on',
  });
}

function buildTmuxSessionHandle({
  ptySession,
  socketName,
  tmuxServerInfo,
  sanitizedEnv,
  paneExitPath,
  ownedScratchDir,
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

function preparePaneLaunchPlan(options) {
  const ownedScratchDir = options.scratchDir
    ? null
    : fs.mkdtempSync(path.join(os.tmpdir(), 'tuikit-tmux-'));
  const activeScratchDir = options.scratchDir ?? ownedScratchDir;
  const { wrapperPath, paneExitTmpPath, paneExitPath } = writePaneWrapperScript(activeScratchDir);
  const paneCommand = buildPosixShellCommand([
    wrapperPath,
    paneExitTmpPath,
    paneExitPath,
    options.programPath,
    ...(options.programArgs ?? []),
  ]);
  return { ownedScratchDir, paneExitPath, paneCommand };
}

/**
 * Starts `<programPath>` inside an isolated `tmux -L <socket>` server attached to `bridge` via `node-pty`.
 */
export async function createTmuxSession(options) {
  if (!options?.bridge || options.bridge.isTerminalReady !== true) {
    throw new Error(RULE_5_ERROR_MESSAGE);
  }
  const cols = options.cols ?? 100;
  const rows = options.rows ?? 30;
  const configPath = resolveTmuxConfigPath(options.tmuxMode);
  const sanitizedEnv = buildSanitizedPtyEnv(options.env ?? process.env, options.envOverrides ?? {});
  const { ownedScratchDir, paneExitPath, paneCommand } = preparePaneLaunchPlan(options);
  const socketName = buildUniqueTmuxSocketName();
  registerActiveTmuxServer(socketName, sanitizedEnv);
  startDetachedKeeperSession({ socketName, configPath, cols, rows, sanitizedEnv });

  try {
    const tmuxServerInfo = queryTmuxServerInfo(socketName, sanitizedEnv);
    const ptySession = startPtySession({
      command: 'tmux',
      args: ['-L', socketName, 'new-session', '-s', 'probe', paneCommand],
      cols,
      rows,
      cwd: options.cwd ?? process.cwd(),
      env: options.env ?? process.env,
      envOverrides: options.envOverrides ?? {},
      bridge: options.bridge,
      timeoutMs: options.timeoutMs ?? 20000,
    });
    return buildTmuxSessionHandle({
      ptySession,
      socketName,
      tmuxServerInfo,
      sanitizedEnv,
      paneExitPath,
      ownedScratchDir,
    });
  } catch (startupError) {
    killTmuxServer(socketName, sanitizedEnv);
    if (ownedScratchDir !== null) {
      fs.rmSync(ownedScratchDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
    }
    throw startupError;
  }
}

function resolveCanonicalFilePath(targetPath) {
  if (!targetPath) {
    return '';
  }
  const resolvedPath = path.resolve(targetPath);
  const resolveRealPath = fs.realpathSync.native ?? fs.realpathSync;
  try {
    return resolveRealPath(resolvedPath);
  } catch {
    return resolvedPath;
  }
}

if (resolveCanonicalFilePath(process.argv[1]) === resolveCanonicalFilePath(MODULE_FILE_PATH)) {
  const exitStatus = await runHarnessCliMain({
    scriptName: 'run-tmux.sh',
    requireTmuxConfig: true,
    createSession: createTmuxSession,
  });
  process.exit(exitStatus);
}
