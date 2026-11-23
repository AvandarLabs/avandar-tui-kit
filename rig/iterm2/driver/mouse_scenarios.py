"""Condition-driven mouse_lab scenarios in the prepared macOS VM.

The lab schema deliberately stays native. F2 is a FIFO fence, ! requests the
lab panic, and q exits. Probe-specific event readers and F12 are not used.
"""
from __future__ import annotations

import json
import os
import time
import dataclasses
from pathlib import Path
import select
import shlex
import subprocess
import shutil
import sys
from typing import TYPE_CHECKING

from driver.cli_args import DriverRuntimeError
from driver.interactive_steps import ensure_iterm_window_focused, wait_for_session_exit_and_teardown
from driver.iterm_window import (
    SessionLaunchSpec, dismiss_popup_menus_if_any, launch_iterm_session,
    has_popup_menu_window, query_iterm_windows,
    resolve_session_geometry, set_guest_natural_scrolling,
)
from driver.vnc_input import build_chord_tokens, build_click_tokens, build_drag_tokens, cell_center_to_vnc_pixels
from driver.tart_vm import GUEST_SHARED_ROOT
from driver.wait_helpers import poll_until, read_complete_jsonl_records, wait_for_probe_ready

if TYPE_CHECKING:
    from driver.scenarios import ScenarioRuntimeContext

QUERY_MODES = (9, 1000, 1002, 1003, 1005, 1006, 1015, 1016)
SETUP = '\x1b[?1000h\x1b[?1002h\x1b[?1006h'
TEARDOWN = '\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l'


# One RFB connection retains coordinates, held buttons, and modifiers between
# condition waits. Separate vncdo processes each start with an empty mask at 0,0.
VNC_BRIDGE = r'''import json, sys
from vncdotool import api
config=json.loads(sys.stdin.readline())
client=api.connect(config['server'],password=config['password'],timeout=15)
held_keys=set()
held_buttons=set()
try:
    print(json.dumps({'ready':True}),flush=True)
    for line in sys.stdin:
        tokens=json.loads(line)
        index=0
        try:
            while index<len(tokens):
                command=tokens[index]; index+=1
                if command=='move':
                    client.mouseMove(int(tokens[index]),int(tokens[index+1])); index+=2
                elif command in ('mousedown','mouseup','click'):
                    button=int(tokens[index]); index+=1
                    if command!='mouseup': client.mouseDown(button); held_buttons.add(button)
                    if command!='mousedown': client.mouseUp(button); held_buttons.discard(button)
                elif command in ('key','keydown','keyup'):
                    key=tokens[index]; index+=1
                    if command=='key': client.keyPress(key)
                    elif command=='keydown': client.keyDown(key); held_keys.add(key)
                    else: client.keyUp(key); held_keys.discard(key)
                else: raise ValueError('Unsupported VNC command '+command)
            print(json.dumps({'ok':True}),flush=True)
        except Exception as error:
            print(json.dumps({'error':str(error)}),flush=True)
finally:
    for key in held_keys: client.keyUp(key)
    for button in held_buttons: client.mouseUp(button)
    client.disconnect()
    api.shutdown()
'''


