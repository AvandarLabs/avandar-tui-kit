/** Mouse laboratory scenarios using the shared managed terminal session. */
import assert from 'node:assert/strict';
import { movePointerAcrossCells } from './cell_coords.mjs';
import { assertMouseGesture, assertMouseHover, assertMouseModeReplies, assertMouseSetup } from './mouse_assertions.mjs';
import { readCompleteJsonLines, readJsonLines, waitForProbeReady } from './ready_waiter.mjs';
import { runMouseFencedStep, runMouseGesture } from './mouse_fixtures.mjs';
import { withManagedSession } from './scenarios.mjs';

export { runMouseFencedStep } from './mouse_fixtures.mjs';

const IMPLEMENTED_SCENARIOS = Array.from({ length: 14 }, (_, index) => `x${index + 1}`);
const SETUP_SEQUENCE = '\x1b[?1000h\x1b[?1002h\x1b[?1006h';
const TEARDOWN_SEQUENCE = '\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l';

async function waitForMouseReady(context) {
  await waitForProbeReady(context.logPath, context.timeoutMs, context.session.exitPromise);
  const expectedMode = process.platform === 'win32' ? 'any' : 'drag';
  await context.bridge.page.waitForFunction(
    (mode) => window.__getTerminalState().mouseTrackingMode === mode,
    expectedMode, { timeout: context.timeoutMs },
  );
  const state = await context.bridge.getTerminalState(context.timeoutMs);
  assertMouseSetup(state);
  return state;
}

async function runHoverSteps(context, terminalState) {
  // Prime initial selection observations before either negative assertion window.
  await runMouseFencedStep(context, async () => {});
  const hover = await runMouseFencedStep(context, () => movePointerAcrossCells(
    context.bridge.page, terminalState, { column: 2, row: 2 }, { column: 6, row: 2 },
  ));
  assertMouseHover(hover);
  const paneModeRecords = await runMouseFencedStep(context, () => context.bridge.page.keyboard.press('m'));
  assert.ok(paneModeRecords.some((record) => record.kind === 'sequence'
    && record.label === 'pane_modes' && record.destination === 'parser'
    && Buffer.from(record.bytes).toString().includes('\x1b[?1000h\x1b[?1006h')),
  'right pane must request SGR mouse pass-through before the second hover');
  const layout = readCompleteJsonLines(context.logPath).findLast((record) => record.kind === 'layout');
  const right = layout.areas.find((area) => area.target === 'right');
  const hoverPassThrough = await runMouseFencedStep(context, () => movePointerAcrossCells(
    context.bridge.page, terminalState,
    { column: right.x + 1, row: right.y + 1 }, { column: right.x + 5, row: right.y + 1 },
  ));
  assertMouseHover(hoverPassThrough);
  return { hover, paneModeRecords, hoverPassThrough };
}

function assertLifecycleSequences(records) {
  for (const [label, sequence] of [['setup', SETUP_SEQUENCE], ['teardown', TEARDOWN_SEQUENCE]]) {
    const observations = records.filter((record) => record.kind === 'sequence' && record.label === label);
    assert.equal(observations.length, 1, `expected exactly one ${label} sequence`);
    assert.deepEqual(observations[0].bytes, process.platform === 'win32' ? [] : [...Buffer.from(sequence)]);
  }
}

async function executeMouseScenario(runtime, scenario, modifier = null) {
  const managedArgs = scenario === 'x1' && !runtime.extraArgs.includes('--query-modes') ? ['--query-modes'] : [];
  return withManagedSession(runtime, managedArgs, async (context) => {
    const terminalStateAtReady = await context.diagnostics.run('app-ready', () => waitForMouseReady(context));
    let pointerSteps = null;
    await context.diagnostics.run('gesture', async () => {
      if (scenario === 'x3') pointerSteps = await runHoverSteps(context, terminalStateAtReady);
      else if (Number(scenario.slice(1)) >= 4) {
        pointerSteps = await runMouseGesture(context, terminalStateAtReady, scenario, modifier);
        assertMouseGesture(scenario, pointerSteps, process.platform, context.session.tmuxServerInfo);
      }
    });
    await context.diagnostics.run('terminating-key', () => context.bridge.page.keyboard.press(scenario === 'x2' ? '!' : 'q'));
    const exitOutcome = await context.session.waitForExit(context.timeoutMs);
    if (scenario === 'x2') {
      assert.ok(exitOutcome.exitCode !== 0 || exitOutcome.signal !== 0, 'panic must exit unsuccessfully');
    } else {
      assert.equal(exitOutcome.exitCode, 0, 'mouse lab must exit cleanly');
    }
    const terminalStateAfterExit = await context.bridge.getTerminalState(context.timeoutMs);
    assert.equal(terminalStateAfterExit.mouseTrackingMode, 'none', 'exit must restore outer tracking');
    const records = readJsonLines(context.logPath);
    assertLifecycleSequences(records);
    if (scenario === 'x1') {
      assertMouseModeReplies(records, process.platform, context.session.tmuxServerInfo);
    }
    context.diagnostics.record('state-assertions', 'return');
    return { scenario, ...exitOutcome, tmuxServerInfo: context.session.tmuxServerInfo ?? null, terminalStateAtReady, terminalStateAfterExit, pointerSteps, records,
      assertion: { id: scenario.toUpperCase(), passed: true, platform: process.platform, modifier } };
  }, modifier ? `${scenario}-${modifier.toLowerCase()}` : scenario);
}

/** Executes one lab scenario, or all currently implemented scenarios in order. */
export async function runMouseScenario(runtime, scenario) {
  if (scenario !== 'mouse') {
    assert.ok(IMPLEMENTED_SCENARIOS.includes(scenario), `mouse scenario '${scenario}' is not implemented yet`);
    return runSingleMouseScenario(runtime, scenario);
  }
  const scenarios = {};
  for (const scenarioName of IMPLEMENTED_SCENARIOS) {
    scenarios[scenarioName] = await runSingleMouseScenario(runtime, scenarioName);
  }
  return { scenario: 'mouse', scenarios };
}

async function runSingleMouseScenario(runtime, scenario) {
  if (scenario !== 'x11') return executeMouseScenario(runtime, scenario);
  const sessions = {};
  for (const modifier of ['Shift', 'Alt']) {
    sessions[modifier.toLowerCase()] = await executeMouseScenario(runtime, scenario, modifier);
  }
  return { scenario, sessions, platformCondition: process.platform === 'darwin'
    ? 'Shift and Alt select in the app' : 'Shift selects natively; Alt selects in the app' };
}
