import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { createBridgeServer, startBrowserBridge } from '../src/bridge_server.mjs';
import {
  assertPointerStepResults,
  isTmux33Version,
  shouldExpectOuterMouseEvents,
  waitForReadyOuterTerminalModes,
} from '../src/interactive_scenario.mjs';
import {
  buildUniqueTmuxSocketName,
  createTmuxSession,
  resolveTmuxConfigPath,
} from '../src/tmux_session.mjs';

const TEST_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const XTERM_ROOT = path.resolve(TEST_DIRECTORY, '..');
const REPOSITORY_ROOT = path.resolve(XTERM_ROOT, '..', '..');
const RUN_TMUX_SCRIPT_PATH = path.join(XTERM_ROOT, 'run-tmux.sh');
const SKIP_ON_WIN32 = { skip: process.platform === 'win32' };

function runTmuxCli(cliArgs, options = {}) {
  return spawnSync('bash', [RUN_TMUX_SCRIPT_PATH, ...cliArgs], {
    cwd: options.cwd ?? XTERM_ROOT,
    env: { ...process.env, ...options.envOverrides },
    encoding: 'utf8',
    timeout: options.timeoutMs ?? 90000,
  });
}

function listHarnessTmuxSockets() {
  const uid = typeof process.getuid === 'function' ? process.getuid() : null;
  if (uid === null) {
    return [];
  }
  const baseDirs = new Set(
    ['/tmp', process.env.TMUX_TMPDIR, process.env.TMPDIR, os.tmpdir()]
      .filter(Boolean)
      .map((dir) => dir.replace(/\/+$/, '') || '/'),
  );
  const matchingSockets = new Set();
  for (const baseDir of baseDirs) {
    const socketDir = path.join(baseDir, `tmux-${uid}`);
    if (!fs.existsSync(socketDir)) {
      continue;
    }
    for (const entryName of fs.readdirSync(socketDir)) {
      if (entryName.startsWith('tuikit_xterm_')) {
        matchingSockets.add(path.join(socketDir, entryName));
      }
    }
  }
  return [...matchingSockets];
}

function createNodeOnlyPathDir() {
  const binDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tuikit-notmux-bin-'));
  const bashProbe = spawnSync('/bin/sh', ['-c', 'command -v bash'], { encoding: 'utf8' });
  const bashPath = bashProbe.stdout.trim() || '/bin/bash';
  fs.symlinkSync(bashPath, path.join(binDir, 'bash'));
  fs.symlinkSync(process.execPath, path.join(binDir, path.basename(process.execPath)));
  return binDir;
}

function hasMouseCell(records, kind, column, row) {
  return records.some(
    (record) => record.mouse_kind === kind && record.column === column && record.row === row,
  );
}

function assertExactForwardedMouseCells(pointerSteps, tmuxVersion = '') {
  assert.ok(hasMouseCell(pointerSteps.left_click, 'down:left', 10, 5));
  assert.ok(hasMouseCell(pointerSteps.left_click, 'up:left', 10, 5));
  assert.ok(hasMouseCell(pointerSteps.left_drag, 'down:left', 12, 6));
  assert.ok(pointerSteps.left_drag.some((record) => record.mouse_kind === 'drag:left'));
  assert.ok(hasMouseCell(pointerSteps.left_drag, 'up:left', 18, 6));
  assert.ok(hasMouseCell(pointerSteps.wheel_down, 'scroll_down', 10, 5));
  assert.ok(hasMouseCell(pointerSteps.wheel_up, 'scroll_up', 10, 5));
  if (!isTmux33Version(tmuxVersion)) {
    assert.ok(hasMouseCell(pointerSteps.right_click, 'down:right', 15, 7));
  }
  assert.ok(hasMouseCell(pointerSteps.right_click, 'up:right', 15, 7));
}

function assertStockTmuxInteractive(interactiveReport) {
  const tmuxVersion = interactiveReport.tmuxServerInfo?.version ?? '';
  if (isTmux33Version(tmuxVersion)) {
    assert.equal(interactiveReport.tmuxServerInfo?.isMouseEnabled, false);
  }
  assertExactForwardedMouseCells(interactiveReport.pointerSteps, tmuxVersion);
  const focusRecords = [
    ...interactiveReport.focus.focusLostEvents,
    ...interactiveReport.focus.focusGainedEvents,
  ].filter((record) => record.event_type === 'focus');
  assert.equal(focusRecords.length, 0, 'stock tmux focus-events off must forward zero focus events');
  assert.ok(interactiveReport.keyboardSteps.arrow_up.some((record) => record.code === 'up'));
  assert.ok(interactiveReport.keyboardSteps.f1.some((record) => record.code === 'f1'));
  assert.ok(
    interactiveReport.pasteEvents.some(
      (record) =>
        record.event_type === 'paste' &&
        (record.text === 'alpha\rbeta' || record.text === 'alpha\nbeta'),
    ),
  );
  assert.equal(interactiveReport.terminalStateAfterExit.mouseTrackingMode, 'none');
}