class PersistentLabVnc:
    """Acknowledged vncdotool IPC, keeping button state alive through log waits."""
    def __init__(self, original):
        self.original = original
        self.process = None
        self._response_buffer = b""

    def __enter__(self):
        interpreter = shlex.split(Path(self.original.vncdo_bin).read_text().splitlines()[0][2:])
        self.process = subprocess.Popen([*interpreter,'-u','-c',VNC_BRIDGE], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, env=self.original.env)
        endpoint = self.original.endpoint
        try:
            self.process.stdin.write(json.dumps({'server':f'{endpoint.host}::{endpoint.port}', 'password':endpoint.password})+'\n')
            self.process.stdin.flush()
            self._response(15)
            return self
        except BaseException:
            self.__exit__()
            raise

    def _response(self, timeout):
        deadline = time.monotonic() + timeout
        while b"\n" not in self._response_buffer:
            remaining = max(0,deadline-time.monotonic())
            require(select.select([self.process.stdout],[],[],remaining)[0],
                    'Persistent VNC acknowledgment timed out.')
            chunk = os.read(self.process.stdout.fileno(),4096)
            require(bool(chunk), 'Persistent VNC connection exited unexpectedly.')
            self._response_buffer += chunk
        line, self._response_buffer = self._response_buffer.split(b"\n",1)
        response = json.loads(line)
        require('error' not in response, f'Persistent VNC input failed: {response.get("error")}.')

    def run_tokens(self, tokens, timeout_seconds=15):
        """Sends tokens to the owned RFB connection and waits for acknowledgment."""
        if not tokens: return
        self.process.stdin.write(json.dumps(list(tokens))+'\n')
        self.process.stdin.flush()
        self._response(timeout_seconds)

    def send_fence_key(self):
        """Sends F2 through the same ordered RFB connection."""
        self.run_tokens(['key','f2'])

    def calibrate_modifiers(self, vm):
        """Uses the existing CoreGraphics calibration and its cached mapping."""
        return self.original.calibrate_modifiers(vm)

    def __exit__(self, *_error):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        finally:
            self.process.stdout.close()


def require(condition, description):
    """Raises an actionable driver error even when Python assertions are disabled."""
    if not condition:
        raise DriverRuntimeError(description, 'Inspect the archived native mouse_lab JSONL and VM screenshot.')


def outcomes(records, kind):
    """Returns semantic outcomes of the requested kind from native reports."""
    return [record['outcome'] for record in records if record.get('kind') == 'report' and record['outcome']['kind'] == kind]


def mouse_events(records):
    """Returns decoded crossterm mouse events from the lab schema."""
    return [record['event'] for record in records if record.get('kind') == 'event' and record['event']['kind'] == 'Mouse']


def check_copy(records, target, text):
    """Requires one exact copy outcome, including its owning surface."""
    expected = [{'kind': 'Copy', 'target': target, 'text': text}]
    require(outcomes(records, 'Copy') == expected, f'Expected exact Copy {expected}, got {outcomes(records, "Copy")}.')


def check_forward(records, sequences):
    """Requires exact ordered pane-relative mouse or arrow bytes."""
    expected = [{'kind':'Forward', 'target':'right', 'bytes':list(sequence.encode())} for sequence in sequences]
    require(outcomes(records, 'Forward') == expected, f'Unexpected pane forwarding: {outcomes(records, "Forward")}, expected {expected}.')


def check_hover(records):
    """Rejects input, selection changes, forwarding, and redraw from bare hover."""
    require(not mouse_events(records), 'Drag tracking must not report bare hover.')
    require(not any(record.get('kind') == 'selection' for record in records), 'Hover changed selection.')
    for record in [record for record in records if record.get('kind') == 'report']:
        require(record['outcome']['kind'] == 'Ignored' and record['pressed_target'] is None
                and record['is_redraw_due'] is False, 'Hover acted or requested redraw.')


def assert_lifecycle(records, query_modes, is_panic=False):
    """Checks exact mode bytes plus actual mode observations when queries are enabled."""
    for phase, sequence in [('setup', SETUP), ('teardown', TEARDOWN)]:
        observed = [record['bytes'] for record in records if record.get('kind') == 'sequence' and record.get('label') == phase]
        require(observed == [list(sequence.encode())], f'Missing or wrong {phase} sequence.')
        if not query_modes or (is_panic and phase == "teardown"):
            continue
        replies = [record for record in records if record.get('kind') == 'mode_reply' and record['phase'] == phase]
        require(len(replies) == 8 and {record['mode'] for record in replies} == set(QUERY_MODES), f'Missing {phase} mode observations.')
        for reply in replies:
            require(not reply['is_absent'] and reply['state'] in [0, 1, 2, 3, 4], f'Unanswered iTerm2 mode {reply}.')
            if reply['mode'] in [1000, 1002, 1003, 1006]:
                expected = 1 if phase == 'setup' and reply['mode'] in [1002, 1006] else 2
                require(reply['state'] == expected, f'Wrong {phase} mode {reply}.')
            else:
                require(reply['state'] in [0,2,4], f'Inactive mouse encoding remains enabled: {reply}.')


