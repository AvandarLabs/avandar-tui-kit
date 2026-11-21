/**
 * Pure report normalizer converting Layer 3/4 (`rig/xterm`), Layer 5 (`rig/orca` JSONL),
 * and Layer 6 (`rig/iterm2`) capability probe outputs into the canonical `rig/expected` schema.
 */

const TOOLKIT_GLYPH_SAMPLE_ORDER = [
  'ellipsis', 'middle_dot', 'chevron', 'pointer_triangle',
  'box_horizontal', 'box_vertical', 'box_top_left', 'box_tee_right', 'box_bottom_left',
  'block_eighth_1', 'block_eighth_3', 'block_eighth_4', 'block_eighth_6', 'block_full',
  'braille_dot_1', 'braille_full', 'private_use_icon',
];

const CANONICAL_WIDTH_SAMPLE_ORDER = Object.freeze([
  'ascii_a', 'precomposed_acute', 'combining_acute',
  'cjk_han', 'hangul_syllable', 'fullwidth_latin_a',
  ...TOOLKIT_GLYPH_SAMPLE_ORDER,
  'plain_emoji', 'family_zwj', 'regional_flag_us',
  'heart_vs16', 'heart_plain', 'thumbs_up_skin_tone', 'thumbs_up_plain',
  'rainbow_flag', 'white_flag_base', 'zwsp',
]);

const TOOLKIT_GLYPH_NAMES = new Set(TOOLKIT_GLYPH_SAMPLE_ORDER);

const CANONICAL_POINTER_STEPS = Object.freeze([
  'hover', 'left_click', 'left_drag', 'wheel_down', 'wheel_up', 'right_click',
  'shift_click', 'alt_click', 'cmd_click', 'alt_drag', 'ctrl_click', 'double_click',
]);

const CANONICAL_KEYBOARD_STEPS = Object.freeze([
  'arrow_up', 'f1', 'ctrl_a', 'alt_a', 'alt_enter', 'shift_enter', 'ctrl_enter',
]);

const CANONICAL_KITTY_ENTER_STEPS = Object.freeze(['alt_enter', 'shift_enter', 'ctrl_enter']);

function createInvalidInputError(messageText, recoveryHint) {
  const parseError = new Error(messageText);
  parseError.recoveryHint = recoveryHint;
  return parseError;
}

function parseJsonLines(rawText) {
  const rawLines = rawText.split(/\r?\n/);
  const parsedRecords = [];
  for (let lineIndex = 0; lineIndex < rawLines.length; lineIndex += 1) {
    const trimmedLine = rawLines[lineIndex].trim();
    if (trimmedLine.length === 0) {
      continue;
    }
    try {
      parsedRecords.push(JSON.parse(trimmedLine));
    } catch (lineError) {
      throw createInvalidInputError(
        `Invalid JSON on line ${lineIndex + 1}: ${lineError.message}`,
        'Ensure the input file contains valid JSON or newline-delimited JSONL records.',
      );
    }
  }
  return parsedRecords;
}

function parseTextInput(rawText) {
  const trimmedText = rawText.trim();
  if (trimmedText.length === 0) {
    throw createInvalidInputError(
      'Probe report input is empty.',
      'Pass a non-empty JSON report or JSONL probe output file.',
    );
  }
  try {
    const parsedSingle = JSON.parse(trimmedText);
    return parsedSingle?.kind === 'section' || parsedSingle?.kind === 'summary'
      ? [parsedSingle]
      : parsedSingle;
  } catch (wholeJsonError) {
    const firstNonEmptyLine = trimmedText.split(/\r?\n/, 1)[0].trim();
    if (firstNonEmptyLine === '{' || firstNonEmptyLine === '[') {
      throw createInvalidInputError(
        `Invalid JSON report: ${wholeJsonError.message}`,
        'Ensure the JSON report is syntactically valid.',
      );
    }
    return parseJsonLines(rawText);
  }
}

function isPlainObject(candidateValue) {
  return candidateValue !== null && typeof candidateValue === 'object' && !Array.isArray(candidateValue);
}

function requirePlainSection(sectionValue, sectionName, recoveryHint = 'Verify probe output.') {
  if (!isPlainObject(sectionValue)) {
    throw createInvalidInputError(`Missing '${sectionName}' section in probe report.`, recoveryHint);
  }
}

function orderKeysDeterministically(sourceObject, preferredOrder) {
  const orderedKeys = [];
  const seenKeys = new Set();
  for (const preferredKey of preferredOrder) {
    if (Object.hasOwn(sourceObject, preferredKey)) {
      orderedKeys.push(preferredKey);
      seenKeys.add(preferredKey);
    }
  }
  const extraKeys = Object.keys(sourceObject)
    .filter((keyName) => !seenKeys.has(keyName))
    .sort();
  return [...orderedKeys, ...extraKeys];
}

