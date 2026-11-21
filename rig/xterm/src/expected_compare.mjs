/**
 * Baseline comparator and CLI verifying live capability probe outputs against
 * committed JSON capability baselines with human-readable dot-path diffs.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { CliUsageError, hasHelpFlagBeforeDoubleDash } from './args.mjs';
import { normalizeProbeReport } from './expected_normalize.mjs';
import { createCliTheme, isDirectCliExecution } from './harness.mjs';

export { normalizeProbeReport } from './expected_normalize.mjs';

const MODULE_FILE_PATH = fileURLToPath(import.meta.url);

function isPlainObject(candidateValue) {
  return (
    candidateValue !== null && typeof candidateValue === 'object' && !Array.isArray(candidateValue)
  );
}

function appendValueDiff(diffs, pathLabel, expectedValue, actualValue) {
  diffs.push(
    `${pathLabel}: expected ${JSON.stringify(expectedValue)}, got ${JSON.stringify(actualValue)}`,
  );
}

function collectStructuralDiffs(expectedValue, actualValue, currentPath, diffs) {
  if (Object.is(expectedValue, actualValue)) {
    return;
  }
  if (Array.isArray(expectedValue) && Array.isArray(actualValue)) {
    if (expectedValue.length !== actualValue.length) {
      appendValueDiff(diffs, `${currentPath}.length`, expectedValue.length, actualValue.length);
      return;
    }
    expectedValue.forEach((expectedItem, itemIndex) => {
      collectStructuralDiffs(
        expectedItem,
        actualValue[itemIndex],
        `${currentPath}[${itemIndex}]`,
        diffs,
      );
    });
    return;
  }
  if (isPlainObject(expectedValue) && isPlainObject(actualValue)) {
    const mergedKeys = [
      ...new Set([...Object.keys(expectedValue), ...Object.keys(actualValue)]),
    ].sort();
    mergedKeys.forEach((keyName) => {
      const childPath = currentPath.length > 0 ? `${currentPath}.${keyName}` : keyName;
      if (!Object.hasOwn(expectedValue, keyName)) {
        diffs.push(
          `${childPath}: unexpected key with value ${JSON.stringify(actualValue[keyName])}`,
        );
      } else if (!Object.hasOwn(actualValue, keyName)) {
        diffs.push(
          `${childPath}: missing expected key (${JSON.stringify(expectedValue[keyName])})`,
        );
      } else {
        collectStructuralDiffs(expectedValue[keyName], actualValue[keyName], childPath, diffs);
      }
    });
    return;
  }
  appendValueDiff(diffs, currentPath || '<root>', expectedValue, actualValue);
}

/**
 * Normalizes `liveReport` and `expectedBaseline` and returns `{ isMatch, diffs,
 * normalizedActual, normalizedExpected }` for baseline parity checks.
 */
export function compareReportAgainstExpected(liveReport, expectedBaseline) {
  const normalizedActual = normalizeProbeReport(liveReport);
  const normalizedExpected = normalizeProbeReport(expectedBaseline);
  const diffs = [];
  collectStructuralDiffs(normalizedExpected, normalizedActual, '', diffs);
  return {
    isMatch: diffs.length === 0,
    diffs,
    normalizedActual,
    normalizedExpected,
  };
}

/**
 * Formats the structured `--help` text for the baseline comparison CLI,
 * returning the multi-section usage string written to stdout or stderr.
 */
export function buildExpectedCompareHelpText(theme) {
  return [
    `${theme.heading}Usage:${theme.reset}`,
    `  ${theme.bold}node rig/xterm/src/expected_compare.mjs${theme.reset} <actual-report-or--> <expected-baseline.json>`,
    `  ${theme.bold}node rig/xterm/src/expected_compare.mjs${theme.reset} --normalize <input-report-or-->`,
    '',
    `${theme.heading}Purpose:${theme.reset}`,
    '  Normalize capability_probe reports (JSON or JSONL) and verify exact capability parity against rig/expected/*.json.',
    '',
    `${theme.heading}Arguments:${theme.reset}`,
    `  ${theme.accent}<actual-report-or-->${theme.reset}   Path to live JSON/JSONL probe output, or '-' to read from stdin`,
    `  ${theme.accent}<expected-baseline>${theme.reset}    Path to canonical baseline file under rig/expected/`,
    '',
    `${theme.heading}Options:${theme.reset}`,
    `  ${theme.accent}-h, --help${theme.reset}             Show this help text on stdout and exit 0`,
    `  ${theme.accent}--normalize <input>${theme.reset}    Print canonical normalized JSON to stdout and exit 0`,
    `  ${theme.accent}--${theme.reset}                     Treat remaining arguments as positional file paths`,
    '',
    `${theme.heading}Streams:${theme.reset}`,
    '  stdout                 Help text or canonical normalized JSON (when --normalize is set)',
    '  stderr                 Match confirmation, structural capability diffs, errors, and recovery hints',
    '',
    `${theme.heading}Examples:${theme.reset}`,
    '  node rig/xterm/src/expected_compare.mjs target/ci-event-logs/macos/stdout-xterm.json rig/expected/xterm-macos.json',
    '  ./rig/orca/run.sh capability_probe | node rig/xterm/src/expected_compare.mjs - rig/expected/orca.json',
    '  node rig/xterm/src/expected_compare.mjs --normalize target/windows-ci/latest-windows-xterm.json',
  ].join('\n');
}

