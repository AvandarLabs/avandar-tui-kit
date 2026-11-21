import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  compareReportAgainstExpected,
  normalizeProbeReport,
} from '../src/expected_compare.mjs';

const TEST_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
const XTERM_ROOT = path.resolve(TEST_DIRECTORY, '..');
const REPOSITORY_ROOT = path.resolve(XTERM_ROOT, '..', '..');
const EXPECTED_DIR = path.join(REPOSITORY_ROOT, 'rig', 'expected');
const COMPARE_CLI_PATH = path.join(XTERM_ROOT, 'src', 'expected_compare.mjs');

const ALL_EXPECTED_BASELINE_NAMES = Object.freeze([
  'iterm2-tmux-cc.json', 'iterm2-tmux-full.json', 'iterm2-tmux-stock.json', 'iterm2.json',
  'orca-tmux.json', 'orca.json',
  'xterm-linux.json', 'xterm-macos.json',
  'xterm-tmux-full-linux.json', 'xterm-tmux-full-macos.json',
  'xterm-tmux-stock-linux.json', 'xterm-tmux-stock-macos.json',
  'xterm-windows.json',
]);

function createIdentityAndModeRecords(identityOverrides = {}) {
  return [
    {
      kind: 'section',
      section: 'identity',
      xtversion: 'xterm.js(6.0.0)',
      os: 'macos',
      arch: 'aarch64',
      term: 'xterm-256color',
      term_program: 'Orca',
      term_program_version: '1.99.999',
      is_tmux: false,
      is_ssh: true,
      has_no_color: true,
      orca_version: '0.1.0+abc',
      is_stdin_tty: true,
      is_stdout_tty: true,
      ...identityOverrides,
    },
    {
      kind: 'section',
      section: 'mouse_modes',
      is_m1_expected: true,
      is_m5_reset: true,
      after_m1: [{ mode: 1002, state: 'set' }, { mode: 1006, state: 'set' }],
      after_m5: [{ mode: 1002, state: 'reset' }, { mode: 1006, state: 'reset' }],
    },
    { kind: 'section', section: 'kitty_keyboard', answered: true, initial_flags: 0, pushed_flags: 1, popped_flags: 0 },
    {
      kind: 'section',
      section: 'modes',
      modes: [
        { mode: 2004, name: 'bracketed_paste', after_enable: 'set', after_disable: 'reset' },
        { mode: 1004, name: 'focus_events', after_enable: 'set', after_disable: 'reset' },
        { mode: 2026, name: 'synchronized_output', after_enable: 'set', after_disable: 'reset' },
      ],
    },
  ];
}

function createSampleBatchRecords(overrides = {}) {
  const widthTuples = [
    ['ascii_a', 'ascii_and_accents', 1],
    ['cjk_han', 'east_asian', 2],
    ['ellipsis', 'toolkit_glyph', 1],
    ['rainbow_flag', 'emoji_split', 2],
  ];
  return [
    ...createIdentityAndModeRecords(overrides.identity),
    { kind: 'section', section: 'cell_size', answered: false, height_px: null, width_px: null },
    {
      kind: 'section',
      section: 'widths',
      samples: widthTuples.map(([name, category, cells]) => ({ name, category, cells })),
    },
    { kind: 'section', section: 'background_color', answered: true, elapsed_ms: 42, is_dark: true },
    { kind: 'summary', mode: 'batch', exit_chord: 'Ctrl+C', panic_key: 'F12' },
  ];
}

function createMouseEvent(mouseKind, modifiers = {}) {
  return {
    event_type: 'mouse',
    mouse_kind: mouseKind,
    shift: Boolean(modifiers.shift),
    alt: Boolean(modifiers.alt),
    ctrl: Boolean(modifiers.ctrl),
  };
}

function createKeyPressEvent(code, modifiers = {}) {
  return {
    event_type: 'key',
    key_kind: 'press',
    code,
    shift: Boolean(modifiers.shift),
    alt: Boolean(modifiers.alt),
    ctrl: Boolean(modifiers.ctrl),
  };
}

