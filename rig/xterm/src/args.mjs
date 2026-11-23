/**
 * Pure CLI argument parser for the headless xterm.js and tmux harness runners,
 * validating configuration profiles, scenarios, timeouts, and `--` passthrough.
 */

/**
 * Allowed private tmux configuration names accepted by the tmux harness CLI
 * (`'stock'` empty baseline or `'full'` capability-enabled profile).
 */
export const VALID_TMUX_CONFIGS = Object.freeze(['stock', 'full']);

/**
 * Mouse-lab scenario names accepted only for the mouse_lab example.
 */
export const MOUSE_SCENARIOS = Object.freeze([
  'mouse', ...Array.from({ length: 14 }, (_, index) => `x${index + 1}`),
]);

/** Scenario names accepted by the harness parser, including probe and lab checks. */
export const VALID_SCENARIOS = Object.freeze([
  ...MOUSE_SCENARIOS,
  'all',
  'batch',
  'osc52',
  'interactive',
  'panic',
  'passthrough',
]);

/**
 * Default bounded per-condition timeout in milliseconds returned by
 * `parseHarnessArgs` when `--timeout-ms` is omitted on the command line.
 */
export const DEFAULT_TIMEOUT_MS = 20000;

const POSITIVE_INTEGER_PATTERN = /^[1-9][0-9]*$/;

/**
 * Typed CLI usage error carrying an actionable `recoveryHint` string so CLI
 * entrypoints can format usage diagnostics and exit with code `2`.
 */
export class CliUsageError extends Error {
  constructor(message, recoveryHint) {
    super(message);
    this.name = 'CliUsageError';
    this.recoveryHint = recoveryHint;
  }
}

/**
 * Checks whether `-h` or `--help` appears before `--` in `argv`, returning a
 * boolean so CLI entrypoints can short-circuit validation and print help text.
 */
export function hasHelpFlagBeforeDoubleDash(argv) {
  const firstMatch = argv.find(
    (token) => token === '--' || token === '-h' || token === '--help',
  );
  return firstMatch === '-h' || firstMatch === '--help';
}

function parsePositiveTimeoutMs(rawTimeoutValue) {
  if (typeof rawTimeoutValue !== 'string' || !POSITIVE_INTEGER_PATTERN.test(rawTimeoutValue)) {
    throw new CliUsageError(
      `Option '--timeout-ms' requires a positive integer in milliseconds, got '${rawTimeoutValue ?? ''}'.`,
      'Pass a positive integer, for example: --timeout-ms 20000',
    );
  }
  return Number.parseInt(rawTimeoutValue, 10);
}

function splitTokensAndOptions(argv) {
  const doubleDashIndex = argv.indexOf('--');
  const preDashArgs = doubleDashIndex === -1 ? argv : argv.slice(0, doubleDashIndex);
  const extraArgs = doubleDashIndex === -1 ? [] : argv.slice(doubleDashIndex + 1);
  const initialState = {
    positionalArgs: [],
    timeoutMs: DEFAULT_TIMEOUT_MS,
    shouldSkipNext: false,
  };
  const finalState = preDashArgs.reduce((state, token, argIndex) => {
    if (state.shouldSkipNext) {
      return { ...state, shouldSkipNext: false };
    }
    if (token === '--timeout-ms') {
      if (argIndex + 1 >= preDashArgs.length) {
        throw new CliUsageError(
          "Option '--timeout-ms' is missing its millisecond value.",
          'Pass a positive integer after --timeout-ms, for example: --timeout-ms 20000',
        );
      }
      return {
        ...state,
        timeoutMs: parsePositiveTimeoutMs(preDashArgs[argIndex + 1]),
        shouldSkipNext: true,
      };
    }
    if (token.startsWith('--timeout-ms=')) {
      return {
        ...state,
        timeoutMs: parsePositiveTimeoutMs(token.slice('--timeout-ms='.length)),
      };
    }
    if (token.startsWith('-')) {
      throw new CliUsageError(
        `Unknown harness option '${token}'.`,
        "Run with '--help' to see harness options, or place '--' before flags meant for <program>.",
      );
    }
    return { ...state, positionalArgs: [...state.positionalArgs, token] };
  }, initialState);

  return {
    positionalArgs: finalState.positionalArgs,
    extraArgs,
    timeoutMs: finalState.timeoutMs,
  };
}

