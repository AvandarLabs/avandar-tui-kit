import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { CliUsageError, parseHarnessArgs } from '../src/args.mjs';
import { createBridgeServer, startBrowserBridge } from '../src/bridge_server.mjs';
import { decodeOsc52Payload } from '../src/osc52.mjs';
import {
  buildSanitizedPtyEnv,
  startBridgeAndPtySession,
  startPtySession,
} from '../src/pty_session.mjs';
import { buildReadySentinelPath, readJsonLines, waitForProbeReady } from '../src/ready_waiter.mjs';

function createTempTestDir(prefix) {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

function countActiveTimers() {
  return process.getActiveResourcesInfo().filter((resource) => resource === 'Timeout').length;
}

function waitForTerminalInputMatching(bridge, isMatch, timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    let accumulatedInput = '';
    const unsubscribe = bridge.onTerminalInput((inputChunk) => {
      accumulatedInput += Buffer.isBuffer(inputChunk)
        ? inputChunk.toString('latin1')
        : String(inputChunk);
      if (isMatch(accumulatedInput)) {
        clearTimeout(failureTimer);
        unsubscribe();
        resolve(accumulatedInput);
      }
    });
    const failureTimer = setTimeout(() => {
      unsubscribe();
      reject(new Error(`Timed out after ${timeoutMs}ms waiting for terminal input`));
    }, timeoutMs);
  });
}

function waitForBinaryInputBuffer(bridge, expectedLength, timeoutMs = 5000) {
  return new Promise((resolve, reject) => {
    const receivedChunks = [];
    let totalBytes = 0;
    const unsubscribe = bridge.onTerminalInput((inputChunk) => {
      receivedChunks.push(inputChunk);
      totalBytes += Buffer.isBuffer(inputChunk) ? inputChunk.length : Buffer.byteLength(inputChunk);
      if (totalBytes >= expectedLength) {
        clearTimeout(failureTimer);
        unsubscribe();
        resolve(receivedChunks);
      }
    });
    const failureTimer = setTimeout(() => {
      unsubscribe();
      reject(new Error(`Timed out after ${timeoutMs}ms waiting for binary terminal input`));
    }, timeoutMs);
  });
}

test('parseHarnessArgs short-circuits help before double-dash while forwarding help after double-dash', () => {
  assert.equal(parseHarnessArgs(['-h']).isHelp, true);
  assert.equal(parseHarnessArgs(['--help'], { requireTmuxConfig: true }).isHelp, true);
  const forwardedParsed = parseHarnessArgs(['capability_probe', 'batch', '--', '--help']);
  assert.equal(forwardedParsed.isHelp, false);
  assert.deepEqual(forwardedParsed.extraArgs, ['--help']);
});

test('parseHarnessArgs selects all scenario for capability_probe and passthrough for custom programs', () => {
  assert.deepEqual(parseHarnessArgs(['capability_probe']), {
    isHelp: false,
    tmuxConfig: null,
    program: 'capability_probe',
    scenario: 'all',
    timeoutMs: 20000,
    extraArgs: [],
  });
  assert.equal(parseHarnessArgs(['custom_example']).scenario, 'passthrough');
});

