import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { CliUsageError, parseHarnessArgs } from '../src/args.mjs';
import { createBridgeServer, startBrowserBridge } from '../src/bridge_server.mjs';
import { decodeOsc52Payload } from '../src/osc52.mjs';
import { startBridgeAndPtySession, startPtySession } from '../src/pty_session.mjs';
import {
  buildReadySentinelPath,
  readJsonLines,
  waitForProbeReady,
} from '../src/ready_waiter.mjs';

function createTempTestDir(prefix) {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

function waitForTerminalInputMatching(bridge, isMatch, timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    let accumulatedInput = '';
    const unsubscribe = bridge.onTerminalInput((inputChunk) => {
      accumulatedInput += inputChunk;
      if (isMatch(accumulatedInput)) {
        clearTimeout(failureTimer);
        unsubscribe();
        resolve(accumulatedInput);
      }
    });
    const failureTimer = setTimeout(() => {
      unsubscribe();
      reject(
        new Error(
          `Timed out after ${timeoutMs}ms waiting for terminal input; got ${JSON.stringify(accumulatedInput)}`,
        ),
      );
    }, timeoutMs);
  });
}

test('parseHarnessArgs parses help flags before double-dash and forwards after double-dash', () => {
  assert.equal(parseHarnessArgs(['-h']).isHelp, true);
  assert.equal(parseHarnessArgs(['--help'], { requireTmuxConfig: true }).isHelp, true);

  const forwardedParsed = parseHarnessArgs(['capability_probe', 'batch', '--', '--help']);
  assert.equal(forwardedParsed.isHelp, false);
  assert.deepEqual(forwardedParsed.extraArgs, ['--help']);
});

test('parseHarnessArgs applies scenario defaults and parses options in plain mode', () => {
  const probeDefaults = parseHarnessArgs(['capability_probe']);
  assert.deepEqual(probeDefaults, {
    isHelp: false,
    tmuxConfig: null,
    program: 'capability_probe',
    scenario: 'all',
    timeoutMs: 20000,
    extraArgs: [],
  });

  const labDefaults = parseHarnessArgs(['mouse_lab']);
  assert.equal(labDefaults.scenario, 'passthrough');

  const explicitOptions = parseHarnessArgs([
    'capability_probe',
    'interactive',
    '--timeout-ms',
    '15000',
    '--',
    '--push-kitty',
  ]);
  assert.equal(explicitOptions.scenario, 'interactive');
  assert.equal(explicitOptions.timeoutMs, 15000);
  assert.deepEqual(explicitOptions.extraArgs, ['--push-kitty']);
});

test('parseHarnessArgs parses tmux config mode and validates stock or full', () => {
  const stockParsed = parseHarnessArgs(['stock', 'capability_probe'], {
    requireTmuxConfig: true,
  });
  assert.equal(stockParsed.tmuxConfig, 'stock');
  assert.equal(stockParsed.program, 'capability_probe');
  assert.equal(stockParsed.scenario, 'all');

  const fullParsed = parseHarnessArgs(
    ['full', 'capability_probe', 'osc52', '--timeout-ms=12000', '--', '--osc52-form', 'tmux'],
    { requireTmuxConfig: true },
  );
  assert.equal(fullParsed.tmuxConfig, 'full');
  assert.equal(fullParsed.scenario, 'osc52');
  assert.equal(fullParsed.timeoutMs, 12000);
  assert.deepEqual(fullParsed.extraArgs, ['--osc52-form', 'tmux']);
});

test('parseHarnessArgs rejects invalid arguments with CliUsageError and recovery hints', () => {
  const invalidCases = [
    { argv: [], options: {} },
    { argv: ['capability_probe', 'invalid_scenario'], options: {} },
    { argv: ['capability_probe', '--timeout-ms', '0'], options: {} },
    { argv: ['capability_probe', '--timeout-ms', '-10'], options: {} },
    { argv: ['capability_probe', '--timeout-ms', 'abc'], options: {} },
    { argv: ['capability_probe', '--timeout-ms'], options: {} },
    { argv: ['capability_probe', '--unknown-flag'], options: {} },
    { argv: ['capability_probe', 'batch', 'extra_positional'], options: {} },
    { argv: ['custom', 'capability_probe'], options: { requireTmuxConfig: true } },
    { argv: ['stock'], options: { requireTmuxConfig: true } },
  ];

  for (const invalidCase of invalidCases) {
    assert.throws(
      () => parseHarnessArgs(invalidCase.argv, invalidCase.options),
      (thrownError) => {
        assert.ok(thrownError instanceof CliUsageError);
        assert.equal(typeof thrownError.recoveryHint, 'string');
        assert.ok(thrownError.recoveryHint.length > 0);
        return true;
      },
    );
  }
});

