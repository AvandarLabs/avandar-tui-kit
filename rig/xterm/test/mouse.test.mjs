import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { CliUsageError, parseHarnessArgs } from '../src/args.mjs';
import * as harness from '../src/harness.mjs';

const harnessPath = new URL('../src/harness.mjs', import.meta.url);

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
    const outcome = spawnSync(process.execPath, [harnessPath.pathname, 'invalid', 'x99', '--help'], {
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
  const outcome = spawnSync(process.execPath, [harnessPath.pathname, 'capability_probe', 'x1'], {
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


test('live mouse lab lifecycle restores clean and panic sessions and fences both hover paths', () => {
  const outcome = spawnSync(process.execPath, [harnessPath.pathname, 'mouse_lab'], {
    encoding: 'utf8', timeout: 90000, env: { ...process.env, NO_COLOR: '1' },
  });
  assert.equal(outcome.status, 0, outcome.stderr);
  const report = JSON.parse(outcome.stdout);
  assert.equal(report.program, 'mouse_lab');
  assert.equal(report.scenario, 'mouse');
  assert.deepEqual(Object.keys(report.scenarios), ['x1', 'x2', 'x3']);
  for (const scenario of Object.values(report.scenarios)) {
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
  assert.doesNotMatch(outcome.stderr, /\x1b\[/);
});
