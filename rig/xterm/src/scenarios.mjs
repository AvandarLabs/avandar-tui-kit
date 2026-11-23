/**
 * Scenario coordinator (`batch`, `osc52`, `interactive`, `panic`, `all`, and
 * `passthrough`) shared by direct xterm.js and private-socket tmux runs.
 */

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { MOUSE_SCENARIOS } from './args.mjs';
import { runMouseScenario } from './mouse_scenarios.mjs';
import { startBrowserBridge } from './bridge_server.mjs';
import {
  executeInteractiveScenario,
  shouldExpectOuterMouseEvents,
  waitForReadyOuterTerminalModes,
} from './interactive_scenario.mjs';
import { archiveSessionProbeLog, buildSessionArchiveLabel } from './log_archive.mjs';
import { startPtySession } from './pty_session.mjs';
import { readJsonLines, waitForProbeReady } from './ready_waiter.mjs';

const DEFAULT_TERMINAL_COLS = 100;
const DEFAULT_TERMINAL_ROWS = 30;
const BOOLEAN_PROBE_FLAGS = new Set(['--batch', '--push-kitty']);
const VALUE_PROBE_FLAGS = new Set(['--log', '--osc52', '--osc52-form']);

/**
 * Merges harness-managed `capability_probe` flags with forwarded `extraArgs`,
 * giving `managedArgs` precedence on duplicate probe flags while preserving
 * unknown flags for custom `<program>` runs.
 */
export function mergeProbeArgs(managedArgs = [], extraArgs = []) {
  const combinedTokens = [...managedArgs, ...extraArgs];
  const initialState = {
    mergedArgs: [],
    seenProbeFlags: new Set(),
    shouldSkipNext: false,
  };
  const reduced = combinedTokens.reduce((state, token, index) => {
    if (state.shouldSkipNext) {
      return { ...state, shouldSkipNext: false };
    }
    const equalsOffset = token.indexOf('=');
    const inlineFlag = equalsOffset > 0 ? token.slice(0, equalsOffset) : null;
    if (BOOLEAN_PROBE_FLAGS.has(token)) {
      if (!state.seenProbeFlags.has(token)) {
        state.seenProbeFlags.add(token);
        state.mergedArgs.push(token);
      }
      return state;
    }
    if (VALUE_PROBE_FLAGS.has(token)) {
      const valueToken = combinedTokens[index + 1];
      if (!state.seenProbeFlags.has(token)) {
        state.seenProbeFlags.add(token);
        state.mergedArgs.push(token);
        if (valueToken !== undefined) {
          state.mergedArgs.push(valueToken);
        }
      }
      return { ...state, shouldSkipNext: true };
    }
    if (inlineFlag !== null && VALUE_PROBE_FLAGS.has(inlineFlag)) {
      if (!state.seenProbeFlags.has(inlineFlag)) {
        state.seenProbeFlags.add(inlineFlag);
        state.mergedArgs.push(token);
      }
      return state;
    }
    state.mergedArgs.push(token);
    return state;
  }, initialState);
  return reduced.mergedArgs;
}

function createDefaultPtySession({ programPath, programArgs, cols, rows, bridge }) {
  return startPtySession({
    command: programPath,
    args: programArgs,
    cols,
    rows,
    bridge,
  });
}

function normalizeScenarioRuntime(runtime) {
  return {
    program: runtime.program ?? path.basename(runtime.programPath),
    programPath: runtime.programPath,
    extraArgs: runtime.extraArgs ?? [],
    timeoutMs: runtime.timeoutMs ?? 20000,
    createSession: runtime.createSession ?? createDefaultPtySession,
    tmuxMode: runtime.tmuxMode ?? null,
    readTmuxBuffer: runtime.readTmuxBuffer ?? null,
  };
}

function waitBrieflyForSessionExit(exitPromise, maxWaitMs = 1500) {
  if (!exitPromise || typeof exitPromise.then !== 'function') {
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    const fallbackTimer = setTimeout(resolve, maxWaitMs);
    fallbackTimer.unref?.();
    const finishWait = () => {
      clearTimeout(fallbackTimer);
      resolve();
    };
    exitPromise.then(finishWait, finishWait);
  });
}

/**
 * Disposes `session`, closes `bridge`, and removes `scratchDir` across scenario
 * teardown, rethrowing the first cleanup error if any step fails.
 */
