/**
 * Scenario coordinator (`batch`, `osc52`, `interactive`, `panic`, `all`, `passthrough`)
 * shared by Layer 3 (`rig/xterm/run.sh`) and Layer 4 (`rig/xterm/run-tmux.sh`).
 */

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { startBrowserBridge } from './bridge_server.mjs';
import { executeInteractiveScenario } from './interactive_scenario.mjs';
import { startPtySession } from './pty_session.mjs';
import { readJsonLines, waitForProbeReady } from './ready_waiter.mjs';

const DEFAULT_TERMINAL_COLS = 100;
const DEFAULT_TERMINAL_ROWS = 30;
const BOOLEAN_PROBE_FLAGS = new Set(['--batch', '--push-kitty']);
const VALUE_PROBE_FLAGS = new Set(['--log', '--osc52', '--osc52-form']);

/**
 * Merges scenario-managed flags with forwarded `extraArgs`, deduplicating known `capability_probe`
 * flags (giving `managedArgs` precedence) while preserving unknown flags for custom `<program>` runs.
 */
export function mergeProbeArgs(managedArgs = [], extraArgs = []) {
  const combinedTokens = [...managedArgs, ...extraArgs];
  const mergedArgs = [];
  const seenProbeFlags = new Set();

  for (let index = 0; index < combinedTokens.length; index += 1) {
    const token = combinedTokens[index];
    const equalsOffset = token.indexOf('=');
    const inlineFlag = equalsOffset > 0 ? token.slice(0, equalsOffset) : null;
    if (BOOLEAN_PROBE_FLAGS.has(token)) {
      if (!seenProbeFlags.has(token)) {
        seenProbeFlags.add(token);
        mergedArgs.push(token);
      }
      continue;
    }
    if (VALUE_PROBE_FLAGS.has(token)) {
      const valueToken = combinedTokens[index + 1];
      index += 1;
      if (!seenProbeFlags.has(token)) {
        seenProbeFlags.add(token);
        mergedArgs.push(token);
        if (valueToken !== undefined) {
          mergedArgs.push(valueToken);
        }
      }
      continue;
    }
    if (inlineFlag !== null && VALUE_PROBE_FLAGS.has(inlineFlag)) {
      if (!seenProbeFlags.has(inlineFlag)) {
        seenProbeFlags.add(inlineFlag);
        mergedArgs.push(token);
      }
      continue;
    }
    mergedArgs.push(token);
  }
  return mergedArgs;
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

async function withManagedSession(runtime, managedArgs, runWithContext) {
  const scratchDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tuikit-xterm-'));
  const logPath = path.join(scratchDir, 'probe.jsonl');
  let bridge = null;
  let session = null;
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
  } finally {
    await cleanupManagedSession(session, bridge, scratchDir);
  }
}

async function executeBatchScenario(runtime) {
  return withManagedSession(runtime, ['--batch'], async (stepContext) => {
    await waitForProbeReady(
      stepContext.logPath,
      stepContext.timeoutMs,
      stepContext.session.exitPromise,
    );
    const exitOutcome = await stepContext.session.waitForExit(stepContext.timeoutMs);
    assert.equal(exitOutcome.exitCode, 0, `batch probe exited with ${exitOutcome.exitCode}`);

    const terminalStateAfterExit = await stepContext.bridge.getTerminalState(stepContext.timeoutMs);
    assert.equal(terminalStateAfterExit.unicodeVersion, '11');
    assert.equal(terminalStateAfterExit.mouseTrackingMode, 'none');

    const records = readJsonLines(stepContext.logPath);
    return {
      scenario: 'batch',
      exitCode: exitOutcome.exitCode,
      terminalStateAfterExit,
      records,
    };
  });
}

async function readActiveTmuxBuffer(runtime, session) {
  if (runtime.tmuxMode === null) {
    return null;
  }
  if (typeof session.readTmuxBuffer === 'function') {
    return session.readTmuxBuffer();
  }
  if (typeof runtime.readTmuxBuffer === 'function') {
    return runtime.readTmuxBuffer(session);
  }
  return null;
}

