/**
 * CLI entrypoint and shared runner building Cargo workspace examples, executing
 * headless xterm.js scenarios, and emitting structured JSON reports on stdout.
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { CliUsageError, parseHarnessArgs } from './args.mjs';
import { archiveHarnessError, archiveHarnessReport, createLifecycleDiagnostics } from './log_archive.mjs';
import { runScenario } from './scenarios.mjs';

const MODULE_FILE_PATH = fileURLToPath(import.meta.url);
const MODULE_DIRECTORY = path.dirname(MODULE_FILE_PATH);
const REPOSITORY_ROOT = path.resolve(MODULE_DIRECTORY, '..', '..', '..');
const VALID_PROGRAM_NAME_PATTERN = /^[A-Za-z0-9_-]+$/;

/**
 * Builds semantic ANSI escape tokens for `stream`, returning empty strings when
 * `NO_COLOR`, `TERM=dumb`, or a non-TTY stream is detected.
 */
export function createCliTheme(stream, env = process.env) {
  const isPlain = !stream?.isTTY || Boolean(env.NO_COLOR) || env.TERM === 'dumb';
  if (isPlain) {
    return {
      reset: '',
      bold: '',
      heading: '',
      accent: '',
      muted: '',
      success: '',
      warn: '',
      error: '',
    };
  }
  return {
    reset: '\x1b[0m',
    bold: '\x1b[1m',
    heading: '\x1b[1;36m',
    accent: '\x1b[36m',
    muted: '\x1b[2m',
    success: '\x1b[32m',
    warn: '\x1b[33m',
    error: '\x1b[31m',
  };
}

function createModeHelpMetadata(theme, isTmux) {
  if (isTmux) {
    return {
      usageLines: [
        `  ${theme.bold}just xterm-tmux${theme.reset} <stock|full> <program> [scenario] [--timeout-ms <ms>] [-- extra_args...]`,
        `  ${theme.bold}./rig/xterm/run-tmux.sh${theme.reset} <stock|full> <program> [scenario] [--timeout-ms <ms>] [-- extra_args...]`,
      ],
      purposeLine:
        '  Build a workspace example binary and run it inside a private tmux server attached to headless xterm.js 6.0.0 (Layer 4).',
      argumentLines: [
        `  ${theme.accent}<stock|full>${theme.reset}           Private tmux config profile (rig/tmux/stock.conf or rig/tmux/full.conf)`,
        `  ${theme.accent}<program>${theme.reset}              Cargo example binary name (for example: capability_probe)`,
        `  ${theme.accent}[scenario]${theme.reset}             Scenario name (default: 'mouse' for mouse_lab, 'all' for capability_probe, 'passthrough' otherwise)`,
      ],
      exampleLines: [
        '  just xterm-tmux full capability_probe',
        '  just xterm-tmux stock capability_probe interactive',
        '  ./rig/xterm/run-tmux.sh full capability_probe osc52 --timeout-ms 25000',
      ],
    };
  }
  return {
    usageLines: [
      `  ${theme.bold}just xterm${theme.reset} <program> [scenario] [--timeout-ms <ms>] [-- extra_args...]`,
      `  ${theme.bold}./rig/xterm/run.sh${theme.reset} <program> [scenario] [--timeout-ms <ms>] [-- extra_args...]`,
    ],
    purposeLine:
      '  Build a workspace example binary and run it inside headless Playwright Chromium xterm.js 6.0.0 (Layer 3).',
    argumentLines: [
      `  ${theme.accent}<program>${theme.reset}              Cargo example binary name (for example: capability_probe)`,
      `  ${theme.accent}[scenario]${theme.reset}             Scenario name (default: 'mouse' for mouse_lab, 'all' for capability_probe, 'passthrough' otherwise)`,
    ],
    exampleLines: [
      '  just xterm mouse_lab x1 -- --query-modes',
      '  just xterm capability_probe',
      '  just xterm capability_probe interactive',
      '  just xterm capability_probe panic --timeout-ms 15000',
      '  ./rig/xterm/run.sh capability_probe batch -- --osc52 "tuikit-probe"',
    ],
  };
}

