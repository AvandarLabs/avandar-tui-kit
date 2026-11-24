import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { CliUsageError, parseHarnessArgs } from '../src/args.mjs';
import * as harness from '../src/harness.mjs';
import { describeChildOutcome } from '../src/log_archive.mjs';

const harnessPath = fileURLToPath(new URL('../src/harness.mjs', import.meta.url));

test('mouse lab defaults and explicit scenario names preserve forwarded arguments', () => {
  assert.equal(parseHarnessArgs(['mouse_lab']).scenario, 'mouse');
  for (let scenarioNumber = 1; scenarioNumber <= 14; scenarioNumber += 1) {
    const scenario = `x${scenarioNumber}`;
    const parsed = parseHarnessArgs(['mouse_lab', scenario, '--', '--query-modes']);
    assert.equal(parsed.scenario, scenario);
    assert.deepEqual(parsed.extraArgs, ['--query-modes']);
  }
  assert.equal(parseHarnessArgs(['capability_probe']).scenario, 'all');
  assert.equal(parseHarnessArgs(['custom_example']).scenario, 'passthrough');
  for (const program of ['capability_probe', 'custom_example']) {
    for (const scenario of ['mouse', 'x1', 'x14']) {
      assert.throws(() => parseHarnessArgs([program, scenario]), CliUsageError);
    }
  }
});

test('mouse example build enables only its required feature and keeps probe builds unchanged', () => {
  assert.equal(typeof harness.buildCargoExampleArgs, 'function');
  assert.deepEqual(harness.buildCargoExampleArgs('mouse_lab', '/repo/Cargo.toml'), [
    'build', '--manifest-path', '/repo/Cargo.toml', '-p', 'ratatui-mouse',
    '--example', 'mouse_lab', '--features', 'vt100',
  ]);
  assert.deepEqual(harness.buildCargoExampleArgs('capability_probe', '/repo/Cargo.toml'), [
    'build', '--manifest-path', '/repo/Cargo.toml', '--workspace', '--example', 'capability_probe',
  ]);
});

test('mouse help precedes invalid arguments and build checks outside a repository', () => {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'mouse-help-'));
  try {
    const outcome = spawnSync(process.execPath, [harnessPath, 'invalid', 'x99', '--help'], {
      cwd: scratch, encoding: 'utf8', env: { ...process.env, NO_COLOR: '1', RIG_XTERM_PROGRAM_BIN: '/missing' },
    });
    assert.equal(outcome.status, 0);
    assert.match(outcome.stdout, /mouse_lab/);
    assert.match(outcome.stdout, /x1.*x14/);
    assert.match(outcome.stdout, /default:.*mouse/);
    assert.equal(outcome.stderr, '');
    assert.doesNotMatch(outcome.stdout, /\x1b\[/);
    assert.deepEqual(fs.readdirSync(scratch), []);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
});

