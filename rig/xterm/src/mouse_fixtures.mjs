/** Real browser gestures over the mouse lab's logged content rectangles. */
import assert from 'node:assert/strict';
import { cellCenterToViewportPx, clickCell, dragAcrossCells, wheelAtCell } from './cell_coords.mjs';
import { readCompleteJsonLines, waitForLogRecords } from './ready_waiter.mjs';

function records(context) {
  return readCompleteJsonLines(context.logPath);
}

function area(context, target) {
  return records(context).findLast((record) => record.kind === 'layout').areas
    .find((rectangle) => rectangle.target === target);
}

function point(context, target, column = 0, row = 0) {
  const rectangle = area(context, target);
  return { column: rectangle.x + column, row: rectangle.y + row };
}

function waitForRecords(context, selector, description) {
  return waitForLogRecords(context.logPath, selector, {
    timeoutMs: context.timeoutMs, processExitPromise: context.session.exitPromise, description,
  });
}

/** Runs an action then waits for the lab to process a FIFO F2 input/log fence. */
export async function runMouseFencedStep(context, performAction) {
  const startCount = readCompleteJsonLines(context.logPath).length;
  await performAction();
  await context.bridge.page.keyboard.press('F2');
  return waitForRecords(context, (records) => {
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

async function move(context, state, cell) {
  const position = cellCenterToViewportPx(state, cell.column, cell.row);
  await context.bridge.page.mouse.move(position.x, position.y);
}

async function observedMouse(context, action, mouseKind) {
  const startCount = records(context).length;
  await action();
  return waitForRecords(context, (allRecords) => {
    const trailing = allRecords.slice(startCount);
    const eventIndex = trailing.findLastIndex((record) => record.kind === 'event'
      && record.event.kind === 'Mouse' && record.event.mouse_kind === mouseKind);
    return eventIndex >= 0 && trailing.slice(eventIndex + 1).some((record) => record.kind === 'report')
      ? trailing : null;
  }, `mouse_lab ${mouseKind} report`);
}

async function click(context, state, target, column = 0, row = 0, options = {}) {
  const cell = point(context, target, column, row);
  const button = options.button === 'right' ? 'Right' : 'Left';
  return runMouseFencedStep(context, () => observedMouse(context, () => clickCell(
    context.bridge.page, state, cell.column, cell.row, options,
  ), `Up(${button})`));
}

async function wheel(context, state, target, deltaY, column = 2, row = 2) {
  const cell = point(context, target, column, row);
  // xterm ignores subthreshold pixel wheel deltas; 100 pixels emits one mouse report.
  return observedMouse(context, () => wheelAtCell(context.bridge.page, state,
    cell.column, cell.row, 0, Math.sign(deltaY) * 100), deltaY > 0 ? 'ScrollDown' : 'ScrollUp');
}

async function drag(context, state, target, start, end, options = {}) {
  return runMouseFencedStep(context, () => observedMouse(context, () => dragAcrossCells(
    context.bridge.page, state, point(context, target, ...start), point(context, target, ...end),
    { steps: 1, ...options },
  ), 'Up(Left)'));
}

async function repeatClick(context, state, target, column, row, count) {
  await move(context, state, point(context, target, column, row));
  const clicks = [];
  for (let clickIndex = 0; clickIndex < count; clickIndex += 1) {
    const startCount = records(context).length;
    await observedMouse(context, () => context.bridge.page.mouse.down(), 'Down(Left)');
    await observedMouse(context, () => context.bridge.page.mouse.up(), 'Up(Left)');
    clicks.push(records(context).slice(startCount));
  }
  return clicks;
}

async function historyDrag(context, state) {
  const startCount = records(context).length;
  const right = area(context, 'right');
  assert.ok(right.height < 31, 'X7 history endpoint must exceed one viewport');
  await move(context, state, point(context, 'right'));
  await observedMouse(context, () => context.bridge.page.mouse.down(), 'Down(Left)');
  try {
    await observedMouse(context, () => move(context, state, {
      column: right.x + right.width - 1, row: right.y + right.height,
    }), 'Drag(Left)');
    await waitForRecords(context, (allRecords) => allRecords.slice(startCount).find((record) =>
      record.kind === 'selection' && record.selection?.target === 'right'
      && record.selection.range.end.line >= 30), 'X7 selection reaches right history 030');
  } finally {
    await observedMouse(context, () => context.bridge.page.mouse.up(), 'Up(Left)');
  }
  // Close the release window before the fence clears the finished selection.
  const history = records(context).slice(startCount);
  await runMouseFencedStep(context, async () => {});
  return { history };
}

async function wheelDuringDrag(context, state) {
  await move(context, state, point(context, 'left'));
  await observedMouse(context, () => context.bridge.page.mouse.down(), 'Down(Left)');
  try {
    await observedMouse(context, () => move(context, state, point(context, 'left', 4, 2)), 'Drag(Left)');
    const beforeEnd = records(context).findLast((record) => record.kind === 'selection')?.selection.range.end;
    const startCount = records(context).length;
    await wheel(context, state, 'left', 1, 4, 2);
    await waitForRecords(context, (allRecords) => allRecords.slice(startCount).find((record) =>
      record.kind === 'selection' && record.selection?.range.end.line === 5), 'X9 wheel selection endpoint');
    return { beforeEnd, wheel: records(context).slice(startCount) };
  } finally {
    await observedMouse(context, () => context.bridge.page.mouse.up(), 'Up(Left)');
  }
}

async function passThrough(context, state) {
  await runMouseFencedStep(context, () => context.bridge.page.keyboard.press('m'));
  const clicked = await click(context, state, 'right', 2, 2);
  await runMouseFencedStep(context, () => context.bridge.page.keyboard.press('m'));
  const dragged = await drag(context, state, 'right', [2, 2], [6, 4]);
  const wheeled = await runMouseFencedStep(context, () => wheel(context, state, 'right', 1));
  return { click: clicked, drag: dragged, wheel: wheeled };
}

async function nativeShiftDrag(context, state) {
  const start = point(context, 'right', 0, 0);
  const end = point(context, 'right', 5, 0);
  const startPixel = cellCenterToViewportPx(state, start.column, start.row);
  const endPixel = cellCenterToViewportPx(state, end.column, end.row);
  // Native selections use exclusive cell boundaries. Stay left of each midpoint.
  const offset = state.cellWidthPx / 4;
  const page = context.bridge.page;
  await page.mouse.move(startPixel.x - offset, startPixel.y);
  await page.keyboard.down('Shift');
  try {
    await page.mouse.down();
    try {
      await page.mouse.move(endPixel.x - offset, endPixel.y);
    } finally {
      await page.mouse.up();
    }
  } finally {
    await page.keyboard.up('Shift');
  }
  return page.evaluate(() => window.__getNativeSelection());
}

async function modifierDrag(context, state, modifier) {
  await runMouseFencedStep(context, () => context.bridge.page.keyboard.press('m'));
  const performDrag = () => dragAcrossCells(context.bridge.page, state,
    point(context, 'right', 0, 0), point(context, 'right', 4, 0), { modifiers: [modifier], steps: 1 });
  let nativeSelection = '';
  const dragRecords = await runMouseFencedStep(context, async () => {
    if (modifier === 'Shift' && process.platform !== 'darwin') {
      // xterm clears native selection on user input, including the closing F2 fence.
      nativeSelection = await nativeShiftDrag(context, state);
    } else {
      await observedMouse(context, performDrag, 'Up(Left)');
    }
  });
  return { modifier, drag: dragRecords, nativeSelection };
}

async function keyboardSelection(context) {
  const keys = ['Tab', 'Tab', 's', 'g', 'j', 'j', 'j', 'Home', 'v', 'l', 'v',
    'V', 'j', 'V', 'g', 'j', 'j', 'j', 'Home', 'W', 'y'];
  const keyboard = await runMouseFencedStep(context, async () => {
    for (const key of keys) await context.bridge.page.keyboard.press(key);
  });
  return { keyboard, keys };
}

/** Executes X4 through X14; modifier variants receive independent managed sessions. */
export async function runMouseGesture(context, state, scenario, modifier = null) {
  switch (scenario) {
    case 'x4': {
      const steps = {};
      for (const target of ['left', 'right', 'tab', 'divider']) steps[target] = await click(context, state, target);
      return steps;
    }
    case 'x5': return { drag: await drag(context, state, 'left', [0, 0], [4, 2]) };
    case 'x6': {
      const wordClicks = await repeatClick(context, state, 'left', 2, 3, 2);
      const lineClicks = await repeatClick(context, state, 'left', 0, 5, 3);
      return { word: wordClicks.at(-1), line: lineClicks.at(-1), wordClicks, lineClicks };
    }
    case 'x7': return historyDrag(context, state);
    case 'x8': return {
      left: await runMouseFencedStep(context, () => wheel(context, state, 'left', 1)),
      right: await runMouseFencedStep(context, () => wheel(context, state, 'right', -1)),
    };
    case 'x9': return wheelDuringDrag(context, state);
    case 'x10': return passThrough(context, state);
    case 'x11': return modifierDrag(context, state, modifier);
    case 'x12':
      await runMouseFencedStep(context, () => context.bridge.page.keyboard.press('a'));
      return { wheel: await runMouseFencedStep(context, () => wheel(context, state, 'right', 1)) };
    case 'x13': return keyboardSelection(context);
    case 'x14': return {
      right: await click(context, state, 'left', 0, 0, { button: 'right' }),
      control: await click(context, state, 'right', 0, 0, { modifiers: ['Control'] }),
    };
    default: throw new Error(`Unsupported gesture scenario ${scenario}`);
  }
}
