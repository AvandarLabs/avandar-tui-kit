/** Platform-aware assertions over the laboratory's native log schema. */
import assert from 'node:assert/strict';
import { isTmux33Version } from './interactive_scenario.mjs';

const QUERY_MODES = [9, 1000, 1002, 1003, 1005, 1006, 1015, 1016];

/** Checks the direct outer tracking protocol and SGR encoding at readiness. */
export function assertMouseSetup(state, platform = process.platform) {
  assert.equal(state.mouseTrackingMode, platform === 'win32' ? 'any' : 'drag');
  assert.equal(state.mouseEncoding, 'SGR', 'outer mouse reports must use SGR encoding');
}

/** Rejects actionable hover and, on Unix, any raw mouse event in a fenced slice. */
export function assertMouseHover(records, platform = process.platform) {
  const mouseEvents = records.filter((record) => record.kind === 'event' && record.event?.kind === 'Mouse');
  if (platform !== 'win32') {
    assert.equal(mouseEvents.length, 0, 'Unix drag tracking must not emit bare motion');
  } else {
    assert.ok(mouseEvents.every((record) => record.event.mouse_kind === 'Moved'));
  }
  for (const record of records.filter((record) => record.kind === 'report')) {
    assert.equal(record.outcome.kind, 'Ignored', 'hover must not act or forward bytes');
    assert.equal(record.pressed_target, null, 'hover must not press a target');
    assert.equal(record.is_redraw_due, false, 'hover must not request redraw');
  }
  assert.equal(records.filter((record) => record.kind === 'selection').length, 0, 'hover must not change selection');
}

/** Checks pane mode replies, preserving ConPTY and tmux 3.3 query absence. */
export function assertMouseModeReplies(records, platform = process.platform, tmuxServerInfo = null) {
  for (const phase of ['setup', 'teardown']) {
    const replies = records.filter((record) => record.kind === 'mode_reply' && record.phase === phase);
    assert.equal(replies.length, QUERY_MODES.length, `missing ${phase} mode observations`);
    for (const mode of QUERY_MODES) {
      const reply = replies.find((record) => record.mode === mode);
      assert.ok(reply, `missing ${phase} mode ${mode}`);
      if (platform === 'win32' || isTmux33Version(tmuxServerInfo?.version ?? '')) {
        assert.equal(reply.is_absent, true, 'ConPTY and tmux 3.3 must record unanswered DECRQM queries');
        assert.equal(reply.state, null);
      } else if ([1000, 1002, 1003, 1006].includes(mode)) {
        assert.equal(reply.is_absent, false, `direct xterm must answer mode ${mode}`);
        const expectedState = phase === 'setup' && [1002, 1006].includes(mode) ? 1 : 2;
        assert.equal(reply.state, expectedState, `${phase} mode ${mode}`);
      }
    }
  }
}

/** Checks a fenced gesture's exact semantic evidence. */
export function assertMouseGesture(scenario, steps, platform = process.platform, tmuxServerInfo = null) {
  try {
    if (Number(scenario.slice(1)) < 10) assertSelectionGesture(scenario, steps);
    else assertPaneGesture(scenario, steps, platform, tmuxServerInfo);
  } catch (error) {
    if (error instanceof assert.AssertionError) {
      error.message = `${scenario.toUpperCase()}: ${error.message}`;
    }
    throw error;
  }
}

function outcomes(records, kind) {
  return records.filter((record) => record.kind === 'report' && record.outcome.kind === kind)
    .map((record) => record.outcome);
}

function assertCopy(records, target, text) {
  assert.deepEqual(outcomes(records, 'Copy'), [{ kind: 'Copy', target, text }]);
}

function assertPress(records, target, button = 'Left', modifiers = 'KeyModifiers(0x0)') {
  const presses = records.filter((record) => record.kind === 'report'
    && ['Pressed', 'Clicked'].includes(record.outcome.kind));
  assert.deepEqual(presses.map(({ outcome }) => ({
    kind: outcome.kind, target: outcome.target,
    button: outcome.press.button, modifiers: outcome.press.modifiers,
  })), ['Pressed', 'Clicked'].map((kind) => ({ kind, target, button, modifiers })));
}

function assertForward(records, sequences) {
  assert.deepEqual(outcomes(records, 'Forward'), sequences.map((sequence) => ({
    kind: 'Forward', target: 'right', bytes: [...Buffer.from(sequence)],
  })));
}

function assertHistory(records) {
  assert.ok(records.some((record) => record.kind === 'tick'), 'edge scrolling must tick');
  const selection = records.findLast((record) => record.kind === 'selection' && record.selection)?.selection;
  assert.equal(selection?.target, 'right');
  assert.equal(selection.range.start.line, 0);
  const lastLine = selection.range.end.line;
  assert.ok(lastLine >= 30 && lastLine < 100, 'selection must reach known offscreen history');
  const text = Array.from({ length: lastLine + 1 }, (_, index) =>
    `right history ${String(index).padStart(3, '0')} 界`).join('\n');
  assertCopy(records, 'right', text);
}