test('invalid lab scenario use prints usage and recovery only to stderr', () => {
  const outcome = spawnSync(process.execPath, [harnessPath, 'capability_probe', 'x1'], {
    encoding: 'utf8', env: { ...process.env, NO_COLOR: '1' },
  });
  assert.equal(outcome.status, 2);
  assert.equal(outcome.stdout, '');
  assert.match(outcome.stderr, /Usage:/);
  assert.match(outcome.stderr, /\n  ⚠️/);
  assert.match(outcome.stderr, /mouse_lab/);
  assert.doesNotMatch(outcome.stderr, /\x1b\[/);
});

const mouseAssertions = await import('../src/mouse_assertions.mjs');

test('lifecycle assertions reject wrong Unix or Windows tracking and non-SGR encoding', () => {
  assert.equal(typeof mouseAssertions.assertMouseSetup, 'function');
  mouseAssertions.assertMouseSetup({ mouseTrackingMode: 'drag', mouseEncoding: 'SGR' }, 'darwin');
  mouseAssertions.assertMouseSetup({ mouseTrackingMode: 'any', mouseEncoding: 'SGR' }, 'win32');
  for (const [state, platform] of [
    [{ mouseTrackingMode: 'any', mouseEncoding: 'SGR' }, 'linux'],
    [{ mouseTrackingMode: 'drag', mouseEncoding: 'SGR' }, 'win32'],
    [{ mouseTrackingMode: 'drag', mouseEncoding: 'DEFAULT' }, 'darwin'],
  ]) {
    assert.throws(() => mouseAssertions.assertMouseSetup(state, platform), assert.AssertionError);
  }
});

test('hover assertions reject raw Unix motion and actionable Windows motion', () => {
  assert.equal(typeof mouseAssertions.assertMouseHover, 'function');
  const moved = { kind: 'event', event: { kind: 'Mouse', mouse_kind: 'Moved' } };
  const ignored = { kind: 'report', outcome: { kind: 'Ignored' }, pressed_target: null, is_redraw_due: false };
  mouseAssertions.assertMouseHover([], 'darwin');
  mouseAssertions.assertMouseHover([moved, ignored], 'win32');
  assert.throws(() => mouseAssertions.assertMouseHover([moved, ignored], 'linux'), assert.AssertionError);
  for (const actionable of [
    { ...ignored, outcome: { kind: 'Forward', bytes: [27] } },
    { ...ignored, pressed_target: 'right' },
    { ...ignored, is_redraw_due: true },
    { kind: 'selection', selection: { target: 'right' } },
  ]) {
    assert.throws(() => mouseAssertions.assertMouseHover([moved, actionable], 'win32'), assert.AssertionError);
  }
});

test('queried mode assertions distinguish missing Windows replies from Unix resets', () => {
  assert.equal(typeof mouseAssertions.assertMouseModeReplies, 'function');
  const modes = [9, 1000, 1002, 1003, 1005, 1006, 1015, 1016];
  const records = ['setup', 'teardown'].flatMap((phase) => modes.map((mode) => ({
    kind: 'mode_reply', phase, mode, is_absent: false,
    state: phase === 'setup' && [1002, 1006].includes(mode) ? 1 : 2,
  })));
  mouseAssertions.assertMouseModeReplies(records, 'linux');
  assert.throws(() => mouseAssertions.assertMouseModeReplies([], 'linux'), assert.AssertionError);
  const absent = records.map((record) => ({ ...record, state: null, is_absent: true }));
  mouseAssertions.assertMouseModeReplies(absent, 'win32');
  assert.throws(() => mouseAssertions.assertMouseModeReplies(absent, 'linux'), assert.AssertionError);
  assert.throws(() => mouseAssertions.assertMouseModeReplies(records, 'win32'), assert.AssertionError);
});

test('terminal state observes actual outer SGR encoding independently of mode replies', async () => {
  const { startBrowserBridge } = await import('../src/bridge_server.mjs');
  const bridge = await startBrowserBridge();
  try {
    await bridge.writeAndFlush('\x1b[?1002h\x1b[?1006h');
    const state = await bridge.getTerminalState();
    assert.equal(state.mouseTrackingMode, 'drag');
    assert.equal(state.mouseEncoding, 'SGR');
    await bridge.writeAndFlush('\x1b[?1006l');
    assert.equal((await bridge.getTerminalState()).mouseEncoding, 'DEFAULT');
  } finally {
    await bridge.close();
  }
});


test('live mouse lab covers X1 through X14 with independent modifier sessions and keys-only copy', () => {
  const outcome = spawnSync(process.execPath, [harnessPath, 'mouse_lab'], {
    encoding: 'utf8', timeout: 180000,
    env: { ...process.env, NO_COLOR: '1', CARGO_TERM_COLOR: 'always', RIG_XTERM_PROGRAM_BIN: '' },
  });
  assert.equal(outcome.status, 0, describeChildOutcome(outcome, { label: 'mouse-direct' }));
  const report = JSON.parse(outcome.stdout);
  assert.equal(report.program, 'mouse_lab');
  assert.equal(report.scenario, 'mouse');
  assert.deepEqual(Object.keys(report.scenarios), Array.from({ length: 14 }, (_, index) => `x${index + 1}`));
  for (const scenario of Object.values(report.scenarios).flatMap((scenario) => scenario.sessions ? Object.values(scenario.sessions) : [scenario])) {
    assert.equal(scenario.terminalStateAtReady.mouseEncoding, 'SGR');
    assert.equal(scenario.terminalStateAfterExit.mouseTrackingMode, 'none');
    assert.ok(scenario.records.some((record) => record.kind === 'ready'));
    assert.ok(scenario.records.every((record, index) => record.sequence === index + 1));
  }
  assert.equal(report.scenarios.x1.exitCode, 0);
  assert.notEqual(report.scenarios.x2.exitCode, 0);
  assert.equal(report.scenarios.x3.exitCode, 0);
  mouseAssertions.assertMouseHover(report.scenarios.x3.pointerSteps.hover);
  mouseAssertions.assertMouseHover(report.scenarios.x3.pointerSteps.hoverPassThrough);
  assert.ok(report.scenarios.x3.pointerSteps.paneModeRecords.some((record) => record.label === 'pane_modes'));
  if (process.platform !== 'win32') {
    assert.deepEqual(report.scenarios.x3.pointerSteps.hoverPassThrough, []);
  }
  assert.equal(report.scenarios.x11.sessions.shift.assertion.modifier, 'Shift');
  assert.equal(report.scenarios.x11.sessions.alt.assertion.modifier, 'Alt');
  assert.equal(report.scenarios.x13.records.filter((record) => record.kind === 'event'
    && record.event.kind === 'Mouse').length, 0);
  assert.doesNotMatch(outcome.stderr, /\x1b\[/);
});

const outcomeRecord = (outcome) => ({ kind: 'report', outcome });
const copyRecords = (target, text) => [outcomeRecord({ kind: 'Copy', target, text })];
const pressRecords = (target, button = 'Left', modifiers = 'KeyModifiers(0x0)') =>
  ['Pressed', 'Clicked'].map((kind) => outcomeRecord({ kind, target, press: { button, modifiers } }));
const forwardRecords = (...sequences) => sequences.map((bytes) => outcomeRecord({
  kind: 'Forward', target: 'right', bytes: [...Buffer.from(bytes)],
}));
const historyText = Array.from({ length: 31 }, (_, index) => `right history ${String(index).padStart(3, '0')} 界`).join('\n');
const gestureCases = [
  ['x4', Object.fromEntries(['left', 'right', 'tab', 'divider'].map((target) => [target, pressRecords(target)])),
    (steps) => { steps.right[1].outcome.target = 'left'; }],
  ['x5', { drag: copyRecords('left', 'alpha\nbeta\ngamma') },
    (steps) => { steps.drag[0].outcome.text = 'alpha\nbeta\ngamm'; }],
  ['x6', { word: copyRecords('left', 'file.rs:12'), line: copyRecords('left', 'soft wrap over three rows') },
    (steps) => { steps.line[0].outcome.text = 'soft wrap\nover three\nrows'; }],
  ['x7', { history: [{ kind: 'tick' }, { kind: 'selection', selection: { target: 'right', range: { start: { line: 0 }, end: { line: 30 } } } }, ...copyRecords('right', historyText)] },
    (steps) => { steps.history.at(-1).outcome.text = 'right history 030 界'; }],
  ['x8', { left: [outcomeRecord({ kind: 'Wheel', target: 'left', rows: 3, columns: 0 })], right: [outcomeRecord({ kind: 'Wheel', target: 'right', rows: -3, columns: 0 })] },
    (steps) => { steps.right[0].outcome.rows = 3; }],
  ['x9', { wheel: [{ kind: 'selection', selection: { target: 'left', range: { start: { line: 0, column: 0 }, end: { line: 5, column: 4 } } } }, outcomeRecord({ kind: 'Consumed' })], beforeEnd: { line: 2, column: 4 } },
    (steps) => { steps.wheel.push(outcomeRecord({ kind: 'Wheel', target: 'left', rows: 3 })); }],
  ['x10', { click: forwardRecords('\x1b[<0;3;3M', '\x1b[<0;3;3m'), drag: forwardRecords('\x1b[<0;3;3M', '\x1b[<32;7;5M', '\x1b[<0;7;5m'), wheel: forwardRecords('\x1b[<65;3;3M') },
    (steps) => { steps.click[0].outcome.bytes = [...Buffer.from('\x1b[<0;54;5M')]; }],
  ['x11', { modifier: 'Alt', drag: copyRecords('right', 'right') },
    (steps) => { steps.drag.push(...forwardRecords('\x1b[<0;1;1M')); }],
  ['x12', { wheel: forwardRecords('\x1b[B\x1b[B\x1b[B') },
    (steps) => { steps.wheel[0].outcome.bytes = [...Buffer.from('\x1bOB\x1bOB\x1bOB')]; }],
  ['x13', { keyboard: copyRecords('left', 'file.rs:12') },
    (steps) => { steps.keyboard = []; }],
  ['x14', { prime: pressRecords('left'), controlPrime: pressRecords('left'), right: pressRecords('left', 'Right'), control: pressRecords('right', 'Left', 'KeyModifiers(CONTROL)') },
    (steps) => { steps.control[1].outcome.press.modifiers = 'KeyModifiers(0x0)'; }],
];

for (const [scenario, validSteps, corrupt] of gestureCases) {
  test(`${scenario} rejects deliberately wrong semantic gesture reports with its scenario ID`, () => {
    mouseAssertions.assertMouseGesture(scenario, validSteps, 'darwin');
    const wrongSteps = structuredClone(validSteps);
    corrupt(wrongSteps);
    assert.throws(() => mouseAssertions.assertMouseGesture(scenario, wrongSteps, 'darwin'),
      (error) => error instanceof assert.AssertionError && error.message.includes(scenario.toUpperCase()));
  });
}

test('X11 reserves Shift for native selection on Linux and Windows and rejects app selection', () => {
  for (const platform of ['linux', 'win32']) {
    mouseAssertions.assertMouseGesture('x11', { modifier: 'Shift', nativeSelection: 'right', drag: [] }, platform);
    assert.throws(() => mouseAssertions.assertMouseGesture('x11', {
      modifier: 'Shift', nativeSelection: 'right', drag: [{ kind: 'selection', selection: { target: 'right' } }],
    }, platform), /X11/);
  }
});


test('X11 requires native Shift selection and X14 preserves each button', () => {
  assert.throws(() => mouseAssertions.assertMouseGesture('x11', {
    modifier: 'Shift', nativeSelection: '', drag: [],
  }, 'linux'), /X11/);
  const wrongButton = structuredClone(gestureCases.find(([scenario]) => scenario === 'x14')[1]);
  wrongButton.right[1].outcome.press.button = 'Left';
  assert.throws(() => mouseAssertions.assertMouseGesture('x14', wrongButton), /X14/);
});


const mouseEvents = (kinds, modifiers = 'KeyModifiers(ALT)') => kinds.map((mouse_kind) => ({
  kind: 'event', event: { kind: 'Mouse', mouse_kind, modifiers },
}));

function assertTmuxAltMotion(tmux38, tmux33) {
  const altWithoutMotion = { modifier: 'Alt', drag: [
    ...mouseEvents(['Down(Left)', 'Up(Left)']), ...pressRecords('right', 'Left', 'KeyModifiers(ALT)'),
    { kind: 'selection', selection: { target: 'right', stage: 'Pressed',
      range: { start: { line: 0, column: 0 }, end: { line: 0, column: 0 } } } },
  ] };
  mouseAssertions.assertMouseGesture('x11', altWithoutMotion, 'darwin', tmux38);
  assert.throws(() => mouseAssertions.assertMouseGesture('x11', altWithoutMotion, 'linux', tmux33), /X11/);
  const altWithMotion = { modifier: 'Alt', drag: [
    ...mouseEvents(['Down(Left)', 'Drag(Left)', 'Up(Left)']), ...copyRecords('right', 'right'),
  ] };
  mouseAssertions.assertMouseGesture('x11', altWithMotion, 'linux', tmux33);
  assert.throws(() => mouseAssertions.assertMouseGesture('x11', altWithMotion, 'darwin', tmux38), /X11/);
  const missingAltMotion = { ...altWithMotion, drag: altWithMotion.drag.filter((record) => record.event?.mouse_kind !== 'Drag(Left)') };
  assert.throws(() => mouseAssertions.assertMouseGesture('x11', missingAltMotion, 'linux', tmux33), /X11/);
}

function assertTmuxQueryAbsence(tmux38, tmux33) {
  const absent = ['setup', 'teardown'].flatMap((phase) =>
    [9, 1000, 1002, 1003, 1005, 1006, 1015, 1016].map((mode) => ({
      kind: 'mode_reply', phase, mode, state: null, is_absent: true,
    })));
  mouseAssertions.assertMouseModeReplies(absent, 'linux', tmux33);
  assert.throws(() => mouseAssertions.assertMouseModeReplies(absent, 'darwin', tmux38), assert.AssertionError);
  const falseReset = absent.map((record) => ({ ...record, state: 2 }));
  assert.throws(() => mouseAssertions.assertMouseModeReplies(falseReset, 'linux', tmux33), assert.AssertionError);
}

test('tmux_mouse_expectations_follow_measured_version', () => {
  for (const config of ['stock', 'full']) {
    const tmux38 = { version: 'tmux 3.8', isMouseEnabled: true, config };
    const tmux33 = { version: 'tmux 3.3a', isMouseEnabled: config === 'full', config };
    // Both configurations must retain positive, exact pane forwarding assertions.
    const paneSteps = gestureCases.find(([scenario]) => scenario === 'x10')[1];
    mouseAssertions.assertMouseGesture('x10', paneSteps, 'linux', tmux33);
    assert.throws(() => mouseAssertions.assertMouseGesture('x10', {
      ...paneSteps, click: [],
    }, 'linux', tmux33), /X10/);
    assertTmuxAltMotion(tmux38, tmux33);
    const upOnly = {
      prime: pressRecords('left'), controlPrime: pressRecords('left'),
      right: [...mouseEvents(['Up(Right)'], 'KeyModifiers(0x0)'), outcomeRecord({ kind: 'Ignored', target: null })],
      control: [...mouseEvents(['Up(Left)'], 'KeyModifiers(CONTROL)'), outcomeRecord({ kind: 'Ignored', target: null })],
    };
    mouseAssertions.assertMouseGesture('x14', upOnly, 'linux', tmux33);
    assert.throws(() => mouseAssertions.assertMouseGesture('x14', upOnly, 'darwin', tmux38), /X14/);
    assert.throws(() => mouseAssertions.assertMouseGesture('x14', { ...upOnly, prime: [] }, 'linux', tmux33), /X14/);
    const phantomClick = structuredClone(upOnly);
    phantomClick.right.push(...pressRecords('left', 'Right'));
    assert.throws(() => mouseAssertions.assertMouseGesture('x14', phantomClick, 'linux', tmux33), /X14/);
    assert.throws(() => mouseAssertions.assertMouseGesture('x14', {
      ...upOnly, right: [],
    }, 'linux', tmux33), /X14/);
    assertTmuxQueryAbsence(tmux38, tmux33);
    assert.throws(() => mouseAssertions.assertMouseGesture('x13', { keyboard: [] }, 'linux', tmux33), /X13/);
  }
});


for (const config of ['stock', 'full']) {
  test(`live ${config} tmux mouse matrix executes all X1-X14 including degradation and keyboard Copy`,
    { skip: process.platform === 'win32' }, () => {
      const tmuxHarnessPath = fileURLToPath(new URL('../src/tmux_session.mjs', import.meta.url));
      const outcome = spawnSync(process.execPath, [tmuxHarnessPath, config, 'mouse_lab', 'mouse'], {
        encoding: 'utf8', timeout: 180000, env: { ...process.env, NO_COLOR: '1' },
      });
      assert.equal(outcome.status, 0, describeChildOutcome(outcome, { label: `mouse-tmux-${config}` }));
      const report = JSON.parse(outcome.stdout);
      assert.equal(report.layer, 4);
      assert.equal(report.tmuxConfig, config);
      assert.deepEqual(Object.keys(report.scenarios), Array.from({ length: 14 }, (_, index) => `x${index + 1}`));
      for (const scenario of Object.values(report.scenarios).flatMap((scenario) =>
        scenario.sessions ? Object.values(scenario.sessions) : [scenario])) {
        assert.equal(scenario.assertion.passed, true);
        assert.match(scenario.tmuxServerInfo.version, /^tmux /);
        assert.equal(scenario.terminalStateAfterExit.mouseTrackingMode, 'none');
      }
      assert.deepEqual(report.scenarios.x13.pointerSteps.keyboard.filter((record) =>
        record.kind === 'report' && record.outcome.kind === 'Copy').map((record) => record.outcome.text), ['file.rs:12']);
    });
}