async function runSingleOsc52Probe(runtime, form, token) {
  const managedOsc52Args = ['--batch', '--osc52', token, '--osc52-form', form];
  return withManagedSession(runtime, managedOsc52Args, async (stepContext) => {
    await waitForProbeReady(
      stepContext.logPath,
      stepContext.timeoutMs,
      stepContext.session.exitPromise,
    );
    const exitOutcome = await stepContext.session.waitForExit(stepContext.timeoutMs);
    assert.equal(exitOutcome.exitCode, 0, `osc52 (${form}) exited with ${exitOutcome.exitCode}`);

    const terminalState = await stepContext.bridge.getTerminalState(stepContext.timeoutMs);
    const tmuxBuffer = await readActiveTmuxBuffer(runtime, stepContext.session);

    if (runtime.tmuxMode === null) {
      assert.ok(
        terminalState.osc52Payloads.some((payload) => payload.decodedText === token),
        `expected xterm.js OSC 52 payload '${token}' in Layer 3`,
      );
    }
    return {
      form,
      token,
      osc52Payloads: terminalState.osc52Payloads,
      tmuxBuffer,
    };
  });
}

async function executeOsc52Scenario(runtime) {
  const probeSpecs =
    runtime.tmuxMode === null
      ? [{ form: 'plain', token: 'tuikit-xterm-osc52-plain' }]
      : [
          { form: 'plain', token: 'tuikit-tmux-osc52-plain' },
          { form: 'tmux', token: 'tuikit-tmux-osc52-wrapped' },
        ];
  const probes = [];
  for (const spec of probeSpecs) {
    probes.push(await runSingleOsc52Probe(runtime, spec.form, spec.token));
  }
  return { scenario: 'osc52', probes };
}

async function executePanicScenario(runtime) {
  return withManagedSession(runtime, [], async (stepContext) => {
    await waitForProbeReady(
      stepContext.logPath,
      stepContext.timeoutMs,
      stepContext.session.exitPromise,
    );
    const terminalStateBeforePanic = await stepContext.bridge.getTerminalState(
      stepContext.timeoutMs,
    );
    if (runtime.tmuxMode !== 'stock') {
      assert.notEqual(
        terminalStateBeforePanic.mouseTrackingMode,
        'none',
        'expected active mouse tracking before triggering F12 panic',
      );
    }

    await stepContext.bridge.page.keyboard.press('F12');
    const exitOutcome = await stepContext.session.waitForExit(stepContext.timeoutMs);
    const hasNonZeroExit = exitOutcome.exitCode !== 0 || exitOutcome.signal !== 0;
    assert.ok(hasNonZeroExit, 'expected non-zero exit status when F12 panic triggers');

    const terminalStateAfterPanic = await stepContext.bridge.getTerminalState(
      stepContext.timeoutMs,
    );
    assert.equal(terminalStateAfterPanic.mouseTrackingMode, 'none');
    assert.equal(terminalStateAfterPanic.bracketedPasteMode, false);
    assert.equal(terminalStateAfterPanic.sendFocusMode, false);

    return {
      scenario: 'panic',
      exitCode: exitOutcome.exitCode,
      signal: exitOutcome.signal,
      terminalStateBeforePanic,
      terminalStateAfterPanic,
    };
  });
}

async function executePassthroughScenario(runtime) {
  return withManagedSession(runtime, [], async (stepContext) => {
    const exitOutcome = await stepContext.session.waitForExit(stepContext.timeoutMs);
    assert.equal(exitOutcome.exitCode, 0, `passthrough program exited with ${exitOutcome.exitCode}`);
    const records = fs.existsSync(stepContext.logPath) ? readJsonLines(stepContext.logPath) : [];
    return {
      scenario: 'passthrough',
      exitCode: exitOutcome.exitCode,
      records,
    };
  });
}

async function dispatchScenarioByName(scenarioName, runtime) {
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
 * Runs the requested harness scenario (`all`, `batch`, `osc52`, `interactive`, `panic`, or
 * `passthrough`) and wraps the result with Layer 3 / Layer 4 metadata.
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