function normalizeIdentitySection(identityRecord) {
  requirePlainSection(
    identityRecord,
    'identity',
    'Run capability_probe to completion so all batch sections are recorded.',
  );
  return {
    os: String(identityRecord.os ?? ''),
    xtversion: identityRecord.xtversion ?? null,
    term: identityRecord.term ?? null,
    term_program: identityRecord.term_program ?? null,
    is_tmux: Boolean(identityRecord.is_tmux),
    is_stdin_tty: Boolean(identityRecord.is_stdin_tty),
    is_stdout_tty: Boolean(identityRecord.is_stdout_tty),
  };
}

function normalizeMouseModesSection(mouseRecord) {
  requirePlainSection(mouseRecord, 'mouse_modes');
  const mapReports = (reports) =>
    (reports ?? []).map((entry) => ({ mode: Number(entry.mode), state: String(entry.state) }));
  return {
    is_m1_expected: Boolean(mouseRecord.is_m1_expected),
    is_m5_reset: Boolean(mouseRecord.is_m5_reset),
    after_m1: mapReports(mouseRecord.after_m1),
    after_m5: mapReports(mouseRecord.after_m5),
  };
}

function normalizeKittySection(kittyRecord) {
  requirePlainSection(kittyRecord, 'kitty_keyboard');
  return {
    answered: Boolean(kittyRecord.answered),
    initial_flags: kittyRecord.initial_flags ?? null,
    pushed_flags: kittyRecord.pushed_flags ?? null,
    popped_flags: kittyRecord.popped_flags ?? null,
  };
}

function normalizeModesSection(modesSection) {
  requirePlainSection(modesSection, 'modes');
  const sourceByModeName = {};
  if (Array.isArray(modesSection.modes)) {
    for (const modeItem of modesSection.modes) {
      sourceByModeName[modeItem.name] = modeItem;
    }
  } else {
    Object.assign(sourceByModeName, modesSection);
  }
  const normalizedModes = {};
  for (const modeName of ['bracketed_paste', 'focus_events', 'synchronized_output']) {
    const modeData = sourceByModeName[modeName] ?? {};
    normalizedModes[modeName] = {
      after_enable: modeData.after_enable ?? null,
      after_disable: modeData.after_disable ?? null,
    };
  }
  return normalizedModes;
}

function normalizeWidthsSection(widthsSection) {
  requirePlainSection(widthsSection, 'widths');
  const rawSamplesMap = {};
  if (Array.isArray(widthsSection.samples)) {
    for (const sampleItem of widthsSection.samples) {
      rawSamplesMap[sampleItem.name] = sampleItem.cells ?? null;
    }
  } else if (isPlainObject(widthsSection.samples)) {
    Object.assign(rawSamplesMap, widthsSection.samples);
  }
  const orderedNames = orderKeysDeterministically(rawSamplesMap, CANONICAL_WIDTH_SAMPLE_ORDER);
  const canonicalSamples = {};
  for (const sampleName of orderedNames) {
    canonicalSamples[sampleName] = rawSamplesMap[sampleName] ?? null;
  }
  const presentToolkitNames = orderedNames.filter((sampleName) => TOOLKIT_GLYPH_NAMES.has(sampleName));
  const areAllToolkitSingleCell =
    presentToolkitNames.length > 0 &&
    presentToolkitNames.every((sampleName) => canonicalSamples[sampleName] === 1);
  return { all_toolkit_glyphs_single_cell: areAllToolkitSingleCell, samples: canonicalSamples };
}

function normalizeOsc52Probes(osc52Input) {
  if (osc52Input === null || osc52Input === undefined) {
    return null;
  }
  const probeList = Array.isArray(osc52Input) ? osc52Input : osc52Input.probes;
  if (!Array.isArray(probeList)) {
    return null;
  }
  return probeList.map((probeEntry) => {
    if (typeof probeEntry.updatedOuterClipboard === 'boolean') {
      return {
        form: String(probeEntry.form),
        updatedOuterClipboard: probeEntry.updatedOuterClipboard,
        updatedTmuxBuffer: Boolean(probeEntry.updatedTmuxBuffer),
      };
    }
    const token = probeEntry.token ?? '';
    const hasOuterPayload = Array.isArray(probeEntry.osc52Payloads)
      ? probeEntry.osc52Payloads.some((payload) => payload.decodedText === token)
      : Boolean(probeEntry.clipboardUpdated);
    return {
      form: String(probeEntry.form),
      updatedOuterClipboard: hasOuterPayload,
      updatedTmuxBuffer: token.length > 0 && probeEntry.tmuxBuffer === token,
    };
  });
}