function assertFullTmuxInteractive(interactiveReport) {
  const tmuxVersion = interactiveReport.tmuxServerInfo?.version ?? '';
  assert.equal(interactiveReport.tmuxServerInfo?.isMouseEnabled, true);
  assertExactForwardedMouseCells(interactiveReport.pointerSteps, tmuxVersion);
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
  assert.equal(interactiveReport.terminalStateAfterExit.mouseTrackingMode, 'none');
}

function assertTmuxBatchReport(batchReport) {
  assert.equal(batchReport.exitCode, 0);
  assert.equal(batchReport.terminalStateAfterExit.mouseTrackingMode, 'none');
  const identityRecord = batchReport.records.find((record) => record.section === 'identity');
  const mouseModesRecord = batchReport.records.find((record) => record.section === 'mouse_modes');
  const colorRecord = batchReport.records.find((record) => record.section === 'background_color');
  assert.equal(identityRecord?.is_tmux, true);
  assert.match(identityRecord?.term ?? '', /^tmux/);
  if (isTmux33Version(identityRecord?.xtversion ?? '')) {
    assert.equal(mouseModesRecord?.timed_out, false);
    assert.deepEqual(mouseModesRecord?.after_m1, []);
    assert.equal(colorRecord?.answered, false);
    assert.equal(colorRecord?.phase, 'timeout');
  } else {
    assert.equal(mouseModesRecord?.is_m1_expected, true);
    assert.equal(mouseModesRecord?.is_m5_reset, true);
    assert.equal(colorRecord?.answered, true);
    assert.equal(colorRecord?.is_dark, true);
  }
}

test('run-tmux.sh --help prints plain help to stdout and exits 0 even without tmux in PATH', SKIP_ON_WIN32, () => {
  const nodeOnlyBinDir = createNodeOnlyPathDir();
  try {
    const helpResult = runTmuxCli(['--help'], {
      cwd: os.tmpdir(),
      envOverrides: { NO_COLOR: '1', PATH: nodeOnlyBinDir },
    });
    assert.equal(helpResult.status, 0, `stderr:\n${helpResult.stderr}`);
    assert.equal(helpResult.stderr, '');
    assert.ok(!helpResult.stdout.includes('\x1b['));
    for (const requiredText of [
      'Usage:',
      'just xterm-tmux <stock|full> <program>',
      'Purpose:',
      'Arguments:',
      'Scenarios:',
      'Options:',
      'Streams:',
      'Examples:',
    ]) {
      assert.ok(helpResult.stdout.includes(requiredText), `missing '${requiredText}'`);
    }
  } finally {
    fs.rmSync(nodeOnlyBinDir, { recursive: true, force: true });
  }
});

test('run-tmux.sh fails with structured recovery hints on invalid args or missing tmux', SKIP_ON_WIN32, () => {
  const invalidCases = [
    [],
    ['invalid_mode', 'capability_probe'],
    ['stock'],
    ['full', 'capability_probe', 'unknown_scenario'],
    ['stock', 'capability_probe', 'batch', '--', '--log', '/tmp/manual.jsonl'],
  ];
  for (const cliArgs of invalidCases) {
    const failed = runTmuxCli(cliArgs, { envOverrides: { NO_COLOR: '1' } });
    assert.equal(failed.status, 2, `expected exit 2 for ${JSON.stringify(cliArgs)}`);
    assert.equal(failed.stdout, '');
    assert.match(failed.stderr, /Usage:/);
    assert.match(failed.stderr, /❌/);
    assert.match(failed.stderr, /\n {2}⚠️ /);
  }

  const nodeOnlyBinDir = createNodeOnlyPathDir();
  try {
    const missingTmux = runTmuxCli(['stock', 'capability_probe', 'batch'], {
      envOverrides: { NO_COLOR: '1', PATH: nodeOnlyBinDir },
    });
    assert.equal(missingTmux.status, 1);
    assert.equal(missingTmux.stdout, '');
    assert.match(missingTmux.stderr, /❌ run-tmux\.sh: .*tmux/i);
    assert.match(missingTmux.stderr, /\n {2}⚠️ /);
  } finally {
    fs.rmSync(nodeOnlyBinDir, { recursive: true, force: true });
  }
});