test('decodeOsc52Payload decodes ASCII, UTF-8 CJK/emoji, empty clear, and query payloads', () => {
  const asciiDecoded = decodeOsc52Payload('c;cHJvYmUtMQ==');
  assert.equal(asciiDecoded.raw, 'c;cHJvYmUtMQ==');
  assert.equal(asciiDecoded.selection, 'c');
  assert.equal(asciiDecoded.decodedText, 'probe-1');
  assert.equal(asciiDecoded.isQuery, false);
  assert.equal(asciiDecoded.isValid, true);

  const utf8Sample = '你好🦀';
  const utf8Base64 = Buffer.from(utf8Sample, 'utf8').toString('base64');
  const utf8Decoded = decodeOsc52Payload(`pc;${utf8Base64}`);
  assert.equal(utf8Decoded.selection, 'pc');
  assert.equal(utf8Decoded.decodedText, utf8Sample);
  assert.equal(utf8Decoded.isValid, true);

  const clearDecoded = decodeOsc52Payload('c;');
  assert.equal(clearDecoded.decodedText, '');
  assert.equal(clearDecoded.isValid, true);

  const queryDecoded = decodeOsc52Payload('s0;?');
  assert.equal(queryDecoded.selection, 's0');
  assert.equal(queryDecoded.decodedText, null);
  assert.equal(queryDecoded.isQuery, true);
  assert.equal(queryDecoded.isValid, true);
});

test('decodeOsc52Payload marks malformed OSC 52 payloads invalid without throwing', () => {
  const malformedInputs = ['cHJvYmUtMQ==', 'c;not-valid-base64!!!', 'c;abc', 'c;//8='];
  for (const malformedInput of malformedInputs) {
    const decoded = decodeOsc52Payload(malformedInput);
    assert.equal(decoded.decodedText, null);
    assert.equal(decoded.isValid, false);
  }
});