function consumeNormalizeOption(preDashArgs, argumentIndex, token) {
  if (token === '--normalize') {
    const valueToken = preDashArgs[argumentIndex + 1] ?? '';
    if (valueToken.length === 0 || (valueToken.startsWith('-') && valueToken !== '-')) {
      throw new CliUsageError(
        "Option '--normalize' requires an <input> file path or '-'.",
        'Usage: node rig/xterm/src/expected_compare.mjs --normalize <input-report-or-->',
      );
    }
    return { targetPath: valueToken, shouldSkipNext: true };
  }
  const inlinePath = token.slice('--normalize='.length);
  if (inlinePath.length === 0) {
    throw new CliUsageError(
      "Option '--normalize=' requires a non-empty <input> file path or '-'.",
      'Usage: node rig/xterm/src/expected_compare.mjs --normalize <input-report-or-->',
    );
  }
  return { targetPath: inlinePath, shouldSkipNext: false };
}

function collectCompareTokens(argv) {
  const doubleDashIndex = argv.indexOf('--');
  const preDashArgs = doubleDashIndex === -1 ? argv : argv.slice(0, doubleDashIndex);
  const postDashArgs = doubleDashIndex === -1 ? [] : argv.slice(doubleDashIndex + 1);
  const initialState = {
    positionalArguments: [],
    normalizeTarget: null,
    shouldSkipNext: false,
  };
  const reduced = preDashArgs.reduce((state, token, argumentIndex) => {
    if (state.shouldSkipNext) {
      return { ...state, shouldSkipNext: false };
    }
    if (token === '--normalize' || token.startsWith('--normalize=')) {
      if (state.normalizeTarget !== null) {
        throw new CliUsageError("Duplicate '--normalize' option.", "Pass '--normalize' at most once.");
      }
      const consumed = consumeNormalizeOption(preDashArgs, argumentIndex, token);
      return {
        ...state,
        normalizeTarget: consumed.targetPath,
        shouldSkipNext: consumed.shouldSkipNext,
      };
    }
    if (token !== '-' && token.startsWith('-')) {
      throw new CliUsageError(
        `Unknown option '${token}'.`,
        "Run with '--help' to see valid options, or place '--' before file paths starting with '-'.",
      );
    }
    return {
      ...state,
      positionalArguments: [...state.positionalArguments, token],
    };
  }, initialState);
  return {
    positionalArguments: [...reduced.positionalArguments, ...postDashArgs],
    normalizeTarget: reduced.normalizeTarget,
  };
}

/**
 * Parses CLI tokens for the baseline comparator, returning `{ isHelp,
 * isNormalizeOnly, actualPath, expectedPath }` or throwing `CliUsageError`.
 */