function summarizePointerStepValue(stepValue) {
  if (isPlainObject(stepValue) && typeof stepValue.hasDown === 'boolean') {
    const stepModifiers = stepValue.modifiers ?? {};
    return {
      hasMoved: Boolean(stepValue.hasMoved),
      hasDown: Boolean(stepValue.hasDown),
      hasDrag: Boolean(stepValue.hasDrag),
      hasUp: Boolean(stepValue.hasUp),
      hasScrollDown: Boolean(stepValue.hasScrollDown),
      hasScrollUp: Boolean(stepValue.hasScrollUp),
      modifiers: { shift: Boolean(stepModifiers.shift), alt: Boolean(stepModifiers.alt), ctrl: Boolean(stepModifiers.ctrl) },
    };
  }
  const mouseEvents = (stepValue ?? []).filter((record) => record?.event_type === 'mouse');
  const hasPrefix = (prefix) => mouseEvents.some((record) => String(record.mouse_kind).startsWith(prefix));
  const hasModifier = (modifierKey) => mouseEvents.some((record) => record[modifierKey] === true);
  return {
    hasMoved: mouseEvents.some((record) => record.mouse_kind === 'moved'),
    hasDown: hasPrefix('down:'),
    hasDrag: hasPrefix('drag:'),
    hasUp: hasPrefix('up:'),
    hasScrollDown: mouseEvents.some((record) => record.mouse_kind === 'scroll_down'),
    hasScrollUp: mouseEvents.some((record) => record.mouse_kind === 'scroll_up'),
    modifiers: { shift: hasModifier('shift'), alt: hasModifier('alt'), ctrl: hasModifier('ctrl') },
  };
}

function summarizeKeyboardMap(stepsObject, preferredSteps) {
  const sourceSteps = stepsObject ?? {};
  const normalizedKeyboard = {};
  const isKeyPressItem = (item) =>
    isPlainObject(item) &&
    (!('event_type' in item) || (item.event_type === 'key' && item.key_kind === 'press'));
  for (const stepName of orderKeysDeterministically(sourceSteps, preferredSteps)) {
    const rawItems = sourceSteps[stepName] ?? [];
    normalizedKeyboard[stepName] = rawItems.filter(isKeyPressItem).map((item) => ({
      code: String(item.code),
      shift: Boolean(item.shift),
      alt: Boolean(item.alt),
      ctrl: Boolean(item.ctrl),
    }));
  }
  return normalizedKeyboard;
}

function summarizePasteSection(interactiveSection) {
  if (isPlainObject(interactiveSection.paste) && typeof interactiveSection.paste.hasPasteEvent === 'boolean') {
    const pasteSummary = interactiveSection.paste;
    return {
      hasPasteEvent: pasteSummary.hasPasteEvent,
      keyPressCount: Number(pasteSummary.keyPressCount),
      arrivedInSingleBurst: Boolean(pasteSummary.arrivedInSingleBurst),
    };
  }
  const pasteRecords = (interactiveSection.pasteEvents ?? []).filter((record) => record.event_type === 'paste');
  const keyPressRecords = (interactiveSection.pasteEvents ?? []).filter(
    (record) => record.event_type === 'key' && record.key_kind === 'press',
  );
  const hasPasteEvent = pasteRecords.length > 0;
  const keyPressCount = keyPressRecords.length;
  const arrivedInSingleBurst = hasPasteEvent
    ? pasteRecords.length === 1 && keyPressCount === 0
    : keyPressCount > 0 && keyPressRecords.every((record) => record.burst_press_count === keyPressCount);
  return { hasPasteEvent, keyPressCount, arrivedInSingleBurst };
}

function summarizeFocusAndTeardown(interactiveSection, batchSection, panicSection) {
  const focusSource = interactiveSection.focus ?? interactiveSection;
  const hasFocusFlag = (events, targetState) =>
    (events ?? []).some((record) => record.event_type === 'focus' && record.focused === targetState);
  const focus =
    typeof focusSource.hasFocusLost === 'boolean'
      ? { hasFocusLost: focusSource.hasFocusLost, hasFocusGained: Boolean(focusSource.hasFocusGained) }
      : {
          hasFocusLost: hasFocusFlag(focusSource.focusLostEvents, false),
          hasFocusGained: hasFocusFlag(focusSource.focusGainedEvents, true),
        };
  if (isPlainObject(interactiveSection.teardown)) {
    const { cleanExitResetMouse, panicExitResetMouse } = interactiveSection.teardown;
    return {
      focus,
      teardown: { cleanExitResetMouse: Boolean(cleanExitResetMouse), panicExitResetMouse: Boolean(panicExitResetMouse) },
    };
  }
  const cleanState = interactiveSection.terminalStateAfterExit ?? interactiveSection.teardownState ?? null;
  const batchState = batchSection?.terminalStateAfterExit ?? batchSection?.teardown ?? null;
  const panicState = panicSection?.terminalStateAfterPanic ?? panicSection?.teardown ?? null;
  const hasPanicExit =
    isPlainObject(panicSection) && ((panicSection.exitCode ?? 0) !== 0 || (panicSection.signal ?? 0) !== 0);
  return {
    focus,
    teardown: {
      cleanExitResetMouse:
        cleanState?.mouseTrackingMode === 'none' &&
        (batchState === null || batchState?.mouseTrackingMode === 'none'),
      panicExitResetMouse: hasPanicExit && panicState?.mouseTrackingMode === 'none',
    },
  };
}

