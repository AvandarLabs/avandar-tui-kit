import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import * as diagnosticsModule from '../src/log_archive.mjs';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { runHarnessCliMain } from '../src/harness.mjs';
import {
  archiveHarnessError,
  archiveHarnessReport,
  archiveSessionProbeLog,
  resetArchiveSessionSequence,
} from '../src/log_archive.mjs';
import { cleanupManagedSession, runScenario, withManagedSession } from '../src/scenarios.mjs';
import { startPtySession } from '../src/pty_session.mjs';

function withTempDirectory(prefix, callback) {
  const directoryPath = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  return Promise.resolve()
    .then(() => callback(directoryPath))
    .finally(() => {
      fs.rmSync(directoryPath, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
    });
}

async function withProcessEnvOverrides(envOverrides, callback) {
  const previousValues = new Map();
  for (const [key, value] of Object.entries(envOverrides)) {
    previousValues.set(key, process.env[key]);
    if (value === undefined) {
      delete process.env[key];
    } else {
      process.env[key] = value;
    }
  }
  try {
    return await callback();
  } finally {
    for (const [key, previousValue] of previousValues.entries()) {
      if (previousValue === undefined) {
        delete process.env[key];
      } else {
        process.env[key] = previousValue;
      }
    }
  }
}

function extractLogPathFromArgs(programArgs) {
  const flagIndex = programArgs.indexOf('--log');
  return flagIndex >= 0 ? programArgs[flagIndex + 1] : null;
}

function createFakeStubBinary(directoryPath) {
  const binaryPath = path.join(directoryPath, 'stub_probe');
  fs.writeFileSync(binaryPath, '#!/usr/bin/env bash\nexit 0\n', { mode: 0o755 });
  return binaryPath;
}

test('log_archive helpers no-op when RIG_XTERM_LOG_DIR is unset or blank', async () => {
  await withTempDirectory('tuikit-log-noop-', async (tempDir) => {
    const sampleLog = path.join(tempDir, 'probe.jsonl');
    fs.writeFileSync(sampleLog, '{"kind":"ready"}\n', 'utf8');
    const parsedArgs = { program: 'capability_probe', scenario: 'batch', tmuxConfig: null };

    for (const blankEnv of [{}, { RIG_XTERM_LOG_DIR: '' }, { RIG_XTERM_LOG_DIR: '   ' }]) {
      assert.equal(
        archiveSessionProbeLog({ logPath: sampleLog, label: 'batch', env: blankEnv }),
        null,
      );
      assert.equal(archiveHarnessReport({ report: { ok: true }, parsedArgs, env: blankEnv }), null);
      assert.equal(
        archiveHarnessError({ error: new Error('boom'), parsedArgs, env: blankEnv }),
        null,
      );
    }
  });
});

test('archiveSessionProbeLog copies existing logs with monotonic sequence and prefix', async () => {
  await withTempDirectory('tuikit-log-seq-', async (tempDir) => {
    resetArchiveSessionSequence();
    const archiveDir = path.join(tempDir, 'nested', 'ci-logs');
    const firstLog = path.join(tempDir, 'one.jsonl');
    const secondLog = path.join(tempDir, 'two.jsonl');
    fs.writeFileSync(firstLog, '{"step":1}\n', 'utf8');
    fs.writeFileSync(secondLog, '{"step":2}\n', 'utf8');

    const env = { RIG_XTERM_LOG_DIR: archiveDir };
    const missingPath = path.join(tempDir, 'missing.jsonl');
    assert.equal(archiveSessionProbeLog({ logPath: missingPath, label: 'ignored', env }), null);

    const archivedFirst = archiveSessionProbeLog({
      logPath: firstLog,
      label: 'capability_probe-batch',
      tmuxMode: null,
      env,
    });
    const archivedSecond = archiveSessionProbeLog({
      logPath: secondLog,
      label: 'capability_probe-osc52-plain',
      tmuxMode: 'full',
      env,
    });

    assert.equal(path.basename(archivedFirst), 'xterm-01-capability_probe-batch.jsonl');
    assert.equal(
      path.basename(archivedSecond),
      'xterm-tmux-full-02-capability_probe-osc52-plain.jsonl',
    );
    assert.equal(fs.readFileSync(archivedFirst, 'utf8'), '{"step":1}\n');
    assert.equal(fs.readFileSync(archivedSecond, 'utf8'), '{"step":2}\n');
  });
});

test('runScenario preserves partial probe.jsonl in RIG_XTERM_LOG_DIR when a session fails', async () => {
  await withTempDirectory('tuikit-log-fail-', async (tempDir) => {
    resetArchiveSessionSequence();
    const archiveDir = path.join(tempDir, 'archived');
    let capturedScratchLogPath = null;
    await withProcessEnvOverrides({ RIG_XTERM_LOG_DIR: archiveDir }, async () => {
      await assert.rejects(
        () =>
          runScenario('passthrough', {
            program: 'custom_probe',
            programPath: '/bin/true',
            createSession({ programArgs }) {
              capturedScratchLogPath = extractLogPathFromArgs(programArgs);
              const eventLine = '{"kind":"event","note":"captured-before-crash"}\n';
              fs.writeFileSync(capturedScratchLogPath, eventLine, 'utf8');
              return {
                exitPromise: Promise.resolve({ exitCode: 9, signal: 0 }),
                async waitForExit() {
                  return { exitCode: 9, signal: 0 };
                },
                async dispose() {},
              };
            },
          }),
        /passthrough program exited with 9/,
      );
      assert.equal(fs.existsSync(capturedScratchLogPath), false);
      const archivedFiles = fs.readdirSync(archiveDir).filter((name) => name.startsWith('xterm-'));
      assert.deepEqual(archivedFiles, ['xterm-01-custom_probe-passthrough.jsonl']);
      const archivedContent = fs.readFileSync(path.join(archiveDir, archivedFiles[0]), 'utf8');
      assert.match(archivedContent, /captured-before-crash/);
    });
  });
});

test('runHarnessCliMain archives formatted report JSON on success', async () => {
  await withTempDirectory('tuikit-harness-ok-', async (tempDir) => {
    resetArchiveSessionSequence();
    const archiveDir = path.join(tempDir, 'reports');
    const stubBinary = createFakeStubBinary(tempDir);
    await withProcessEnvOverrides(
      { RIG_XTERM_LOG_DIR: archiveDir, RIG_XTERM_PROGRAM_BIN: stubBinary },
      async () => {
        const successExitCode = await runHarnessCliMain({
          argv: ['custom_demo', 'passthrough'],
          createSession({ programArgs }) {
            fs.writeFileSync(extractLogPathFromArgs(programArgs), '{"kind":"summary"}\n', 'utf8');
            return {
              exitPromise: Promise.resolve({ exitCode: 0, signal: 0 }),
              async waitForExit() {
                return { exitCode: 0, signal: 0 };
              },
              async dispose() {},
            };
          },
        });
        assert.equal(successExitCode, 0);
        const reportPath = path.join(archiveDir, 'xterm-custom_demo-passthrough-report.json');
        const parsedReport = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
        assert.equal(parsedReport.program, 'custom_demo');
        assert.equal(parsedReport.scenario, 'passthrough');
      },
    );
  });
});

test('runHarnessCliMain archives formatted error JSON when runScenario throws', async () => {
  await withTempDirectory('tuikit-harness-err-', async (tempDir) => {
    resetArchiveSessionSequence();
    const archiveDir = path.join(tempDir, 'errors');
    const stubBinary = createFakeStubBinary(tempDir);
    await withProcessEnvOverrides(
      { RIG_XTERM_LOG_DIR: archiveDir, RIG_XTERM_PROGRAM_BIN: stubBinary },
      async () => {
        const failureExitCode = await runHarnessCliMain({
          argv: ['full', 'custom_demo', 'passthrough'],
          requireTmuxConfig: true,
          scriptName: 'run-tmux.sh',
          createSession() {
            throw new Error('simulated tmux session crash');
          },
        });
        assert.equal(failureExitCode, 1);
        const errorFileName = 'xterm-tmux-full-custom_demo-passthrough-error.json';
        const errorFilePath = path.join(archiveDir, errorFileName);
        const parsedError = JSON.parse(fs.readFileSync(errorFilePath, 'utf8'));
        assert.match(parsedError.error, /simulated tmux session crash/);
        assert.equal(typeof parsedError.stack, 'string');
        assert.equal(typeof parsedError.timestamp, 'string');
      },
    );
  });
});

test('log_archive advances past existing and racing sequences without overwriting files', async () => {
  await withTempDirectory('tuikit-log-multiprocess-', async (tempDir) => {
    const archiveDir = path.join(tempDir, 'shared');
    fs.mkdirSync(archiveDir, { recursive: true });
    fs.writeFileSync(path.join(archiveDir, 'xterm-04-prior-run.jsonl'), '{}\n', 'utf8');
    const newLog = path.join(tempDir, 'fresh.jsonl');
    fs.writeFileSync(newLog, '{"fresh":true}\n', 'utf8');

    resetArchiveSessionSequence();
    const env = { RIG_XTERM_LOG_DIR: archiveDir };
    const realReaddirSync = fs.readdirSync;
    const racingPath = path.join(archiveDir, 'xterm-05-batch.jsonl');
    fs.readdirSync = (targetDir, ...rest) => {
      const entries = realReaddirSync(targetDir, ...rest);
      fs.writeFileSync(racingPath, '{"raced":true}\n', 'utf8');
      fs.readdirSync = realReaddirSync;
      return entries;
    };
    try {
      const archivedLog = archiveSessionProbeLog({ logPath: newLog, label: 'batch', env });
      assert.equal(path.basename(archivedLog), 'xterm-06-batch.jsonl');
      assert.equal(fs.readFileSync(racingPath, 'utf8'), '{"raced":true}\n');
    } finally {
      fs.readdirSync = realReaddirSync;
    }

    const parsedArgs = { program: 'capability_probe', scenario: 'batch', tmuxConfig: null };
    const firstReport = archiveHarnessReport({ report: { run: 1 }, parsedArgs, env });
    const secondReport = archiveHarnessReport({ report: { run: 2 }, parsedArgs, env });
    assert.notEqual(firstReport, secondReport);
    assert.deepEqual(JSON.parse(fs.readFileSync(firstReport, 'utf8')), { run: 1 });
    assert.deepEqual(JSON.parse(fs.readFileSync(secondReport, 'utf8')), { run: 2 });

    const firstError = archiveHarnessError({ error: new Error('first'), parsedArgs, env });
    const secondError = archiveHarnessError({ error: new Error('second'), parsedArgs, env });
    assert.notEqual(firstError, secondError);
    assert.equal(JSON.parse(fs.readFileSync(firstError, 'utf8')).error, 'first');
    assert.equal(JSON.parse(fs.readFileSync(secondError, 'utf8')).error, 'second');
  });
});


function readLifecycle(diagnostics) {
  return fs.readFileSync(diagnostics.filePath, 'utf8').trim().split('\n').map(JSON.parse);
}

test('lifecycle writes enter before unresolved operations and preserves thrown error identity', async () => {
  assert.equal(typeof diagnosticsModule.createLifecycleDiagnostics, 'function');
  await withTempDirectory('tuikit-lifecycle-', async (directory) => {
    let time = 100;
    const diagnostics = diagnosticsModule.createLifecycleDiagnostics({
      env: { RIG_XTERM_LOG_DIR: directory }, clock: () => time,
      labels: { session: 'mouse_lab-x11-alt' },
    });
    let resolveCleanup;
    const heldCleanup = new Promise((resolve) => { resolveCleanup = resolve; });
    const pending = diagnostics.run('session-dispose', () => {
      const records = readLifecycle(diagnostics);
      assert.deepEqual(records.map(({ boundary, phase }) => [boundary, phase]), [['session-dispose', 'enter']]);
      return heldCleanup;
    });
    assert.equal(readLifecycle(diagnostics).length, 1);
    time = 125;
    resolveCleanup('disposed');
    assert.equal(await pending, 'disposed');
    const failure = Object.assign(new Error('native spawn failed'), { code: 'EIO', syscall: 'spawn' });
    assert.throws(() => diagnostics.runSync('pty-spawn', () => { throw failure; }), (error) => error === failure);
    const records = readLifecycle(diagnostics);
    assert.deepEqual(records.map(({ phase, elapsedMs }) => [phase, elapsedMs]),
      [['enter', 0], ['return', 25], ['enter', 25], ['error', 25]]);
    assert.ok(records.every((record) => record.session === 'mouse_lab-x11-alt' && record.harnessPid === process.pid));
    assert.deepEqual(records.at(-1).error, { name: 'Error', message: 'native spawn failed', code: 'EIO', syscall: 'spawn' });
    const other = diagnosticsModule.createLifecycleDiagnostics({ env: { RIG_XTERM_LOG_DIR: directory } });
    assert.notEqual(other.filePath, diagnostics.filePath);
  });
});

test('lifecycle managed session survives blocked X11 startup without an app archive', async () => {
  assert.equal(typeof diagnosticsModule.createLifecycleDiagnostics, 'function');
  await withTempDirectory('tuikit-lifecycle-start-', async (directory) => {
    const diagnostics = diagnosticsModule.createLifecycleDiagnostics({ env: { RIG_XTERM_LOG_DIR: directory } });
    let rejectBridge;
    const heldBridge = new Promise((resolve, reject) => { rejectBridge = reject; });
    const failure = new Error('bridge failed');
    const pending = withManagedSession({ program: 'mouse_lab', diagnostics, timeoutMs: 50,
      createBridge: () => heldBridge }, [], () => assert.fail('callback must not run'), 'x11-shift');
    const records = readLifecycle(diagnostics);
    assert.deepEqual(records.map(({ boundary, phase, session }) => [boundary, phase, session]), [
      ['scenario', 'enter', 'mouse_lab-x11-shift'], ['bridge-start', 'enter', 'mouse_lab-x11-shift'],
    ]);
    assert.equal(fs.readdirSync(directory).length, 1);
    rejectBridge(failure);
    await assert.rejects(pending, (error) => error === failure);
    assert.ok(readLifecycle(diagnostics).some((record) => record.boundary === 'bridge-start' && record.phase === 'error'));
    assert.equal(readLifecycle(diagnostics).at(-1).phase, 'error');
  });
});

test('lifecycle managed cleanup is on disk while disposal remains unresolved', async () => {
  assert.equal(typeof diagnosticsModule.createLifecycleDiagnostics, 'function');
  await withTempDirectory('tuikit-lifecycle-cleanup-', async (directory) => {
    const diagnostics = diagnosticsModule.createLifecycleDiagnostics({ env: { RIG_XTERM_LOG_DIR: directory } });
    const entered = Promise.withResolvers();
    const heldCleanup = Promise.withResolvers();
    const pending = withManagedSession({ program: 'mouse_lab', diagnostics,
      createBridge: async () => ({ close: async () => {} }),
      createSession: () => ({ pid: 234, dispose: () => { entered.resolve(); return heldCleanup.promise; } }),
    }, [], () => 'result', 'x10');
    await entered.promise;
    const records = readLifecycle(diagnostics);
    assert.equal(records.at(-1).boundary, 'session-dispose');
    assert.equal(records.at(-1).phase, 'enter');
    assert.ok(records.some((record) => record.boundary === 'archive' && record.phase === 'return'));
    assert.ok(records.some((record) => record.ptyPid === 234));
    assert.ok(!records.some((record) => record.boundary === 'scenario' && record.phase === 'return'));
    heldCleanup.resolve();
    assert.equal(await pending, 'result');
    assert.equal(readLifecycle(diagnostics).at(-1).boundary, 'scenario');
    assert.equal(readLifecycle(diagnostics).at(-1).phase, 'return');
  });
});

test('lifecycle artifact never leaks into machine stdout or plain stderr and is disabled without log directory', async () => {
  assert.equal(typeof diagnosticsModule.createLifecycleDiagnostics, 'function');
  await withTempDirectory('tuikit-lifecycle-streams-', async (directory) => {
    const moduleUrl = new URL('../src/log_archive.mjs', import.meta.url).href;
    const source = `import { createLifecycleDiagnostics } from ${JSON.stringify(moduleUrl)};
      const diagnostics = createLifecycleDiagnostics();
      await diagnostics.run('bridge-start', async () => {});
      process.stdout.write(JSON.stringify({scenario:'mouse',passed:true})+'\\n');
      process.stderr.write('expected plain diagnostic\\n');`;
    for (const logDirectory of [directory, '']) {
      const outcome = spawnSync(process.execPath, ['--input-type=module', '-e', source], {
        encoding: 'utf8', env: { ...process.env, RIG_XTERM_LOG_DIR: logDirectory },
      });
      assert.equal(outcome.status, 0, outcome.stderr);
      assert.deepEqual(JSON.parse(outcome.stdout), { scenario: 'mouse', passed: true });
      assert.equal(outcome.stderr, 'expected plain diagnostic\n');
      assert.doesNotMatch(outcome.stdout + outcome.stderr, /\x1b|elapsedMs|bridge-start/);
    }
    assert.equal(fs.readdirSync(directory).length, 1);
    const disabled = diagnosticsModule.createLifecycleDiagnostics({ env: {} });
    assert.equal(disabled.filePath, null);
    assert.equal(await disabled.run('cleanup', () => 12), 12);
  });
});

test('child outcome diagnostics preserve timeout and signal fields plus partial machine stdout', async () => {
  assert.equal(typeof diagnosticsModule.describeChildOutcome, 'function');
  await withTempDirectory('tuikit-child-outcome-', async (directory) => {
    const error = Object.assign(new Error('spawnSync node ETIMEDOUT'), { code: 'ETIMEDOUT', syscall: 'spawnSync node' });
    const diagnostic = diagnosticsModule.describeChildOutcome({ status: null, signal: 'SIGTERM', error,
      stdout: '{"scenario":', stderr: 'child stderr\n' }, {
      label: 'mouse-direct', env: { RIG_XTERM_LOG_DIR: directory },
    });
    assert.match(diagnostic, /"status":null/);
    assert.match(diagnostic, /"signal":"SIGTERM"/);
    assert.match(diagnostic, /"code":"ETIMEDOUT"/);
    assert.match(diagnostic, /"syscall":"spawnSync node"/);
    assert.match(diagnostic, /spawnSync node ETIMEDOUT/);
    assert.match(diagnostic, /child stderr/);
    assert.doesNotMatch(diagnostic, /\x1b/);
    const artifact = JSON.parse(fs.readFileSync(path.join(directory, fs.readdirSync(directory)[0]), 'utf8'));
    assert.equal(artifact.stdout, '{"scenario":');
    assert.equal(artifact.status, null);
    const signalDiagnostic = diagnosticsModule.describeChildOutcome({ status: null, signal: 'SIGKILL', error: undefined, stderr: '' }, { env: {} });
    assert.match(signalDiagnostic, /"error":null/);
    assert.match(signalDiagnostic, /SIGKILL/);
    assert.equal(diagnosticsModule.describeChildOutcome({status: 0, signal: null, stderr: ''}, {env: { RIG_XTERM_LOG_DIR: directory }}), '');
    assert.equal(fs.readdirSync(directory).length, 1);
  });
});


test('lifecycle imported runner does not retain process hooks after its artifact directory is removed', async () => {
  await withTempDirectory('tuikit-lifecycle-import-', async (directory) => {
    const moduleUrl = new URL('../src/harness.mjs', import.meta.url).href;
    const source = `import fs from 'node:fs';
      import { runHarnessCliMain } from ${JSON.stringify(moduleUrl)};
      const status = await runHarnessCliMain({argv:['mouse_lab','x1']});
      fs.rmSync(process.env.RIG_XTERM_LOG_DIR,{recursive:true,force:true});
      process.stdout.write(JSON.stringify({status}));`;
    const outcome = spawnSync(process.execPath, ['--input-type=module', '-e', source], {
      encoding: 'utf8', env: { ...process.env, RIG_XTERM_LOG_DIR: path.join(directory, 'logs'), RIG_XTERM_PROGRAM_BIN: path.join(directory, 'missing') },
    });
    assert.equal(outcome.status, 0, outcome.stderr);
    assert.deepEqual(JSON.parse(outcome.stdout), { status: 1 });
    assert.doesNotMatch(outcome.stderr, /ENOENT|\x1b/);
  });
});


function makeLifecycleArtifactUnwritable(diagnostics) {
  fs.rmSync(diagnostics.filePath);
  fs.mkdirSync(diagnostics.filePath);
}

test('lifecycle append failure before cleanup cannot skip disposal, bridge closure or scratch removal', async () => {
  await withTempDirectory('tuikit-lifecycle-cleanup-write-error-', async (directory) => {
    const diagnostics = diagnosticsModule.createLifecycleDiagnostics({ env: { RIG_XTERM_LOG_DIR: directory } });
    const scratch = path.join(directory, 'scratch');
    fs.mkdirSync(scratch);
    fs.writeFileSync(path.join(scratch, 'owned-resource'), 'pending');
    let isSessionDisposed = false;
    let isBridgeClosed = false;
    makeLifecycleArtifactUnwritable(diagnostics);
    await cleanupManagedSession(
      { dispose() { isSessionDisposed = true; } },
      { close() { isBridgeClosed = true; } }, scratch, diagnostics,
    );
    assert.equal(isSessionDisposed, true);
    assert.equal(isBridgeClosed, true);
    assert.equal(fs.existsSync(scratch), false);
  });
});

test('lifecycle append failure while recording operation errors preserves synchronous and async error identity', async () => {
  await withTempDirectory('tuikit-lifecycle-operation-write-error-', async (directory) => {
    for (const method of ['runSync', 'run']) {
      const diagnostics = diagnosticsModule.createLifecycleDiagnostics({ env: { RIG_XTERM_LOG_DIR: directory } });
      const originalError = new Error(`original ${method} failure`);
      await assert.rejects(async () => diagnostics[method]('operation', () => {
        assert.equal(readLifecycle(diagnostics).at(-1).phase, 'enter');
        makeLifecycleArtifactUnwritable(diagnostics);
        throw originalError;
      }), (error) => error === originalError);
    }
  });
});

test('lifecycle append failure in PTY exit callback cannot prevent exit bookkeeping or promise resolution', async () => {
  await withTempDirectory('tuikit-lifecycle-exit-write-error-', async (directory) => {
    const diagnostics = diagnosticsModule.createLifecycleDiagnostics({ env: { RIG_XTERM_LOG_DIR: directory } });
    let notifyExit;
    let isInputUnsubscribed = false;
    const session = startPtySession({
      command: 'unused', diagnostics,
      bridge: { isTerminalReady: true, isOpen: false,
        onTerminalInput: () => () => { isInputUnsubscribed = true; } },
      ptySpawner: () => ({ pid: 456, onData() {}, onExit(callback) { notifyExit = callback; } }),
    });
    makeLifecycleArtifactUnwritable(diagnostics);
    assert.doesNotThrow(() => notifyExit({ exitCode: 17, signal: 0 }));
    assert.equal(isInputUnsubscribed, true);
    assert.deepEqual(await session.waitForExit(100), { exitCode: 17, signal: 0 });
  });
});