export function parseExpectedCompareArgs(argv) {
  if (hasHelpFlagBeforeDoubleDash(argv)) {
    return { isHelp: true, isNormalizeOnly: false, actualPath: null, expectedPath: null };
  }
  const { positionalArguments, normalizeTarget } = collectCompareTokens(argv);
  if (normalizeTarget !== null) {
    if (positionalArguments.length > 0) {
      throw new CliUsageError(
        `Unexpected extra argument '${positionalArguments[0]}' with '--normalize'.`,
        'Usage: node rig/xterm/src/expected_compare.mjs --normalize <input-report-or-->',
      );
    }
    return {
      isHelp: false,
      isNormalizeOnly: true,
      actualPath: normalizeTarget,
      expectedPath: null,
    };
  }
  if (positionalArguments.length !== 2) {
    throw new CliUsageError(
      positionalArguments.length < 2
        ? 'Expected <actual-report> and <expected-baseline.json> arguments.'
        : `Unexpected positional argument '${positionalArguments[2]}'.`,
      'Usage: node rig/xterm/src/expected_compare.mjs <actual-report-or--> <expected-baseline.json>',
    );
  }
  const [actualPath, expectedPath] = positionalArguments;
  if (expectedPath === '-') {
    throw new CliUsageError(
      "Standard input '-' is valid only for <actual-report-or-->, not <expected-baseline.json>.",
      'Pass a baseline file path (for example rig/expected/orca.json) as <expected-baseline.json>.',
    );
  }
  return { isHelp: false, isNormalizeOnly: false, actualPath, expectedPath };
}

function readInputSourceText(sourcePath, stdinText) {
  if (sourcePath === '-') {
    return stdinText !== undefined ? stdinText : fs.readFileSync(0, 'utf8');
  }
  const resolvedPath = path.resolve(sourcePath);
  if (!fs.existsSync(resolvedPath)) {
    const missingError = new Error(`Input file not found at '${sourcePath}'.`);
    missingError.recoveryHint = `Verify '${sourcePath}' exists and re-run the probe or check the path.`;
    throw missingError;
  }
  return fs.readFileSync(resolvedPath, 'utf8');
}

function writeCliFailure(stderr, stderrTheme, errorMessage, recoveryHint) {
  stderr.write(`${stderrTheme.error}\u274c expected_compare.mjs: ${errorMessage}${stderrTheme.reset}\n`);
  stderr.write(`  ${stderrTheme.warn}\u26a0\ufe0f ${recoveryHint}${stderrTheme.reset}\n`);
}

function reportComparisonOutcome(comparison, expectedPath, stderr, stderrTheme) {
  if (comparison.isMatch) {
    stderr.write(
      `${stderrTheme.success}\u2705 Baseline matched '${expectedPath}'.${stderrTheme.reset}\n`,
    );
    return 0;
  }
  comparison.diffs.forEach((diffLine) => {
    stderr.write(`  ${stderrTheme.error}-${stderrTheme.reset} ${diffLine}\n`);
  });
  writeCliFailure(
    stderr,
    stderrTheme,
    `Capability report diverged from '${expectedPath}' (${comparison.diffs.length} difference(s)).`,
    `Inspect the capability diffs above; if intentional, update '${expectedPath}' and docs/terminals.md together.`,
  );
  return 1;
}

/**
 * Executes the baseline comparison or `--normalize` CLI workflow against I/O
 * streams and returns the numeric exit status (`0`, `1`, or `2`).
 */
export function runExpectedCompareCli({
  argv = process.argv.slice(2),
  stdinText,
  stdout = process.stdout,
  stderr = process.stderr,
  env = process.env,
} = {}) {
  const stdoutTheme = createCliTheme(stdout, env);
  const stderrTheme = createCliTheme(stderr, env);
  try {
    const parsedArgs = parseExpectedCompareArgs(argv);
    if (parsedArgs.isHelp) {
      stdout.write(`${buildExpectedCompareHelpText(stdoutTheme)}\n`);
      return 0;
    }
    const actualText = readInputSourceText(parsedArgs.actualPath, stdinText);
    if (parsedArgs.isNormalizeOnly) {
      const normalized = normalizeProbeReport(actualText);
      stdout.write(`${JSON.stringify(normalized, null, 2)}\n`);
      return 0;
    }
    const expectedText = readInputSourceText(parsedArgs.expectedPath);
    const comparison = compareReportAgainstExpected(actualText, expectedText);
    return reportComparisonOutcome(comparison, parsedArgs.expectedPath, stderr, stderrTheme);
  } catch (caughtError) {
    if (caughtError instanceof CliUsageError) {
      stderr.write(`${buildExpectedCompareHelpText(stderrTheme)}\n\n`);
      writeCliFailure(stderr, stderrTheme, caughtError.message, caughtError.recoveryHint);
      return 2;
    }
    const fallbackHint =
      caughtError?.recoveryHint ?? 'Inspect the probe report file and re-run with --help if needed.';
    writeCliFailure(stderr, stderrTheme, caughtError?.message ?? String(caughtError), fallbackHint);
    return 1;
  }
}

if (isDirectCliExecution(MODULE_FILE_PATH)) {
  process.exit(runExpectedCompareCli());
}