/**
 * Formats structured `--help` text for the direct xterm.js or tmux harness CLI,
 * returning the multi-section help string written to stdout or usage errors.
 */
export function buildHarnessHelpText(theme, { isTmux = false } = {}) {
  const modeHelp = createModeHelpMetadata(theme, isTmux);
  return [
    `${theme.heading}Usage:${theme.reset}`,
    ...modeHelp.usageLines,
    '',
    `${theme.heading}Purpose:${theme.reset}`,
    modeHelp.purposeLine,
    '',
    `${theme.heading}Arguments:${theme.reset}`,
    ...modeHelp.argumentLines,
    '',
    `${theme.heading}Scenarios:${theme.reset}`,
    `  ${theme.accent}mouse${theme.reset}                  Run all implemented mouse_lab scenarios (mouse_lab only)`,
    `  ${theme.accent}x1 through x14${theme.reset}          Individual mouse_lab scenarios (mouse_lab only):`,
    '    x1 clean exit and mode queries; x2 panic cleanup; x3 ignored hover',
    '    x4 targets; x5 selection drag; x6 multi-click; x7 edge scroll; x8 wheel',
    '    x9 wheel during drag; x10 forwarding; x11 modifiers; x12 wheel arrows',
    '    x13 keyboard selection; x14 buttons and Control (x4-x14 pending)',
    `  ${theme.accent}all${theme.reset}                    Run batch, osc52, interactive, and panic sequentially`,
    `  ${theme.accent}batch${theme.reset}                  Run non-interactive capability_probe sections and verify modes`,
    `  ${theme.accent}osc52${theme.reset}                  Verify OSC 52 clipboard payload delivery`,
    `  ${theme.accent}interactive${theme.reset}            Drive pointer, keyboard, paste, focus, and Kitty Enter checks`,
    `  ${theme.accent}panic${theme.reset}                  Trigger F12 test panic and verify terminal mode teardown`,
    `  ${theme.accent}passthrough${theme.reset}            Run a custom <program> with --log managed by the harness`,
    '',
    `${theme.heading}Options:${theme.reset}`,
    `  ${theme.accent}-h, --help${theme.reset}             Show this help text on stdout and exit 0`,
    `  ${theme.accent}--timeout-ms <ms>${theme.reset}      Bounded per-condition timeout in milliseconds (default: 20000)`,
    `  ${theme.accent}--${theme.reset}                     Forward all remaining arguments verbatim to <program>`,
    `  ${theme.muted}(Note: --log <path> is managed automatically by the harness and must not be passed manually.)${theme.reset}`,
    '',
    `${theme.heading}Streams:${theme.reset}`,
    '  stdout                 Single machine-parseable JSON report object',
    '  stderr                 Build/scenario progress, warnings, errors, and final status summary',
    '',
    `${theme.heading}Examples:${theme.reset}`,
    ...modeHelp.exampleLines,
  ].join('\n');
}

function logProgress(theme, messageText) {
  process.stderr.write(`${theme.accent}==>${theme.reset} ${messageText}\n`);
}

function logSuccess(theme, messageText) {
  process.stderr.write(`${theme.success}\u2705 ${messageText}${theme.reset}\n`);
}

function writeErrorWithHint(theme, scriptName, errorMessage, recoveryHint) {
  process.stderr.write(`${theme.error}\u274c ${scriptName}: ${errorMessage}${theme.reset}\n`);
  process.stderr.write(`  ${theme.warn}\u26a0\ufe0f ${recoveryHint}${theme.reset}\n`);
}

function validateProgramAndExtraArgs(parsedArgs) {
  if (!VALID_PROGRAM_NAME_PATTERN.test(parsedArgs.program)) {
    throw new CliUsageError(
      `Invalid <program> name '${parsedArgs.program}'.`,
      "Pass a valid Cargo example binary name such as 'capability_probe'.",
    );
  }
  const hasManualLogFlag = parsedArgs.extraArgs.some(
    (argItem) => argItem === '--log' || argItem.startsWith('--log='),
  );
  if (hasManualLogFlag) {
    throw new CliUsageError(
      "Do not pass '--log' manually; the harness manages temporary JSONL log files automatically.",
      "Omit '--log' and read the structured JSON report from stdout instead.",
    );
  }
}