function assertModifierSelection(steps, platform, tmuxServerInfo) {
  assert.equal(outcomes(steps.drag, 'Forward').length, 0, 'modifier gesture must never forward');
  if (steps.modifier === 'Alt' && /^tmux 3\.8(?:$|[^0-9])/.test(tmuxServerInfo?.version ?? '')) {
    assertMouseKinds(steps.drag, ['Down(Left)', 'Up(Left)'], 'KeyModifiers(ALT)');
    assertPress(steps.drag, 'right', 'Left', 'KeyModifiers(ALT)');
    for (const { selection } of steps.drag.filter((record) => record.kind === 'selection' && record.selection)) {
      assert.equal(selection.stage, 'Pressed', 'missing Alt motion must not create a drag selection');
      assert.deepEqual(selection.range.start, selection.range.end);
    }
    assert.equal(outcomes(steps.drag, 'Copy').length, 0);
  } else if (steps.modifier === 'Shift' && platform !== 'darwin') {
    assert.equal(steps.nativeSelection, 'right', 'Shift must select exact native text');
    assert.equal(steps.drag.filter((record) => record.kind === 'selection' && record.selection).length, 0);
    assert.equal(outcomes(steps.drag, 'Copy').length, 0);
    const mouseEvents = steps.drag.filter((record) => record.kind === 'event' && record.event.kind === 'Mouse');
    assert.ok(mouseEvents.every((record) => platform === 'win32' && record.event.mouse_kind === 'Moved'));
    for (const record of steps.drag.filter((record) => record.kind === 'report')) {
      assert.equal(record.outcome.kind, 'Ignored');
    }
  } else {
    if (steps.modifier === 'Alt' && isTmux33Version(tmuxServerInfo?.version ?? '')) {
      assertMouseKinds(steps.drag, ['Down(Left)', 'Drag(Left)', 'Up(Left)'], 'KeyModifiers(ALT)');
    }
    assertCopy(steps.drag, 'right', 'right');
  }
}

function assertSelectionGesture(scenario, steps) {
  switch (scenario) {
    case 'x4':
      for (const target of ['left', 'right', 'tab', 'divider']) assertPress(steps[target], target);
      break;
    case 'x5':
      assertCopy(steps.drag, 'left', 'alpha\nbeta\ngamma');
      break;
    case 'x6':
      assertCopy(steps.word, 'left', 'file.rs:12');
      assertCopy(steps.line, 'left', 'soft wrap over three rows');
      break;
    case 'x7':
      assertHistory(steps.history);
      break;
    case 'x8':
      assert.deepEqual(outcomes(steps.left, 'Wheel').map(({ target, rows, columns }) => ({ target, rows, columns })),
        [{ target: 'left', rows: 3, columns: 0 }]);
      assert.deepEqual(outcomes(steps.right, 'Wheel').map(({ target, rows, columns }) => ({ target, rows, columns })),
        [{ target: 'right', rows: -3, columns: 0 }]);
      break;
    case 'x9': {
      assert.equal(outcomes(steps.wheel, 'Wheel').length, 0);
      assert.ok(outcomes(steps.wheel, 'Consumed').length > 0);
      const selection = steps.wheel.findLast((record) => record.kind === 'selection')?.selection;
      assert.equal(selection?.target, 'left');
      assert.notDeepEqual(selection.range.end, steps.beforeEnd);
      assert.deepEqual(selection.range.end, { line: 5, column: 4 });
      break;
    }
    default:
      assert.fail(`unknown selection scenario ${scenario}`);
  }
}

function assertPaneGesture(scenario, steps, platform, tmuxServerInfo) {
  switch (scenario) {
    case 'x10':
      assertForward(steps.click, ['\x1b[<0;3;3M', '\x1b[<0;3;3m']);
      assertForward(steps.drag, ['\x1b[<0;3;3M', '\x1b[<32;7;5M', '\x1b[<0;7;5m']);
      assertForward(steps.wheel, ['\x1b[<65;3;3M']);
      break;
    case 'x11':
      assertModifierSelection(steps, platform, tmuxServerInfo);
      break;
    case 'x12':
      assertForward(steps.wheel, ['\x1b[B\x1b[B\x1b[B']);
      break;
    case 'x13':
      assertCopy(steps.keyboard, 'left', 'file.rs:12');
      assert.equal(steps.keyboard.filter((record) => record.kind === 'event' && record.event.kind === 'Mouse').length, 0);
      break;
    case 'x14':
      assertPress(steps.prime, 'left');
      assertPress(steps.controlPrime, 'left');
      if (isTmux33Version(tmuxServerInfo?.version ?? '')) {
        assertAbsorbedClick(steps.right, 'Right', 'KeyModifiers(0x0)');
        assertAbsorbedClick(steps.control, 'Left', 'KeyModifiers(CONTROL)');
        break;
      }
      assertPress(steps.right, 'left', 'Right');
      assertPress(steps.control, 'right', 'Left', 'KeyModifiers(CONTROL)');
      break;
    default:
      assert.fail(`unknown gesture scenario ${scenario}`);
  }
}

function assertMouseKinds(records, kinds, modifiers) {
  const events = records.filter((record) => record.kind === 'event' && record.event.kind === 'Mouse');
  assert.deepEqual(events.map((record) => record.event.mouse_kind), kinds);
  assert.ok(events.every((record) => record.event.modifiers === modifiers), 'mouse modifiers must survive tmux');
}

function assertNoSelectionOrCopy(records) {
  assert.equal(records.filter((record) => record.kind === 'selection' && record.selection).length, 0);
  assert.equal(outcomes(records, 'Copy').length, 0);
  assert.equal(outcomes(records, 'Forward').length, 0);
}

function assertAbsorbedClick(records, button, modifiers) {
  assertMouseKinds(records, [`Up(${button})`], modifiers);
  assertNoSelectionOrCopy(records);
  assert.equal(outcomes(records, 'Pressed').length, 0);
  assert.equal(outcomes(records, 'Clicked').length, 0);
}
