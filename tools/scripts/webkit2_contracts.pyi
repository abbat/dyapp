"""Consumed WebKit2 4.1 GTK3 API; upstream stubs cover WebKit6 GTK4."""

from collections.abc import Callable
from typing import Protocol

from gi.repository import Gio, GLib, GObject, Gtk

class JavaScriptValue(Protocol):
    def to_string(self) -> str: ...

class SnapshotSurface(Protocol):
    def write_to_png(self, filename: str) -> None: ...

class LoadEvent(GObject.GEnum):
    STARTED = 0
    REDIRECTED = 1
    COMMITTED = 2
    FINISHED = 3

class SnapshotOptions(GObject.GFlags):
    NONE = 0
    INCLUDE_SELECTION_HIGHLIGHTING = 1
    TRANSPARENT_BACKGROUND = 2

class SnapshotRegion(GObject.GEnum):
    VISIBLE = 0
    FULL_DOCUMENT = 1

class WebView(Gtk.Container):
    def load_uri(self, uri: str) -> None: ...
    def call_async_javascript_function(
        self,
        body: str,
        length: int,
        arguments: GLib.Variant | None = None,
        world_name: str | None = None,
        source_uri: str | None = None,
        cancellable: Gio.Cancellable | None = None,
        callback: Callable[[WebView, Gio.AsyncResult], None] | None = None,
    ) -> None: ...
    def call_async_javascript_function_finish(
        self, result: Gio.AsyncResult
    ) -> JavaScriptValue: ...
    def get_snapshot(
        self,
        region: SnapshotRegion,
        options: SnapshotOptions,
        cancellable: Gio.Cancellable | None = None,
        callback: Callable[[WebView, Gio.AsyncResult], None] | None = None,
    ) -> None: ...
    def get_snapshot_finish(
        self, result: Gio.AsyncResult
    ) -> SnapshotSurface: ...