test('tmux_session helpers and pointer assertions enforce stock and tmux 3.3 contracts', () => {
  const firstSocket = buildUniqueTmuxSocketName();
  const secondSocket = buildUniqueTmuxSocketName();
  assert.notEqual(firstSocket, secondSocket);
  assert.match(firstSocket, /^tuikit_xterm_/);
  assert.ok(firstSocket.length <= 48, `socket name too long: ${firstSocket}`);

  const stockPath = resolveTmuxConfigPath('stock');
  const fullPath = resolveTmuxConfigPath('full');
  assert.equal(stockPath, path.join(REPOSITORY_ROOT, 'rig', 'tmux', 'stock.conf'));
  assert.equal(fullPath, path.join(REPOSITORY_ROOT, 'rig', 'tmux', 'full.conf'));
  assert.equal(fs.statSync(stockPath).size, 0, 'stock.conf must remain 0 bytes');
  assert.throws(() => resolveTmuxConfigPath('custom'), /stock|full/i);

  assert.equal(
    shouldExpectOuterMouseEvents({
      tmuxMode: 'stock',
      session: { tmuxServerInfo: { version: 'tmux 3.3a', isMouseEnabled: false } },
    }),
    true,
  );
  assert.equal(isTmux33Version('tmux 3.3'), true);
  assert.equal(isTmux33Version('tmux 3.3a'), true);
  assert.equal(isTmux33Version('tmux 3.8'), false);
  assert.equal(isTmux33Version('tmux 3.30'), false);
  assert.equal(shouldExpectOuterMouseEvents({ shouldExpectMouseEvents: false }), false);
  const emptySteps = {
    hover: [],
    left_click: [],
    left_drag: [],
    wheel_down: [],
    wheel_up: [],
    right_click: [],
  };
  assertPointerStepResults(emptySteps, { shouldExpectMouseEvents: false });
  assert.throws(
    () =>
      assertPointerStepResults(
        { ...emptySteps, left_click: [{ event_type: 'mouse', mouse_kind: 'down:left' }] },
        { shouldExpectMouseEvents: false },
      ),
    /Mouse-disabled/,
  );
});

test('createTmuxSession enforces Rule 5 before starting any tmux server', async () => {
  const unreadyBridge = await createBridgeServer({ cols: 100, rows: 30 });
  try {
    await assert.rejects(
      () =>
        createTmuxSession({
          programPath: process.execPath,
          programArgs: ['-e', 'process.exit(0)'],
          tmuxMode: 'full',
          bridge: unreadyBridge,
        }),
      /Rule 5 violation/,
    );
    assert.deepEqual(listHarnessTmuxSockets(), []);
  } finally {
    await unreadyBridge.close();
  }
});

