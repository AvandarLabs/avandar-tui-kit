/**
 * Interactive Layer 3 / Layer 4 scenario driver using condition-based FIFO `F2` key fences.
 */

import assert from 'node:assert/strict';

import {
  clickCell,
  dragAcrossCells,
  movePointerAcrossCells,
  wheelAtCell,
} from './cell_coords.mjs';
import { readCompleteJsonLines, readJsonLines, waitForLogRecords, waitForProbeReady } from './ready_waiter.mjs';

function isFenceKeyRecord(record) {
  return record?.kind === 'event' && record.event_type === 'key' && record.code === 'f2';
}

function isFencePressRecord(record) {
  return isFenceKeyRecord(record) && record.key_kind === 'press';
}

async function sendFenceKey(page, shouldPreserveBlur) {
  if (!shouldPreserveBlur) {
    await page.keyboard.press('F2');
    return;
  }
  await page.evaluate(() => {
    const helperTextarea = document.querySelector('.xterm-helper-textarea');
    const init = { key: 'F2', code: 'F2', keyCode: 113, which: 113, bubbles: true, cancelable: true };
    helperTextarea?.dispatchEvent(new KeyboardEvent('keydown', init));
  });
}

function waitForFenceSlice(stepContext, startRecordCount, stepLabel) {
  return waitForLogRecords(
    stepContext.logPath,
    (records) => {
      for (let index = startRecordCount; index < records.length; index += 1) {
        if (isFencePressRecord(records[index])) {
          return records
            .slice(startRecordCount, index)
            .filter((record) => record.kind === 'event' && !isFenceKeyRecord(record));
        }
      }
      return null;
    },
    {
      timeoutMs: stepContext.timeoutMs,
      processExitPromise: stepContext.session.exitPromise,
      description: `F2 fence for interactive step '${stepLabel}'`,
    },
  );
}

async function runFencedStep(
  stepContext,
  stepLabel,
  performAction,
  { shouldPreserveBlur = false } = {},
) {
  const startRecordCount = readCompleteJsonLines(stepContext.logPath).length;
  await performAction();
  await sendFenceKey(stepContext.bridge.page, shouldPreserveBlur);
  return waitForFenceSlice(stepContext, startRecordCount, stepLabel);
}

function hasMouseRecord(records, mouseKind, column, row) {
  return records.some(
    (record) =>
      record.event_type === 'mouse' &&
      record.mouse_kind === mouseKind &&
      (column === null || record.column === column) &&
      (row === null || record.row === row),
  );
}

export function shouldExpectOuterMouseEvents(stepContext) {
  if (stepContext.tmuxMode === null) {
    return true;
  }
  if (typeof stepContext.session?.tmuxServerInfo?.isMouseEnabled === 'boolean') {
    return stepContext.session.tmuxServerInfo.isMouseEnabled;
  }
  return stepContext.tmuxMode === 'full';
}

/**
 * Verifies pointer step logs for either mouse-disabled stock tmux (`< 3.8`) or mouse-enabled runs.
 */
export function assertPointerStepResults(pointerSteps, { shouldExpectMouseEvents = true } = {}) {
  if (process.platform !== 'win32') {
    const hoverMovedCount = pointerSteps.hover.filter((record) => record.mouse_kind === 'moved').length;
    assert.equal(hoverMovedCount, 0, 'Rule M1 mode 1002 must not emit bare pointer-move events');
  }
  if (!shouldExpectMouseEvents) {
    assert.deepEqual(pointerSteps.left_click, [], 'Stock tmux left_click must log empty array');
    assert.deepEqual(pointerSteps.left_drag, [], 'Stock tmux left_drag must log empty array');
    assert.deepEqual(pointerSteps.wheel_down, [], 'Stock tmux wheel_down must log empty array');
    assert.deepEqual(pointerSteps.wheel_up, [], 'Stock tmux wheel_up must log empty array');
    const allMouseCount = Object.values(pointerSteps)
      .flat()
      .filter((record) => record.event_type === 'mouse').length;
    assert.equal(allMouseCount, 0, 'Stock tmux (mouse off) must forward zero mouse events');
    return;
  }
  assert.ok(hasMouseRecord(pointerSteps.left_click, 'down:left', 10, 5), 'missing left_click down');
  assert.ok(hasMouseRecord(pointerSteps.left_click, 'up:left', 10, 5), 'missing left_click up');
  assert.ok(hasMouseRecord(pointerSteps.left_drag, 'down:left', 12, 6), 'missing left_drag down');
  assert.ok(hasMouseRecord(pointerSteps.left_drag, 'drag:left', null, null), 'missing left_drag');
  assert.ok(hasMouseRecord(pointerSteps.left_drag, 'up:left', 18, 6), 'missing left_drag up');
  assert.ok(hasMouseRecord(pointerSteps.wheel_down, 'scroll_down', 10, 5), 'missing wheel_down');
  assert.ok(hasMouseRecord(pointerSteps.wheel_up, 'scroll_up', 10, 5), 'missing wheel_up');
  assert.ok(hasMouseRecord(pointerSteps.right_click, 'down:right', 15, 7), 'missing right down');
  assert.ok(hasMouseRecord(pointerSteps.right_click, 'up:right', 15, 7), 'missing right up');
}

