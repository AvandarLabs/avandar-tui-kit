import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { WebSocket } from 'ws';

import { createBridgeServer, startBrowserBridge } from '../src/bridge_server.mjs';
import {
  cellCenterToViewportPx,
  clickCell,
  dragAcrossCells,
  movePointerAcrossCells,
  wheelAtCell,
} from '../src/cell_coords.mjs';
import { hasDeliveredPastePayload } from '../src/interactive_scenario.mjs';
import { waitForLogRecords } from '../src/ready_waiter.mjs';
import { cleanupManagedSession, mergeProbeArgs } from '../src/scenarios.mjs';

const TEST_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const XTERM_ROOT = path.resolve(TEST_DIRECTORY, '..');
const RUN_SCRIPT_PATH = path.join(XTERM_ROOT, 'run.sh');
const HARNESS_SCRIPT_PATH = path.join(XTERM_ROOT, 'src', 'harness.mjs');

const SAMPLE_TERMINAL_STATE = Object.freeze({
  cols: 100,
  rows: 30,
  cellWidthPx: 9,
  cellHeightPx: 18,
  screenLeftPx: 12,
  screenTopPx: 20,
});

function runHarnessCli(cliArgs, options = {}) {
  const isWin = process.platform === 'win32';
  const cliTarget = isWin ? [HARNESS_SCRIPT_PATH, ...cliArgs] : [RUN_SCRIPT_PATH, ...cliArgs];
  return spawnSync(isWin ? process.execPath : 'bash', cliTarget, {
    cwd: options.cwd ?? XTERM_ROOT,
    env: { ...process.env, ...options.envOverrides },
    encoding: 'utf8',
    timeout: options.timeoutMs ?? 60000,
  });
}

function createFakePlaywrightPage(recordedActions, options = {}) {
  return {
    keyboard: {
      async down(keyName) {
        recordedActions.push(`keyDown:${keyName}`);
      },
      async up(keyName) {
        recordedActions.push(`keyUp:${keyName}`);
      },
    },
    mouse: {
      async click(coordX, coordY, clickOptions) {
        recordedActions.push(`click:${coordX},${coordY}:${clickOptions.button}:${clickOptions.clickCount}`);
        if (options.shouldFailClick) {
          throw new Error('synthetic click failure');
        }
      },
      async move(coordX, coordY, moveOptions) {
        recordedActions.push(`move:${coordX},${coordY}:${moveOptions?.steps ?? 1}`);
      },
      async down(downOptions) {
        recordedActions.push(`down:${downOptions.button}`);
      },
      async up(upOptions) {
        recordedActions.push(`up:${upOptions.button}`);
      },
      async wheel(deltaX, deltaY) {
        recordedActions.push(`wheel:${deltaX},${deltaY}`);
      },
    },
  };
}

function hasMouseEvent(records, mouseKind, column = null, row = null) {
  return records.some(
    (record) =>
      record.mouse_kind === mouseKind &&
      (column === null || record.column === column) &&
      (row === null || record.row === row),
  );
}

function assertInteractiveMouseSteps(interactiveReport) {
  const pointerSteps = interactiveReport.pointerSteps;
  if (process.platform !== 'win32') {
    assert.equal(pointerSteps.hover.filter((record) => record.mouse_kind === 'moved').length, 0);
  }
  assert.ok(hasMouseEvent(pointerSteps.left_click, 'down:left', 10, 5));
  assert.ok(hasMouseEvent(pointerSteps.left_click, 'up:left', 10, 5));
  assert.ok(hasMouseEvent(pointerSteps.left_drag, 'down:left', 12, 6));
  assert.ok(hasMouseEvent(pointerSteps.left_drag, 'drag:left'));
  assert.ok(hasMouseEvent(pointerSteps.left_drag, 'up:left', 18, 6));
  assert.ok(hasMouseEvent(pointerSteps.wheel_down, 'scroll_down', 10, 5));
  assert.ok(hasMouseEvent(pointerSteps.wheel_up, 'scroll_up', 10, 5));
  assert.equal(
    pointerSteps.shift_click.some((record) => record.mouse_kind === 'down:left' && record.shift),
    process.platform === 'darwin',
  );
  assert.ok(pointerSteps.alt_click.some((record) => record.mouse_kind === 'down:left' && record.alt));
  assert.ok(pointerSteps.alt_drag.some((record) => record.mouse_kind === 'drag:left' && record.alt));
}