def assert_exit_snapshot(state):
    """Requires real reset replies, never a default state inferred from silence."""
    require(state['mouseTrackingMode'] == 'none' and not state['sgrMouseMode'],
            'Wrapper PTY tracking was not reset on exit.')
    require(all(state['rawModeStates'].get(str(mode)) == 2 for mode in [1000,1002,1003,1006]),
            'Missing or enabled post-exit mouse mode replies.')


def assert_gesture(scenario, steps, tmux_version=None, is_natural_scrolling=False):
    """Checks the scenario's semantic evidence, independent of VM IO."""
    if int(scenario[1:]) >= 10:
        return _assert_pane_gesture(scenario, steps, tmux_version)
    if scenario == 'x3':
        check_hover(steps['hover']); check_hover(steps['passHover'])
        require(any(record.get('label') == 'pane_modes' and record.get('destination') == 'parser'
                    and '\x1b[?1000h\x1b[?1006h' in bytes(record['bytes']).decode()
                    for record in steps['paneModes']), 'Missing pane pass-through mode request.')
    elif scenario == 'x4':
        for target in ['left', 'right', 'tab', 'divider']:
            presses = [record['outcome'] for record in steps[target] if record.get('kind') == 'report' and record['outcome']['kind'] in ['Pressed', 'Clicked']]
            require([(record['kind'], record['target'], record['press']['button'], record['press']['modifiers']) for record in presses]
                    == [(outcome_kind, target, 'Left', 'KeyModifiers(0x0)') for outcome_kind in ['Pressed', 'Clicked']], f'Wrong target ownership for {target}.')
    elif scenario == 'x5':
        check_copy(steps['drag'], 'left', 'alpha\nbeta\ngamma')
    elif scenario == 'x6':
        check_copy(steps['word'], 'left', 'file.rs:12')
        check_copy(steps['line'], 'left', 'soft wrap over three rows')
    elif scenario == 'x7':
        records = steps['history']
        require(any(record.get('kind') == 'tick' for record in records), 'Missing edge-scroll tick.')
        selection = next((record['selection'] for record in reversed(records) if record.get('kind') == 'selection' and record['selection']), None)
        require(selection and selection['target'] == 'right' and selection['range']['start']['line'] == 0
                and 30 <= selection['range']['end']['line'] < 100, 'History selection failed to reach offscreen row 030.')
        text = '\n'.join(f'right history {record_index:03} 界' for record_index in range(selection['range']['end']['line'] + 1))
        check_copy(records, 'right', text)
    elif scenario == 'x8':
        direction_sign = -1 if is_natural_scrolling else 1
        for name, target, rows in [('left', 'left', 3*direction_sign), ('right', 'right', -3*direction_sign)]:
            wheels = outcomes(steps[name], 'Wheel')
            require([(record['target'], record['rows'], record['columns']) for record in wheels] == [(target, rows, 0)], f'Wrong signed wheel step {name}: {wheels}.')
    elif scenario == 'x9':
        require(not outcomes(steps['wheel'], 'Wheel') and outcomes(steps['wheel'], 'Consumed'), 'Wheel during drag escaped selection.')
        selection = next(record['selection'] for record in reversed(steps['wheel']) if record.get('kind') == 'selection')
        require(selection['target'] == 'left' and selection['range']['end'] == {'line': 5, 'column':4}
                and selection['range']['end'] != steps['beforeEnd'], 'Wheel failed to move selection endpoint.')