test('createTmuxSession captures child exit code, preserves args, reads buffer, and kills server', SKIP_ON_WIN32, async () => {
  const bridge = await startBrowserBridge({ cols: 100, rows: 30 });
  const scratchDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tuikit-tmux-unit-'));
  try {
    const argOutputFile = path.join(scratchDir, 'arg.txt');
    const childScript = [
      'const fs = require("node:fs");',
      'fs.writeFileSync(process.argv[1], process.argv[2], "utf8");',
      'const b64 = Buffer.from("buf-payload", "utf8").toString("base64");',
      'process.stdout.write(`\\x1b]52;c;${b64}\\x07`);',
      'process.exit(42);',
    ].join(' ');
    const session = await createTmuxSession({
      programPath: process.execPath,
      programArgs: ['-e', childScript, argOutputFile, 'hello world $1 "quoted"'],
      tmuxMode: 'full',
      bridge,
      scratchDir,
    });
    const socketName = session.socketName;
    const exitOutcome = await session.waitForExit(10000);
    assert.equal(exitOutcome.exitCode, 42);
    assert.equal(fs.readFileSync(argOutputFile, 'utf8'), 'hello world $1 "quoted"');
    assert.equal(session.readTmuxBuffer(), 'buf-payload');

    session.dispose();
    session.dispose();
    const listAfterDispose = spawnSync('tmux', ['-L', socketName, 'list-sessions'], {
      encoding: 'utf8',
    });
    assert.notEqual(listAfterDispose.status, 0);
  } finally {
    await bridge.close();
    fs.rmSync(scratchDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

test('run-tmux.sh stock capability_probe verifies M0 Rows 9 and 12 and clean teardown', SKIP_ON_WIN32, () => {
  const stockResult = runTmuxCli(['stock', 'capability_probe'], { timeoutMs: 90000 });
  assert.equal(stockResult.status, 0, `stderr:\n${stockResult.stderr}`);
  const stdoutLines = stockResult.stdout.trim().split('\n');
  assert.equal(stdoutLines.length, 1);

  const report = JSON.parse(stdoutLines[0]);
  assert.equal(report.layer, 4);
  assert.equal(report.tmuxConfig, 'stock');
  assertTmuxBatchReport(report.batch);
  assertStockTmuxInteractive(report.interactive);

  const expectedStockOsc52 = [
    ['plain', 'tuikit-tmux-osc52-plain'],
    ['tmux', 'tuikit-tmux-osc52-wrapped'],
    ['both', 'tuikit-tmux-osc52-both'],
  ];
  for (const [form, token] of expectedStockOsc52) {
    const probe = report.osc52.probes.find((item) => item.form === form);
    assert.equal(probe?.token, token);
    assert.equal(probe?.tmuxBuffer, null);
    assert.equal(probe?.osc52Payloads.length, 0);
  }

  assert.notEqual(report.panic.exitCode, 0);
  assert.equal(report.panic.terminalStateAfterPanic.mouseTrackingMode, 'none');
  assert.deepEqual(listHarnessTmuxSockets(), []);
});

test('run-tmux.sh full capability_probe verifies M0 Rows 10 and 12 and clean teardown', SKIP_ON_WIN32, () => {
  const fullResult = runTmuxCli(['full', 'capability_probe'], { timeoutMs: 90000 });
  assert.equal(fullResult.status, 0, `stderr:\n${fullResult.stderr}`);
  const stdoutLines = fullResult.stdout.trim().split('\n');
  assert.equal(stdoutLines.length, 1);

  const report = JSON.parse(stdoutLines[0]);
  assert.equal(report.layer, 4);
  assert.equal(report.tmuxConfig, 'full');
  assertTmuxBatchReport(report.batch);
  assertFullTmuxInteractive(report.interactive);

  const expectedFullOsc52 = [
    ['plain', 'tuikit-tmux-osc52-plain', 'tuikit-tmux-osc52-plain'],
    ['tmux', 'tuikit-tmux-osc52-wrapped', null],
    ['both', 'tuikit-tmux-osc52-both', 'tuikit-tmux-osc52-both'],
  ];
  for (const [form, token, expectedBuffer] of expectedFullOsc52) {
    const probe = report.osc52.probes.find((item) => item.form === form);
    assert.equal(probe?.token, token);
    assert.equal(probe?.tmuxBuffer, expectedBuffer);
    assert.ok(probe?.osc52Payloads.some((item) => item.decodedText === token));
  }

  assert.notEqual(report.panic.terminalStateBeforePanic.mouseTrackingMode, 'none');
  assert.notEqual(report.panic.exitCode, 0);
  assert.equal(report.panic.terminalStateAfterPanic.mouseTrackingMode, 'none');
  assert.deepEqual(listHarnessTmuxSockets(), []);
});

test('waitForReadyOuterTerminalModes awaits mouse on win32 and createTmuxSession cleans scratchDir', async () => {
  let waitCalls = 0;
  const page = { async waitForFunction() { waitCalls += 1; } };
  await waitForReadyOuterTerminalModes({ tmuxMode: null, timeoutMs: 500, bridge: { page } }, 'win32');
  assert.equal(waitCalls, 1, 'win32 must wait for ConPTY mouseTrackingMode when mouse is expected');
  await waitForReadyOuterTerminalModes(
    { shouldExpectMouseEvents: false, timeoutMs: 500, bridge: { page } },
    'win32',
  );
  assert.equal(waitCalls, 1, 'win32 must skip bracketedPaste wait when outer mouse is disabled');

  const beforeDirs = new Set(fs.readdirSync(os.tmpdir()).filter((entry) => entry.startsWith('tuikit-tmux-')));
  await assert.rejects(() =>
    createTmuxSession({
      programPath: process.execPath,
      tmuxMode: 'full',
      bridge: { isTerminalReady: true },
      envOverrides: { PATH: '/nonexistent' },
    }),
  );
  const afterDirs = fs.readdirSync(os.tmpdir()).filter((entry) => entry.startsWith('tuikit-tmux-'));
  assert.deepEqual(afterDirs.filter((entry) => !beforeDirs.has(entry)), []);
});