function assertInteractiveKeyboardPasteAndFocus(interactiveReport) {
  const keySteps = interactiveReport.keyboardSteps;
  assert.ok(keySteps.arrow_up.some((record) => record.code === 'up'));
  assert.ok(keySteps.f1.some((record) => record.code === 'f1'));
  assert.ok(keySteps.ctrl_a.some((record) => record.code === 'char:a' && record.ctrl === true));
  const kittyEnter = interactiveReport.kittyEnterSteps;
  assert.ok(kittyEnter.alt_enter.some((record) => record.code === 'enter' && record.alt === true));
  assert.ok(kittyEnter.shift_enter.some((record) => record.code === 'enter' && !record.shift));
  assert.ok(kittyEnter.ctrl_enter.some((record) => record.code === 'enter' && !record.ctrl));

  if (process.platform !== 'win32') {
    assert.ok(
      interactiveReport.pasteEvents.some(
        (record) =>
          record.event_type === 'paste' &&
          (record.text === 'alpha\rbeta' || record.text === 'alpha\nbeta'),
      ),
    );
    assert.ok(
      interactiveReport.focus.focusLostEvents.some(
        (record) => record.event_type === 'focus' && record.focused === false,
      ),
    );
    assert.ok(
      interactiveReport.focus.focusGainedEvents.some(
        (record) => record.event_type === 'focus' && record.focused === true,
      ),
    );
  }
  assert.equal(interactiveReport.terminalStateAfterExit.mouseTrackingMode, 'none');
  assert.equal(interactiveReport.terminalStateAfterExit.bracketedPasteMode, false);
  assert.equal(interactiveReport.terminalStateAfterExit.sendFocusMode, process.platform === 'win32');
}

test('cellCenterToViewportPx maps cell indices to pixel centers and rejects invalid bounds', () => {
  assert.deepEqual(cellCenterToViewportPx(SAMPLE_TERMINAL_STATE, 0, 0), { x: 16.5, y: 29 });
  assert.deepEqual(cellCenterToViewportPx(SAMPLE_TERMINAL_STATE, 10, 5), { x: 106.5, y: 119 });
  assert.deepEqual(cellCenterToViewportPx(SAMPLE_TERMINAL_STATE, 99, 29), { x: 907.5, y: 551 });

  const invalidCalls = [
    () => cellCenterToViewportPx(SAMPLE_TERMINAL_STATE, -1, 0),
    () => cellCenterToViewportPx(SAMPLE_TERMINAL_STATE, 100, 0),
    () => cellCenterToViewportPx(SAMPLE_TERMINAL_STATE, 0, -1),
    () => cellCenterToViewportPx(SAMPLE_TERMINAL_STATE, 0, 30),
    () => cellCenterToViewportPx(SAMPLE_TERMINAL_STATE, 1.5, 2),
    () => cellCenterToViewportPx({ ...SAMPLE_TERMINAL_STATE, cellWidthPx: 0 }, 0, 0),
  ];
  for (const invokeInvalid of invalidCalls) {
    assert.throws(invokeInvalid, RangeError);
  }
});

test('pointer helpers emit click, drag, hover-move, and wheel sequences at cell centers', async () => {
  const recordedActions = [];
  const fakePage = createFakePlaywrightPage(recordedActions);
  await clickCell(fakePage, SAMPLE_TERMINAL_STATE, 10, 5, {
    button: 'right',
    modifiers: ['Shift', 'Alt'],
    clickCount: 2,
  });
  await dragAcrossCells(
    fakePage,
    SAMPLE_TERMINAL_STATE,
    { column: 1, row: 2 },
    { column: 3, row: 2 },
    { modifiers: ['Alt'], steps: 3 },
  );
  await movePointerAcrossCells(
    fakePage,
    SAMPLE_TERMINAL_STATE,
    { column: 0, row: 0 },
    { column: 2, row: 0 },
    { steps: 2 },
  );
  await wheelAtCell(fakePage, SAMPLE_TERMINAL_STATE, 4, 1, 0, 100);
  assert.deepEqual(recordedActions, [
    'keyDown:Shift',
    'keyDown:Alt',
    'click:106.5,119:right:2',
    'keyUp:Alt',
    'keyUp:Shift',
    'move:25.5,65:1',
    'keyDown:Alt',
    'down:left',
    'move:43.5,65:3',
    'up:left',
    'keyUp:Alt',
    'move:16.5,29:1',
    'move:34.5,29:2',
    'move:52.5,47:1',
    'wheel:0,100',
  ]);
});

test('clickCell releases held modifiers in reverse order when Playwright click throws', async () => {
  const failureActions = [];
  const failingPage = createFakePlaywrightPage(failureActions, { shouldFailClick: true });
  await assert.rejects(() =>
    clickCell(failingPage, SAMPLE_TERMINAL_STATE, 2, 2, { modifiers: ['Shift', 'Control'] }),
  );
  assert.deepEqual(failureActions, [
    'keyDown:Shift',
    'keyDown:Control',
    'click:34.5,65:left:1',
    'keyUp:Control',
    'keyUp:Shift',
  ]);
});

