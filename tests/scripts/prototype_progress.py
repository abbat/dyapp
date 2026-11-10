"""Failure-only deadlines for callback-driven prototype transitions."""

from collections.abc import Callable


class Progress:
    def __init__(
        self,
        schedule: Callable[[Callable[[], bool]], int],
        cancel: Callable[[int], bool],
        failed: Callable[[str], None],
    ) -> None:
        self.schedule, self.cancel, self.failed = schedule, cancel, failed
        self.timer = 0
        self.generation = 0

    def begin(self, phase: str) -> None:
        self.close()
        generation = self.generation

        def expired() -> bool:
            if generation == self.generation:
                self.timer = 0
                self.generation += 1
                self.failed(phase)
            return False

        self.timer = self.schedule(expired)

    def close(self) -> None:
        self.generation += 1
        if self.timer:
            self.cancel(self.timer)
            self.timer = 0