def _assert_pane_gesture(scenario, steps, tmux_version):
    if scenario == 'x10':
        check_forward(steps['click'], ['\x1b[<0;3;3M', '\x1b[<0;3;3m'])
        check_forward(steps['drag'], ['\x1b[<0;3;3M', '\x1b[<32;7;5M', '\x1b[<0;7;5m'])
        check_forward(steps['wheel'], ['\x1b[<65;3;3M'])
    elif scenario == 'x11':
        _assert_modifier_gesture(steps, tmux_version)
    elif scenario == 'x12':
        check_forward(steps['wheel'], ['\x1b[B\x1b[B\x1b[B'])
    elif scenario == 'x13':
        check_copy(steps['keyboard'], 'left', 'file.rs:12')
        require(not mouse_events(steps['keyboard']), 'Keyboard-only scenario received mouse input.')
    elif scenario == 'x14':
        for label in ['right', 'control']:
            require(not mouse_events(steps[label]), f'Default iTerm2 {label} context menu reached app.')
            require(not any(outcomes(steps[label], kind) for kind in ['Pressed','Clicked','Copy','Forward']), f'Context click {label} acted in app.')


def _assert_modifier_gesture(steps, tmux_version):
    for label, modifiers in [('shift', 'KeyModifiers(SHIFT)'), ('command', 'KeyModifiers(ALT)')]:
        events = mouse_events(steps[label])
        has_tmux38_motion_filter = label == 'command' and tmux_version == 'tmux 3.8'
        kinds = ['Down(Left)', 'Up(Left)'] if has_tmux38_motion_filter else ['Down(Left)', 'Drag(Left)', 'Up(Left)']
        require([event['mouse_kind'] for event in events] == kinds
                and all(event['modifiers'] == modifiers for event in events), f'Wrong {label} app mouse routing: {events}.')
        require(not outcomes(steps[label], 'Forward'), f'{label} forwarded to pane.')
        if has_tmux38_motion_filter:
            require(not outcomes(steps[label], 'Copy'), 'tmux 3.8 Command gesture must not invent a copied drag.')
            _assert_filtered_command_selection(steps[label])
        else:
            check_copy(steps[label], 'right', 'right')
    for label, text in [('option', 'alpha'), ('option_command', 'alp\nbet\ngam')]:
        native = steps[label]
        require(native['clipboard'] == text, f'Wrong native {label} selection: {native["clipboard"]!r}, expected {text!r}.')
        require(not mouse_events(native['records']) and not outcomes(native['records'], 'Copy')
                and not outcomes(native['records'], 'Forward')
                and not any(record.get('kind') == 'selection' and record.get('selection') for record in native['records']),
                f'Native {label} reached app.')


def _assert_filtered_command_selection(records):
    selections = [record['selection'] for record in records
                  if record.get('kind') == 'selection' and record.get('selection') is not None]
    require(bool(selections), 'Filtered Command gesture must observe a right-target Pressed selection.')
    for selection in selections:
        content_range = selection.get('range', {})
        require(selection.get('target') == 'right' and selection.get('stage') == 'Pressed'
                and content_range.get('start') is not None and content_range.get('start') == content_range.get('end'),
                'Filtered Command motion must leave only a right-target zero-length Pressed selection.')


def lab_step(context, session, action, dismiss_menu=False):
    """Runs an input step and waits for the lab's own F2 event followed by its report."""
    start = len(read_complete_jsonl_records(session.host_log_path))
    action()
    if dismiss_menu:
        dismiss_popup_menus_if_any(context.vm, context.vnc, context.timeout_seconds)
    context.vnc.send_fence_key()
    def ready():
        trailing = read_complete_jsonl_records(session.host_log_path)[start:]
        for record_index, record in enumerate(trailing):
            event = record.get('event', {})
            if event.get('kind') == 'Key' and event.get('code') == 'F(2)' and event.get('key_kind') != 'Release':
                if any(record.get('kind') == 'report' for record in trailing[record_index+1:]):
                    return trailing[:record_index]
        require(not session.host_exit_path.is_file(), 'Lab exited before F2 fence.')
        return None
    # A fence can legitimately contain zero preceding records.
    def fenced():
        result = ready()
        return (result,) if result is not None else None
    result = poll_until(fenced,
                        context.timeout_seconds, 'mouse_lab F2 event and report')
    return result[0]


