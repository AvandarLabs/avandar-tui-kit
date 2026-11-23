import assert from 'node:assert/strict';
import { mkdtempSync, appendFileSync, writeFileSync, rmSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { runMouseLab, assertLabRecords } from '../mouse_lab.mjs';

const setup = '\x1b[?1000h\x1b[?1002h\x1b[?1006h';
const teardown = '\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l';
const sequence = (label, text) => ({ kind: 'sequence', label, bytes: [...Buffer.from(text)] });
function replies(phase) {
  return [9, 1000, 1002, 1003, 1005, 1006, 1015, 1016].map(mode => ({
    kind: 'mode_reply', phase, mode, is_absent: false,
    state: phase === 'setup' && [1002, 1006].includes(mode) ? 1 : 2,
  }));
}
function fixture(failure = null) {
  const directory = mkdtempSync(path.join(tmpdir(), 'orca-lab-test-'));
  const logPath = path.join(directory, 'lab.jsonl');
  const exitPath = path.join(directory, 'exit');
  const calls = [];
  const record = value => appendFileSync(logPath, `${JSON.stringify(value)}\n`);
  const driver = {
    async create(command) {
      calls.push({ type: 'create', command });
      writeFileSync(logPath, '');
      // Delayed readiness proves input is gated by both log and sentinel.
      setImmediate(() => {
        record(sequence('setup', setup));
        replies('setup').forEach(record);
        record({ kind: 'ready' });
        setImmediate(() => writeFileSync(`${logPath}.ready`, 'ready'));
      });
      return 'term_test';
    },
    async read() { calls.push({ type: 'read' }); },
    async send(handle, text) {
      assert.ok(calls.some(call => call.type === 'ready'));
      assert.ok(existsSync(`${logPath}.ready`));
      assert.match(readFileSync(logPath, 'utf8'), /"kind":"ready"/);
      calls.push({ type: 'keyboard', text });
      if (failure === 'send') throw new Error('send failed');
      if (text === 'q') {
        record(sequence('teardown', teardown));
        record(sequence('leave_screen', '\x1b[?1049l\x1b[?25h'));
        replies('teardown').forEach(record);
        writeFileSync(exitPath, failure === 'exit' ? '1' : '0');
      } else if (failure !== 'no-copy') {
        setImmediate(() => {
          record({ kind: 'event', event: { kind: 'Key' } });
          record({ kind: 'report', outcome: { kind: 'Copy', target: 'left', text: 'file.rs:12' } });
        });
      }
    },
    async waitExit() { calls.push({ type: 'exit' }); },
    async close() { calls.push({ type: 'close' }); },
  };
  return { directory, logPath, exitPath, calls, driver };
}

async function runFixture(failure) {
  const context = fixture(failure);
  try {
    const records = await runMouseLab({ ...context, binaryPath: '/bin/mouse_lab',
      programArgs: ['--query-modes'], timeoutMs: 1000,
      onReady: () => context.calls.push({ type: 'ready' }),
    });
    return { ...context, records };
  } finally { rmSync(context.directory, { recursive: true, force: true }); }
}

test('lab waits for readiness and Copy, sends keys only, validates X1/X13, closes', async () => {
  const { records, calls } = await runFixture();
  const programArgs = calls.find(call => call.type === 'create').command;
  assert.equal(programArgs.includes('--batch'), false);
  const inputCalls = calls.filter(call => ['keyboard', 'pointer'].includes(call.type));
  assert.equal(inputCalls.some(call => call.type === 'pointer'), false);
  const copyReport = records.find(record => record.outcome?.kind === 'Copy');
  assert.equal(copyReport.outcome.text, 'file.rs:12');
  assert.deepEqual(calls.slice(-2).map(call => call.type), ['exit', 'close']);
  assert.equal(inputCalls.at(-1).text, 'q');
});

for (const failure of ['send', 'exit', 'no-copy']) {
  test(`lab closes its terminal on ${failure} failure`, async () => {
    const context = fixture(failure);
    try {
      await assert.rejects(runMouseLab({ ...context, binaryPath: '/bin/mouse_lab',
        programArgs: ['--query-modes'], timeoutMs: failure === 'no-copy' ? 50 : 1000,
        onReady: () => context.calls.push({ type: 'ready' }),
      }), /failed|status|Timed out/);
      assert.equal(context.calls.at(-1).type, 'close');
    } finally { rmSync(context.directory, { recursive: true, force: true }); }
  });
}

test('X1/X13 assertions reject incorrect Copy, pointer input, modes, and missing cleanup', async () => {
  const { records } = await runFixture();
  const altered = structuredClone(records);
  altered.find(record => record.outcome?.kind === 'Copy').outcome.text = 'wrong';
  assert.throws(() => assertLabRecords(altered), /exact keyboard Copy/);
  assert.throws(() => assertLabRecords([...records, { event: { kind: 'Mouse' } }]), /mouse input/);
  assert.throws(() => assertLabRecords(records.filter(record => record.label !== 'teardown')), /teardown/);
  const modes = structuredClone(records);
  modes.find(record => record.kind === 'mode_reply' && record.mode === 1002).state = 2;
  assert.throws(() => assertLabRecords(modes, true), /setup mode 1002/);
});

test('driver CLI help takes precedence and missing internal arguments fail without work', () => {
  const script = fileURLToPath(new URL('../mouse_lab.mjs', import.meta.url));
  const help = spawnSync(process.execPath, [script, '--bad', '--help'], { encoding: 'utf8' });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /Usage:/);
  assert.equal(help.stderr, '');
  const invalid = spawnSync(process.execPath, [script], { encoding: 'utf8' });
  assert.equal(invalid.status, 2);
  assert.equal(invalid.stdout, '');
  assert.match(invalid.stderr, /Usage:/);
});