/** Returns the minimal feature-aware Cargo command for a workspace example. */
export function buildCargoExampleArgs(programName, manifestPath) {
  const selection = programName === 'mouse_lab' ? ['-p', 'ratatui-mouse'] : ['--workspace'];
  const features = programName === 'mouse_lab' ? ['--features', 'vt100'] : [];
  return ['build', '--manifest-path', manifestPath, ...selection, '--example', programName, ...features];
}

function invokeCargoExampleBuild(programName, stderrTheme) {
  const manifestPath = path.join(REPOSITORY_ROOT, 'Cargo.toml');
  logProgress(stderrTheme, `Building workspace example '${programName}'...`);
  const buildOutcome = spawnSync(
    'cargo',
    buildCargoExampleArgs(programName, manifestPath),
    {
      stdio: ['ignore', 'ignore', 'inherit'],
      // Apply the stderr plain-output policy even when CI forces Cargo color.
      env: stderrTheme.reset ? process.env : { ...process.env, CARGO_TERM_COLOR: 'never' },
    },
  );
  if (buildOutcome.error) {
    const cargoMissing = new Error(`Failed to invoke cargo: ${buildOutcome.error.message}`);
    cargoMissing.recoveryHint = 'Install Rust via rustup and ensure cargo is available in PATH.';
    throw cargoMissing;
  }
  if (buildOutcome.status !== 0) {
    const buildFailed = new Error(`Cargo failed to build example '${programName}'.`);
    buildFailed.recoveryHint =
      'Check available example names under rig/probe/examples/ and fix any compile errors above.';
    throw buildFailed;
  }
}

function ensureCompiledProgramBinary(programName, stderrTheme) {
  const overrideBinary = process.env.RIG_XTERM_PROGRAM_BIN ?? '';
  if (overrideBinary.length > 0) {
    const overrideBinaryPath = path.resolve(overrideBinary);
    if (!fs.existsSync(overrideBinaryPath)) {
      const missingError = new Error(
        `RIG_XTERM_PROGRAM_BIN does not exist at '${overrideBinaryPath}'.`,
      );
      missingError.recoveryHint =
        'Unset RIG_XTERM_PROGRAM_BIN or point it to an executable binary.';
      throw missingError;
    }
    return overrideBinaryPath;
  }
  invokeCargoExampleBuild(programName, stderrTheme);
  const targetDirectory = process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(REPOSITORY_ROOT, 'target');
  const executableName = process.platform === 'win32' ? `${programName}.exe` : programName;
  const binaryPath = path.join(targetDirectory, 'debug', 'examples', executableName);
  if (!fs.existsSync(binaryPath)) {
    const missingBinary = new Error(`Compiled example binary was not found at '${binaryPath}'.`);
    missingBinary.recoveryHint = `Verify '${programName}' is declared as a Cargo workspace example.`;
    throw missingBinary;
  }
  return binaryPath;
}

function handleCliFailure(caughtError, stderrTheme, scriptName, shouldRequireTmuxConfig) {
  if (caughtError instanceof CliUsageError) {
    const helpBlock = buildHarnessHelpText(stderrTheme, { isTmux: shouldRequireTmuxConfig });
    process.stderr.write(`${helpBlock}\n\n`);
    writeErrorWithHint(stderrTheme, scriptName, caughtError.message, caughtError.recoveryHint);
    return 2;
  }
  const fallbackHint =
    caughtError?.recoveryHint ??
    `Re-run with a larger '--timeout-ms' or inspect stderr diagnostics above.`;
  writeErrorWithHint(
    stderrTheme,
    scriptName,
    caughtError?.message ?? String(caughtError),
    fallbackHint,
  );
  return 1;
}

