"""Exact socket diagnostic and ordinary owned-stream controls, never native proof."""
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import unittest

from acceptance.suite.soak_socket_queues import decode_queue, observe_unix_queues, _queue


def reply(inode=123,sequence=1,port=321,state=1,queue=18,attribute_type=4):
    body=struct.pack('=BBBBIII',socket.AF_UNIX,socket.SOCK_STREAM,state,0,inode,7,9)
    body+=struct.pack('=HHII',12,attribute_type,queue,0)
    return struct.pack('=IHHII',16+len(body),20,0,sequence,port)+body


class SocketDiagnosticTests(unittest.TestCase):
    def test_exact_request_never_uses_dump_and_targets_only_the_owned_inode(self):
        class Observer:
            request=None
            def settimeout(self,value): self.timeout=value
            def sendto(self,raw,target): self.request=raw;self.target=target
            def recvmsg(self,maximum): return reply(),[],0,(0,0)
            def getsockname(self): return (321,0)
        observer=Observer();result=_queue(123,1,observer,0.5)
        self.assertEqual(result['queued_bytes'],18)
        self.assertEqual(struct.unpack('=IHHII',observer.request[:16]),(40,20,1,1,0))
        self.assertEqual(struct.unpack('=BBHIIIII',observer.request[16:]),
            (socket.AF_UNIX,0,0,0xffffffff,123,0x14,0xffffffff,0xffffffff))
        self.assertEqual(observer.target,(0,0));self.assertEqual(observer.timeout,0.5)

    def test_foreign_identity_truncated_duplicate_or_missing_queue_attribute_refuses(self):
        altered=[reply(inode=124),reply(sequence=2),reply(port=322),reply(attribute_type=3),reply()[:-1]]
        duplicate=reply()+reply()[32:]
        duplicate=struct.pack('=I',len(duplicate))+duplicate[4:];altered.append(duplicate)
        for raw in altered:
            with self.assertRaises(ValueError):decode_queue(raw,123,1,321)
        self.assertEqual(decode_queue(reply(queue=0),123,1,321)['queued_bytes'],0)

    def test_connection_counts_are_preserved_and_cannot_be_treated_as_capture_bytes(self):
        value=decode_queue(reply(state=10,queue=3),123,1,321)
        self.assertIsNone(value['queued_bytes']);self.assertEqual(value['queued_connections'],3)

    @unittest.skipUnless(sys.platform=='linux','actual owned Linux Unix capture sockets')
    def test_actual_owned_unix_queue_is_measured_without_consuming_data_or_retaining_endpoints(self):
        left,right=socket.socketpair()
        child=subprocess.Popen([sys.executable,'-c',
            'import sys; print("ready",flush=True); sys.stdin.read(1)'],pass_fds=(left.fileno(),),
            stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
        try:
            self.assertEqual(child.stdout.readline(),b'ready\n')
            right.sendall(b'owned capture queue')
            inode=os.fstat(left.fileno()).st_ino
            birth=Path('/proc',str(child.pid),'stat').read_bytes().rpartition(b') ')[2].split()[19].decode()
            result=observe_unix_queues(dict(pid=child.pid,pid_starttime=birth))
            self.assertEqual(result['queued_unix_bytes'],len(b'owned capture queue'))
            self.assertEqual([row['inode'] for row in result['endpoints']],[inode])
            self.assertEqual(left.recv(64),b'owned capture queue')
            empty=observe_unix_queues(dict(pid=child.pid,pid_starttime=birth))
            self.assertEqual(empty['queued_unix_bytes'],0)
            self.assertEqual(len(empty['endpoints']),1)
            child.stdin.write(b'Q');child.stdin.flush();child.wait(timeout=5)
        finally:
            if child.poll() is None:child.terminate();child.wait(timeout=5)
            child.stdin.close();child.stdout.close();left.close();right.close()


if __name__=='__main__':unittest.main()
