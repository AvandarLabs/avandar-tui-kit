/**
 * Zero-based terminal cell coordinate math and Playwright pointer interaction
 * helpers used by interactive xterm.js scenarios.
 */

function assertValidTerminalMetrics(terminalState) {
  const hasValidGrid =
    terminalState !== null &&
    typeof terminalState === 'object' &&
    Number.isInteger(terminalState.cols) &&
    terminalState.cols > 0 &&
    Number.isInteger(terminalState.rows) &&
    terminalState.rows > 0;
  const hasValidPixels =
    hasValidGrid &&
    Number.isFinite(terminalState.cellWidthPx) &&
    terminalState.cellWidthPx > 0 &&
    Number.isFinite(terminalState.cellHeightPx) &&
    terminalState.cellHeightPx > 0 &&
    Number.isFinite(terminalState.screenLeftPx) &&
    Number.isFinite(terminalState.screenTopPx);
  if (!hasValidPixels) {
    throw new RangeError('Invalid terminal layout metrics for cell coordinate conversion.');
  }
}

function normalizeCellCoordinates(cellPoint) {
  if (Array.isArray(cellPoint) && cellPoint.length === 2) {
    return { column: cellPoint[0], row: cellPoint[1] };
  }
  if (cellPoint !== null && typeof cellPoint === 'object') {
    const column = cellPoint.column ?? cellPoint.col;
    return { column, row: cellPoint.row };
  }
  throw new RangeError('Expected a cell coordinate object { column, row }.');
}

async function withHeldModifiers(page, modifiers, performPointerAction) {
  const heldModifiers = [];
  try {
    for (const modifier of modifiers) {
      await page.keyboard.down(modifier);
      heldModifiers.push(modifier);
    }
    return await performPointerAction();
  } finally {
    for (let index = heldModifiers.length - 1; index >= 0; index -= 1) {
      await page.keyboard.up(heldModifiers[index]);
    }
  }
}

/**
 * Converts zero-based `(columnZeroBased, rowZeroBased)` cell indices into
 * viewport pixel center `{ x, y }` for Playwright pointer dispatch.
 */
export function cellCenterToViewportPx(terminalState, columnZeroBased, rowZeroBased) {
  assertValidTerminalMetrics(terminalState);
  if (
    !Number.isInteger(columnZeroBased) ||
    columnZeroBased < 0 ||
    columnZeroBased >= terminalState.cols
  ) {
    throw new RangeError(
      `Column ${columnZeroBased} is outside terminal bounds [0, ${terminalState.cols - 1}].`,
    );
  }
  if (!Number.isInteger(rowZeroBased) || rowZeroBased < 0 || rowZeroBased >= terminalState.rows) {
    throw new RangeError(
      `Row ${rowZeroBased} is outside terminal bounds [0, ${terminalState.rows - 1}].`,
    );
  }
  return {
    x: terminalState.screenLeftPx + (columnZeroBased + 0.5) * terminalState.cellWidthPx,
    y: terminalState.screenTopPx + (rowZeroBased + 0.5) * terminalState.cellHeightPx,
  };
}

/**
 * Clicks the center of `(columnZeroBased, rowZeroBased)` on `page` while
 * holding optional keyboard `modifiers`, resolving once keys are released.
 */
export async function clickCell(
  page,
  terminalState,
  columnZeroBased,
  rowZeroBased,
  { button = 'left', modifiers = [], clickCount = 1 } = {},
) {
  const center = cellCenterToViewportPx(terminalState, columnZeroBased, rowZeroBased);
  await withHeldModifiers(page, modifiers, () =>
    page.mouse.click(center.x, center.y, { button, clickCount }),
  );
}

/**
 * Drags the pointer from `startCell` center to `endCell` center on `page` while
 * holding optional `modifiers`, guaranteeing button and modifier release.
 */
export async function dragAcrossCells(
  page,
  terminalState,
  startCell,
  endCell,
  { button = 'left', modifiers = [], steps = 4 } = {},
) {
  const startCellCoords = normalizeCellCoordinates(startCell);
  const endCellCoords = normalizeCellCoordinates(endCell);
  const startPoint = cellCenterToViewportPx(
    terminalState,
    startCellCoords.column,
    startCellCoords.row,
  );
  const endPoint = cellCenterToViewportPx(terminalState, endCellCoords.column, endCellCoords.row);

  await page.mouse.move(startPoint.x, startPoint.y);
  await withHeldModifiers(page, modifiers, async () => {
    await page.mouse.down({ button });
    try {
      await page.mouse.move(endPoint.x, endPoint.y, { steps });
    } finally {
      await page.mouse.up({ button });
    }
  });
}

/**
 * Moves the pointer from `startCell` center to `endCell` center on `page` with
 * no mouse button pressed to verify hover motion reporting.
 */
export async function movePointerAcrossCells(
  page,
  terminalState,
  startCell,
  endCell,
  { steps = 4 } = {},
) {
  const startCellCoords = normalizeCellCoordinates(startCell);
  const endCellCoords = normalizeCellCoordinates(endCell);
  const startPoint = cellCenterToViewportPx(
    terminalState,
    startCellCoords.column,
    startCellCoords.row,
  );
  const endPoint = cellCenterToViewportPx(terminalState, endCellCoords.column, endCellCoords.row);
  await page.mouse.move(startPoint.x, startPoint.y);
  await page.mouse.move(endPoint.x, endPoint.y, { steps });
}

/**
 * Moves the pointer to `(columnZeroBased, rowZeroBased)` center on `page` and
 * dispatches a mouse wheel event with `(deltaX, deltaY)`.
 */
export async function wheelAtCell(
  page,
  terminalState,
  columnZeroBased,
  rowZeroBased,
  deltaX,
  deltaY,
) {
  const center = cellCenterToViewportPx(terminalState, columnZeroBased, rowZeroBased);
  await page.mouse.move(center.x, center.y);
  await page.mouse.wheel(deltaX, deltaY);
}