def build_lab_wheel_tokens(x, y, direction):
    """Maps semantic direction to standard RFB wheel buttons: 4 up, 5 down."""
    require(direction in ('up','down'), 'Unsupported laboratory wheel direction.')
    return ['move',str(x),str(y),'click','4' if direction == 'up' else '5']


def final_click_records(records):
    """Scopes a multi-click assertion to the final press/release gesture."""
    offset = max(record_index for record_index, record in enumerate(records) if record.get('event', {}).get('mouse_kind') == 'Down(Left)')
    return records[offset:]


def build_lab_key_tokens(key):
    """Adds physical Shift for symbols the VNC server maps to unshifted keycodes."""
    if key == '!':
        return build_chord_tokens('1', ('shift',))
    if len(key) == 1 and key.isupper():
        return build_chord_tokens(key.lower(), ('shift',))
    return ['key', key]


class LabInput:
    """Maps logged content cells through guest window geometry to VNC input."""
    def __init__(self, context, session, geometry):
        self.context, self.session, self.geometry = context, session, geometry
        self.wheel_inputs = []

    def records(self):
        """Reads complete, flushed native lab records."""
        return read_complete_jsonl_records(self.session.host_log_path)

    def point(self, target, column=0, row=0):
        """Maps a target-relative logged cell to guest framebuffer coordinates."""
        layout = next(record for record in reversed(self.records()) if record['kind'] == 'layout')
        area = next(rectangle for rectangle in layout['areas'] if rectangle['target'] == target)
        return cell_center_to_vnc_pixels(self.geometry, area['x']+column, area['y']+row)

    def send(self, tokens):
        """Sends input without changing the laboratory's semantic log window."""
        self.context.vnc.run_tokens(tokens)

    def step(self, action, dismiss_menu=False):
        """Returns a FIFO-fenced native record window for an action."""
        return lab_step(self.context, self.session, action, dismiss_menu)

    def key(self, key):
        """Sends a physical key and waits for its logged report fence."""
        return self.step(lambda: None) if key == 'f2' else self.step(lambda: self.send(build_lab_key_tokens(key)))

    def observed(self, tokens, kind):
        """Waits on actual mouse processing while retaining held input state."""
        start = len(self.records())
        self.send(tokens)
        def arrived():
            trailing = self.records()[start:]
            for record_index, record in enumerate(trailing):
                if record.get('event', {}).get('mouse_kind') == kind and any(trailing_record.get('kind') == 'report' for trailing_record in trailing[record_index+1:]):
                    return trailing
            return None
        return poll_until(arrived, self.context.timeout_seconds, f'mouse_lab {kind} and report')

    def click(self, target, column=0, row=0, count=1):
        """Clicks a logged content cell and scopes repeated clicks to the last."""
        point = self.point(target, column, row)
        records = self.step(lambda: self.observed(build_click_tokens(*point, click_count=count), 'Up(Left)'))
        return final_click_records(records) if count > 1 else records

    def drag(self, target, start, end, modifiers=(), modifier_mask=0, wait_for_motion=True):
        """Drags while awaiting each application event on one VNC connection."""
        start_point, end_point = self.point(target, *start), self.point(target, *end)
        def action():
            try:
                self.send(['move', *map(str, start_point), *[token for modifier in modifiers for token in ['keydown',modifier]]])
                if modifier_mask: wait_for_guest_modifiers(self.context,modifier_mask)
                self.observed(['mousedown','1'], 'Down(Left)')
                if wait_for_motion:
                    self.observed(['move', *map(str, end_point)], 'Drag(Left)')
                else:
                    self.send(['move', *map(str, end_point)])
                self.observed(['mouseup','1'], 'Up(Left)')
            finally:
                self.send([token for modifier in reversed(modifiers) for token in ['keyup',modifier]]) if modifiers else None
        return self.step(action)

    def wheel(self, target, direction, column=2, row=2):
        """Delivers one wheel pulse and waits for its decoded mouse event."""
        tokens = build_lab_wheel_tokens(*self.point(target,column,row), direction)
        self.wheel_inputs.append({'target':target,'direction':direction,'vncButton':int(tokens[-1])})
        is_reported_down = (direction == 'down') != self.context.is_natural_scrolling
        kind = 'ScrollDown' if is_reported_down else 'ScrollUp'
        return self.step(lambda: self.observed(tokens, kind))