test('waitForLogRecords ignores incomplete trailing lines and resolves on complete match', async () => {
  const tempDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'rig-xterm-log-wait-'));
  try {
    const logPath = path.join(tempDirectory, 'events.jsonl');
    fs.writeFileSync(logPath, '{"kind":"event","code":"up"}\n{"kind":"event","code":"f2"', 'utf8');
    const waitPromise = waitForLogRecords(
      logPath,
      (records) => records.find((record) => record.code === 'f2') ?? null,
      { timeoutMs: 3000 },
    );
    setImmediate(() => {
      fs.appendFileSync(logPath, ',"key_kind":"press"}\n', 'utf8');
    });
    const matchedRecord = await waitPromise;
    assert.equal(matchedRecord.code, 'f2');
    assert.equal(matchedRecord.key_kind, 'press');
  } finally {
    fs.rmSync(tempDirectory, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

test('hasDeliveredPastePayload waits for full alpha+beta key burst when bracketed paste is off', () => {
  const pasteRecord = { kind: 'event', event_type: 'paste', text: 'alpha\rbeta' };
  assert.equal(hasDeliveredPastePayload([pasteRecord]), true);
  const keyPress = (code) => ({ kind: 'event', event_type: 'key', key_kind: 'press', code });
  const keyRelease = (code) => ({ kind: 'event', event_type: 'key', key_kind: 'release', code });
  const partialEvents = [
    keyPress('char:a'),
    keyRelease('char:a'),
    ...['l', 'p', 'h', 'a'].map((letter) => keyPress(`char:${letter}`)),
    keyPress('enter'),
    ...['b', 'e', 't'].map((letter) => keyPress(`char:${letter}`)),
  ];
  assert.equal(hasDeliveredPastePayload(partialEvents), false);
  assert.equal(hasDeliveredPastePayload([...partialEvents, keyPress('char:a')]), true);
});

test('mergeProbeArgs deduplicates managed capability_probe flags while preserving custom flags', () => {
  assert.deepEqual(mergeProbeArgs(['--push-kitty'], ['--push-kitty']), ['--push-kitty']);
  const managedOsc52 = ['--batch', '--osc52', 'managed-token', '--osc52-form', 'plain'];
  const extraOsc52 = ['--batch', '--osc52', 'extra-token', '--osc52-form', 'tmux', '--push-kitty'];
  assert.deepEqual(mergeProbeArgs(managedOsc52, extraOsc52), [...managedOsc52, '--push-kitty']);
  const customFlags = ['--custom', 'one', '--custom', 'two'];
  assert.deepEqual(mergeProbeArgs([], customFlags), customFlags);
});

function assertBatchSectionRecords(batchReport) {
  assert.equal(batchReport.exitCode, 0);
  assert.equal(batchReport.terminalStateAfterExit.unicodeVersion, '11');
  assert.equal(batchReport.terminalStateAfterExit.mouseTrackingMode, 'none');
  const identityRecord = batchReport.records.find((record) => record.section === 'identity');
  const mouseModesRecord = batchReport.records.find((record) => record.section === 'mouse_modes');
  const colorRecord = batchReport.records.find((record) => record.section === 'background_color');
  assert.equal(identityRecord?.is_tty_available, true);
  assert.equal(identityRecord?.timed_out, false);
  assert.equal(identityRecord?.is_tmux, false);
  assert.ok(mouseModesRecord, 'expected mouse_modes section record');
  assert.ok(colorRecord, 'expected background_color section record');
  if (process.platform !== 'win32') {
    assert.equal(mouseModesRecord.is_m1_expected, true);
    assert.equal(mouseModesRecord.is_m5_reset, true);
    assert.equal(colorRecord.answered, true);
    assert.equal(colorRecord.is_dark, true);
  }
}

test('startBrowserBridge blurTerminal and focusTerminal toggle active xterm helper focus', async () => {
  const bridge = await startBrowserBridge({ cols: 100, rows: 30 });
  try {
    await bridge.writeAndFlush('\x1b[?1004h');
    await bridge.blurTerminal();
    const blurredActiveTag = await bridge.page.evaluate(() => document.activeElement?.className ?? '');
    assert.equal(blurredActiveTag, '');
    await bridge.focusTerminal();
    const focusedActiveTag = await bridge.page.evaluate(() => document.activeElement?.className ?? '');
    assert.match(focusedActiveTag, /xterm-helper-textarea/);
  } finally {
    await bridge.close();
  }
});

test('flushTerminalWrites rejects immediately when the browser WebSocket closes mid-write', async () => {
  const rawBridge = await createBridgeServer({ cols: 100, rows: 30 });
  try {
    const mockSocket = new WebSocket(`ws://127.0.0.1:${rawBridge.port}/bridge`);
    await new Promise((resolve) => mockSocket.once('open', resolve));
    mockSocket.send(JSON.stringify({ type: 'terminal_ready', state: SAMPLE_TERMINAL_STATE }));
    const pongReceived = new Promise((resolve) => mockSocket.once('pong', resolve));
    mockSocket.ping();
    await pongReceived;
    rawBridge.writeToTerminal('unacknowledged-output');
    const pendingFlush = rawBridge.flushTerminalWrites(5000);
    mockSocket.close();
    await assert.rejects(pendingFlush, /Browser terminal WebSocket closed unexpectedly/);
  } finally {
    await rawBridge.close();
  }
});

test('run.sh --help prints structured plain help to stdout and exits 0 outside repository', () => {
  const helpResult = runHarnessCli(['--help'], {
    cwd: os.tmpdir(),
    envOverrides: { NO_COLOR: '1' },
  });
  assert.equal(helpResult.status, 0);
  assert.equal(helpResult.stderr, '');
  assert.ok(!helpResult.stdout.includes('\x1b['));
  for (const requiredHeading of [
    'Usage:', 'Purpose:', 'Arguments:', 'Scenarios:', 'Options:', 'Streams:', 'Examples:',
  ]) {
    assert.ok(helpResult.stdout.includes(requiredHeading), `missing ${requiredHeading}`);
  }
});

test('run.sh rejects invalid CLI arguments with exit code 2, empty stdout, and recovery hints', () => {
  const invalidInvocations = [
    [],
    ['--unknown-option'],
    ['capability_probe', 'not_a_scenario'],
    ['capability_probe', '--timeout-ms', '0'],
    ['capability_probe', 'batch', '--', '--log', '/tmp/manual.jsonl'],
  ];
  for (const cliArgs of invalidInvocations) {
    const failedResult = runHarnessCli(cliArgs, { envOverrides: { NO_COLOR: '1' } });
    assert.equal(failedResult.status, 2, `expected exit 2 for ${JSON.stringify(cliArgs)}`);
    assert.equal(failedResult.stdout, '');
    assert.match(failedResult.stderr, /Usage:/);
    assert.match(failedResult.stderr, /\u274c/);
    assert.match(failedResult.stderr, /\n {2}\u26a0\ufe0f /);
  }
});

test('run.sh executes capability_probe all scenario with clean JSON stdout and verified teardown', () => {
  const probeResult = runHarnessCli(['capability_probe', '--', '--push-kitty'], {
    timeoutMs: 60000,
  });
  assert.equal(probeResult.status, 0, `stderr:\n${probeResult.stderr}`);
  assert.match(probeResult.stderr, /==>/);
  assert.match(probeResult.stderr, /\u2705/);

  const stdoutLines = probeResult.stdout.trim().split('\n');
  assert.equal(stdoutLines.length, 1);
  const report = JSON.parse(stdoutLines[0]);
  assert.equal(report.layer, 3);
  assert.equal(report.program, 'capability_probe');
  assert.equal(report.scenario, 'all');

  assertBatchSectionRecords(report.batch);
  assert.ok(
    report.osc52.probes[0].osc52Payloads.some(
      (payload) => payload.decodedText === 'tuikit-xterm-osc52-plain',
    ),
  );
  assertInteractiveMouseSteps(report.interactive);
  assertInteractiveKeyboardPasteAndFocus(report.interactive);

  assert.notEqual(report.panic.exitCode, 0);
  assert.notEqual(report.panic.terminalStateBeforePanic.mouseTrackingMode, 'none');
  assert.equal(report.panic.terminalStateAfterPanic.mouseTrackingMode, 'none');
  assert.equal(report.panic.terminalStateAfterPanic.bracketedPasteMode, false);
  assert.equal(report.panic.terminalStateAfterPanic.sendFocusMode, process.platform === 'win32');
});

test('cleanupManagedSession closes bridge and deletes scratchDir when dispose throws', async () => {
  const scratchDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tuikit-cleanup-'));
  let hasClosedBridge = false;
  const failingSession = { async dispose() { throw new Error('simulated dispose failure'); } };
  const stubBridge = { async close() { hasClosedBridge = true; } };
  await assert.rejects(
    () => cleanupManagedSession(failingSession, stubBridge, scratchDir),
    /simulated dispose failure/,
  );
  assert.equal(hasClosedBridge, true);
  assert.equal(fs.existsSync(scratchDir), false);
});