function consumeTmuxConfigArg(positionalArgs, shouldRequireTmuxConfig) {
  if (!shouldRequireTmuxConfig) {
    return { tmuxConfig: null, nextIndex: 0 };
  }
  const tmuxConfig = positionalArgs[0] ?? '';
  if (!VALID_TMUX_CONFIGS.includes(tmuxConfig)) {
    throw new CliUsageError(
      tmuxConfig.length === 0
        ? "Missing required <config> argument ('stock' or 'full')."
        : `Invalid tmux config '${tmuxConfig}'; expected 'stock' or 'full'.`,
      'Usage: just xterm-tmux <stock|full> <program> [scenario] [-- extra_args...]',
    );
  }
  return { tmuxConfig, nextIndex: 1 };
}

function validatePositionals(positionalArgs, shouldRequireTmuxConfig) {
  const { tmuxConfig, nextIndex } = consumeTmuxConfigArg(positionalArgs, shouldRequireTmuxConfig);
  const program = positionalArgs[nextIndex] ?? '';
  if (program.length === 0) {
    throw new CliUsageError(
      'Missing required <program> argument.',
      shouldRequireTmuxConfig
        ? 'Usage: just xterm-tmux <stock|full> <program> [scenario]'
        : 'Usage: just xterm <program> [scenario]',
    );
  }

  const explicitScenario = positionalArgs[nextIndex + 1] ?? null;
  if (explicitScenario !== null && !VALID_SCENARIOS.includes(explicitScenario)) {
    throw new CliUsageError(
      `Invalid scenario '${explicitScenario}'.`,
      `Expected one of: ${VALID_SCENARIOS.join(', ')}.`,
    );
  }
  if (MOUSE_SCENARIOS.includes(explicitScenario) && program !== 'mouse_lab') {
    throw new CliUsageError(
      `Scenario '${explicitScenario}' requires mouse_lab.`,
      `Run: just xterm mouse_lab ${explicitScenario}`,
    );
  }
  const trailingIndex = explicitScenario === null ? nextIndex + 1 : nextIndex + 2;
  if (trailingIndex < positionalArgs.length) {
    throw new CliUsageError(
      `Unexpected positional argument '${positionalArgs[trailingIndex]}'.`,
      "Use '--' before forwarding extra arguments to <program>.",
    );
  }

  const defaultScenario = program === 'mouse_lab' ? 'mouse' : program === 'capability_probe' ? 'all' : 'passthrough';
  const scenario = explicitScenario ?? defaultScenario;
  return { tmuxConfig, program, scenario };
}

/**
 * Parses CLI tokens for the direct xterm.js runner (`requireTmuxConfig: false`)
 * or tmux runner (`requireTmuxConfig: true`), returning normalized `{ isHelp,
 * tmuxConfig, program, scenario, timeoutMs, extraArgs }` or throwing
 * `CliUsageError`.
 */
export function parseHarnessArgs(argv, { requireTmuxConfig = false } = {}) {
  if (!Array.isArray(argv)) {
    throw new CliUsageError(
      'Expected an array of CLI argument strings.',
      'Pass process.argv.slice(2) to parseHarnessArgs.',
    );
  }
  if (hasHelpFlagBeforeDoubleDash(argv)) {
    return {
      isHelp: true,
      tmuxConfig: null,
      program: null,
      scenario: null,
      timeoutMs: DEFAULT_TIMEOUT_MS,
      extraArgs: [],
    };
  }
  const { positionalArgs, extraArgs, timeoutMs } = splitTokensAndOptions(argv);
  const { tmuxConfig, program, scenario } = validatePositionals(
    positionalArgs,
    Boolean(requireTmuxConfig),
  );
  return { isHelp: false, tmuxConfig, program, scenario, timeoutMs, extraArgs };
}
