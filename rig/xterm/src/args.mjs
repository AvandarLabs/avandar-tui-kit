/**
 * Pure CLI argument parser for `rig/xterm/run.sh` and `rig/xterm/run-tmux.sh`.
 */

/** Allowed private tmux configuration names for Layer 4 runs. */
export const VALID_TMUX_CONFIGS = Object.freeze(['stock', 'full']);

/** Allowed harness scenario names for Layer 3 and Layer 4 runs. */
export const VALID_SCENARIOS = Object.freeze([
  'all',
  'batch',
  'osc52',
  'interactive',
  'panic',
  'passthrough',
]);

/** Default bounded scenario timeout in milliseconds. */
export const DEFAULT_TIMEOUT_MS = 20000;

const POSITIVE_INTEGER_PATTERN = /^[1-9][0-9]*$/;

/**
 * Typed CLI usage error carrying an actionable recovery hint for stderr diagnostics.
 */
export class CliUsageError extends Error {
  constructor(message, recoveryHint) {
    super(message);
    this.name = 'CliUsageError';
    this.recoveryHint = recoveryHint;
  }
}

function hasHelpFlagBeforeDoubleDash(argv) {
  for (const token of argv) {
    if (token === '--') {
      return false;
    }
    if (token === '-h' || token === '--help') {
      return true;
    }
  }
  return false;
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
  const positionalArgs = [];
  const extraArgs = [];
  let timeoutMs = DEFAULT_TIMEOUT_MS;

  for (let argIndex = 0; argIndex < argv.length; argIndex += 1) {
    const token = argv[argIndex];
    if (token === '--') {
      extraArgs.push(...argv.slice(argIndex + 1));
      break;
    }
    if (token === '--timeout-ms') {
      if (argIndex + 1 >= argv.length || argv[argIndex + 1] === '--') {
        throw new CliUsageError(
          "Option '--timeout-ms' is missing its millisecond value.",
          'Pass a positive integer after --timeout-ms, for example: --timeout-ms 20000',
        );
      }
      timeoutMs = parsePositiveTimeoutMs(argv[argIndex + 1]);
      argIndex += 1;
      continue;
    }
    if (token.startsWith('--timeout-ms=')) {
      timeoutMs = parsePositiveTimeoutMs(token.slice('--timeout-ms='.length));
      continue;
    }
    if (token.startsWith('-')) {
      throw new CliUsageError(
        `Unknown harness option '${token}'.`,
        "Run with '--help' to see harness options, or place '--' before flags meant for <program>.",
      );
    }
    positionalArgs.push(token);
  }

  return { positionalArgs, extraArgs, timeoutMs };
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
  const trailingIndex = explicitScenario === null ? nextIndex + 1 : nextIndex + 2;
  if (trailingIndex < positionalArgs.length) {
    throw new CliUsageError(
      `Unexpected positional argument '${positionalArgs[trailingIndex]}'.`,
      "Use '--' before forwarding extra arguments to <program>.",
    );
  }

  const scenario = explicitScenario ?? (program === 'capability_probe' ? 'all' : 'passthrough');
  return { tmuxConfig, program, scenario };
}

/**
 * Parses CLI tokens for `rig/xterm/run.sh` (`requireTmuxConfig: false`) and
 * `rig/xterm/run-tmux.sh` (`requireTmuxConfig: true`).
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