function normalizeInteractiveSection(interactiveSection, batchSection, panicSection) {
  if (!isPlainObject(interactiveSection)) {
    return null;
  }
  const rawPointer = interactiveSection.pointer ?? interactiveSection.pointerSteps ?? {};
  const pointer = {};
  for (const stepName of orderKeysDeterministically(rawPointer, CANONICAL_POINTER_STEPS)) {
    pointer[stepName] = summarizePointerStepValue(rawPointer[stepName]);
  }
  const keyboard = summarizeKeyboardMap(interactiveSection.keyboard ?? interactiveSection.keyboardSteps, CANONICAL_KEYBOARD_STEPS);
  const kittyEnter = summarizeKeyboardMap(interactiveSection.kitty_enter ?? interactiveSection.kittyEnterSteps, CANONICAL_KITTY_ENTER_STEPS);
  const paste = summarizePasteSection(interactiveSection);
  const { focus, teardown } = summarizeFocusAndTeardown(interactiveSection, batchSection, panicSection);
  return { pointer, keyboard, kitty_enter: kittyEnter, paste, focus, teardown };
}

function extractSectionsByName(parsedObjectOrRecords) {
  if (Array.isArray(parsedObjectOrRecords)) {
    const sectionsMap = {};
    for (const record of parsedObjectOrRecords) {
      if (record?.kind === 'section' && typeof record.section === 'string') {
        sectionsMap[record.section] = record;
      }
    }
    return { sectionsMap, batchSection: null, osc52Input: null, interactiveSection: null, panicSection: null };
  }
  if (!isPlainObject(parsedObjectOrRecords)) {
    throw createInvalidInputError('Probe report must be a JSON object or JSONL record array.', 'Verify input.');
  }
  const hasBatchRecords = Array.isArray(parsedObjectOrRecords.batch?.records);
  const sectionsMap = hasBatchRecords
    ? extractSectionsByName(parsedObjectOrRecords.batch.records).sectionsMap
    : parsedObjectOrRecords;
  return {
    sectionsMap,
    batchSection: hasBatchRecords ? parsedObjectOrRecords.batch : null,
    osc52Input: parsedObjectOrRecords.osc52 ?? null,
    interactiveSection: parsedObjectOrRecords.interactive ?? null,
    panicSection: hasBatchRecords ? (parsedObjectOrRecords.panic ?? null) : null,
  };
}

/**
 * Normalizes a raw probe report (`xterm` JSON, `iterm2` JSON, `orca` JSONL text/records, or an
 * existing `rig/expected` baseline) into a canonical, host-portable capability baseline object.
 */
export function normalizeProbeReport(rawInput) {
  const parsedInput = typeof rawInput === 'string' ? parseTextInput(rawInput) : rawInput;
  const { sectionsMap, batchSection, osc52Input, interactiveSection, panicSection } =
    extractSectionsByName(parsedInput);
  const primaryBackgroundRecord = sectionsMap.background_color;
  const cellSizeRecord = sectionsMap.cell_size;
  if (!isPlainObject(primaryBackgroundRecord) || !isPlainObject(cellSizeRecord)) {
    const missingHint = 'Ensure capability_probe completed all automated batch sections.';
    throw createInvalidInputError("Missing 'background_color' or 'cell_size' section in probe report.", missingHint);
  }
  const backgroundColorRecord =
    !primaryBackgroundRecord.answered && isPlainObject(sectionsMap.background_color_late)
      ? sectionsMap.background_color_late
      : primaryBackgroundRecord;
  return {
    identity: normalizeIdentitySection(sectionsMap.identity),
    mouse_modes: normalizeMouseModesSection(sectionsMap.mouse_modes),
    kitty_keyboard: normalizeKittySection(sectionsMap.kitty_keyboard),
    modes: normalizeModesSection(sectionsMap.modes),
    background_color: {
      answered: Boolean(backgroundColorRecord.answered),
      is_dark: backgroundColorRecord.answered ? Boolean(backgroundColorRecord.is_dark) : null,
    },
    cell_size: { answered: Boolean(cellSizeRecord.answered) },
    widths: normalizeWidthsSection(sectionsMap.widths),
    osc52: normalizeOsc52Probes(osc52Input),
    interactive: normalizeInteractiveSection(interactiveSection, batchSection, panicSection),
  };
}
