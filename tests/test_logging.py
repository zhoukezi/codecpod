"""Configuration of FFmpeg's internal logging via ``set_log``.

The logger is process-global, so every test restores the default level on exit to
avoid leaking a custom callback into unrelated tests.
"""

import threading

import pytest
from conftest import DEFAULT_RATE, tone

import codecpod


@pytest.fixture(autouse=True)
def restore_logging():
    """Reset the global logger back to FFmpeg's default after each test."""
    yield
    codecpod.set_log("info")


def _decode_roundtrip():
    """Encode a short tone to MP3 in memory and decode it, exercising the demuxer
    path that emits FFmpeg log messages."""
    sig = tone(dur=0.2)
    data = codecpod.save_bytes(sig, DEFAULT_RATE, codec=codecpod.Mp3())
    codecpod.load(data)


def test_set_level_accepts_all_names():
    for name in (
        "quiet",
        "panic",
        "fatal",
        "error",
        "warning",
        "info",
        "verbose",
        "debug",
        "trace",
    ):
        codecpod.set_log(name)


def test_set_level_rejects_unknown_name():
    with pytest.raises(ValueError):
        codecpod.set_log("loud")


def test_set_log_rejects_non_callable_non_str():
    with pytest.raises(TypeError):
        codecpod.set_log(42)


def test_callback_receives_messages():
    messages = []
    codecpod.set_log(lambda level, text: messages.append((level, text)))
    _decode_roundtrip()

    assert messages, "callback received no messages"
    # Every entry is a (level string, text) pair.
    levels = {
        "quiet",
        "panic",
        "fatal",
        "error",
        "warning",
        "info",
        "verbose",
        "debug",
        "trace",
    }
    for level, text in messages:
        assert level in levels
        assert isinstance(text, str)
        assert not text.endswith("\n")


def test_callback_replaces_previous_callback():
    first = []
    second = []
    codecpod.set_log(lambda level, text: first.append(text))
    codecpod.set_log(lambda level, text: second.append(text))
    _decode_roundtrip()

    assert second, "second callback should receive messages"
    assert not first, "first callback should have been replaced"


def test_level_after_callback_restores_stderr(capfd):
    # Install a callback, then switch back to a level; the callback must stop firing.
    captured = []
    codecpod.set_log(lambda level, text: captured.append(text))
    codecpod.set_log("quiet")
    captured.clear()
    _decode_roundtrip()

    assert not captured, "callback kept firing after switching back to a level"
    err = capfd.readouterr().err
    assert err == "", f"quiet level should silence stderr, got: {err!r}"


def test_level_after_callback_restores_stderr_output(capfd):
    # Switching from a callback back to a verbose level must reinstate FFmpeg's default
    # stderr handler, not merely stop the callback. "trace" keeps every message, so any
    # log produced by the roundtrip has to reappear on stderr.
    codecpod.set_log(lambda level, text: None)
    codecpod.set_log("trace")
    _decode_roundtrip()

    err = capfd.readouterr().err
    assert err != "", "switching back to a level should restore FFmpeg's stderr output"


# A raising callback is reported through Python's unraisable hook rather than propagating
# into FFmpeg; pytest surfaces that as a PytestUnraisableExceptionWarning, which is expected.
@pytest.mark.filterwarnings("ignore::pytest.PytestUnraisableExceptionWarning")
def test_callback_exception_does_not_crash():
    def boom(level, text):
        raise RuntimeError("callback failure")

    codecpod.set_log(boom)
    # A raising callback must not propagate into FFmpeg; the decode still succeeds.
    _decode_roundtrip()


def test_callback_invoked_from_worker_thread():
    # FFmpeg logs on whatever thread emits the message, so driving a decode from a
    # background thread exercises the callback's cross-thread GIL acquisition.
    seen = []
    codecpod.set_log(lambda level, text: seen.append(threading.get_ident()))

    worker = threading.Thread(target=_decode_roundtrip)
    worker.start()
    worker.join()

    assert seen, "callback received no messages"
    main_thread = threading.get_ident()
    assert all(ident != main_thread for ident in seen), (
        "callback should have fired on the worker thread, not the main thread"
    )