def _history_drag(driver):
    start = len(driver.records())
    layout = next(record for record in reversed(driver.records()) if record['kind'] == 'layout')
    right = next(rectangle for rectangle in layout['areas'] if rectangle['target'] == 'right')
    require(right['height'] < 31, 'History endpoint must exceed viewport.')
    driver.send(['move', *map(str,driver.point('right'))])
    driver.observed(['mousedown','1'], 'Down(Left)')
    try:
        driver.observed(['move', *map(str,driver.point('right',right['width']-1,right['height']))], 'Drag(Left)')
        poll_until(lambda: next((record for record in driver.records()[start:] if record.get('kind') == 'selection'
                   and record.get('selection') and record['selection']['range']['end']['line'] >= 30), None),
                   driver.context.timeout_seconds, 'history selection reaches row 030')
    finally:
        driver.observed(['mouseup','1'], 'Up(Left)')
    records = driver.records()[start:]
    driver.key('f2')
    return {'history': records}


def _wheel_during_drag(driver):
    driver.send(['move',*map(str,driver.point('left'))])
    driver.observed(['mousedown','1'],'Down(Left)')
    try:
        driver.observed(['move',*map(str,driver.point('left',4,2))], 'Drag(Left)')
        before = next(record['selection']['range']['end'] for record in reversed(driver.records()) if record.get('kind') == 'selection' and record['selection'])
        # F2 would clear selection, so wait on the actual wheel report instead.
        records = driver.observed(build_lab_wheel_tokens(*driver.point('left',4,2),'down'), 'ScrollDown')
        return {'beforeEnd':before, 'wheel':records}
    finally:
        driver.observed(['mouseup','1'], 'Up(Left)')


def _native_selection(driver, modifiers, rectangle):
    context = driver.context
    context.vm.write_guest_clipboard('TUIKIT_UNTOUCHED')
    start = len(driver.records())
    start_point = driver.point('left')
    end_point = driver.point('left',3,2) if rectangle else driver.point('left',5,0)
    # Native selection uses cell boundaries rather than the inclusive app endpoint.
    offset = round(driver.geometry.cell_width_pt * driver.geometry.backing_scale / 4)
    start_point = (start_point[0]-offset,start_point[1])
    end_point = (end_point[0]-offset,end_point[1])
    try:
        driver.send([token for modifier in modifiers for token in ['keydown',modifier]])
        wait_for_guest_modifiers(context,0x80000 | (0x100000 if rectangle else 0))
        driver.send(build_drag_tokens(start_point,end_point,steps=2))
        clipboard = poll_until(lambda: (text if (text := context.vm.read_guest_clipboard()) != 'TUIKIT_UNTOUCHED' else None),
                               context.timeout_seconds, 'native selection guest clipboard update')
    finally:
        driver.send([token for modifier in reversed(modifiers) for token in ['keyup',modifier]])
    records = driver.step(lambda:None)
    return {'clipboard':clipboard, 'records':driver.records()[start:], 'fenceRecords':records}


def _modifiers(driver):
    modifiers = driver.context.vnc.calibrate_modifiers(driver.context.vm)
    # Fresh sessions avoid native selection state or multi-click timing leaking across cases.
    return modifiers


def wait_for_guest_modifiers(context, mask):
    """Waits until CoreGraphics confirms every modifier used by a gesture."""
    def modifiers_are_down():
        flags = context.vm.exec_guest(f'python3 "{GUEST_SHARED_ROOT}/_cg_flags.py"',10.0).stdout.strip()
        return True if int(flags,16) & mask == mask else None
    poll_until(modifiers_are_down,context.timeout_seconds,'guest CoreGraphics modifier state')


def wait_for_control_modifier(context):
    """Waits on CoreGraphics Control state before synthesizing a context click."""
    wait_for_guest_modifiers(context,0x40000)