test('parseHarnessArgs accepts explicit scenario, timeout-ms, and double-dash program flags', () => {
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

test('parseHarnessArgs extracts required stock or full tmux profile when requireTmuxConfig is true', () => {
  const stockParsed = parseHarnessArgs(['stock', 'capability_probe'], { requireTmuxConfig: true });
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

test('parseHarnessArgs throws CliUsageError with recoveryHint on invalid options or positionals', () => {
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
      (thrownError) =>
        thrownError instanceof CliUsageError &&
        typeof thrownError.recoveryHint === 'string' &&
        thrownError.recoveryHint.length > 0,
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

  const utf8Sample = '\u4f60\u597d\ud83e\udd80';
  const utf8Decoded = decodeOsc52Payload(`pc;${Buffer.from(utf8Sample, 'utf8').toString('base64')}`);
  assert.equal(utf8Decoded.selection, 'pc');
  assert.equal(utf8Decoded.decodedText, utf8Sample);

  const clearDecoded = decodeOsc52Payload('c;');
  assert.equal(clearDecoded.decodedText, '');
  assert.equal(clearDecoded.isValid, true);

  const queryDecoded = decodeOsc52Payload('s0;?');
  assert.equal(queryDecoded.decodedText, null);
  assert.equal(queryDecoded.isQuery, true);
  assert.equal(queryDecoded.isValid, true);
});

test('decodeOsc52Payload marks malformed OSC 52 payloads invalid without throwing', () => {
  for (const malformedInput of ['cHJvYmUtMQ==', 'c;not-valid-base64!!!', 'c;abc', 'c;//8=']) {
    const decoded = decodeOsc52Payload(malformedInput);
    assert.equal(decoded.decodedText, null);
    assert.equal(decoded.isValid, false);
  }
});

test('waitForProbeReady resolves immediately when sentinel and ready record already exist', async () => {
  const tempDir = createTempTestDir('rig-xterm-ready-immediate-');
  try {
    const logPath = path.join(tempDir, 'probe.jsonl');
    fs.writeFileSync(logPath, '{"kind":"section","section":"identity"}\n{"kind":"ready"}\n', 'utf8');
    fs.writeFileSync(buildReadySentinelPath(logPath), 'ready\n', 'utf8');

    const records = await waitForProbeReady(logPath, 2000);
    assert.equal(records.length, 2);
    assert.equal(records[0].section, 'identity');
    assert.equal(records[1].kind, 'ready');
    assert.deepEqual(readJsonLines(logPath), records);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
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
    fs.rmSync(tempDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

test('waitForProbeReady rejects immediately when child exits before writing ready sentinel', async () => {
  const tempDir = createTempTestDir('rig-xterm-ready-crash-');
  try {
    const crashLogPath = path.join(tempDir, 'crashed.jsonl');
    await assert.rejects(
      () => waitForProbeReady(crashLogPath, 5000, Promise.resolve({ exitCode: 101, signal: 0 })),
      /exited before writing ready sentinel/i,
    );
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

test('waitForProbeReady rejects after timeoutMs when sentinel never arrives', async () => {
  const tempDir = createTempTestDir('rig-xterm-ready-timeout-');
  try {
    await assert.rejects(
      () => waitForProbeReady(path.join(tempDir, 'missing.jsonl'), 80),
      /timed out/i,
    );
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

test('buildSanitizedPtyEnv strips outer terminal and tmux vars while preserving explicit overrides', () => {
  const pollutedOuterEnv = {
    PATH: '/usr/bin:/bin',
    TERM: 'dumb',
    TMUX: '/tmp/tmux-501/default,123,0',
    TMUX_PANE: '%1',
    STY: '1234.pts-0',
    WINDOW: '0',
    WINDOWID: '999',
    TERMCAP: 'xterm|foo',
    COLUMNS: '120',
    LINES: '40',
    TERM_PROGRAM: 'Orca',
    TERM_PROGRAM_VERSION: '1.4.258',
    ORCA_VERSION: '1.4.258',
    ITERM_SESSION_ID: 'w0t0p0',
    ITERM_PROFILE: 'Default',
    LC_TERMINAL: 'iTerm2',
    LC_TERMINAL_VERSION: '3.7.7',
    WT_SESSION: 'guid-123',
    VSCODE_INJECTION: '1',
  };
  const sanitized = buildSanitizedPtyEnv(pollutedOuterEnv, {
    CUSTOM_HARNESS_FLAG: 'enabled',
    TERM_PROGRAM: 'ExplicitOverride',
  });
  assert.equal(sanitized.PATH, '/usr/bin:/bin');
  assert.equal(sanitized.TERM, 'xterm-256color');
  for (const strippedKey of ['TMUX', 'TMUX_PANE', 'ORCA_VERSION', 'ITERM_SESSION_ID', 'WT_SESSION', 'COLUMNS']) {
    assert.equal(sanitized[strippedKey], undefined);
  }
  assert.equal(sanitized.TERM_PROGRAM, 'ExplicitOverride');
  assert.equal(sanitized.CUSTOM_HARNESS_FLAG, 'enabled');
});

test('startPtySession refuses unready bridges and disposes Windows ConPTY handles idempotently', async () => {
  assert.throws(
    () => startPtySession({ command: process.execPath, bridge: { isTerminalReady: false } }),
    /Rule 5 violation: cannot spawn child process before browser terminal is listening/,
  );

  let killCallCount = 0;
  let hasDestroyedInSocket = false;
  let triggerExit = null;
  const fakePtySpawner = () => ({
    pid: 4242,
    _agent: { _inSocket: { destroy: () => { hasDestroyedInSocket = true; } } },
    onData: () => {},
    onExit: (handler) => { triggerExit = handler; },
    write: () => {},
    kill: () => { killCallCount += 1; },
  });
  const fakeBridge = { isTerminalReady: true, isOpen: false, onTerminalInput: () => () => {} };
  const winSession = startPtySession({
    command: 'cmd.exe',
    bridge: fakeBridge,
    platform: 'win32',
    ptySpawner: fakePtySpawner,
  });
  triggerExit({ exitCode: 0, signal: 0 });
  await winSession.waitForExit(1000);
  winSession.dispose();
  winSession.dispose();
  assert.equal(killCallCount, 1);
  assert.equal(hasDestroyedInSocket, true);
});

test('startBridgeAndPtySession rejects an unready caller-supplied bridge before spawning', async () => {
  const unreadyServer = await createBridgeServer({ cols: 100, rows: 30 });
  try {
    assert.equal(unreadyServer.isTerminalReady, false);
    await assert.rejects(
      () => startBridgeAndPtySession({ command: process.execPath, bridge: unreadyServer }),
      /Rule 5 violation: cannot spawn child process before browser terminal is listening/,
    );
  } finally {
    await unreadyServer.close();
  }
});

test('startBrowserBridge clears its ready timeout handle when Chromium launch fails', async () => {
  const baselineTimerCount = countActiveTimers();
  await assert.rejects(
    () =>
      startBrowserBridge({
        cols: 100,
        rows: 30,
        timeoutMs: 60000,
        executablePath: '/nonexistent-chromium-binary-rig-xterm-test',
      }),
    /executable doesn't exist|Failed to launch|ENOENT/i,
  );
  assert.equal(countActiveTimers(), baselineTimerCount);
});

test('browser bridge loads xterm.js 6.0.0 + Unicode11, preserves binary >= 0x80, and captures OSC 52', async () => {
  const bridge = await startBrowserBridge({ cols: 100, rows: 30 });
  try {
    const initial = bridge.initialTerminalState;
    assert.equal(initial.unicodeVersion, '11');
    assert.ok(initial.cellWidthPx > 0 && initial.cellHeightPx > 0);

    const queryReplyPromise = waitForTerminalInputMatching(
      bridge,
      (inputText) => inputText.includes('\x1b[?1002;1$y') && inputText.includes('\x1b[?1;2c'),
    );
    await bridge.writeAndFlush(
      '\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?2004h\x1b[?1004h\x1b]52;c;aGVsbG8=\x07\x1b]11;?\x07\x1b[?1002$p\x1b[c',
    );
    assert.match(await queryReplyPromise, /rgb:1212\/2727\/3838/);

    const activeState = await bridge.getTerminalState();
    assert.equal(activeState.mouseTrackingMode, 'drag');
    assert.equal(activeState.osc52Payloads[0].decodedText, 'hello');

    await bridge.writeAndFlush('\x1b[?1006l\x1b[?1002l\x1b[?1000h');
    const binaryChunksPromise = waitForBinaryInputBuffer(bridge, 12);
    await bridge.page.mouse.click(
      initial.screenLeftPx + 97.5 * initial.cellWidthPx,
      initial.screenTopPx + 5.5 * initial.cellHeightPx,
    );
    const binaryChunks = await binaryChunksPromise;
    assert.ok(Buffer.isBuffer(binaryChunks[0]));
    const combinedBinary = Buffer.concat(binaryChunks);
    assert.equal(combinedBinary.length, 12);
    assert.equal(combinedBinary[4], 0x82);
  } finally {
    await bridge.close();
  }
});

test(
  'startPtySession round-trips binary bytes >= 0x80 and Unicode11 CPR width without UTF-8 expansion',
  { skip: process.platform === 'win32' },
  async () => {
    const bridge = await startBrowserBridge({ cols: 100, rows: 30 });
    try {
      const childScript = [
        'process.stdin.setRawMode(true);',
        'const chunks = [];',
        'process.stdin.on("data", (chunk) => {',
        '  chunks.push(chunk);',
        '  const buf = Buffer.concat(chunks);',
        '  if (buf.includes(Buffer.from([0x1b, 0x5b, 0x4d, 0x20, 0x82, 0x26]))) process.exit(0);',
        '});',
        'process.stdout.write("\\x1b[?1000h\\x1b[1;1H\ud83e\udd80\\x1b[6n");',
      ].join(' ');

      const cprReplyPromise = waitForTerminalInputMatching(bridge, (text) => text.includes('\x1b[1;3R'));
      const ptySession = startPtySession({ command: process.execPath, args: ['-e', childScript], bridge });
      try {
        await cprReplyPromise;
        const initial = bridge.initialTerminalState;
        await bridge.page.mouse.click(
          initial.screenLeftPx + 97.5 * initial.cellWidthPx,
          initial.screenTopPx + 5.5 * initial.cellHeightPx,
        );
        const exitOutcome = await ptySession.waitForExit(5000);
        assert.equal(exitOutcome.exitCode, 0);
      } finally {
        ptySession.dispose();
      }
    } finally {
      await bridge.close();
    }
  },
);
