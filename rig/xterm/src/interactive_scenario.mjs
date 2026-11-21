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

function isFencePressRecord(record) {
  return (
    record?.kind === 'event' &&
    record.event_type === 'key' &&
    record.code === 'f2' &&
    record.key_kind === 'press'
  );
}

function isFenceKeyRecord(record) {
  return record?.kind === 'event' && record.event_type === 'key' && record.code === 'f2';
}

async function sendFenceKey(page, shouldPreserveBlur) {
  if (!shouldPreserveBlur) {
    await page.keyboard.press('F2');
    return;
  }
  await page.evaluate(() => {
    const helperTextarea = document.querySelector('.xterm-helper-textarea');
    helperTextarea?.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'F2',
        code: 'F2',
        keyCode: 113,
        which: 113,
        bubbles: true,
        cancelable: true,
      }),
    );
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

function assertPointerStepResults(pointerSteps, tmuxMode) {
  if (process.platform !== 'win32') {
    const hoverMovedCount = pointerSteps.hover.filter((item) => item.mouse_kind === 'moved').length;
    assert.equal(hoverMovedCount, 0, 'Rule M1 mode 1002 must not emit bare pointer-move events');
  }
  if (tmuxMode === 'stock') {
    const stockSteps = [
      ...pointerSteps.left_click,
      ...pointerSteps.left_drag,
      ...pointerSteps.wheel_down,
      ...pointerSteps.wheel_up,
    ];
    const mouseCount = stockSteps.filter((item) => item.event_type === 'mouse').length;
    assert.equal(mouseCount, 0, 'Stock tmux (mouse off) must forward zero mouse events to probe');
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
  assertPointerStepResults(pointerSteps, stepContext.tmuxMode);
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
  assert.ok(keyboardSteps.arrow_up.some((item) => item.code === 'up'), 'missing ArrowUp key');
  assert.ok(keyboardSteps.f1.some((item) => item.code === 'f1'), 'missing F1 key');
  assert.ok(
    keyboardSteps.ctrl_a.some((item) => item.code === 'char:a' && item.ctrl === true),
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
  for (const item of recordsAfterStart) {
    if (item?.kind !== 'event') {
      continue;
    }
    if (item.event_type === 'paste') {
      return true;
    }
    if (
      item.event_type === 'key' &&
      item.key_kind === 'press' &&
      item.code === EXPECTED_PASTE_KEY_CODES[matchedKeyCount]
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
        (item) =>
          item.event_type === 'paste' &&
          (item.text === 'alpha\rbeta' || item.text === 'alpha\nbeta'),
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
      focusLostEvents.some((item) => item.event_type === 'focus' && item.focused === false),
      'expected FocusLost event on terminal blur',
    );
    assert.ok(
      focusGainedEvents.some((item) => item.event_type === 'focus' && item.focused === true),
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
    allRecords.some((item) => item.kind === 'summary' && item.mode === 'interactive'),
    'missing interactive summary record',
  );
  assert.ok(
    allRecords.some(
      (item) => item.kind === 'event' && item.event_type === 'key' && item.code === 'char:c' && item.ctrl,
    ),
    'missing clean-exit Ctrl+C key event record',
  );

  const terminalStateAfterExit = await stepContext.bridge.getTerminalState(stepContext.timeoutMs);
  assert.equal(terminalStateAfterExit.mouseTrackingMode, 'none');
  assert.equal(terminalStateAfterExit.bracketedPasteMode, false);
  assert.equal(terminalStateAfterExit.sendFocusMode, false);
  return { exitCode: exitOutcome.exitCode, exitReason: 'clean_key_exit', terminalStateAfterExit };
}

async function runPrimaryInteractiveSubrun(runtime, withManagedSession) {
  return withManagedSession(runtime, [], async (stepContext) => {
    await waitForProbeReady(
      stepContext.logPath,
      stepContext.timeoutMs,
      stepContext.session.exitPromise,
    );
    const terminalStateAtReady = await stepContext.bridge.getTerminalState(stepContext.timeoutMs);
    if (runtime.tmuxMode === null && process.platform !== 'win32') {
      assert.equal(terminalStateAtReady.mouseTrackingMode, 'drag');
      assert.equal(terminalStateAtReady.bracketedPasteMode, true);
      assert.equal(terminalStateAtReady.sendFocusMode, true);
    }
    await runFencedStep(stepContext, 'sync_ready', async () => {});

    const pointerSteps = await runPointerSteps(stepContext, terminalStateAtReady);
    const keyboardSteps = await runKeyboardSteps(stepContext);
    const pasteEvents = await runPasteStep(stepContext, terminalStateAtReady);
    const focus = await runFocusSteps(stepContext);
    const cleanExit = await finishInteractiveCleanExit(stepContext);

    return {
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
    await waitForProbeReady(
      stepContext.logPath,
      stepContext.timeoutMs,
      stepContext.session.exitPromise,
    );
    await stepContext.bridge.getTerminalState(stepContext.timeoutMs);
    await runFencedStep(stepContext, 'sync_kitty_ready', async () => {});

    const page = stepContext.bridge.page;
    const kittyEnterSteps = {
      alt_enter: await runFencedStep(stepContext, 'kitty_alt_enter', () =>
        page.keyboard.press('Alt+Enter'),
      ),
      shift_enter: await runFencedStep(stepContext, 'kitty_shift_enter', () =>
        page.keyboard.press('Shift+Enter'),
      ),
      ctrl_enter: await runFencedStep(stepContext, 'kitty_ctrl_enter', () =>
        page.keyboard.press('Control+Enter'),
      ),
    };
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
  return {
    scenario: 'interactive',
    ...primaryResult,
    kittyEnterSteps,
  };
}