def _context_click(driver, target, button, control_key=None):
    def action():
        try:
            if control_key:
                driver.send(['keydown',control_key])
                wait_for_control_modifier(driver.context)
            driver.send(build_click_tokens(*driver.point(target),button=button))
            poll_until(lambda: True if has_popup_menu_window(query_iterm_windows(driver.context.vm)) else None,
                       driver.context.timeout_seconds,'iTerm2 context menu window')
        finally:
            if control_key: driver.send(['keyup',control_key])
    return driver.step(action,True)


def run_gesture(driver, scenario, variant=None):
    """Executes the fixed laboratory fixture for a scenario and modifier variant."""
    if scenario == 'x3':
        hover = driver.step(lambda:driver.send(['move',*map(str,driver.point('left',2,1)), 'move',*map(str,driver.point('left',6,1))]))
        modes = driver.key('m')
        passed = driver.step(lambda:driver.send(['move',*map(str,driver.point('right',2,1)), 'move',*map(str,driver.point('right',6,1))]))
        return {'hover':hover,'paneModes':modes,'passHover':passed}
    if scenario == 'x4':
        return {target:driver.click(target) for target in ['left','right','tab','divider']}
    if scenario == 'x5': return {'drag':driver.drag('left',(0,0),(4,2))}
    if scenario == 'x6': return {'word':driver.click('left',2,3,2), 'line':driver.click('left',0,5,3)}
    if scenario == 'x7': return _history_drag(driver)
    if scenario == 'x8': return {'left':driver.wheel('left','down'), 'right':driver.wheel('right','up')}
    if scenario == 'x9': return _wheel_during_drag(driver)
    if scenario == 'x10':
        driver.key('m'); click = driver.click('right',2,2)
        driver.key('m'); drag = driver.drag('right',(2,2),(6,4))
        return {'click':click,'drag':drag,'wheel':driver.wheel('right','down')}
    if scenario == 'x11':
        modifiers = _modifiers(driver)
        if variant in ['option','option_command']:
            keys = (modifiers.option_key,) + ((modifiers.command_key,) if variant == 'option_command' else ())
            return {variant:_native_selection(driver, keys, variant == 'option_command')}
        driver.key('m')
        key = modifiers.shift_key if variant == 'shift' else modifiers.command_key
        mask = 0x20000 if variant == 'shift' else 0x100000
        return {variant:driver.drag('right',(0,0),(4,0),(key,),mask,
                                    wait_for_motion=not (variant == 'command' and driver.context.tmux_mode))}
    if scenario == 'x12':
        driver.key('a'); return {'wheel':driver.wheel('right','down')}
    if scenario == 'x13':
        keys = ['tab','tab','s','g','j','j','j','home','v','l','v','V','j','V','g','j','j','j','home','W','y']
        return {'keyboard':driver.step(lambda:[driver.send(build_lab_key_tokens(key)) for key in keys])}
    if scenario == 'x14':
        control = _modifiers(driver).ctrl_key
        return {'right':_context_click(driver,'left',3),
                'control':_context_click(driver,'right',1,control)}
    return {}


def prime_lab_input(driver):
    """Consumes the initial selection observation as well as its single F2 fence."""
    driver.key('f2')
    poll_until(lambda: True if any(record.get('kind') == 'selection' for record in driver.records()) else None,
               driver.context.timeout_seconds,'initial laboratory selection baseline')