function createSampleInteractiveSection() {
  return {
    pointerSteps: {
      hover: [],
      left_click: [createMouseEvent('down:left'), createMouseEvent('up:left')],
      left_drag: [createMouseEvent('down:left'), createMouseEvent('drag:left'), createMouseEvent('drag:left'), createMouseEvent('up:left')],
      wheel_down: [createMouseEvent('scroll_down')],
      wheel_up: [createMouseEvent('scroll_up')],
      right_click: [],
      shift_click: [createMouseEvent('down:left', { shift: true })],
      alt_click: [],
      alt_drag: [],
      ctrl_click: [],
      double_click: [createMouseEvent('down:left')],
    },
    keyboardSteps: {
      arrow_up: [createKeyPressEvent('up')],
      f1: [createKeyPressEvent('f1')],
      ctrl_a: [createKeyPressEvent('char:a', { ctrl: true })],
      alt_a: [],
      alt_enter: [createKeyPressEvent('enter', { alt: true })],
      shift_enter: [createKeyPressEvent('enter')],
      ctrl_enter: [createKeyPressEvent('enter')],
    },
    kittyEnterSteps: {
      alt_enter: [createKeyPressEvent('enter', { alt: true })],
      shift_enter: [createKeyPressEvent('enter', { shift: true })],
      ctrl_enter: [createKeyPressEvent('enter', { ctrl: true })],
    },
    pasteEvents: [{ event_type: 'paste', text: 'alpha\rbeta', burst_press_count: 0 }],
    focus: {
      focusLostEvents: [{ event_type: 'focus', focused: false }],
      focusGainedEvents: [{ event_type: 'focus', focused: true }],
    },
    terminalStateAfterExit: { mouseTrackingMode: 'none' },
  };
}

function runCompareCli(cliArgs, options = {}) {
  return spawnSync(process.execPath, [COMPARE_CLI_PATH, ...cliArgs], {
    cwd: REPOSITORY_ROOT,
    env: { ...process.env, NO_COLOR: '1', ...options.envOverrides },
    input: options.stdinText,
    encoding: 'utf8',
  });
}

test('normalizeProbeReport normalizes xterm, iterm2, and orca JSONL while stripping host noise', () => {
  const xtermReport = {
    layer: 3,
    platform: 'linux',
    arch: 'arm64',
    batch: { records: createSampleBatchRecords({ identity: { os: 'linux', arch: 'aarch64' } }), terminalStateAfterExit: { mouseTrackingMode: 'none' } },
    osc52: { probes: [{ form: 'plain', token: 'tok-1', osc52Payloads: [{ decodedText: 'tok-1' }], tmuxBuffer: null }] },
    interactive: createSampleInteractiveSection(),
    panic: { exitCode: 101, terminalStateAfterPanic: { mouseTrackingMode: 'none' } },
  };
  const x64Clone = structuredClone(xtermReport);
  x64Clone.arch = 'x64';
  x64Clone.batch.records[0].arch = 'x86_64';
  x64Clone.batch.records[0].has_no_color = false;
  x64Clone.batch.records[0].is_ssh = false;
  x64Clone.batch.records[6].elapsed_ms = 3;
  x64Clone.interactive.pointerSteps.left_drag.splice(2, 1);

  const normalizedArm = normalizeProbeReport(xtermReport);
  assert.deepEqual(normalizedArm, normalizeProbeReport(x64Clone));
  assert.equal('arch' in normalizedArm.identity, false);
  assert.deepEqual(normalizedArm.osc52, [
    { form: 'plain', updatedOuterClipboard: true, updatedTmuxBuffer: false },
  ]);

  const itermReport = {
    layer: 6,
    batch: { records: createSampleBatchRecords(), terminalStateAfterExit: { mouseTrackingMode: 'none' } },
    osc52: { probes: [{ form: 'plain', token: 'tok-2', clipboardUpdated: false, tmuxBuffer: 'tok-2' }] },
    interactive: createSampleInteractiveSection(),
    panic: { exitCode: 101, terminalStateAfterPanic: { mouseTrackingMode: 'none' } },
  };
  assert.deepEqual(normalizeProbeReport(itermReport).osc52, [
    { form: 'plain', updatedOuterClipboard: false, updatedTmuxBuffer: true },
  ]);

  const jsonlText = createSampleBatchRecords().map((rec) => JSON.stringify(rec)).join('\n') + '\n';
  const normalizedOrca = normalizeProbeReport(jsonlText);
  assert.equal(normalizedOrca.osc52, null);
  assert.equal(normalizedOrca.interactive, null);
  assert.deepEqual(normalizedOrca, normalizeProbeReport(createSampleBatchRecords()));
});