async function executeAndArchiveScenario({
  parsedArgs,
  programPath,
  createSession,
  readTmuxBuffer,
  diagnostics,
}) {
  try {
    const report = await diagnostics.run('scenario-loop', () => runScenario(parsedArgs.scenario, {
      program: parsedArgs.program,
      programPath,
      extraArgs: parsedArgs.extraArgs,
      timeoutMs: parsedArgs.timeoutMs,
      tmuxMode: parsedArgs.tmuxConfig,
      createSession,
      readTmuxBuffer,
      diagnostics,
    }));
    diagnostics.runSync('report-archive', () => archiveHarnessReport({ report, parsedArgs }));
    return report;
  } catch (scenarioError) {
    try {
      archiveHarnessError({ error: scenarioError, parsedArgs });
    } catch {
      // Preserve the primary scenario failure if error-report archiving fails.
    }
    throw scenarioError;
  }
}

function createHarnessDiagnostics(parsedArgs, observeProcessLifecycle) {
  const diagnostics = createLifecycleDiagnostics({ labels: {
    program: parsedArgs.program, scenario: parsedArgs.scenario, tmuxMode: parsedArgs.tmuxConfig,
  } });
  if (observeProcessLifecycle && diagnostics.filePath !== null) {
    process.once('beforeExit', (code) => diagnostics.record('process-before-exit', 'return', { code }));
    process.once('exit', (code) => diagnostics.record('process-exit', 'return', { code }));
  }
  return diagnostics;
}

/**
 * Parses CLI flags, builds `<program>`, executes the requested scenario, writes
 * the JSON report to `stdout`, and returns the numeric CLI exit status.
 */
export async function runHarnessCliMain({
  argv = process.argv.slice(2),
  scriptName = 'run.sh',
  requireTmuxConfig = false,
  observeProcessLifecycle = false,
  createSession,
  readTmuxBuffer,
} = {}) {
  const stdoutTheme = createCliTheme(process.stdout);
  const stderrTheme = createCliTheme(process.stderr);
  try {
    const parsedArgs = parseHarnessArgs(argv, { requireTmuxConfig });
    if (parsedArgs.isHelp) {
      process.stdout.write(`${buildHarnessHelpText(stdoutTheme, { isTmux: requireTmuxConfig })}\n`);
      return 0;
    }
    validateProgramAndExtraArgs(parsedArgs);
    const diagnostics = createHarnessDiagnostics(parsedArgs, observeProcessLifecycle);
    const programPath = diagnostics.runSync('build', () => ensureCompiledProgramBinary(parsedArgs.program, stderrTheme));
    const layerNumber = parsedArgs.tmuxConfig === null ? 3 : 4;
    const modeSuffix = parsedArgs.tmuxConfig ? ` [tmux=${parsedArgs.tmuxConfig}]` : '';
    logProgress(
      stderrTheme,
      `Running Layer ${layerNumber} xterm.js scenario '${parsedArgs.scenario}' for '${parsedArgs.program}'${modeSuffix}...`,
    );
    const report = await executeAndArchiveScenario({
      parsedArgs,
      programPath,
      createSession,
      readTmuxBuffer,
      diagnostics,
    });
    diagnostics.runSync('report-stdout', () => process.stdout.write(`${JSON.stringify(report)}\n`));
    logSuccess(
      stderrTheme,
      `Completed '${parsedArgs.program}' (${parsedArgs.scenario})${modeSuffix} in headless xterm.js.`,
    );
    return 0;
  } catch (caughtError) {
    return handleCliFailure(caughtError, stderrTheme, scriptName, requireTmuxConfig);
  }
}

function canonicalizeExistingPath(targetPath) {
  if (!targetPath) {
    return '';
  }
  const absolutePath = path.resolve(targetPath);
  const readRealPath = fs.realpathSync.native ?? fs.realpathSync;
  try {
    return readRealPath(absolutePath);
  } catch {
    return absolutePath;
  }
}

/**
 * Checks whether Node was invoked directly on `moduleFilePath` (following
 * symlinks) rather than importing the module from a test or peer script.
 */
export function isDirectCliExecution(moduleFilePath, entryPath = process.argv[1]) {
  return (
    Boolean(entryPath) &&
    canonicalizeExistingPath(entryPath) === canonicalizeExistingPath(moduleFilePath)
  );
}

if (isDirectCliExecution(MODULE_FILE_PATH)) {
  const exitStatus = await runHarnessCliMain({ observeProcessLifecycle: true });
  process.exitCode = exitStatus;
}