async function runBasicPointerSteps(stepContext, terminalState) {
  const page = stepContext.bridge.page;
  return {
    hover: await runFencedStep(stepContext, 'hover', () =>
      movePointerAcrossCells(page, terminalState, { column: 2, row: 2 }, { column: 6, row: 2 }),
    ),
    left_click: await runFencedStep(stepContext, 'left_click', () =>
      clickCell(page, terminalState, 10, 5),
    ),
    left_drag: await runFencedStep(stepContext, 'left_drag', () =>
      dragAcrossCells(page, terminalState, { column: 12, row: 6 }, { column: 18, row: 6 }),
    ),
    wheel_down: await runFencedStep(stepContext, 'wheel_down', () =>
      wheelAtCell(page, terminalState, 10, 5, 0, 100),
    ),
    wheel_up: await runFencedStep(stepContext, 'wheel_up', () =>
      wheelAtCell(page, terminalState, 10, 5, 0, -100),
    ),
    right_click: await runFencedStep(stepContext, 'right_click', () =>
      clickCell(page, terminalState, 15, 7, { button: 'right' }),
    ),
  };
}

async function runModifiedPointerSteps(stepContext, terminalState) {
  const page = stepContext.bridge.page;
  return {
    shift_click: await runFencedStep(stepContext, 'shift_click', () =>
      clickCell(page, terminalState, 11, 5, { modifiers: ['Shift'] }),
    ),
    alt_click: await runFencedStep(stepContext, 'alt_click', () =>
      clickCell(page, terminalState, 12, 5, { modifiers: ['Alt'] }),
    ),
    alt_drag: await runFencedStep(stepContext, 'alt_drag', () =>
      dragAcrossCells(
        page,
        terminalState,
        { column: 12, row: 8 },
        { column: 16, row: 8 },
        { modifiers: ['Alt'] },
      ),
    ),
    ctrl_click: await runFencedStep(stepContext, 'ctrl_click', () =>
      clickCell(page, terminalState, 13, 5, { modifiers: ['Control'] }),
    ),
    double_click: await runFencedStep(stepContext, 'double_click', () =>
      clickCell(page, terminalState, 14, 5, { clickCount: 2 }),
    ),
  };
}

async function runPointerSteps(stepContext, terminalState) {
  const basicSteps = await runBasicPointerSteps(stepContext, terminalState);
  const modifiedSteps = await runModifiedPointerSteps(stepContext, terminalState);
  const pointerSteps = { ...basicSteps, ...modifiedSteps };
  assertPointerStepResults(pointerSteps, {
    shouldExpectMouseEvents: shouldExpectOuterMouseEvents(stepContext),
  });
  return pointerSteps;
}

async function runKeyboardSteps(stepContext) {
  const page = stepContext.bridge.page;
  const chordTable = [
    ['arrow_up', 'ArrowUp'],
    ['f1', 'F1'],
    ['ctrl_a', 'Control+a'],
    ['alt_a', 'Alt+a'],
    ['alt_enter', 'Alt+Enter'],
    ['shift_enter', 'Shift+Enter'],
    ['ctrl_enter', 'Control+Enter'],
  ];
  const keyboardSteps = {};
  for (const [stepName, keyChord] of chordTable) {
    keyboardSteps[stepName] = await runFencedStep(stepContext, stepName, () =>
      page.keyboard.press(keyChord),
    );
  }
  assert.ok(keyboardSteps.arrow_up.some((record) => record.code === 'up'), 'missing ArrowUp key');
  assert.ok(keyboardSteps.f1.some((record) => record.code === 'f1'), 'missing F1 key');
  assert.ok(
    keyboardSteps.ctrl_a.some((record) => record.code === 'char:a' && record.ctrl === true),
    'missing Ctrl+a key',
  );
  return keyboardSteps;
}