test('compareReportAgainstExpected detects regressions with dot-path diffs', () => {
  const baseRecords = createSampleBatchRecords();
  const baseline = normalizeProbeReport(baseRecords);
  assert.equal(compareReportAgainstExpected(baseRecords, baseline).isMatch, true);

  const regressed = structuredClone(baseline);
  regressed.mouse_modes.is_m1_expected = false;
  regressed.widths.samples.rainbow_flag = 5;
  const mismatch = compareReportAgainstExpected(regressed, baseline);
  assert.equal(mismatch.isMatch, false);
  assert.ok(mismatch.diffs.some((line) => line.includes('mouse_modes.is_m1_expected')));
  assert.ok(mismatch.diffs.some((line) => line.includes('widths.samples.rainbow_flag')));
});

test('expected_compare.mjs CLI enforces help, usage exit 2, normalize, stdin, and mismatch exit 1', () => {
  const helpRun = runCompareCli(['--help']);
  assert.equal(helpRun.status, 0);
  assert.equal(helpRun.stderr, '');
  assert.match(helpRun.stdout, /Usage:/);

  const badUsage = runCompareCli([]);
  assert.equal(badUsage.status, 2);
  assert.equal(badUsage.stdout, '');
  assert.match(badUsage.stderr, /\u274c/);
  assert.match(badUsage.stderr, /\n {2}\u26a0\ufe0f /);

  const scratchDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tuikit-expected-test-'));
  try {
    const rawJsonl = createSampleBatchRecords().map((rec) => JSON.stringify(rec)).join('\n') + '\n';
    const rawFile = path.join(scratchDir, 'orca.jsonl');
    const expectedFile = path.join(scratchDir, 'expected.json');
    fs.writeFileSync(rawFile, rawJsonl, 'utf8');

    const normalizedRun = runCompareCli(['--normalize', rawFile]);
    assert.equal(normalizedRun.status, 0);
    fs.writeFileSync(expectedFile, normalizedRun.stdout, 'utf8');

    const stdinMatch = runCompareCli(['-', expectedFile], { stdinText: rawJsonl });
    assert.equal(stdinMatch.status, 0);

    const brokenBaseline = JSON.parse(normalizedRun.stdout);
    brokenBaseline.kitty_keyboard.answered = false;
    fs.writeFileSync(expectedFile, `${JSON.stringify(brokenBaseline, null, 2)}\n`, 'utf8');
    const mismatchRun = runCompareCli([rawFile, expectedFile]);
    assert.equal(mismatchRun.status, 1);
    assert.match(mismatchRun.stderr, /kitty_keyboard\.answered/);
    assert.match(mismatchRun.stderr, /\u274c/);
    assert.match(mismatchRun.stderr, /\n {2}\u26a0\ufe0f /);
  } finally {
    fs.rmSync(scratchDir, { recursive: true, force: true });
  }
});

test('all 13 rig/expected baselines exist, have zero em dashes, and are canonical under normalization', () => {
  const actualFiles = fs.readdirSync(EXPECTED_DIR).sort();
  assert.deepEqual(actualFiles, ALL_EXPECTED_BASELINE_NAMES);

  for (const fileName of ALL_EXPECTED_BASELINE_NAMES) {
    const filePath = path.join(EXPECTED_DIR, fileName);
    const rawContent = fs.readFileSync(filePath, 'utf8');
    assert.equal(rawContent.includes('\u2014'), false, `${fileName} contains an em dash`);
    const parsedBaseline = JSON.parse(rawContent);
    const canonicalJson = `${JSON.stringify(normalizeProbeReport(parsedBaseline), null, 2)}\n`;
    assert.equal(rawContent, canonicalJson, `${fileName} is not canonically formatted`);
    assert.equal(compareReportAgainstExpected(parsedBaseline, parsedBaseline).isMatch, true);
  }
});
