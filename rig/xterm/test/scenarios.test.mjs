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
import { waitForLogRecords } from '../src/ready_waiter.mjs';

const TEST_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const XTERM_ROOT = path.resolve(TEST_DIRECTORY, '..');
const RUN_SCRIPT_PATH = path.join(XTERM_ROOT, 'run.sh');

const SAMPLE_TERMINAL_STATE = Object.freeze({
  cols: 100,
  rows: 30,
  cellWidthPx: 9,
  cellHeightPx: 18,
  screenLeftPx: 12,
  screenTopPx: 20,
});

function runHarnessCli(cliArgs, options = {}) {
  return spawnSync('bash', [RUN_SCRIPT_PATH, ...cliArgs], {
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

function assertInteractiveMouseSteps(interactiveReport) {
  const pointerSteps = interactiveReport.pointerSteps;
  if (process.platform !== 'win32') {
    assert.equal(
      pointerSteps.hover.filter((record) => record.mouse_kind === 'moved').length,
      0,
    );
  }
  assert.ok(
    pointerSteps.left_click.some(
      (record) => record.mouse_kind === 'down:left' && record.column === 10 && record.row === 5,
    ),
  );
  assert.ok(
    pointerSteps.left_click.some(
      (record) => record.mouse_kind === 'up:left' && record.column === 10 && record.row === 5,
    ),
  );
  assert.ok(
    pointerSteps.left_drag.some(
      (record) => record.mouse_kind === 'down:left' && record.column === 12 && record.row === 6,
    ),
  );
  assert.ok(pointerSteps.left_drag.some((record) => record.mouse_kind === 'drag:left'));
  assert.ok(
    pointerSteps.left_drag.some(
      (record) => record.mouse_kind === 'up:left' && record.column === 18 && record.row === 6,
    ),
  );
  assert.ok(
    pointerSteps.wheel_down.some(
      (record) => record.mouse_kind === 'scroll_down' && record.column === 10 && record.row === 5,
    ),
  );
  assert.ok(
    pointerSteps.wheel_up.some(
      (record) => record.mouse_kind === 'scroll_up' && record.column === 10 && record.row === 5,
    ),
  );
  assert.ok(Array.isArray(pointerSteps.shift_click));
  assert.ok(Array.isArray(pointerSteps.alt_click));
  assert.ok(Array.isArray(pointerSteps.alt_drag));
}

function assertInteractiveKeyboardPasteAndFocus(interactiveReport) {
  const keySteps = interactiveReport.keyboardSteps;
  assert.ok(keySteps.arrow_up.some((record) => record.code === 'up'));
  assert.ok(keySteps.f1.some((record) => record.code === 'f1'));
  assert.ok(keySteps.ctrl_a.some((record) => record.code === 'char:a' && record.ctrl === true));
  assert.ok(Array.isArray(interactiveReport.kittyEnterSteps.alt_enter));
  assert.ok(Array.isArray(interactiveReport.kittyEnterSteps.shift_enter));
  assert.ok(Array.isArray(interactiveReport.kittyEnterSteps.ctrl_enter));

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
  assert.equal(interactiveReport.terminalStateAfterExit.sendFocusMode, false);
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

test('pointer helpers drive Playwright mouse and release modifiers in reverse order on error', async () => {
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
  assert.deepEqual(recordedActions.slice(0, 5), [
    'keyDown:Shift',
    'keyDown:Alt',
    'click:106.5,119:right:2',
    'keyUp:Alt',
    'keyUp:Shift',
  ]);

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
    fs.rmSync(tempDirectory, { recursive: true, force: true });
  }
});

test('bridge_server exposes blur/focus helpers and fails flush fast if browser socket closes', async () => {
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
    'Usage:',
    'Purpose:',
    'Arguments:',
    'Scenarios:',
    'Options:',
    'Streams:',
    'Examples:',
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
    assert.match(failedResult.stderr, /❌/);
    assert.match(failedResult.stderr, /\n {2}⚠️ /);
  }
});

test('run.sh executes capability_probe all scenario with clean JSON stdout and verified teardown', () => {
  const probeResult = runHarnessCli(['capability_probe'], { timeoutMs: 60000 });
  assert.equal(probeResult.status, 0, `stderr:\n${probeResult.stderr}`);
  assert.match(probeResult.stderr, /==>/);
  assert.match(probeResult.stderr, /✅/);

  const stdoutLines = probeResult.stdout.trim().split('\n');
  assert.equal(stdoutLines.length, 1);
  const report = JSON.parse(stdoutLines[0]);
  assert.equal(report.layer, 3);
  assert.equal(report.program, 'capability_probe');
  assert.equal(report.scenario, 'all');

  assert.equal(report.batch.exitCode, 0);
  assert.equal(report.batch.terminalStateAfterExit.unicodeVersion, '11');
  assert.equal(report.batch.terminalStateAfterExit.mouseTrackingMode, 'none');
  const identityRecord = report.batch.records.find((record) => record.section === 'identity');
  const mouseModesRecord = report.batch.records.find((record) => record.section === 'mouse_modes');
  const colorRecord = report.batch.records.find((record) => record.section === 'background_color');
  assert.equal(identityRecord?.is_tty_available, true);
  assert.equal(identityRecord?.timed_out, false);
  assert.equal(identityRecord?.is_tmux, false);
  assert.equal(mouseModesRecord?.is_m1_expected, true);
  assert.equal(mouseModesRecord?.is_m5_reset, true);
  assert.equal(colorRecord?.answered, true);
  assert.equal(colorRecord?.is_dark, true);

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
  assert.equal(report.panic.terminalStateAfterPanic.sendFocusMode, false);
});
