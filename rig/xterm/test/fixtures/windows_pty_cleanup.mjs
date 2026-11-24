/** Windows-only subprocess fixture for the pinned ConPTY resource lifecycle. */
import assert from 'node:assert/strict';
import { once } from 'node:events';
import pty from 'node-pty';
import { startPtySession } from '../../src/pty_session.mjs';

function capturePtyCleanup(state, ...args) {
  const ptyProcess = pty.spawn(...args);
  const connection = ptyProcess._agent._conoutSocketWorker;
  state.ptyProcess = ptyProcess;
  state.workerExit = once(connection._worker, 'exit');
  state.inputClose = once(ptyProcess._agent._inSocket, 'close');
  const originalKill = ptyProcess.kill.bind(ptyProcess);
  ptyProcess.kill = () => { state.killCount += 1; originalKill(); };
  const originalDispose = connection.dispose.bind(connection);
  connection.dispose = () => { state.workerDisposeCount += 1; originalDispose(); };
  return ptyProcess;
}

async function runCleanupCase(isNaturalExit) {
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
  await ready;
  if (isNaturalExit) { inputHandler('q'); await session.waitForExit(5000); }
  session.dispose();
  session.dispose();
  if (!isNaturalExit) await session.waitForExit(5000);
  await Promise.all([state.workerExit, state.inputClose]);
  assert.equal(state.killCount, isNaturalExit ? 0 : 1);
  assert.equal(state.workerDisposeCount, 1);
  assert.equal(state.ptyProcess._agent._conoutSocketWorker._worker.threadId, -1);
  assert.equal(state.ptyProcess._agent._inSocket.destroyed, true);
  assert.equal(inputHandler, undefined);
  return { isNaturalExit, killCount: state.killCount, workerDisposeCount: state.workerDisposeCount, closed: true };
}

const outcomes = [];
const cleanupCases = process.argv[2] === 'running' ? [false] : [true, true];
for (const isNaturalExit of cleanupCases) outcomes.push(await runCleanupCase(isNaturalExit));
process.stdout.write(`${JSON.stringify(outcomes)}\n`);