const EXPECTED_PASTE_KEY_CODES = Object.freeze([
  'char:a', 'char:l', 'char:p', 'char:h', 'char:a', 'char:b', 'char:e', 'char:t', 'char:a',
]);

/**
 * Returns `true` once `recordsAfterStart` contains either a bracketed `paste` event or the full
 * `'alpha'` + `'beta'` key-press sequence emitted when bracketed paste mode is disabled.
 */
export function hasDeliveredPastePayload(recordsAfterStart) {
  let matchedKeyCount = 0;
  for (const record of recordsAfterStart) {
    if (record?.kind !== 'event') {
      continue;
    }
    if (record.event_type === 'paste') {
      return true;
    }
    if (
      record.event_type === 'key' &&
      record.key_kind === 'press' &&
      record.code === EXPECTED_PASTE_KEY_CODES[matchedKeyCount]
    ) {
      matchedKeyCount += 1;
      if (matchedKeyCount === EXPECTED_PASTE_KEY_CODES.length) {
        return true;
      }
    }
  }
  return false;
}

async function runPasteStep(stepContext, terminalStateAtReady) {
  const startRecordCount = readCompleteJsonLines(stepContext.logPath).length;
  await stepContext.bridge.pasteText('alpha\nbeta');
  await waitForLogRecords(
    stepContext.logPath,
    (records) => (hasDeliveredPastePayload(records.slice(startRecordCount)) ? true : null),
    {
      timeoutMs: stepContext.timeoutMs,
      processExitPromise: stepContext.session.exitPromise,
      description: 'paste arrival before F2 fence',
    },
  );
  await sendFenceKey(stepContext.bridge.page, false);
  const pasteEvents = await waitForFenceSlice(stepContext, startRecordCount, 'paste');
  if (process.platform !== 'win32' && terminalStateAtReady.bracketedPasteMode === true) {
    assert.ok(
      pasteEvents.some(
        (record) =>
          record.event_type === 'paste' &&
          (record.text === 'alpha\rbeta' || record.text === 'alpha\nbeta'),
      ),
      'expected bracketed paste event with text "alpha\\rbeta" or "alpha\\nbeta"',
    );
  }
  return pasteEvents;
}

async function runFocusSteps(stepContext) {
  const focusLostEvents = await runFencedStep(
    stepContext,
    'focus_lost',
    () => stepContext.bridge.blurTerminal(),
    { shouldPreserveBlur: true },
  );
  const focusGainedEvents = await runFencedStep(stepContext, 'focus_gained', () =>
    stepContext.bridge.focusTerminal(),
  );
  if (process.platform !== 'win32' && stepContext.tmuxMode !== 'stock') {
    assert.ok(
      focusLostEvents.some((record) => record.event_type === 'focus' && record.focused === false),
      'expected FocusLost event on terminal blur',
    );
    assert.ok(
      focusGainedEvents.some((record) => record.event_type === 'focus' && record.focused === true),
      'expected FocusGained event on terminal focus',
    );
  }
  return { focusLostEvents, focusGainedEvents };
}

async function finishInteractiveCleanExit(stepContext) {
  await stepContext.bridge.page.keyboard.press('Control+c');
  const exitOutcome = await stepContext.session.waitForExit(stepContext.timeoutMs);
  assert.equal(exitOutcome.exitCode, 0, 'expected exit code 0 on Ctrl+C clean exit');

  const allRecords = readJsonLines(stepContext.logPath);
  assert.ok(
    allRecords.some((record) => record.kind === 'summary' && record.mode === 'interactive'),
    'missing interactive summary record',
  );
  assert.ok(
    allRecords.some(
      (record) =>
        record.kind === 'event' &&
        record.event_type === 'key' &&
        record.code === 'char:c' &&
        record.ctrl,
    ),
    'missing clean-exit Ctrl+C key event record',
  );

  const terminalStateAfterExit = await stepContext.bridge.getTerminalState(stepContext.timeoutMs);
  assert.equal(terminalStateAfterExit.mouseTrackingMode, 'none');
  assert.equal(terminalStateAfterExit.bracketedPasteMode, false);
  assert.equal(terminalStateAfterExit.sendFocusMode, false);
  return { exitCode: exitOutcome.exitCode, exitReason: 'clean_key_exit', terminalStateAfterExit };
}

