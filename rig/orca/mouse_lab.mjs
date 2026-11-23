/** Live Orca keyboard checks over the laboratory's native JSONL records. */
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import fs from 'node:fs';
import path from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';

const execute = promisify(execFile);
const HELP = `Verify mouse laboratory lifecycle and keyboard Copy through live Orca.

Usage: node rig/orca/mouse_lab.mjs <orca-cli> <repository-root> <lab-binary> <timeout-ms> [program_args...]

Arguments:
  <orca-cli>         Resolved Orca CLI executable
  <repository-root>  Workspace directory registered in Orca
  <lab-binary>       Built mouse_lab example with the vt100 feature
  <timeout-ms>       Positive integer condition and CLI timeout in milliseconds
  [program_args...]  Forwarded lab options; no arguments by default
                     --query-modes checks setup/teardown mode replies
                     --settings <json> overrides lab settings (default: {})
Options:
  -h, --help         Show help on stdout and exit 0 before doing work

Use the public wrapper to build the binary and resolve these arguments:
  just orca mouse_lab -- --query-modes
  just orca mouse_lab --timeout-ms 30000
`;

const setup = '\x1b[?1000h\x1b[?1002h\x1b[?1006h';
const teardown = '\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l';
const keyboardSelection = ['\t', '\t', 's', 'g', 'j', 'j', 'j', '\x1b[H',
  'v', 'l', 'v', 'V', 'j', 'V', 'g', 'j', 'j', 'j', '\x1b[H', 'W', 'y'].join('');
const quote = value => `'${value.replaceAll("'", "'\\''")}'`;

function readRecords(logPath) {
  if (!fs.existsSync(logPath)) return [];
  // The writer flushes whole records, but a watcher can wake during a write.
  return fs.readFileSync(logPath, 'utf8').split('\n').slice(0, -1)
    .filter(Boolean).map(line => JSON.parse(line));
}

function waitForLog({ logPath, exitPath, timeoutMs }, predicate, label) {
  return new Promise((resolve, reject) => {
    const finish = (error, records) => {
      clearTimeout(timer);
      watcher.close();
      logWatcher?.close();
      if (error) reject(error); else resolve(records);
    };
    const check = () => {
      try {
        const records = readRecords(logPath);
        if (predicate(records)) finish(null, records);
        else if (fs.existsSync(exitPath)) finish(new Error(`Lab exited before ${label}`));
      } catch (error) { finish(error); }
    };
    const watcher = fs.watch(path.dirname(logPath), check);
    // macOS directory watchers do not reliably report changes to existing files.
    const logWatcher = fs.existsSync(logPath) ? fs.watch(logPath, check) : null;
    const timer = setTimeout(() => finish(new Error(`Timed out waiting for ${label}`)), timeoutMs);
    check();
  });
}

/** Validates X1 lifecycle and X13 keyboard Copy without inferring pointer behavior. */
export function assertLabRecords(records, shouldQueryModes = false) {
  for (const [label, text] of [['setup', setup], ['teardown', teardown],
    ['leave_screen', '\x1b[?1049l\x1b[?25h']]) {
    const record = records.find(record => record.kind === 'sequence' && record.label === label);
    assert.deepEqual(record?.bytes, [...Buffer.from(text)], `X1 ${label}`);
  }
  const readyIndex = records.findIndex(record => record.kind === 'ready');
  assert.ok(readyIndex >= 0, 'X1 ready record');
  assert.ok(records.findIndex(record => record.label === 'setup') < readyIndex, 'X1 setup before ready');
  assert.ok(records.findIndex(record => record.label === 'teardown') > readyIndex, 'X1 teardown after ready');
  assert.equal(records.filter(record => record.event?.kind === 'Mouse').length, 0, 'X13 mouse input');
  const copies = records.filter(record => record.outcome?.kind === 'Copy');
  assert.deepEqual(copies.map(record => record.outcome),
    [{ kind: 'Copy', target: 'left', text: 'file.rs:12' }], 'X13 exact keyboard Copy');
  if (shouldQueryModes) assertModeReplies(records);
}

