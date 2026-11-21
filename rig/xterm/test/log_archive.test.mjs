import assert from 'node:assert/strict';
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
import { runScenario } from '../src/scenarios.mjs';

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
      const archivedFiles = fs.readdirSync(archiveDir);
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
        const okStatus = await runHarnessCliMain({
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
        assert.equal(okStatus, 0);
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
        const failStatus = await runHarnessCliMain({
          argv: ['full', 'custom_demo', 'passthrough'],
          requireTmuxConfig: true,
          scriptName: 'run-tmux.sh',
          createSession() {
            throw new Error('simulated tmux session crash');
          },
        });
        assert.equal(failStatus, 1);
        const errFile = 'xterm-tmux-full-custom_demo-passthrough-error.json';
        const parsedError = JSON.parse(fs.readFileSync(path.join(archiveDir, errFile), 'utf8'));
        assert.match(parsedError.error, /simulated tmux session crash/);
        assert.equal(typeof parsedError.stack, 'string');
        assert.equal(typeof parsedError.timestamp, 'string');
      },
    );
  });
});