/** Waits until `xterm.js` receives the outer terminal mode sequences expected after `<log>.ready`. */
export async function waitForReadyOuterTerminalModes(
  stepContext,
  targetPlatform = process.platform,
) {
  const shouldExpectMouse = shouldExpectOuterMouseEvents(stepContext);
  if (!shouldExpectMouse && targetPlatform === 'win32') {
    return;
  }
  const hasExpectedOuterMode = shouldExpectMouse
    ? () => window.__getTerminalState().mouseTrackingMode !== 'none'
    : () => window.__getTerminalState().bracketedPasteMode === true;
  await stepContext.bridge.page.waitForFunction(hasExpectedOuterMode, undefined, {
    timeout: stepContext.timeoutMs,
  });
}

async function prepareInteractiveSubrun(stepContext, fenceLabel) {
  await waitForProbeReady(
    stepContext.logPath,
    stepContext.timeoutMs,
    stepContext.session.exitPromise,
  );
  await waitForReadyOuterTerminalModes(stepContext);
  const terminalStateAtReady = await stepContext.bridge.getTerminalState(stepContext.timeoutMs);
  await runFencedStep(stepContext, fenceLabel, async () => {});
  return terminalStateAtReady;
}

async function runPrimaryInteractiveSubrun(runtime, withManagedSession) {
  return withManagedSession(runtime, [], async (stepContext) => {
    const terminalStateAtReady = await prepareInteractiveSubrun(stepContext, 'sync_ready');
    if (runtime.tmuxMode === null && process.platform !== 'win32') {
      assert.equal(terminalStateAtReady.mouseTrackingMode, 'drag');
      assert.equal(terminalStateAtReady.bracketedPasteMode, true);
      assert.equal(terminalStateAtReady.sendFocusMode, true);
    }
    const pointerSteps = await runPointerSteps(stepContext, terminalStateAtReady);
    const keyboardSteps = await runKeyboardSteps(stepContext);
    const pasteEvents = await runPasteStep(stepContext, terminalStateAtReady);
    const focus = await runFocusSteps(stepContext);
    const cleanExit = await finishInteractiveCleanExit(stepContext);

    return {
      tmuxServerInfo: stepContext.session.tmuxServerInfo ?? null,
      terminalStateAtReady,
      pointerSteps,
      keyboardSteps,
      pasteEvents,
      focus,
      exitCode: cleanExit.exitCode,
      exitReason: cleanExit.exitReason,
      terminalStateAfterExit: cleanExit.terminalStateAfterExit,
    };
  });
}

async function runKittyEnterSubrun(runtime, withManagedSession) {
  return withManagedSession(runtime, ['--push-kitty'], async (stepContext) => {
    await prepareInteractiveSubrun(stepContext, 'sync_kitty_ready');
    const page = stepContext.bridge.page;
    const kittyEnterSteps = {};
    for (const [stepName, chord] of [
      ['alt_enter', 'Alt+Enter'],
      ['shift_enter', 'Shift+Enter'],
      ['ctrl_enter', 'Control+Enter'],
    ]) {
      kittyEnterSteps[stepName] = await runFencedStep(stepContext, `kitty_${stepName}`, () =>
        page.keyboard.press(chord),
      );
    }
    await finishInteractiveCleanExit(stepContext);
    return kittyEnterSteps;
  });
}

/**
 * Executes Sub-run A (pointer, keyboard, paste, focus, clean teardown) and Sub-run B
 * (`--push-kitty` Enter modifier check) for `capability_probe`.
 */
export async function executeInteractiveScenario(runtime, withManagedSession) {
  const primaryResult = await runPrimaryInteractiveSubrun(runtime, withManagedSession);
  const kittyEnterSteps = await runKittyEnterSubrun(runtime, withManagedSession);
  return { scenario: 'interactive', ...primaryResult, kittyEnterSteps };
}
