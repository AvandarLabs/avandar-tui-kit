/** Windows-only subprocess fixture for the pinned ConPTY resource lifecycle. */
import assert from 'node:assert/strict';
import { once } from 'node:events';
import pty from 'node-pty';
import { fileURLToPath } from 'node:url';
import { startPtySession } from '../../src/pty_session.mjs';

function capturePtyCleanup(state, ...args) {
  const ptyProcess = pty.spawn(...args);
  const connection = ptyProcess._agent._conoutSocketWorker;
  state.ptyProcess = ptyProcess;
  // A broken output pipe can emit exit before the native child has an exit code.
  ptyProcess.onExit(({ exitCode }) => { if (exitCode !== undefined) state.hasExited = true; });
  state.workerExit = once(connection._worker, 'exit');
  state.inputClose = once(ptyProcess._agent._inSocket, 'close');
  // Observe early rejection even while the readiness phase still owns control.
  void state.workerExit.catch(() => {});
  void state.inputClose.catch(() => {});
  const originalKill = ptyProcess.kill.bind(ptyProcess);
  ptyProcess.kill = () => { state.killCount += 1; originalKill(); };
  const originalDispose = connection.dispose.bind(connection);
  connection.dispose = () => { state.workerDisposeCount += 1; originalDispose(); };
  return ptyProcess;
}

async function runCleanupCase(isNaturalExit, deadline) {
  let resolveReady;
  const ready = new Promise((resolve) => { resolveReady = resolve; });
  let inputHandler;
  let output = '';
  const state = { killCount: 0, workerDisposeCount: 0 };
  const bridge = {
    isTerminalReady: true, isOpen: true,
    onTerminalInput(handler) { inputHandler = handler; return () => { inputHandler = undefined; }; },
    writeToTerminal(chunk) {
      output += chunk;
      if (output.includes('CLEANUP_READY')) resolveReady();
    },
    async flushTerminalWrites() {},
  };
  const session = startPtySession({
    command: process.execPath,
    args: ['-e', "process.stdin.setRawMode(true); process.stdin.once('data', () => process.exit(0)); process.stdout.write('CLEANUP_READY');"],
    bridge,
    ptySpawner: (...args) => capturePtyCleanup(state, ...args),
  });
  return runOwnedCleanupCase({
    ready, session, state, isNaturalExit, deadline, sendExit: () => inputHandler('q'),
    validateClosed() {
      assert.equal(state.killCount, isNaturalExit ? 0 : 1);
      assert.equal(state.workerDisposeCount, 1);
      assert.equal(state.ptyProcess._agent._conoutSocketWorker._worker.threadId, -1);
      assert.equal(state.ptyProcess._agent._inSocket.destroyed, true);
      assert.equal(inputHandler, undefined);
    },
  });
}

/** Bounds a named fixture phase without replacing its initiating error. */
export async function waitForCleanupPhase(label, operation, timeoutMs = 5000) {
  let failureTimer;
  try {
    return await Promise.race([
      Promise.resolve().then(operation),
      new Promise((resolve, reject) => {
        failureTimer = setTimeout(() => reject(new Error(`Windows PTY cleanup ${label} timed out`)), Math.max(0, timeoutMs));
      }),
    ]);
  } finally {
    clearTimeout(failureTimer);
  }
}

/** Reclaims the captured child even when public PTY kill is deferred. */
export async function reclaimPtyChild(state) {
  const childPid = state.ptyProcess?.pid;
  if (!childPid || state.hasExited) return;
  try {
    process.kill(childPid, 'SIGKILL');
  } catch (error) {
    if (error.code === 'ESRCH') return;
    throw error;
  }
  const deadline = performance.now() + 1500;
  while (!state.hasExited) {
    try {
      process.kill(childPid, 0);
    } catch (error) {
      if (error.code === 'ESRCH') return;
      throw error;
    }
    if (performance.now() >= deadline) throw new Error('Captured PTY child is still live after emergency reclaim');
    await new Promise((resolve) => setImmediate(resolve));
  }
}

async function emergencyCleanup(session, state, reclaimChild) {
  const releases = [
    ['session-dispose', () => session.dispose()],
    ['worker-dispose', () => state.ptyProcess?._agent?._conoutSocketWorker?.dispose()],
    ['input-destroy', () => state.ptyProcess?._agent?._inSocket?.destroy()],
    ['worker-terminate', () => state.ptyProcess?._agent?._conoutSocketWorker?._worker?.terminate()],
    ['child-reclaim', () => reclaimChild(state)],
  ];
  const results = await Promise.allSettled(releases.map(([label, release]) => waitForCleanupPhase(`emergency ${label}`, release, 2000)));
  return results.filter((result) => result.status === 'rejected').map((result) => result.reason);
}

/** Runs the owned lifecycle; forced teardown can never satisfy success checks. */
export async function runOwnedCleanupCase({
  ready, session, state, isNaturalExit, sendExit, validateClosed,
  reclaimChild = reclaimPtyChild, deadline = performance.now() + 10000,
}) {
  const phase = (label, operation) => waitForCleanupPhase(label, operation, Math.min(5000, deadline - performance.now()));
  let hasCompleted = false;
  let initiatingError;
  try {
    await phase('readiness', () => ready);
    if (isNaturalExit) { sendExit(); await phase('child-exit', () => session.waitForExit(5000)); }
    session.dispose();
    session.dispose();
    if (!isNaturalExit) await phase('child-exit', () => session.waitForExit(5000));
    await phase('worker-exit', () => state.workerExit);
    await phase('input-close', () => state.inputClose);
    validateClosed();
    hasCompleted = true;
    return { isNaturalExit, killCount: state.killCount, workerDisposeCount: state.workerDisposeCount, closed: true };
  } catch (error) {
    initiatingError = error;
    throw error;
  } finally {
    if (!hasCompleted) {
      const cleanupErrors = await emergencyCleanup(session, state, reclaimChild);
      if (cleanupErrors.length && initiatingError && Object.isExtensible(initiatingError)) initiatingError.cleanupErrors = cleanupErrors;
    }
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const outcomes = [];
  const cleanupCases = process.argv[2] === 'running' ? [false] : [true, true];
  const deadline = performance.now() + 10000;
  for (const isNaturalExit of cleanupCases) outcomes.push(await runCleanupCase(isNaturalExit, deadline));
  process.stdout.write(`${JSON.stringify(outcomes)}\n`);
}