def _execute_connected_session(context, scenario, variant=None):
    """Launches one isolated lab session and retains native logs on every exit path."""
    label = context.next_session_id(scenario + ('_'+variant if variant else ''))
    args = list(context.extra_args)
    if scenario == 'x1' and '--query-modes' not in args: args.append('--query-modes')
    session = launch_iterm_session(context.vm, SessionLaunchSpec(
        label, context.guest_program_path, args, context.tmux_mode,
        context.get_guest_tmux_config_path_from_mode(context.tmux_mode), should_copy_selection=True))
    try:
        wait_for_probe_ready(session.host_log_path, context.timeout_seconds, session.host_exit_path)
        geometry = resolve_session_geometry(context.vm, session, context.timeout_seconds)
        ensure_iterm_window_focused(context.vm,context.vnc,geometry,context.timeout_seconds)
        driver = LabInput(context,session,geometry)
        prime_lab_input(driver)
        steps = run_gesture(driver,scenario,variant)
        context.vnc.run_tokens(build_lab_key_tokens('!' if scenario == 'x2' else 'q'))
        exit_outcome = wait_for_session_exit_and_teardown(session,context.timeout_seconds)
        require((exit_outcome['exitCode'] != 0) if scenario == 'x2' else exit_outcome['exitCode'] == 0,
                f'Wrong {scenario} process exit {exit_outcome}.')
        records = driver.records()
        assert_lifecycle(records,'--query-modes' in args,is_panic=scenario == 'x2')
        state = exit_outcome['terminalStateAfterExit']
        assert_exit_snapshot(state)
        if scenario != 'x11': assert_gesture(scenario, steps,is_natural_scrolling=context.is_natural_scrolling)
        metadata = json.loads(session.host_meta_path.read_text())
        return {'scenario':scenario,'variant':variant,'pointerSteps':steps,'records':records,
                'windowGeometry':vars(geometry),'sessionMetadata':metadata,'naturalScrolling':context.is_natural_scrolling,
                'wheelInputs':driver.wheel_inputs,**exit_outcome,'passed':True}
    finally:
        if context.artifact_dir and session.host_log_path.is_file():
            context.artifact_dir.mkdir(parents=True,exist_ok=True)
            shutil.copy2(session.host_log_path,context.artifact_dir/f'{label}.jsonl')


def execute_session(context, scenario, variant=None):
    """Keeps one acknowledged VNC connection through each managed lab session."""
    with PersistentLabVnc(context.vnc) as vnc:
        connected = dataclasses.replace(context,vnc=vnc)
        try:
            return _execute_connected_session(connected,scenario,variant)
        finally:
            context._session_counter = connected._session_counter


def _run_single(context, scenario):
    sys.stderr.write(f'==> mouse_lab {scenario} (tmux={context.tmux_mode or "none"})\n'); sys.stderr.flush()
    if scenario != 'x11': return execute_session(context, scenario)
    sessions = {variant:execute_session(context,scenario,variant) for variant in ['shift','command','option','option_command']}
    combined = {key:value for session in sessions.values() for key,value in session['pointerSteps'].items()}
    tmux_version = sessions['command'].get('sessionMetadata',{}).get('tmuxVersion')
    assert_gesture('x11',combined,tmux_version=tmux_version)
    return {'scenario':'x11','sessions':sessions,'passed':True}


def run_mouse_scenario(context: ScenarioRuntimeContext, scenario: str) -> dict:
    """Runs a scenario or complete matrix and restores guest clipboard and wheel preference."""
    clipboard = context.vm.read_guest_clipboard()
    try:
        # A day-boundary tips opt-in prompt can otherwise prevent .command launch.
        # The outer guest-defaults backup restores this test-only preference.
        context.vm.exec_guest('defaults write com.googlecode.iterm2 NoSyncTipsDisabled -bool true')
        set_guest_natural_scrolling(context.vm,False)
        context.is_natural_scrolling = False
        if scenario != 'mouse': return _run_single(context,scenario)
        sessions = {f'x{scenario_number}':_run_single(context,f'x{scenario_number}') for scenario_number in range(1,15)}
        # Real wheel events are asserted with both macOS preferences, in this tmux mode.
        set_guest_natural_scrolling(context.vm,True)
        context.is_natural_scrolling = True
        natural = _run_single(context,'x8')
        return {'scenario':'mouse','scenarios':sessions,'naturalScrollingOn':natural,'passed':True}
    finally:
        try:
            context.vm.write_guest_clipboard(clipboard)
        finally:
            set_guest_natural_scrolling(context.vm,context.is_initial_natural_scrolling)