function assertModeReplies(records) {
  for (const phase of ['setup', 'teardown']) {
    const replies = records.filter(record => record.kind === 'mode_reply' && record.phase === phase);
    assert.deepEqual(replies.map(record => record.mode).sort((left, right) => left - right),
      [9, 1000, 1002, 1003, 1005, 1006, 1015, 1016], `X1 ${phase} queried modes`);
    for (const mode of [1000, 1002, 1003, 1006]) {
      const reply = replies.find(record => record.mode === mode);
      assert.equal(reply?.is_absent, false, `X1 ${phase} mode ${mode} reply`);
      assert.equal(reply?.state, phase === 'setup' && [1002, 1006].includes(mode) ? 1 : 2,
        `X1 ${phase} mode ${mode}`);
    }
  }
}

/** Drives one unfocused session, always closing the exact terminal it creates. */
export async function runMouseLab({ driver, binaryPath, logPath, exitPath,
  programArgs = [], timeoutMs = 20000, onReady = () => {} }) {
  const options = { logPath, exitPath, timeoutMs };
  const command = [binaryPath, '--log', logPath, ...programArgs].map(quote).join(' ')
    + `; printf '%s\\n' "$?" > ${quote(exitPath)}; exit`;
  const handle = await driver.create(command);
  try {
    await waitForLog(options, records => fs.existsSync(`${logPath}.ready`)
      && records.some(record => record.kind === 'ready'), 'readiness');
    onReady();
    await driver.read(handle);
    await driver.send(handle, keyboardSelection);
    await waitForLog(options, records => records.some(record => record.outcome?.kind === 'Copy'), 'Copy');
    await driver.send(handle, 'q');
    await driver.waitExit(handle, timeoutMs);
    assert.equal(fs.readFileSync(exitPath, 'utf8').trim(), '0', 'Lab exit status');
    const records = readRecords(logPath);
    assertLabRecords(records, programArgs.includes('--query-modes'));
    return records;
  } finally { await driver.close(handle); }
}

function createOrcaDriver(orcaBinary, repositoryRoot, timeoutMs) {
  const call = async args => {
    const { stdout } = await execute(orcaBinary, ['terminal', ...args, '--json'],
      { maxBuffer: 4 * 1024 * 1024, timeout: timeoutMs });
    const response = JSON.parse(stdout);
    assert.notEqual(response.ok, false, 'Orca command failed');
    return response.result;
  };
  return {
    async create(command) {
      const result = await call(['create', '--worktree', `path:${repositoryRoot}`,
        '--title', 'probe:mouse_lab', '--command', command]);
      assert.ok(result.terminal?.handle, 'Orca terminal handle missing');
      return result.terminal.handle;
    },
    read: handle => call(['read', '--terminal', handle, '--screen']),
    send: (handle, text) => call(['send', '--terminal', handle, '--text', text]),
    async waitExit(handle, timeoutMs) {
      const result = await call(['wait', '--terminal', handle, '--for', 'exit',
        '--timeout-ms', String(timeoutMs)]);
      assert.equal(result.wait?.satisfied, true, 'Orca exit wait failed');
    },
    close: handle => call(['close', '--terminal', handle]),
  };
}

async function main() {
  const args = process.argv.slice(2);
  if (args.some(argument => argument === '-h' || argument === '--help')) {
    process.stdout.write(HELP);
    return;
  }
  const [orcaBinary, repositoryRoot, binaryPath, timeout, ...programArgs] = args;
  if (!orcaBinary || !repositoryRoot || !binaryPath || !/^[1-9][0-9]*$/.test(timeout ?? '')) {
    process.stderr.write(`${HELP}\n❌ mouse_lab: Missing or invalid driver arguments.\n  ⚠️ Run 'just orca mouse_lab -- --query-modes'.\n`);
    process.exitCode = 2;
    return;
  }
  const directory = fs.mkdtempSync(path.join(tmpdir(), 'tuikit-orca-lab.'));
  try {
    const records = await runMouseLab({ driver: createOrcaDriver(orcaBinary, repositoryRoot, Number(timeout)),
      binaryPath, programArgs, timeoutMs: Number(timeout),
      logPath: path.join(directory, 'lab.jsonl'), exitPath: path.join(directory, 'exit_code') });
    process.stdout.write(records.map(record => JSON.stringify(record)).join('\n') + '\n');
    fs.rmSync(directory, { recursive: true, force: true });
    process.stderr.write('✅ mouse_lab: X1/X13 passed; Orca terminal closed.\n');
  } catch (error) {
    process.stderr.write(`❌ mouse_lab: ${error.message}\n  ⚠️ Inspect preserved lab artifacts in '${directory}' and retry 'just orca mouse_lab -- --query-modes'.\n`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
