/** Mouse laboratory scenarios using the shared managed terminal session. */
import assert from 'node:assert/strict';
import { movePointerAcrossCells } from './cell_coords.mjs';
import { assertMouseHover, assertMouseModeReplies, assertMouseSetup } from './mouse_assertions.mjs';
import { readCompleteJsonLines, readJsonLines, waitForLogRecords, waitForProbeReady } from './ready_waiter.mjs';
import { withManagedSession } from './scenarios.mjs';

const IMPLEMENTED_SCENARIOS = ['x1', 'x2', 'x3'];
const SETUP_SEQUENCE = '\x1b[?1000h\x1b[?1002h\x1b[?1006h';
const TEARDOWN_SEQUENCE = '\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l';

function waitForLabRecords(context, selector, description) {
  return waitForLogRecords(context.logPath, selector, {
    timeoutMs: context.timeoutMs,
    processExitPromise: context.session.exitPromise,
    description,
  });
}

/** Runs an action then waits for the lab to process a FIFO F2 input/log fence. */
export async function runMouseFencedStep(context, performAction) {
  const startCount = readCompleteJsonLines(context.logPath).length;
  await performAction();
  await context.bridge.page.keyboard.press('F2');
  return waitForLabRecords(context, (records) => {
    const trailing = records.slice(startCount);
    const fenceOffset = trailing.findIndex((record) => record.kind === 'event'
      && record.event?.kind === 'Key' && record.event.code === 'F(2)'
      && record.event.key_kind !== 'Release');
    if (fenceOffset < 0 || !trailing.slice(fenceOffset + 1).some((record) => record.kind === 'report')) {
      return null;
    }
    return trailing.slice(0, fenceOffset);
  }, 'mouse_lab F2 fence and report');
}

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

async function executeMouseScenario(runtime, scenario) {
  const managedArgs = scenario === 'x1' && !runtime.extraArgs.includes('--query-modes') ? ['--query-modes'] : [];
  return withManagedSession(runtime, managedArgs, async (context) => {
    const terminalStateAtReady = await waitForMouseReady(context);
    const pointerSteps = scenario === 'x3' ? await runHoverSteps(context, terminalStateAtReady) : null;
    await context.bridge.page.keyboard.press(scenario === 'x2' ? '!' : 'q');
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
      assertMouseModeReplies(records);
    }
    return { scenario, ...exitOutcome, terminalStateAtReady, terminalStateAfterExit, pointerSteps, records };
  }, scenario);
}

/** Executes one lab scenario, or all currently implemented scenarios in order. */
export async function runMouseScenario(runtime, scenario) {
  if (scenario !== 'mouse') {
    assert.ok(IMPLEMENTED_SCENARIOS.includes(scenario), `mouse scenario '${scenario}' is not implemented yet`);
    return executeMouseScenario(runtime, scenario);
  }
  const scenarios = {};
  for (const scenarioName of IMPLEMENTED_SCENARIOS) {
    scenarios[scenarioName] = await executeMouseScenario(runtime, scenarioName);
  }
  return { scenario: 'mouse', scenarios };
}
