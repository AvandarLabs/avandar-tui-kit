"""Real-pipe regressions for ownership and failure isolation in the lab bridge."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from driver.cli_args import DriverRuntimeError
from driver.mouse_scenarios import PersistentLabVnc
from driver.tart_vm import VncEndpoint


FAULT_API = '''
import json, os, pathlib
fault = os.environ['BRIDGE_FAULT']
state = pathlib.Path(os.environ['BRIDGE_EVENTS'])
def record(action, value=None):
    with state.open('a') as output: output.write(json.dumps([action, value])+'\\n')
    if action == fault: raise RuntimeError('injected '+action+' after delivery')
class Client:
    def keyDown(self, key): record('key_down', key)
    def keyUp(self, key): record('key_up', key)
    def mouseDown(self, button): record('mouse_down', button)
    def mouseUp(self, button): record('mouse_up', button)
    def disconnect(self): record('disconnect')
def connect(*args, **kwargs): return Client()
def shutdown(): record('shutdown')
'''


class BridgeLifecycleTests(unittest.TestCase):
    def bridge_fixture(self, root, fault):
        package = root/'vncdotool'
        package.mkdir()
        (package/'__init__.py').write_text('')
        (package/'api.py').write_text(FAULT_API)
        launcher = root/'vncdo'
        launcher.write_text('#!'+sys.executable+'\n')
        events = root/'events.jsonl'
        original = mock.Mock(vncdo_bin=str(launcher), endpoint=VncEndpoint('localhost', 5900, 'test'),
                             env={**os.environ, 'PYTHONPATH':str(root), 'BRIDGE_FAULT':fault,
                                  'BRIDGE_EVENTS':str(events)})
        return PersistentLabVnc(original), events

    def test_down_error_after_delivery_still_releases_owned_input(self):
        # Moving ownership after the down call must omit the required release.
        for command, fault, argument, release in [
            ('keydown', 'key_down', 'meta', ['key_up', 'meta']),
            ('mousedown', 'mouse_down', '1', ['mouse_up', 1]),
            ('click', 'mouse_down', '1', ['mouse_up', 1]),
        ]:
            with self.subTest(command=command), tempfile.TemporaryDirectory() as directory:
                bridge, events = self.bridge_fixture(Path(directory), fault)
                with self.assertRaisesRegex(DriverRuntimeError, 'after delivery'):
                    with bridge:
                        bridge.run_tokens([command, argument])
                actions = [json.loads(line) for line in events.read_text().splitlines()]
                self.assertIn(release, actions)
                self.assertEqual(actions[-2:], [['disconnect', None], ['shutdown', None]])
                self.assertEqual(bridge.process.returncode, 0)

    def test_cleanup_faults_do_not_skip_independent_cleanup_or_hide_child_failure(self):
        # Bare sequential cleanup skips later releases, disconnect, or shutdown.
        for fault in ['key_up', 'mouse_up', 'disconnect', 'shutdown']:
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                bridge, events = self.bridge_fixture(Path(directory), fault)
                with tempfile.TemporaryFile(mode='w+t') as diagnostics:
                    start_process = subprocess.Popen
                    def start_with_diagnostics(*args, **kwargs):
                        return start_process(*args, **kwargs, stderr=diagnostics)
                    with mock.patch('driver.mouse_scenarios.subprocess.Popen', side_effect=start_with_diagnostics):
                        with self.assertRaisesRegex(DriverRuntimeError, 'Persistent VNC cleanup'):
                            with bridge:
                                bridge.run_tokens(['keydown','meta','keydown','alt','mousedown','1'])
                    diagnostics.seek(0)
                    self.assertIn('injected '+fault, diagnostics.read())
                actions = [json.loads(line) for line in events.read_text().splitlines()]
                self.assertCountEqual(actions, [
                    ['key_down','meta'], ['key_down','alt'], ['mouse_down',1],
                    ['key_up','meta'], ['key_up','alt'], ['mouse_up',1],
                    ['disconnect',None], ['shutdown',None],
                ])
                self.assertNotEqual(bridge.process.returncode, 0)
                self.assertTrue(bridge.process.stdin.closed)
                self.assertTrue(bridge.process.stdout.closed)

    def test_cleanup_failure_retains_original_scenario_error(self):
        with tempfile.TemporaryDirectory() as directory:
            bridge, events = self.bridge_fixture(Path(directory), 'shutdown')
            original_error = ValueError('original scenario failure')
            with tempfile.TemporaryFile(mode='w+t') as diagnostics:
                start_process = subprocess.Popen
                with mock.patch('driver.mouse_scenarios.subprocess.Popen',
                                side_effect=lambda *a, **k:start_process(*a, **k, stderr=diagnostics)):
                    with self.assertRaises(DriverRuntimeError) as caught:
                        with bridge:
                            raise original_error
            self.assertIs(caught.exception.__cause__, original_error)
            self.assertIn('original scenario failure', str(caught.exception))
            self.assertNotEqual(bridge.process.returncode, 0)

    def test_broken_stdin_close_still_waits_and_closes_stdout(self):
        # Real buffered bytes survive failed flush and fail again on close.
        process = subprocess.Popen([sys.executable, '-c', 'pass'], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, text=True)
        bridge = PersistentLabVnc(None)
        bridge.process = process
        process.wait(timeout=5)
        try:
            with self.assertRaises(BrokenPipeError) as original:
                bridge.run_tokens(['keydown', 'meta'])
            with mock.patch.object(process, 'wait', wraps=process.wait) as wait:
                with self.assertRaises(DriverRuntimeError) as caught:
                    bridge.__exit__(BrokenPipeError, original.exception, original.exception.__traceback__)
                self.assertTrue(wait.called)
            self.assertIs(caught.exception.__cause__, original.exception)
            self.assertTrue(process.stdin.closed)
            self.assertTrue(process.stdout.closed)
        finally:
            try: process.stdin.close()
            except BrokenPipeError: pass
            process.stdout.close()

    def test_startup_flush_error_preserves_cause_and_closes_child_streams(self):
        with tempfile.TemporaryDirectory() as directory:
            bridge, _events = self.bridge_fixture(Path(directory), '')
            start_process = subprocess.Popen
            def start_exited_child(*_args, **_kwargs):
                process = start_process([sys.executable, '-c', 'pass'], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True)
                process.wait(timeout=5)
                return process
            try:
                with mock.patch('driver.mouse_scenarios.subprocess.Popen', side_effect=start_exited_child):
                    with self.assertRaises(DriverRuntimeError) as caught:
                        with bridge: self.fail('A dead child cannot enter a VNC session.')
                self.assertIsInstance(caught.exception.__cause__, BrokenPipeError)
                self.assertEqual(bridge.process.returncode, 0)
                self.assertTrue(bridge.process.stdin.closed)
                self.assertTrue(bridge.process.stdout.closed)
            finally:
                try: bridge.process.stdin.close()
                except BrokenPipeError: pass
                bridge.process.stdout.close()

    @unittest.skipUnless(hasattr(signal, 'pause'), 'POSIX signal wait required for stubborn child fixture')
    def test_broken_stdin_close_cannot_skip_bounded_kill_and_reap(self):
        process = subprocess.Popen([sys.executable, '-u', '-c',
                                   'import os,signal; os.close(0); print("ready",flush=True); signal.pause()'],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        bridge = PersistentLabVnc(None)
        bridge.process = process
        try:
            self.assertEqual(process.stdout.readline(), 'ready\n')
            with self.assertRaises(BrokenPipeError) as original:
                bridge.run_tokens(['keydown', 'meta'])
            real_wait = process.wait
            wait_count = 0
            def bounded_wait(timeout):
                nonlocal wait_count
                wait_count += 1
                return real_wait(timeout=.05 if wait_count == 1 else timeout)
            with mock.patch.object(process, 'wait', side_effect=bounded_wait):
                with self.assertRaises(DriverRuntimeError) as caught:
                    bridge.__exit__(BrokenPipeError, original.exception, original.exception.__traceback__)
            self.assertIs(caught.exception.__cause__, original.exception)
            self.assertEqual(process.returncode, -signal.SIGKILL)
            self.assertTrue(process.stdout.closed)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            try: process.stdin.close()
            except BrokenPipeError: pass
            process.stdout.close()