test('waitForProbeReady resolves immediately when sentinel and ready record already exist', async () => {
  const tempDir = createTempTestDir('rig-xterm-ready-immediate-');
  try {
    const logPath = path.join(tempDir, 'probe.jsonl');
    fs.writeFileSync(
      logPath,
      '{"kind":"section","section":"identity"}\n{"kind":"ready"}\n',
      'utf8',
    );
    fs.writeFileSync(buildReadySentinelPath(logPath), 'ready\n', 'utf8');

    const records = await waitForProbeReady(logPath, 2000);
    assert.equal(records.length, 2);
    assert.equal(records[0].section, 'identity');
    assert.equal(records[1].kind, 'ready');
    assert.deepEqual(readJsonLines(logPath), records);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test('waitForProbeReady wakes via fs.watch when ready sentinel arrives asynchronously', async () => {
  const tempDir = createTempTestDir('rig-xterm-ready-async-');
  try {
    const logPath = path.join(tempDir, 'probe.jsonl');
    const readyPromise = waitForProbeReady(logPath, 3000);

    setImmediate(() => {
      fs.writeFileSync(logPath, '{"kind":"summary","mode":"batch"}\n{"kind":"ready"}\n', 'utf8');
      fs.writeFileSync(buildReadySentinelPath(logPath), 'ready\n', 'utf8');
    });

    const records = await readyPromise;
    assert.equal(records.length, 2);
    assert.equal(records[1].kind, 'ready');
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test('waitForProbeReady fails fast on early process exit and times out when never ready', async () => {
  const tempDir = createTempTestDir('rig-xterm-ready-fail-');
  try {
    const crashLogPath = path.join(tempDir, 'crashed.jsonl');
    const crashedExitPromise = Promise.resolve({ exitCode: 101, signal: 0 });
    await assert.rejects(
      () => waitForProbeReady(crashLogPath, 5000, crashedExitPromise),
      /exited before writing ready sentinel/i,
    );

    const missingLogPath = path.join(tempDir, 'missing.jsonl');
    await assert.rejects(
      () => waitForProbeReady(missingLogPath, 80),
      /timed out/i,
    );
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
});

test('pty_session enforces Rule 5 by refusing to spawn before browser terminal is ready', async () => {
  assert.throws(
    () =>
      startPtySession({
        command: process.execPath,
        args: ['-e', 'process.exit(0)'],
        bridge: { isTerminalReady: false },
      }),
    /Rule 5 violation: cannot spawn child process before browser terminal is listening/,
  );

  const unreadyServer = await createBridgeServer({ cols: 100, rows: 30 });
  try {
    assert.equal(unreadyServer.isTerminalReady, false);
    await assert.rejects(
      () =>
        startBridgeAndPtySession({
          command: process.execPath,
          args: ['-e', 'process.exit(0)'],
          bridge: unreadyServer,
        }),
      /Rule 5 violation: cannot spawn child process before browser terminal is listening/,
    );
  } finally {
    await unreadyServer.close();
  }
});

test('browser bridge loads xterm.js 6.0.0 + Unicode11, replies DECRPM/DA1/OSC11, tracks modes, and captures OSC 52', async () => {
  const bridge = await startBrowserBridge({ cols: 100, rows: 30 });
  try {
    assert.equal(bridge.isTerminalReady, true);
    assert.equal(bridge.initialTerminalState.cols, 100);
    assert.equal(bridge.initialTerminalState.rows, 30);
    assert.ok(bridge.initialTerminalState.cellWidthPx > 0);
    assert.ok(bridge.initialTerminalState.cellHeightPx > 0);
    assert.equal(bridge.initialTerminalState.unicodeVersion, '11');

    const queryReplyPromise = waitForTerminalInputMatching(
      bridge,
      (inputText) => inputText.includes('\x1b[?1002;1$y') && inputText.includes('\x1b[?1;2c'),
    );
    await bridge.writeAndFlush(
      '\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?1004h\x1b]52;c;aGVsbG8=\x07\x1b]11;?\x07\x1b[?1002$p\x1b[c',
    );
    const queryReply = await queryReplyPromise;
    assert.match(queryReply, /rgb:1212\/2727\/3838/);

    const activeState = await bridge.getTerminalState();
    assert.equal(activeState.mouseTrackingMode, 'drag');
    assert.equal(activeState.bracketedPasteMode, true);
    assert.equal(activeState.sendFocusMode, true);
    assert.equal(activeState.osc52Payloads.length, 1);
    assert.equal(activeState.osc52Payloads[0].selection, 'c');
    assert.equal(activeState.osc52Payloads[0].decodedText, 'hello');

    await bridge.clearOsc52Payloads();
    await bridge.writeAndFlush('\x1b[?1006l\x1b[?1002l\x1b[?1000l');
    const resetState = await bridge.getTerminalState();
    assert.equal(resetState.mouseTrackingMode, 'none');
    assert.equal(resetState.osc52Payloads.length, 0);
  } finally {
    await bridge.close();
  }
});

test('startPtySession round-trips terminal queries and Unicode11 CPR width between node-pty and browser xterm.js', async () => {
  const bridge = await startBrowserBridge({ cols: 100, rows: 30 });
  try {
    const childScript = [
      'process.stdin.setRawMode(true);',
      'let received = "";',
      'process.stdin.on("data", (chunk) => {',
      '  received += chunk.toString("utf8");',
      '  if (received.includes("\\x1b[1;3R")) process.exit(0);',
      '});',
      'process.stdout.write("\\x1b[1;1H🦀\\x1b[6n");',
    ].join(' ');

    const ptySession = startPtySession({
      command: process.execPath,
      args: ['-e', childScript],
      cols: 100,
      rows: 30,
      bridge,
    });
    try {
      const exitOutcome = await ptySession.waitForExit(5000);
      assert.equal(exitOutcome.exitCode, 0);
    } finally {
      ptySession.dispose();
    }
  } finally {
    await bridge.close();
  }
});