export async function cleanupManagedSession(session, bridge, scratchDir) {
  let firstCleanupError = null;
  try {
    await session?.dispose?.();
    await waitBrieflyForSessionExit(session?.exitPromise);
  } catch (sessionError) {
    firstCleanupError ??= sessionError;
  }
  try {
    await bridge?.close?.();
  } catch (bridgeError) {
    firstCleanupError ??= bridgeError;
  }
  try {
    fs.rmSync(scratchDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  } catch (removeError) {
    firstCleanupError ??= removeError;
  }
  if (firstCleanupError !== null) {
    throw firstCleanupError;
  }
}

async function finalizeManagedSession(
  session,
  bridge,
  scratchDir,
  archiveOptions,
  hasPrimaryError,
) {
  try {
    archiveSessionProbeLog(archiveOptions);
  } catch (archiveError) {
    if (!hasPrimaryError) {
      throw archiveError;
    }
  } finally {
    await cleanupManagedSession(session, bridge, scratchDir);
  }
}

/** Runs one example using the shared bridge, readiness log, archive, and cleanup lifecycle. */
export async function withManagedSession(runtime, managedArgs, runWithContext, sessionLabel = null) {
  const scratchDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tuikit-xterm-'));
  const logPath = path.join(scratchDir, 'probe.jsonl');
  const label = buildSessionArchiveLabel(runtime, managedArgs, sessionLabel);
  let bridge = null;
  let session = null;
  let hasPrimaryError = false;
  try {
    bridge = await startBrowserBridge({
      cols: DEFAULT_TERMINAL_COLS,
      rows: DEFAULT_TERMINAL_ROWS,
      headless: true,
      timeoutMs: runtime.timeoutMs,
    });
    session = await runtime.createSession({
      programPath: runtime.programPath,
      programArgs: mergeProbeArgs(['--log', logPath, ...managedArgs], runtime.extraArgs),
      cols: DEFAULT_TERMINAL_COLS,
      rows: DEFAULT_TERMINAL_ROWS,
      bridge,
      tmuxMode: runtime.tmuxMode,
      timeoutMs: runtime.timeoutMs,
      scratchDir,
    });
    return await runWithContext({
      bridge,
      session,
      logPath,
      timeoutMs: runtime.timeoutMs,
      tmuxMode: runtime.tmuxMode,
    });
  } catch (caughtError) {
    hasPrimaryError = true;
    throw caughtError;
  } finally {
    const archiveOptions = { logPath, label, tmuxMode: runtime.tmuxMode };
    await finalizeManagedSession(session, bridge, scratchDir, archiveOptions, hasPrimaryError);
  }
}

async function executeBatchScenario(runtime) {
  return withManagedSession(
    runtime,
    ['--batch'],
    async (sessionContext) => {
      await waitForProbeReady(
        sessionContext.logPath,
        sessionContext.timeoutMs,
        sessionContext.session.exitPromise,
      );
      const exitOutcome = await sessionContext.session.waitForExit(sessionContext.timeoutMs);
      assert.equal(exitOutcome.exitCode, 0, `batch probe exited with ${exitOutcome.exitCode}`);

      const terminalStateAfterExit = await sessionContext.bridge.getTerminalState(
        sessionContext.timeoutMs,
      );
      assert.equal(terminalStateAfterExit.unicodeVersion, '11');
      assert.equal(terminalStateAfterExit.mouseTrackingMode, 'none');

      const records = readJsonLines(sessionContext.logPath);
      return {
        scenario: 'batch',
        exitCode: exitOutcome.exitCode,
        terminalStateAfterExit,
        records,
      };
    },
    'batch',
  );
}

async function readActiveTmuxBuffer(runtime, session) {
  if (runtime.tmuxMode === null) {
    return null;
  }
  if (typeof session.readTmuxBuffer === 'function') {
    return session.readTmuxBuffer();
  }
  return typeof runtime.readTmuxBuffer === 'function' ? runtime.readTmuxBuffer(session) : null;
}

async function runSingleOsc52Probe(runtime, form, token) {
  const managedOsc52Args = ['--batch', '--osc52', token, '--osc52-form', form];
  return withManagedSession(
    runtime,
    managedOsc52Args,
    async (sessionContext) => {
      await waitForProbeReady(
        sessionContext.logPath,
        sessionContext.timeoutMs,
        sessionContext.session.exitPromise,
      );
      const exitOutcome = await sessionContext.session.waitForExit(sessionContext.timeoutMs);
      assert.equal(exitOutcome.exitCode, 0, `osc52 (${form}) exited with ${exitOutcome.exitCode}`);

      const terminalState = await sessionContext.bridge.getTerminalState(sessionContext.timeoutMs);
      const tmuxBuffer = await readActiveTmuxBuffer(runtime, sessionContext.session);
      if (runtime.tmuxMode === null) {
        assert.ok(
          terminalState.osc52Payloads.some((payload) => payload.decodedText === token),
          `expected xterm.js OSC 52 payload '${token}' in direct xterm.js run`,
        );
      }
      return { form, token, osc52Payloads: terminalState.osc52Payloads, tmuxBuffer };
    },
    `osc52-${form}`,
  );
}

async function executeOsc52Scenario(runtime) {
  const probeConfigs =
    runtime.tmuxMode === null
      ? [{ form: 'plain', token: 'tuikit-xterm-osc52-plain' }]
      : [
          { form: 'plain', token: 'tuikit-tmux-osc52-plain' },
          { form: 'tmux', token: 'tuikit-tmux-osc52-wrapped' },
          { form: 'both', token: 'tuikit-tmux-osc52-both' },
        ];
  const probes = [];
  for (const probeConfig of probeConfigs) {
    probes.push(await runSingleOsc52Probe(runtime, probeConfig.form, probeConfig.token));
  }
  return { scenario: 'osc52', probes };
}

async function executePanicScenario(runtime) {
  return withManagedSession(
    runtime,
    [],
    async (sessionContext) => {
      await waitForProbeReady(
        sessionContext.logPath,
        sessionContext.timeoutMs,
        sessionContext.session.exitPromise,
      );
      await waitForReadyOuterTerminalModes(sessionContext);
      const terminalStateBeforePanic = await sessionContext.bridge.getTerminalState(
        sessionContext.timeoutMs,
      );
      if (shouldExpectOuterMouseEvents(sessionContext)) {
        assert.notEqual(
          terminalStateBeforePanic.mouseTrackingMode,
          'none',
          'expected active mouse tracking before triggering F12 panic',
        );
      }

      await sessionContext.bridge.page.keyboard.press('F12');
      const exitOutcome = await sessionContext.session.waitForExit(sessionContext.timeoutMs);
      assert.ok(
        exitOutcome.exitCode !== 0 || exitOutcome.signal !== 0,
        'expected non-zero exit status when F12 panic triggers',
      );

      const terminalStateAfterPanic = await sessionContext.bridge.getTerminalState(
        sessionContext.timeoutMs,
      );
      assert.equal(terminalStateAfterPanic.mouseTrackingMode, 'none');
      assert.equal(terminalStateAfterPanic.bracketedPasteMode, false);
      // Windows ConPTY owns outer DECSET 1004 (`?1004h`) until `ClosePseudoConsole` (`dispose()`).
      assert.equal(terminalStateAfterPanic.sendFocusMode, process.platform === 'win32');

      return {
        scenario: 'panic',
        exitCode: exitOutcome.exitCode,
        signal: exitOutcome.signal,
        terminalStateBeforePanic,
        terminalStateAfterPanic,
      };
    },
    'panic',
  );
}

async function executePassthroughScenario(runtime) {
  return withManagedSession(
    runtime,
    [],
    async (sessionContext) => {
      const exitOutcome = await sessionContext.session.waitForExit(sessionContext.timeoutMs);
      assert.equal(
        exitOutcome.exitCode,
        0,
        `passthrough program exited with ${exitOutcome.exitCode}`,
      );
      const records = fs.existsSync(sessionContext.logPath)
        ? readJsonLines(sessionContext.logPath)
        : [];
      return { scenario: 'passthrough', exitCode: exitOutcome.exitCode, records };
    },
    'passthrough',
  );
}

async function dispatchScenarioByName(scenarioName, runtime) {
  if (MOUSE_SCENARIOS.includes(scenarioName)) {
    return runMouseScenario(runtime, scenarioName);
  }
  if (scenarioName === 'batch') {
    return executeBatchScenario(runtime);
  }
  if (scenarioName === 'osc52') {
    return executeOsc52Scenario(runtime);
  }
  if (scenarioName === 'interactive') {
    return executeInteractiveScenario(runtime, withManagedSession);
  }
  if (scenarioName === 'panic') {
    return executePanicScenario(runtime);
  }
  if (scenarioName === 'passthrough') {
    return executePassthroughScenario(runtime);
  }
  if (scenarioName === 'all') {
    const batch = await executeBatchScenario(runtime);
    const osc52 = await executeOsc52Scenario(runtime);
    const interactive = await executeInteractiveScenario(runtime, withManagedSession);
    const panic = await executePanicScenario(runtime);
    return { scenario: 'all', batch, osc52, interactive, panic };
  }
  throw new Error(`Unsupported scenario '${scenarioName}'.`);
}

/**
 * Executes `scenarioName` (`all`, `batch`, `osc52`, `interactive`, `panic`, or
 * `passthrough`) and returns the structured JSON report emitted by the CLI.
 */
export async function runScenario(scenarioName, rawRuntime) {
  const runtime = normalizeScenarioRuntime(rawRuntime);
  const scenarioPayload = await dispatchScenarioByName(scenarioName, runtime);
  return {
    layer: runtime.tmuxMode === null ? 3 : 4,
    platform: process.platform,
    arch: process.arch,
    program: runtime.program,
    tmuxConfig: runtime.tmuxMode,
    ...scenarioPayload,
  };
}
