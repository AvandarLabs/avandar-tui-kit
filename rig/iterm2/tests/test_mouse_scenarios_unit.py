"""Behavioral checks for the VM laboratory driver, with external VM IO isolated."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from driver.cli_args import CliUsageError, DriverRuntimeError, parse_cli_args
from driver import main, scenarios


class MouseDriverTests(unittest.TestCase):
    def test_lab_dispatch_and_passthrough_flags(self):
        for name in ['mouse', *[f'x{i}' for i in range(1, 15)]]:
            parsed = parse_cli_args(['mouse_lab', name, '--tmux', 'full', '--', '--query-modes'])
            self.assertEqual((parsed.scenario, parsed.tmux_mode, parsed.extra_args),
                             (name, 'full', ['--query-modes']))
        self.assertEqual(parse_cli_args(['mouse_lab']).scenario, 'mouse')
        with self.assertRaises(CliUsageError):
            parse_cli_args(['capability_probe', 'x1'])

    def test_lab_build_selects_only_mouse_crate_and_vt100(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'debug/examples/mouse_lab'
            binary.parent.mkdir(parents=True)
            binary.touch(mode=0o755)
            with mock.patch('driver.main.resolve_host_binary', return_value='cargo'), \
                 mock.patch('driver.main.subprocess.run', return_value=mock.Mock(returncode=0)) as run:
                self.assertEqual(main.resolve_or_build_program_binary(
                    'mouse_lab', {'CARGO_TARGET_DIR': directory}, main.create_cli_theme(False, {})), binary.resolve())
            cargo_args = run.call_args.args[0]
            self.assertIn('--features', cargo_args)
            self.assertIn('vt100', cargo_args)
            self.assertIn('ratatui-mouse', cargo_args)
            self.assertNotIn('--workspace', cargo_args)

    def test_wrong_copy_and_keyboard_mouse_input_are_rejected(self):
        from driver.mouse_scenarios import assert_gesture
        copy_report = {'kind': 'report', 'outcome': {'kind': 'Copy', 'target': 'left', 'text': 'file.rs:12'}}
        assert_gesture('x13', {'keyboard': [copy_report]})
        wrong = copy.deepcopy(copy_report)
        wrong['outcome']['text'] = 'file'
        for records in [[wrong], [], [copy_report, {'kind': 'event', 'event': {'kind': 'Mouse'}}]]:
            with self.assertRaises(DriverRuntimeError):
                assert_gesture('x13', {'keyboard': records})

    def test_wrong_mode_states_are_rejected(self):
        from driver.mouse_scenarios import assert_lifecycle
        records = []
        for label, data in [('setup', '\x1b[?1000h\x1b[?1002h\x1b[?1006h'),
                            ('teardown', '\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l')]:
            records.append({'kind': 'sequence', 'label': label, 'bytes': list(data.encode())})
            for mode in [9, 1000, 1002, 1003, 1005, 1006, 1015, 1016]:
                records.append({'kind': 'mode_reply', 'phase': label, 'mode': mode,
                                'state': 1 if label == 'setup' and mode in [1002, 1006] else 2,
                                'is_absent': False})
        assert_lifecycle(records, True)
        wrong = copy.deepcopy(records)
        next(r for r in wrong if r.get('mode') == 1003)['state'] = 1
        with self.assertRaises(DriverRuntimeError):
            assert_lifecycle(wrong, True)

    def test_selection_failure_restores_clipboard_and_scrolling(self):
        from driver.mouse_scenarios import run_mouse_scenario
        context = scenarios.ScenarioRuntimeContext(mock.Mock(), mock.Mock(), 'mouse_lab', '/tmp/lab', None, {}, 100)
        context.vm.read_guest_clipboard.return_value = 'original clipboard'
        context.is_initial_natural_scrolling = True
        def session_result(_context, _scenario, variant):
            if variant in ['shift','command']:
                modifiers = 'KeyModifiers(SHIFT)' if variant == 'shift' else 'KeyModifiers(ALT)'
                records = [{'kind':'event','event':{'kind':'Mouse','mouse_kind':kind,'modifiers':modifiers}}
                           for kind in ['Down(Left)','Drag(Left)','Up(Left)']]
                records.append({'kind':'report','outcome':{'kind':'Copy','target':'right','text':'right'}})
                return {'pointerSteps':{variant:records}}
            return {'pointerSteps':{variant:{'clipboard':'wrong native selection','records':[]}}}
        with mock.patch('driver.mouse_scenarios.execute_session', side_effect=session_result), \
             mock.patch('driver.mouse_scenarios.set_guest_natural_scrolling') as scrolling:
            with self.assertRaises(DriverRuntimeError):
                run_mouse_scenario(context, 'x11')
        self.assertEqual(context.vm.write_guest_clipboard.call_args.args, ('original clipboard',))
        self.assertEqual(scrolling.call_args.args, (context.vm, True))

    def test_outer_guest_defaults_restored_after_lab_failure(self):
        parsed = parse_cli_args(['mouse_lab', 'mouse'])
        vm = mock.Mock()
        with mock.patch('driver.main._stage_guest_binaries_and_configs', return_value=('/tmp/lab', {})), \
             mock.patch('driver.main.backup_guest_defaults') as backup, \
             mock.patch('driver.main.restore_guest_defaults') as restore, \
             mock.patch('driver.main.run_named_scenario', side_effect=DriverRuntimeError('wrong Copy', 'check')):
            with self.assertRaises(DriverRuntimeError):
                main._run_booted_vm_payload(vm, mock.Mock(), parsed, Path('/tmp/lab'), {}, main.create_cli_theme(False, {}))
        self.assertEqual(restore.call_args.args, (vm, backup.return_value))

    def test_lab_fence_waits_for_report_and_keeps_native_records(self):
        from driver.mouse_scenarios import lab_step
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'lab.jsonl'
            session = mock.Mock(host_log_path=path)
            session.host_exit_path.is_file.return_value = False
            context = mock.Mock(timeout_seconds=.2)
            def send_fence():
                records = [{'kind':'selection','selection':None}, {'kind':'event','event':{'kind':'Key','code':'F(2)','key_kind':'Press'}}, {'kind':'report','outcome':{'kind':'Ignored'}}]
                path.write_text(''.join(json.dumps(r)+'\n' for r in records))
            context.vnc.send_fence_key.side_effect = send_fence
            self.assertEqual(lab_step(context, session, lambda:None), [{'kind':'selection','selection':None}])

class GestureEvidenceTests(unittest.TestCase):
    """Every scenario must reject absent evidence, rather than just dispatch."""
    def test_missing_evidence_cannot_pass_pointer_scenarios(self):
        from driver.mouse_scenarios import assert_gesture
        steps = {name: [] for name in ['hover','passHover','paneModes','left','right','tab','divider','drag','word','line','history','wheel','click','keyboard','shift','command']}
        steps['beforeEnd'] = None
        steps['option'] = steps['option_command'] = {'clipboard':'wrong','records':[]}
        for scenario in ['x3','x4','x5','x6','x7','x8','x9','x10','x11','x12','x13']:
            with self.subTest(scenario=scenario), self.assertRaises(DriverRuntimeError):
                assert_gesture(scenario, steps)

    def test_context_menu_rejects_app_events_without_waiting_for_them(self):
        from driver.mouse_scenarios import assert_gesture
        assert_gesture('x14', {'right':[], 'control':[]})
        for label in ['right','control']:
            steps = {'right':[], 'control':[]}
            steps[label] = [{'kind':'event','event':{'kind':'Mouse','mouse_kind':'Down(Left)'}}]
            with self.assertRaises(DriverRuntimeError):
                assert_gesture('x14',steps)

    def test_wrong_pane_bytes_and_wheel_directions_fail(self):
        from driver.mouse_scenarios import assert_gesture
        steps = {'click':[{'kind':'report','outcome':{'kind':'Forward','target':'right','bytes':[27,91,77]}}], 'drag':[], 'wheel':[]}
        with self.assertRaises(DriverRuntimeError): assert_gesture('x10',steps)
        steps = {'left':[{'kind':'report','outcome':{'kind':'Wheel','target':'left','rows':-3,'columns':0}}], 'right':[]}
        with self.assertRaises(DriverRuntimeError): assert_gesture('x8',steps)

class LabPanicInputTests(unittest.TestCase):
    def test_panic_sends_physical_shift_1_for_lab_exclamation(self):
        from driver import mouse_scenarios as mouse
        context = scenarios.ScenarioRuntimeContext(mock.Mock(), mock.Mock(), 'mouse_lab', '/tmp/lab', None, {}, 100)
        session = mock.Mock()
        with mock.patch.object(mouse,'launch_iterm_session',return_value=session), \
             mock.patch.object(mouse,'wait_for_probe_ready'), \
             mock.patch.object(mouse,'resolve_session_geometry'), \
             mock.patch.object(mouse,'ensure_iterm_window_focused'), \
             mock.patch.object(mouse,'LabInput') as lab_input, \
             mock.patch.object(mouse,'wait_for_session_exit_and_teardown',side_effect=DriverRuntimeError('stop after key','test')):
            lab_input.return_value.context=context
            lab_input.return_value.records.return_value=[{'kind':'selection','selection':None}]
            with self.assertRaises(DriverRuntimeError): mouse._execute_connected_session(context,'x2')
        self.assertEqual(context.vnc.run_tokens.call_args.args, (['keydown','shift','key','1','keyup','shift'],))

class LabArgumentValidationTests(unittest.TestCase):
    def test_probe_scenarios_and_control_mode_are_rejected_for_lab(self):
        for args in [['mouse_lab','all'], ['mouse_lab','panic'], ['mouse_lab','mouse','--tmux','cc-full']]:
            with self.subTest(args=args), self.assertRaises(CliUsageError): parse_cli_args(args)

class PanicLifecycleTests(unittest.TestCase):
    def test_panic_uses_external_mode_snapshot_without_invented_teardown_replies(self):
        from driver.mouse_scenarios import assert_lifecycle
        records = [{'kind':'sequence','label':label,'bytes':list(data.encode())} for label,data in
                   [('setup','\x1b[?1000h\x1b[?1002h\x1b[?1006h'), ('teardown','\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l')]]
        records += [{'kind':'mode_reply','phase':'setup','mode':mode,'is_absent':False,'state':1 if mode in [1002,1006] else 2} for mode in [9,1000,1002,1003,1005,1006,1015,1016]]
        assert_lifecycle(records,True,is_panic=True)
        with self.assertRaises(DriverRuntimeError): assert_lifecycle(records,True)

class LabShiftInputTests(unittest.TestCase):
    def test_uppercase_selection_keys_use_physical_shift(self):
        from driver.mouse_scenarios import build_lab_key_tokens
        self.assertEqual(build_lab_key_tokens('W'), ['keydown','shift','key','w','keyup','shift'])
        self.assertEqual(build_lab_key_tokens('V'), ['keydown','shift','key','v','keyup','shift'])
        self.assertEqual(build_lab_key_tokens('q'), ['key','q'])

class InterruptedCleanupTests(unittest.TestCase):
    def test_lab_interrupt_restores_defaults_before_original_vm_stop_handler(self):
        import signal
        parsed = parse_cli_args(['mouse_lab','mouse'])
        installed = {}
        original = mock.Mock()
        restored = []
        def set_handler(number, handler): installed[number] = handler
        def interrupt(*args):
            installed[signal.SIGTERM](signal.SIGTERM, None)
        with mock.patch('driver.main._stage_guest_binaries_and_configs',return_value=('/tmp/lab',{})), \
             mock.patch('driver.main.backup_guest_defaults'), \
             mock.patch('driver.main.restore_guest_defaults',side_effect=lambda *a:restored.append(True)), \
             mock.patch('driver.main.signal.getsignal',return_value=original), \
             mock.patch('driver.main.signal.signal',side_effect=set_handler), \
             mock.patch('driver.main.run_named_scenario',side_effect=interrupt):
            with self.assertRaises(SystemExit):
                main._run_booted_vm_payload(mock.Mock(),mock.Mock(),parsed,Path('/tmp/lab'),{},main.create_cli_theme(False,{}))
        self.assertEqual(restored,[True])
        self.assertFalse(original.called)
        self.assertIs(installed[signal.SIGTERM],original)

class PersistentVncTests(unittest.TestCase):
    def test_held_pointer_state_survives_acknowledged_commands(self):
        from driver.mouse_scenarios import PersistentLabVnc
        from driver.tart_vm import VncEndpoint
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root/'vncdotool'
            package.mkdir()
            (package/'__init__.py').write_text('')
            (package/'api.py').write_text('''
class Client:
    def __init__(self): self.x=0; self.y=0; self.held=False
    def mouseMove(self,x,y): self.x=x; self.y=y
    def mouseDown(self,button): self.held=True
    def mouseUp(self,button):
        assert self.held and (self.x,self.y)==(200,300)
        self.held=False
    def keyDown(self,key): pass
    def keyUp(self,key): pass
    def keyPress(self,key): pass
    def disconnect(self): pass
def connect(*args,**kwargs): return Client()
def shutdown(): pass
''')
            launcher = root/'vncdo'
            launcher.write_text('#!'+sys.executable+'\n')
            original = mock.Mock(vncdo_bin=str(launcher),endpoint=VncEndpoint('localhost',5900,'test'),env={'PYTHONPATH':str(root)})
            with PersistentLabVnc(original) as vnc:
                vnc.run_tokens(['move','100','150','mousedown','1'])
                vnc.run_tokens(['move','200','300'])
                vnc.run_tokens(['mouseup','1'])
                vnc.send_fence_key()

class PersistentCleanupTests(unittest.TestCase):
    def test_bootstrap_failure_closes_owned_process_and_streams(self):
        from driver.mouse_scenarios import PersistentLabVnc
        from driver.tart_vm import VncEndpoint
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            package=root/'vncdotool'; package.mkdir()
            (package/'__init__.py').write_text('')
            (package/'api.py').write_text("def connect(*a,**k): raise RuntimeError('bootstrap failed')\n")
            launcher=root/'vncdo'; launcher.write_text('#!'+sys.executable+'\n')
            original=mock.Mock(vncdo_bin=str(launcher),endpoint=VncEndpoint('localhost',5900,'test'),env={'PYTHONPATH':str(root)})
            vnc=PersistentLabVnc(original)
            import subprocess
            original_popen=subprocess.Popen
            with tempfile.TemporaryFile(mode='w+t') as diagnostics:
                def start_with_captured_diagnostic(*args,**kwargs):
                    kwargs['stderr']=diagnostics
                    return original_popen(*args,**kwargs)
                with mock.patch('driver.mouse_scenarios.subprocess.Popen',side_effect=start_with_captured_diagnostic):
                    with self.assertRaises(DriverRuntimeError):
                        with vnc: pass
                diagnostics.seek(0)
                diagnostic=diagnostics.read()
                self.assertTrue(diagnostic.startswith('Traceback (most recent call last):'))
                self.assertTrue(diagnostic.endswith('RuntimeError: bootstrap failed\n'))
                self.assertEqual(diagnostic.count('Traceback'),1)
            self.assertIsNotNone(vnc.process.poll())
            self.assertTrue(vnc.process.stdin.closed)
            self.assertTrue(vnc.process.stdout.closed)

class MultiClickTests(unittest.TestCase):
    def test_final_click_excludes_intermediate_word_copy_from_triple_click(self):
        from driver.mouse_scenarios import final_click_records, check_copy
        records = []
        for text in [None,'soft','soft wrap over three rows']:
            records += [{'kind':'event','event':{'kind':'Mouse','mouse_kind':'Down(Left)'}},
                        {'kind':'event','event':{'kind':'Mouse','mouse_kind':'Up(Left)'}}]
            if text: records.append({'kind':'report','outcome':{'kind':'Copy','target':'left','text':text}})
        last=final_click_records(records)
        check_copy(last,'left','soft wrap over three rows')
        self.assertEqual(len(last),3)

class WheelEncodingTests(unittest.TestCase):
    def test_semantic_wheel_direction_uses_rfb_four_up_five_down(self):
        from driver.mouse_scenarios import build_lab_wheel_tokens
        self.assertEqual(build_lab_wheel_tokens(50,70,'down'), ['move','50','70','click','5'])
        self.assertEqual(build_lab_wheel_tokens(50,70,'up'), ['move','50','70','click','4'])

    def test_sessions_allocate_distinct_archive_names_after_context_copy(self):
        from driver import mouse_scenarios as mouse
        context=scenarios.ScenarioRuntimeContext(mock.Mock(),mock.Mock(),'mouse_lab','/tmp/lab',None,{},100)
        with mock.patch.object(mouse,'PersistentLabVnc'), mock.patch.object(mouse,'_execute_connected_session',side_effect=lambda c,s,v:c.next_session_id(s)):
            self.assertEqual([mouse.execute_session(context,'x8') for _ in range(2)], ['x8_01','x8_02'])

class StrictEvidenceTests(unittest.TestCase):
    def test_post_exit_query_silence_is_never_interpreted_as_reset(self):
        from driver.mouse_scenarios import assert_exit_snapshot
        state={'mouseTrackingMode':'none','sgrMouseMode':False,'rawModeStates':{}}
        with self.assertRaises(DriverRuntimeError): assert_exit_snapshot(state)
        state['rawModeStates']={'1000':2,'1002':2,'1003':2,'1006':2}
        assert_exit_snapshot(state)
        state['rawModeStates']['1006']=1
        with self.assertRaises(DriverRuntimeError): assert_exit_snapshot(state)

    def test_unsupported_encoding_cannot_remain_enabled_after_teardown(self):
        from driver.mouse_scenarios import assert_lifecycle
        records=[{'kind':'sequence','label':label,'bytes':list(sequence.encode())} for label,sequence in
                 [('setup','\x1b[?1000h\x1b[?1002h\x1b[?1006h'), ('teardown','\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l')]]
        records += [{'kind':'mode_reply','phase':phase,'mode':mode,'is_absent':False,
                     'state':1 if phase=='setup' and mode in [1002,1006] else 2}
                    for phase in ['setup','teardown'] for mode in [9,1000,1002,1003,1005,1006,1015,1016]]
        next(r for r in records if r.get('mode')==1005 and r.get('phase')=='teardown')['state']=1
        with self.assertRaises(DriverRuntimeError): assert_lifecycle(records,True)

    def test_control_click_waits_for_guest_modifier_state(self):
        from driver.mouse_scenarios import wait_for_control_modifier
        context=mock.Mock(timeout_seconds=.2)
        context.vm.exec_guest.side_effect=[mock.Mock(stdout='0x0'),mock.Mock(stdout='0x40000')]
        wait_for_control_modifier(context)
        self.assertEqual(context.vm.exec_guest.call_count,2)

class NativeSelectionEvidenceTests(unittest.TestCase):
    def test_native_selection_cannot_mutate_app_selection(self):
        from driver.mouse_scenarios import assert_gesture
        steps={}
        for name,modifiers in [('shift','KeyModifiers(SHIFT)'),('command','KeyModifiers(ALT)')]:
            steps[name]=[{'kind':'event','event':{'kind':'Mouse','mouse_kind':kind,'modifiers':modifiers}} for kind in ['Down(Left)','Drag(Left)','Up(Left)']]
            steps[name].append({'kind':'report','outcome':{'kind':'Copy','target':'right','text':'right'}})
        steps['option']={'clipboard':'alpha','records':[{'kind':'selection','selection':{'target':'left'}}]}
        steps['option_command']={'clipboard':'alp\nbet\ngam','records':[]}
        with self.assertRaises(DriverRuntimeError): assert_gesture('x11',steps)

class SingleFenceTests(unittest.TestCase):
    def test_requesting_f2_does_not_leave_a_second_fence_for_next_gesture(self):
        from driver.mouse_scenarios import LabInput
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'lab.jsonl'
            context=mock.Mock(timeout_seconds=.2)
            session=mock.Mock(host_log_path=path)
            session.host_exit_path.is_file.return_value=False
            def emit_fence(*args):
                with path.open('a') as output:
                    for record in [{'kind':'event','event':{'kind':'Key','code':'F(2)','key_kind':'Press'}}, {'kind':'report','outcome':{'kind':'Ignored'}}]:
                        output.write(json.dumps(record)+'\n')
            context.vnc.run_tokens.side_effect=emit_fence
            context.vnc.send_fence_key.side_effect=emit_fence
            driver=LabInput(context,session,None)
            driver.key('f2')
            records=[json.loads(line) for line in path.read_text().splitlines()]
            self.assertEqual(len([r for r in records if r['kind']=='event']),1)

class BoundedAcknowledgmentTests(unittest.TestCase):
    def test_partial_acknowledgment_times_out_without_blocking_readline(self):
        import os
        import queue
        import threading
        from driver.mouse_scenarios import PersistentLabVnc
        read_fd,write_fd=os.pipe()
        outcomes=queue.Queue()
        try:
            os.write(write_fd,b'{"ok":')
            with os.fdopen(read_fd,'r') as reader:
                vnc=PersistentLabVnc(mock.Mock())
                vnc.process=mock.Mock(stdout=reader)
                def wait_for_acknowledgment():
                    try: vnc._response(.05)
                    except BaseException as error: outcomes.put(error)
                    else: outcomes.put(None)
                worker=threading.Thread(target=wait_for_acknowledgment)
                worker.start()
                try:
                    try: outcome=outcomes.get(timeout=.5)
                    except queue.Empty:
                        os.close(write_fd);write_fd=None
                        self.fail('Partial acknowledgment exceeded its bounded timeout.')
                    self.assertIsInstance(outcome,DriverRuntimeError)
                finally:
                    if write_fd is not None: os.close(write_fd);write_fd=None
                    worker.join(timeout=.5)
                    self.assertFalse(worker.is_alive())
        finally:
            if write_fd is not None: os.close(write_fd)


class ModifierBarrierTests(unittest.TestCase):
    def test_combined_modifier_wait_requires_every_guest_flag(self):
        from driver.mouse_scenarios import wait_for_guest_modifiers
        context=mock.Mock(timeout_seconds=.2)
        context.vm.exec_guest.side_effect=[mock.Mock(stdout='0x80000'),mock.Mock(stdout='0x180000')]
        wait_for_guest_modifiers(context,0x180000)
        self.assertEqual(context.vm.exec_guest.call_count,2)

    def test_native_gesture_holds_option_until_clipboard_confirmation(self):
        from driver.mouse_scenarios import _native_selection
        driver=mock.Mock()
        driver.records.return_value=[]
        driver.point.side_effect=[(100,100),(170,100)]
        driver.geometry.cell_width_pt=7
        driver.geometry.backing_scale=2
        driver.context.timeout_seconds=.2
        driver.context.vm.exec_guest.return_value.stdout='0x80000'
        held=set()
        def send(tokens):
            for index,token in enumerate(tokens):
                if token=='keydown': held.add(tokens[index+1])
                elif token=='keyup': held.discard(tokens[index+1])
        driver.send.side_effect=send
        def clipboard():
            self.assertIn('meta',held)
            return 'alpha'
        driver.context.vm.read_guest_clipboard.side_effect=clipboard
        result=_native_selection(driver,('meta',),False)
        self.assertEqual(result['clipboard'],'alpha')
        self.assertEqual(held,set())

class ModeUnknownObservationTests(unittest.TestCase):
    def test_tmux_unknown_format_reply_is_preserved_and_differs_from_absence(self):
        from driver.mouse_scenarios import assert_lifecycle
        records=[{'kind':'sequence','label':label,'bytes':list(sequence.encode())} for label,sequence in
                 [('setup','\x1b[?1000h\x1b[?1002h\x1b[?1006h'), ('teardown','\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l')]]
        modes=[9,1000,1002,1003,1005,1006,1015,1016]
        for phase,states in [('setup',[0,2,1,2,2,1,0,0]), ('teardown',[0,2,2,2,2,2,0,0])]:
            records += [{'kind':'mode_reply','phase':phase,'mode':mode,'state':state,'is_absent':False} for mode,state in zip(modes,states)]
        assert_lifecycle(records,True)
        unknown=next(record for record in records if record.get('mode')==9)
        unknown.update(state=None,is_absent=True)
        with self.assertRaises(DriverRuntimeError): assert_lifecycle(records,True)

class PrimingCompletionTests(unittest.TestCase):
    def test_priming_consumes_initial_selection_observation_after_fence_report(self):
        from driver.mouse_scenarios import prime_lab_input
        driver=mock.Mock()
        driver.context.timeout_seconds=.2
        driver.records.side_effect=[[],[{'kind':'selection','selection':None}]]
        prime_lab_input(driver)
        self.assertEqual(driver.records.call_count,2)

class ModifierFailureCleanupTests(unittest.TestCase):
    def create_driver(self):
        driver=mock.Mock()
        driver.records.return_value=[]
        driver.point.side_effect=[(100,100),(170,130)]
        driver.geometry.cell_width_pt=7
        driver.geometry.backing_scale=2
        driver.context.timeout_seconds=.05
        held=set()
        def send(tokens):
            for index,token in enumerate(tokens):
                if token=='keydown': held.add(tokens[index+1])
                elif token=='keyup': held.discard(tokens[index+1])
        driver.send.side_effect=send
        return driver,held

    def test_native_timeout_releases_option_and_command(self):
        from driver.mouse_scenarios import _native_selection
        for modifiers,rectangle,mask in [(('meta',),False,'0x80000'), (('meta','alt'),True,'0x180000')]:
            driver,held=self.create_driver()
            driver.context.vm.exec_guest.return_value.stdout=mask
            driver.context.vm.read_guest_clipboard.return_value='TUIKIT_UNTOUCHED'
            with self.assertRaises(DriverRuntimeError): _native_selection(driver,modifiers,rectangle)
            self.assertEqual(held,set())

    def test_native_barrier_exception_releases_option_and_command(self):
        from driver.mouse_scenarios import _native_selection
        driver,held=self.create_driver()
        driver.context.vm.exec_guest.side_effect=DriverRuntimeError('guest flag read failed','test')
        with self.assertRaises(DriverRuntimeError): _native_selection(driver,('meta','alt'),True)
        self.assertEqual(held,set())

    def test_control_barrier_timeout_releases_control_without_bridge_shutdown(self):
        from driver import mouse_scenarios as mouse
        driver,held=self.create_driver()
        driver.step.side_effect=lambda action,_dismiss:action()
        with mock.patch.object(mouse,'wait_for_control_modifier',side_effect=DriverRuntimeError('control timeout','test')):
            with self.assertRaises(DriverRuntimeError): mouse._context_click(driver,'right',1,'ctrl')
        self.assertEqual(held,set())

class TmuxCommandMotionTests(unittest.TestCase):
    def test_tmux38_command_down_up_is_measured_without_inventing_drag_or_copy(self):
        from driver.mouse_scenarios import assert_gesture
        steps={}
        for name,modifier,kinds in [('shift','KeyModifiers(SHIFT)',['Down(Left)','Drag(Left)','Up(Left)']), ('command','KeyModifiers(ALT)',['Down(Left)','Up(Left)'])]:
            steps[name]=[{'kind':'event','event':{'kind':'Mouse','mouse_kind':kind,'modifiers':modifier}} for kind in kinds]
        steps['shift'].append({'kind':'report','outcome':{'kind':'Copy','target':'right','text':'right'}})
        steps['command'].append({'kind':'selection','selection':{'target':'right','stage':'Pressed','range':{'start':{'line':0,'column':0},'end':{'line':0,'column':0}}}})
        steps['option']={'clipboard':'alpha','records':[]}
        steps['option_command']={'clipboard':'alp\nbet\ngam','records':[]}
        assert_gesture('x11',steps,tmux_version='tmux 3.8')
        with self.assertRaises(DriverRuntimeError): assert_gesture('x11',steps)
        steps['command'].append({'kind':'report','outcome':{'kind':'Copy','target':'right','text':'right'}})
        with self.assertRaises(DriverRuntimeError): assert_gesture('x11',steps,tmux_version='tmux 3.8')

class NaturalWheelDirectionTests(unittest.TestCase):
    def test_natural_scrolling_reverses_received_steps_for_same_hardware_pulses(self):
        from driver.mouse_scenarios import assert_gesture
        steps={target:[{'kind':'report','outcome':{'kind':'Wheel','target':target,'rows':rows,'columns':0}}] for target,rows in [('left',-3),('right',3)]}
        assert_gesture('x8',steps,is_natural_scrolling=True)
        with self.assertRaises(DriverRuntimeError): assert_gesture('x8',steps)

    def test_waiter_expects_reversed_mouse_event_without_changing_hardware_button(self):
        from driver.mouse_scenarios import LabInput
        driver=LabInput(mock.Mock(is_natural_scrolling=True),mock.Mock(),None)
        driver.point=mock.Mock(return_value=(50,70))
        driver.step=lambda action:action()
        driver.observed=mock.Mock(return_value=[])
        driver.wheel('left','down')
        self.assertEqual(driver.observed.call_args.args, (['move','50','70','click','5'],'ScrollUp'))

class BridgeHeldInputCleanupTests(unittest.TestCase):
    def test_exception_shutdown_releases_held_modifiers_and_mouse_button(self):
        from driver.mouse_scenarios import PersistentLabVnc
        from driver.tart_vm import VncEndpoint
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); package=root/'vncdotool';package.mkdir()
            (package/'__init__.py').write_text('')
            (package/'api.py').write_text('''
import json,os,pathlib
class Client:
    def __init__(self): self.keys=set();self.buttons=set()
    def keyDown(self,key): self.keys.add(key)
    def keyUp(self,key): self.keys.remove(key)
    def mouseDown(self,button): self.buttons.add(button)
    def mouseUp(self,button): self.buttons.remove(button)
    def disconnect(self):
        assert not self.keys and not self.buttons
        pathlib.Path(os.environ['STATE_PATH']).write_text(json.dumps({'released':True}))
def connect(*a,**k): return Client()
def shutdown(): pass
''')
            launcher=root/'vncdo';launcher.write_text('#!'+sys.executable+'\n')
            state=root/'state.json'
            original=mock.Mock(vncdo_bin=str(launcher),endpoint=VncEndpoint('localhost',5900,'test'),env={'PYTHONPATH':str(root),'STATE_PATH':str(state)})
            with self.assertRaises(DriverRuntimeError):
                with PersistentLabVnc(original) as vnc:
                    vnc.run_tokens(['keydown','meta','keydown','alt','mousedown','1'])
                    raise DriverRuntimeError('injected gesture failure','test')
            self.assertEqual(vnc.process.returncode,0)
            self.assertEqual(json.loads(state.read_text()),{'released':True})

class PartialModifierSendCleanupTests(unittest.TestCase):
    def test_failed_keydown_send_releases_any_modifiers_already_delivered(self):
        from driver import mouse_scenarios as mouse
        for gesture in ['native','app','control']:
            with self.subTest(gesture=gesture):
                driver=mock.Mock()
                driver.records.return_value=[]
                driver.point.side_effect=[(100,100),(170,130)]
                driver.geometry.cell_width_pt=7
                driver.geometry.backing_scale=2
                driver.step.side_effect=lambda action,*unused:action()
                held=set()
                def send(tokens):
                    for index,token in enumerate(tokens):
                        if token=='keydown': held.add(tokens[index+1])
                        elif token=='keyup': held.discard(tokens[index+1])
                    if 'keydown' in tokens:
                        raise DriverRuntimeError('send failed after delivering keydown','test')
                driver.send.side_effect=send
                with self.assertRaises(DriverRuntimeError):
                    if gesture=='native': mouse._native_selection(driver,('meta','alt'),True)
                    elif gesture=='app': mouse.LabInput.drag(driver,'right',(0,0),(4,0),('alt',),0x100000)
                    else: mouse._context_click(driver,'right',1,'ctrl')
                self.assertEqual(held,set())

class PreferenceCacheCoherenceTests(unittest.TestCase):
    def test_restore_replaces_preference_daemon_cache_before_it_can_flush_test_values(self):
        import os
        import plistlib
        import shlex
        import subprocess
        from driver.iterm_window import GuestDefaultsBackup, restore_guest_defaults
        from driver.guest_scripts import ACTIVATE_SETTINGS_BINARY
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            preferences=root/'restored preferences.plist'
            cached=root/'cached preferences.plist'
            backup_file=root/'original backup.plist'
            original=plistlib.dumps({'NoSyncLaunchExperienceControllerRunCount':7,'NoSyncSuppressedAlertsCatalog':[]})
            modified=plistlib.dumps({'NoSyncLaunchExperienceControllerRunCount':99,'NoSyncSuppressedAlertsCatalog':['test']})
            backup_file.write_bytes(original)
            preferences.write_bytes(modified);cached.write_bytes(modified)
            killall=root/'killall'
            killall.write_text('#!/bin/sh\nfor argument in "$@"; do\n if [ "$argument" = cfprefsd ]; then cp "$ITERM_TEST_CACHE" "$ITERM_TEST_PREFS"; fi\ndone\n')
            defaults=root/'defaults'
            defaults.write_text('#!/bin/sh\nif [ "$1" = import ]; then\n cp "$3" "$ITERM_TEST_PREFS"\n cp "$3" "$ITERM_TEST_CACHE"\nfi\n')
            activate=root/'activate';activate.write_text('#!/bin/sh\nexit 0\n')
            for executable in [killall,defaults,activate]: executable.chmod(0o755)
            environment={**os.environ,'ITERM_TEST_CACHE':str(cached),'ITERM_TEST_PREFS':str(preferences)}
            def execute_guest(command,**unused):
                translated=command.replace('~/Library/Preferences/com.googlecode.iterm2.plist',shlex.quote(str(preferences)))
                translated=translated.replace('killall ',shlex.quote(str(killall))+' ').replace('defaults ',shlex.quote(str(defaults))+' ')
                translated=translated.replace(ACTIVATE_SETTINGS_BINARY,shlex.quote(str(activate)))
                self.assertNotIn('~/Library/',translated)
                return subprocess.run(['/bin/bash','-c',translated],env=environment,check=True,capture_output=True,text=True)
            vm=mock.Mock();vm.exec_guest.side_effect=execute_guest
            restore_guest_defaults(vm,GuestDefaultsBackup(str(backup_file),True,True,True,False))
            self.assertEqual(plistlib.loads(preferences.read_bytes()),plistlib.loads(original))
            self.assertFalse(backup_file.exists())


class IncompleteFenceTests(unittest.TestCase):
    def test_fence_event_without_following_report_cannot_complete_a_step(self):
        from driver.mouse_scenarios import lab_step
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'lab.jsonl'
            session=mock.Mock(host_log_path=path)
            session.host_exit_path.is_file.return_value=False
            context=mock.Mock(timeout_seconds=.05)
            event={'kind':'event','event':{'kind':'Key','code':'F(2)','key_kind':'Press'}}
            context.vnc.send_fence_key.side_effect=lambda:path.write_text(json.dumps(event)+'\n')
            with self.assertRaises(DriverRuntimeError): lab_step(context,session,lambda:None)


class TmuxPositiveSelectionTests(unittest.TestCase):
    def test_filtered_command_requires_right_pressed_zero_range_evidence(self):
        from driver.mouse_scenarios import assert_gesture
        steps={}
        for name,modifier,kinds in [('shift','KeyModifiers(SHIFT)',['Down(Left)','Drag(Left)','Up(Left)']), ('command','KeyModifiers(ALT)',['Down(Left)','Up(Left)'])]:
            steps[name]=[{'kind':'event','event':{'kind':'Mouse','mouse_kind':kind,'modifiers':modifier}} for kind in kinds]
        steps['shift'].append({'kind':'report','outcome':{'kind':'Copy','target':'right','text':'right'}})
        steps['option']={'clipboard':'alpha','records':[]}
        steps['option_command']={'clipboard':'alp\nbet\ngam','records':[]}
        selection={'target':'right','stage':'Pressed','range':{'start':{'line':0,'column':0},'end':{'line':0,'column':0}}}
        valid={'kind':'selection','selection':selection}
        events=copy.deepcopy(steps['command'])
        steps['command']=events+[valid]
        assert_gesture('x11',steps,tmux_version='tmux 3.8')
        wrong_target={'kind':'selection','selection':{**selection,'target':'left'}}
        bad_windows=[[],[{'kind':'selection','selection':None}],
                     [{'kind':'selection','selection':None}]*2,
                     [{'kind':'selection','selection':{}}],[wrong_target],
                     [valid,wrong_target],
                     [{'kind':'selection','selection':{**selection,'stage':'Dragging'}}],
                     [{'kind':'selection','selection':{**selection,'range':{'start':{'line':0,'column':0},'end':{'line':1,'column':0}}}}]]
        for window in bad_windows:
            with self.subTest(window=window):
                steps['command']=events+window
                with self.assertRaises(DriverRuntimeError): assert_gesture('x11',steps,tmux_version='tmux 3.8')

class FailedPreferenceImportTests(unittest.TestCase):
    def test_failed_import_keeps_backup_attempts_globals_and_propagates_failure(self):
        import os
        import shlex
        import subprocess
        from driver.iterm_window import GuestDefaultsBackup, restore_guest_defaults
        from driver.guest_scripts import ACTIVATE_SETTINGS_BINARY
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); backup=root/'original backup.plist';events=root/'cleanup.log'
            backup.write_bytes(b'original preferences')
            defaults=root/'defaults'
            defaults.write_text('#!/bin/sh\nif [ "$1" = import ]; then exit 73; fi\nprintf "%s\\n" "$*" >> "$ITERM_TEST_EVENTS"\n')
            killall=root/'killall';killall.write_text('#!/bin/sh\nprintf "killall %s\\n" "$*" >> "$ITERM_TEST_EVENTS"\n')
            activate=root/'activate';activate.write_text('#!/bin/sh\nprintf "activate\\n" >> "$ITERM_TEST_EVENTS"\n')
            for executable in [defaults,killall,activate]: executable.chmod(0o755)
            def execute_guest(command,**kwargs):
                translated=command.replace('killall ',shlex.quote(str(killall))+' ').replace('defaults ',shlex.quote(str(defaults))+' ')
                translated=translated.replace(ACTIVATE_SETTINGS_BINARY,shlex.quote(str(activate)))
                self.assertNotIn('~/Library/',translated)
                result=subprocess.run(['/bin/bash','-c',translated],env={**os.environ,'ITERM_TEST_EVENTS':str(events)},capture_output=True,text=True,check=False)
                if result.returncode and kwargs.get('should_check',True):
                    raise DriverRuntimeError(f'Guest restoration failed with code {result.returncode}.','test')
                return result
            vm=mock.Mock();vm.exec_guest.side_effect=execute_guest
            with self.assertRaises(DriverRuntimeError) as error:
                restore_guest_defaults(vm,GuestDefaultsBackup(str(backup),True,True,True,False))
            self.assertIn('73',str(error.exception))
            self.assertEqual(backup.read_bytes(),b'original preferences')
            cleanup=events.read_text()
            self.assertIn('write -g com.apple.swipescrolldirection -bool true',cleanup)
            self.assertIn('write com.apple.WindowManager EnableStandardClickToShowDesktop -bool false',cleanup)
            self.assertIn('killall cfprefsd',cleanup)
            self.assertIn('activate',cleanup)


class RetainedBackupNonClobberTests(unittest.TestCase):
    def test_new_run_refuses_to_overwrite_retained_recovery_backup(self):
        import shlex
        import subprocess
        from driver.iterm_window import backup_guest_defaults
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);backup=root/'retained.plist';current=root/'current.plist'
            backup.write_bytes(b'original recovery settings');current.write_bytes(b'modified test settings')
            def execute_guest(command,**kwargs):
                if 'cp ' not in command: raise DriverRuntimeError('Backup setup unexpectedly continued.','test')
                translated=command.replace('~/Library/Preferences/com.googlecode.iterm2.plist',shlex.quote(str(current)))
                translated=translated.replace('/tmp/tuikit_iterm2_plist_backup.plist',shlex.quote(str(backup)))
                result=subprocess.run(['/bin/bash','-c',translated],capture_output=True,text=True,check=False)
                if result.returncode and kwargs.get('should_check',True): raise DriverRuntimeError('Guest command failed.','test')
                return result
            vm=mock.Mock();vm.exec_guest.side_effect=execute_guest
            with self.assertRaises(DriverRuntimeError) as error: backup_guest_defaults(vm)
            self.assertEqual(backup.read_bytes(),b'original recovery settings')
            self.assertIn('/tmp/tuikit_iterm2_plist_backup.plist',error.exception.message)
            self.assertIn('defaults import com.googlecode.iterm2',error.exception.recovery_hint)


if __name__ == '__main__':
    unittest.main()
