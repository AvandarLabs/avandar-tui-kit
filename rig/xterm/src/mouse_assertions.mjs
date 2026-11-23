/** Platform-aware assertions over the laboratory's native log schema. */
import assert from 'node:assert/strict';

const QUERY_MODES = [9, 1000, 1002, 1003, 1005, 1006, 1015, 1016];

/** Checks the direct outer tracking protocol and SGR encoding at readiness. */
export function assertMouseSetup(state, platform = process.platform) {
  assert.equal(state.mouseTrackingMode, platform === 'win32' ? 'any' : 'drag');
  assert.equal(state.mouseEncoding, 'SGR', 'outer mouse reports must use SGR encoding');
}

/** Rejects actionable hover and, on Unix, any raw mouse event in a fenced slice. */
export function assertMouseHover(records, platform = process.platform) {
  const mouseEvents = records.filter((record) => record.kind === 'event' && record.event?.kind === 'Mouse');
  if (platform !== 'win32') {
    assert.equal(mouseEvents.length, 0, 'Unix drag tracking must not emit bare motion');
  } else {
    assert.ok(mouseEvents.every((record) => record.event.mouse_kind === 'Moved'));
  }
  for (const record of records.filter((record) => record.kind === 'report')) {
    assert.equal(record.outcome.kind, 'Ignored', 'hover must not act or forward bytes');
    assert.equal(record.pressed_target, null, 'hover must not press a target');
    assert.equal(record.is_redraw_due, false, 'hover must not request redraw');
  }
  assert.equal(records.filter((record) => record.kind === 'selection').length, 0, 'hover must not change selection');
}

/** Checks requested setup/teardown replies, treating Windows silence as absence. */
export function assertMouseModeReplies(records, platform = process.platform) {
  for (const phase of ['setup', 'teardown']) {
    const replies = records.filter((record) => record.kind === 'mode_reply' && record.phase === phase);
    assert.equal(replies.length, QUERY_MODES.length, `missing ${phase} mode observations`);
    for (const mode of QUERY_MODES) {
      const reply = replies.find((record) => record.mode === mode);
      assert.ok(reply, `missing ${phase} mode ${mode}`);
      if (platform === 'win32') {
        assert.equal(reply.is_absent, true, 'ConPTY must record unanswered DECRQM queries');
        assert.equal(reply.state, null);
      } else if ([1000, 1002, 1003, 1006].includes(mode)) {
        assert.equal(reply.is_absent, false, `direct xterm must answer mode ${mode}`);
        const expectedState = phase === 'setup' && [1002, 1006].includes(mode) ? 1 : 2;
        assert.equal(reply.state, expectedState, `${phase} mode ${mode}`);
      }
    }
  }
}
