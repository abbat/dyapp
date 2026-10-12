"""Bounded in-memory copy of process output; nothing touches the filesystem.

A read-only filesystem must never fail a run on logging, so captured output
lives in memory: the first HEAD and the last TAIL bytes, with a marker for
what was dropped between them. Only an explicit log file gets a full copy.
"""
import os
import threading

HEAD = 64 * 1024
TAIL = 256 * 1024


class Capture:
    def __init__(self, head=HEAD, tail=TAIL, log_file=None):
        self.head_limit, self.tail_limit = head, tail
        self.head, self.tail = bytearray(), bytearray()
        self.total = 0
        self.lock = threading.Lock()
        self.copy = None if log_file is None else open(log_file, "ab")

    def write(self, data):
        with self.lock:
            self.total += len(data)
            room = self.head_limit - len(self.head)
            self.head += data[:room]
            self.tail += data[max(room, 0):]
            if len(self.tail) > self.tail_limit:
                del self.tail[:len(self.tail) - self.tail_limit]
            if self.copy is not None:
                self.copy.write(data)

    def dropped(self):
        return self.total - len(self.head) - len(self.tail)

    def getvalue(self):
        with self.lock:
            dropped = self.dropped()
            marker = (f"\n[... {dropped} bytes truncated ...]\n".encode()
                      if dropped else b"")
            return bytes(self.head) + marker + bytes(self.tail)

    def since(self, offset):
        """Retained output written after `offset` bytes."""
        with self.lock:
            if not self.dropped():
                return bytes(self.head + self.tail)[offset:]
            start = len(self.tail) - (self.total - offset)
            return bytes(self.tail[max(0, start):])

    def pipe(self):
        """A write descriptor for a child; a thread copies it here until
        every writer has closed it. Close the descriptor after spawning."""
        read, write = os.pipe()

        def drain():
            with os.fdopen(read, "rb", buffering=0) as stream:
                while chunk := stream.read(65536):
                    self.write(chunk)

        thread = threading.Thread(target=drain, daemon=True)
        thread.start()
        return write, thread

    def close(self):
        if self.copy is not None:
            self.copy.close()
