"""Labelled ordinary HTTP/socket fault controls, never installed native proof."""
import copy
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import stat
import sys
import threading
import unittest
from unittest.mock import patch

from acceptance.suite.executor_debug_http import capture_http, close_http, listener_record, trigger_http, validate_http
from acceptance.suite.fsm import Scratch
from acceptance.suite.mcp import HttpClient, PROTOCOL_VERSION


class DebugHttpTests(unittest.TestCase):
    def observation(self):
        executable = dict(device=1,inode=10,uid=1000,mode=stat.S_IFREG|0o755)
        tcp = '  sl local_address rem_address st tx_queue rx_queue tr tm retr uid timeout inode\n'
        tcp += '  0: 0100007F:3039 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 12345\n'
        descriptors = {'4':'socket:[12345]'}
        return dict(original={'pid':123,'pid_starttime':'456'},binary_sha256='a'*64,
            initialization={'protocolVersion':PROTOCOL_VERSION},session='labelled-session',
            host='127.0.0.1',port=12345,path='/mcp',executable=executable,
            expected_executable=copy.deepcopy(executable),tcp=tcp,descriptors=descriptors,
            listener=listener_record(tcp,12345,descriptors),
            trigger_endpoints=dict(local=['127.0.0.1',23456],peer=['127.0.0.1',12345]),
            trigger=dict(jsonrpc='2.0',id=2,method='tools/call',params=dict(name='instance_send',
                arguments=dict(instance_id='labelled',event={'name':'start'},request_id='labelled-trigger'))))

    def test_foreign_or_uninitialized_hosts_cannot_prove_original_http(self):
        original=self.observation()
        self.assertEqual(validate_http(original,original['original'],'a'*64),original)
        for change in (dict(original={'pid':124,'pid_starttime':'456'}),dict(binary_sha256='b'*64),
            dict(initialization={}),dict(session=None),dict(session=''),dict(port=True),dict(host='0.0.0.0'),
            dict(path='/foreign'),dict(executable={}),dict(trigger_endpoints={'local':['127.0.0.1',True]})):
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_http({**original,**change},original['original'],'a'*64)

    def test_unowned_foreign_or_duplicate_sockets_cannot_prove_the_listener(self):
        original=self.observation()
        for change in (dict(descriptors={'4':'socket:[54321]'}),dict(descriptors={'4':'socket:[12345]','5':'socket:[12345]'}),
            dict(tcp=original['tcp'].replace('0100007F','00000000')),
            dict(tcp=original['tcp'].replace(' 0A ',' 01 ')),dict(tcp=original['tcp']+original['listener']['row']+'\n')):
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_http({**original,**change},original['original'],'a'*64)
        for change in (dict(inode=True),dict(fd=False),dict(inode=54321),dict(local='0100007F:3038')):
            value=copy.deepcopy(original);value['listener'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_http(value,original['original'],'a'*64)

    def test_poll_or_duplicate_identity_cannot_replace_the_one_start_trigger(self):
        for change in (dict(id=True),dict(method='resources/read'),dict(params={'name':'instance_show'})):
            value=self.observation();value['trigger'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_http(value,value['original'],'a'*64)
        for change in (dict(event={'name':'retry'}),dict(instance_id=''),dict(request_id='')):
            value=self.observation();value['trigger']['params']['arguments'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_http(value,value['original'],'a'*64)

    def test_socket_and_descriptor_inventories_are_bounded(self):
        value=self.observation()
        for change in (dict(tcp='x'*1_048_577),dict(descriptors={str(i):'pipe:[1]' for i in range(257)}),
            dict(descriptors={'foreign':'socket:[12345]'})):
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_http({**value,**change},value['original'],'a'*64)

    def test_missing_session_refuses_before_sending_a_labelled_start_request(self):
        client=HttpClient('127.0.0.1',12345)
        report=type('Report',(),{'equal':lambda _self,a,b,_message:self.assertEqual(a,b)})()
        with patch.object(client,'initialize',return_value={'protocolVersion':PROTOCOL_VERSION}),\
            patch.object(client,'_connection') as connection:
            with self.assertRaises(ValueError):trigger_http(report,client,'labelled','labelled-trigger')
            connection.assert_not_called()

    def test_real_ordinary_socket_identity_and_one_send_do_not_wait_for_the_trigger_reply(self):
        received=[];entered=threading.Event();release=threading.Event()
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*_args):pass
            def do_POST(self):
                message=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                received.append(message)
                if message['method']=='tools/call':
                    entered.set();release.wait(5)
                body=json.dumps(dict(jsonrpc='2.0',id=message.get('id'),
                    result={'protocolVersion':PROTOCOL_VERSION})).encode()
                self.send_response(200);self.send_header('Mcp-Session-Id','labelled-session')
                self.send_header('Content-Length',str(len(body)));self.end_headers()
                self.wfile.write(body)
        with Scratch('labelled-http-cut-control') as scratch:
            server=HTTPServer(('127.0.0.1',0),Handler)
            worker=threading.Thread(target=server.serve_forever);worker.start()
            client=HttpClient('127.0.0.1',server.server_port)
            client.cut_directory=Path(scratch.path);client.cut_connection=None
            try:
                report=type('Report',(),{'equal':lambda _self,a,b,_message:self.assertEqual(a,b)})()
                trigger_http(report,client,'labelled','labelled-trigger')
                self.assertTrue(entered.wait(2));self.assertFalse(release.is_set())
                executable=Path(sys.executable).resolve()
                birth=Path('/proc/self/stat').read_bytes().rpartition(b') ')[2].split()[19].decode()
                ready=dict(original=dict(pid=os.getpid(),pid_starttime=birth),
                    binary_sha256=hashlib.sha256(executable.read_bytes()).hexdigest())
                value=capture_http(client,ready,executable)
                self.assertEqual(value['listener']['inode'],int(Path('/proc/self/fd',str(server.fileno())).readlink().name[8:-1]))
                self.assertEqual([row['method'] for row in received],['initialize','notifications/initialized','tools/call'])
                self.assertEqual(json.loads((Path(scratch.path)/'http.json').read_text()),value)
                value['descriptors'][str(value['listener']['fd'])]='socket:[1]'
                with self.assertRaises(ValueError):validate_http(value,ready['original'],ready['binary_sha256'])
            finally:
                release.set();server.shutdown();worker.join(timeout=3);close_http(client);server.server_close()
            self.assertFalse(worker.is_alive())


if __name__ == '__main__':unittest.main()
