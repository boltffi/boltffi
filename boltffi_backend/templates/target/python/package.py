from __future__ import annotations

{% if uses_dataclass_records || has_data_enums %}
from dataclasses import dataclass

{% endif %}
{% if !enums.is_empty() %}
from enum import IntEnum

{% endif %}
{% if uses_sequence_annotations || uses_callable_annotations %}
from collections.abc import {% if uses_callable_annotations %}Callable{% if uses_sequence_annotations %}, {% endif %}{% endif %}{% if uses_sequence_annotations %}Sequence{% endif %}

{% endif %}
{% if uses_async_helpers %}
import asyncio

{% endif %}
import sys
import uuid
from pathlib import Path

from . import _native


def _shared_library_filename() -> str:
    if sys.platform == "win32":
        return {{ windows_library }}
    if sys.platform == "darwin":
        return {{ macos_library }}
    return {{ unix_library }}


_native._initialize_loader(str(Path(__file__).resolve().with_name(_shared_library_filename())))

{% if uses_async_helpers %}
class _BoltFfiNativeFuture:
    __slots__ = (
        "_handle",
        "_poll",
        "_complete",
        "_cancel",
        "_free",
        "_panic_message",
        "_error_decoder",
    )

    def __init__(
        self,
        handle,
        poll,
        complete,
        cancel,
        free,
        panic_message,
        error_decoder=None,
    ) -> None:
        self._handle = handle
        self._poll = poll
        self._complete = complete
        self._cancel = cancel
        self._free = free
        self._panic_message = panic_message
        self._error_decoder = error_decoder

    def __del__(self) -> None:
        try:
            self.release()
        except Exception:
            pass

    async def wait(self):
        loop = asyncio.get_running_loop()
        handle = self._require_handle()
        try:
            while True:
                ready = loop.create_future()
                self._poll(handle, loop, ready)
                if await ready == 0:
                    break
        except BaseException:
            self.cancel()
            raise
        try:
            return self._complete(handle)
        except RuntimeError as error:
            decoder = self._error_decoder
            if decoder is not None and error.args and isinstance(error.args[0], bytes):
                raise _boltffi_error_exception(decoder(error.args[0])) from error
            raise
        finally:
            self.release()

    def cancel(self) -> None:
        handle = self._handle
        if handle is not None:
            self._handle = None
            self._cancel(handle)
            self._free(handle)

    def release(self) -> None:
        handle = self._handle
        if handle is not None:
            self._handle = None
            self._free(handle)

    def _require_handle(self):
        handle = self._handle
        if handle is None:
            raise RuntimeError("native future is closed")
        return handle

{% endif %}
{% if uses_wire_helpers %}
def _boltffi_u32(value: int) -> bytes:
    return int(value).to_bytes(4, "little", signed=False)


def _boltffi_wire_bool(value: bool) -> bytes:
    return b"\x01" if value else b"\x00"


def _boltffi_wire_i8(value: int) -> bytes:
    return int(value).to_bytes(1, "little", signed=True)


def _boltffi_wire_u8(value: int) -> bytes:
    return int(value).to_bytes(1, "little", signed=False)


def _boltffi_wire_i16(value: int) -> bytes:
    return int(value).to_bytes(2, "little", signed=True)


def _boltffi_wire_u16(value: int) -> bytes:
    return int(value).to_bytes(2, "little", signed=False)


def _boltffi_wire_i32(value: int) -> bytes:
    return int(value).to_bytes(4, "little", signed=True)


def _boltffi_wire_u32(value: int) -> bytes:
    return int(value).to_bytes(4, "little", signed=False)


def _boltffi_wire_i64(value: int) -> bytes:
    return int(value).to_bytes(8, "little", signed=True)


def _boltffi_wire_u64(value: int) -> bytes:
    return int(value).to_bytes(8, "little", signed=False)


def _boltffi_wire_isize(value: int) -> bytes:
    return _boltffi_wire_i64(value)


def _boltffi_wire_usize(value: int) -> bytes:
    return _boltffi_wire_u64(value)


def _boltffi_wire_f32(value: float) -> bytes:
    import struct
    return struct.pack("<f", float(value))


def _boltffi_wire_f64(value: float) -> bytes:
    import struct
    return struct.pack("<d", float(value))


def _boltffi_wire_string(value: str) -> bytes:
    payload = value.encode("utf-8")
    return _boltffi_u32(len(payload)) + payload


def _boltffi_wire_bytes(value: bytes) -> bytes:
    payload = bytes(value)
    return _boltffi_u32(len(payload)) + payload


def _boltffi_split_duration(value: float) -> tuple[int, int]:
    total = float(value)
    if total < 0:
        raise ValueError("duration must be non-negative")
    seconds = int(total)
    nanos = round((total - seconds) * 1_000_000_000)
    if nanos == 1_000_000_000:
        return seconds + 1, 0
    return seconds, nanos


def _boltffi_split_system_time(value: float) -> tuple[int, int]:
    total = float(value)
    seconds = int(total // 1)
    nanos = round((total - seconds) * 1_000_000_000)
    if nanos == 1_000_000_000:
        return seconds + 1, 0
    return seconds, nanos


def _boltffi_wire_duration(value: float) -> bytes:
    seconds, nanos = _boltffi_split_duration(value)
    return seconds.to_bytes(8, "little", signed=False) + nanos.to_bytes(4, "little", signed=False)


def _boltffi_wire_system_time(value: float) -> bytes:
    seconds, nanos = _boltffi_split_system_time(value)
    return seconds.to_bytes(8, "little", signed=True) + nanos.to_bytes(4, "little", signed=False)


def _boltffi_wire_uuid(value: uuid.UUID | str) -> bytes:
    raw = uuid.UUID(str(value)).bytes
    high = int.from_bytes(raw[:8], "big")
    low = int.from_bytes(raw[8:], "big")
    return high.to_bytes(8, "little", signed=False) + low.to_bytes(8, "little", signed=False)


def _boltffi_wire_url(value: str) -> bytes:
    return _boltffi_wire_string(str(value))


def _boltffi_wire_optional(value, encode) -> bytes:
    if value is None:
        return b"\x00"
    return b"\x01" + encode(value)


def _boltffi_wire_result(value, encode_ok, encode_err) -> bytes:
    ok, payload = value
    if ok:
        return b"\x00" + encode_ok(payload)
    return b"\x01" + encode_err(payload)


def _boltffi_wire_sequence(value, count, encode) -> bytes:
    items = list(value)
    if len(items) != count:
        raise ValueError("invalid BoltFFI sequence count")
    return _boltffi_u32(count) + b"".join(encode(item) for item in items)


def _boltffi_wire_map(value, encode_key, encode_value) -> bytes:
    items = list(value.items())
    return _boltffi_u32(len(items)) + b"".join(
        encode_key(key) + encode_value(item) for key, item in items
    )


def _boltffi_enum_value(value, enum_type, enum_name: str) -> int:
    if not isinstance(value, enum_type):
        raise TypeError(f"expected {enum_name}")
    return int(value)


def _boltffi_error_exception(error):
    for error_type in type(error).__mro__:
        exception_type = globals().get(f"{error_type.__name__}Exception")
        if exception_type is not None:
            return exception_type(error)
    return RuntimeError(error)


def _boltffi_call(error_decoder, call):
    try:
        return call()
    except RuntimeError as error:
        if error.args and isinstance(error.args[0], bytes):
            raise _boltffi_error_exception(error_decoder(error.args[0])) from error
        raise


class _BoltFfiWireReader:
    __slots__ = ("_data", "_offset")

    def __init__(self, data: bytes) -> None:
        self._data = memoryview(data)
        self._offset = 0

    def finish(self) -> None:
        if self._offset != len(self._data):
            raise ValueError("trailing BoltFFI wire bytes")

    def read(self, count: int) -> bytes:
        end = self._offset + count
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        value = self._data[self._offset:end].tobytes()
        self._offset = end
        return value

    def bool(self) -> bool:
        value = self.read(1)[0]
        if value > 1:
            raise ValueError("invalid BoltFFI bool")
        return value == 1

    def i8(self) -> int:
        return int.from_bytes(self.read(1), "little", signed=True)

    def u8(self) -> int:
        return int.from_bytes(self.read(1), "little", signed=False)

    def i16(self) -> int:
        return int.from_bytes(self.read(2), "little", signed=True)

    def u16(self) -> int:
        return int.from_bytes(self.read(2), "little", signed=False)

    def i32(self) -> int:
        return int.from_bytes(self.read(4), "little", signed=True)

    def u32(self) -> int:
        return int.from_bytes(self.read(4), "little", signed=False)

    def i64(self) -> int:
        return int.from_bytes(self.read(8), "little", signed=True)

    def u64(self) -> int:
        return int.from_bytes(self.read(8), "little", signed=False)

    def isize(self) -> int:
        return self.i64()

    def usize(self) -> int:
        return self.u64()

    def f32(self) -> float:
        import struct
        return struct.unpack("<f", self.read(4))[0]

    def f64(self) -> float:
        import struct
        return struct.unpack("<d", self.read(8))[0]

    def string(self) -> str:
        return self.read(self.u32()).decode("utf-8")

    def bytes(self) -> bytes:
        return self.read(self.u32())

    def duration(self) -> float:
        return self.u64() + self.u32() / 1_000_000_000

    def system_time(self) -> float:
        return self.i64() + self.u32() / 1_000_000_000

    def uuid(self) -> uuid.UUID:
        high = self.u64().to_bytes(8, "big", signed=False)
        low = self.u64().to_bytes(8, "big", signed=False)
        return uuid.UUID(bytes=high + low)

    def url(self) -> str:
        return self.string()

    def optional(self, decode):
        tag = self.read(1)[0]
        if tag == 0:
            return None
        if tag == 1:
            return decode()
        raise ValueError("invalid BoltFFI option tag")

    def result(self, decode_ok, decode_err):
        tag = self.read(1)[0]
        if tag == 0:
            return (True, decode_ok())
        if tag == 1:
            return (False, decode_err())
        raise ValueError("invalid BoltFFI result tag")

    def sequence(self, decode) -> list:
        return [decode() for _ in range(self.u32())]

    def map(self, decode_key, decode_value) -> dict:
        return {decode_key(): decode_value() for _ in range(self.u32())}


def _boltffi_read_wire(data: bytes, decode):
    reader = _BoltFfiWireReader(data)
    value = decode(reader)
    reader.finish()
    return value

{% endif %}
{% for decoder in codec_decoders %}
def {{ decoder.name() }}(data: bytes):
    return _boltffi_read_wire(data, lambda reader: {{ decoder.expression() }})


_native._register_wire_codec({{ decoder.key() }}, {{ decoder.name() }})

{% endfor %}
{% for encoder in codec_encoders %}
def {{ encoder.name() }}({{ encoder.argument() }}) -> bytes:
    return {{ encoder.expression() }}


_native._register_wire_codec({{ encoder.key() }}, {{ encoder.name() }})

{% endfor %}
{% for record in records %}
{%- if let Some(native_opaque) = record.native_opaque %}
class {{ record.class_name }}:
    __slots__ = ('_handle',)

    def __init__(self) -> None:
        raise TypeError("{{ record.class_name }} cannot be constructed directly; use the exported factory function")

    def close(self) -> None:
        """Release the native handle. Idempotent; safe to call multiple times."""
        handle = getattr(self, '_handle', None)
        if handle is not None:
            # Clear before calling native code: drop glue can re-enter Python
            # (and can raise), so every nested close must observe this object
            # as already closed.
            object.__setattr__(self, '_handle', None)
            _native.{{ native_opaque.drop_fn() }}(handle)

    def __del__(self) -> None:
        self.close()

    def _handle_or_raise(self):
        handle = getattr(self, '_handle', None)
        if handle is None:
            raise RuntimeError("{{ record.class_name }} has already been closed or released")
        return handle

    @classmethod
    def _from_handle(cls, handle) -> "{{ record.class_name }}":
        if not handle:
            raise TypeError("{{ record.class_name }}._from_handle() received a null or zero handle")
        obj = cls.__new__(cls)
        object.__setattr__(obj, '_handle', handle)
        return obj
{%- for prop in native_opaque.props() %}

    @property
    def {{ prop.name() }}(self) -> {{ prop.annotation() }}:
{%- if prop.optional() %}
        if _native.{{ prop.has_fn().unwrap() }}(self._handle_or_raise()) == 0:
            return None
{%- endif %}
{%- if prop.is_primitive() %}
        return _native.{{ prop.get_fn().unwrap() }}(self._handle_or_raise())
{%- else if prop.is_string() %}
        raw = _native.{{ prop.python_borrow_fn().unwrap() }}(self._handle_or_raise())
        if raw is None:
            raise RuntimeError("native opaque borrow failed for {{ prop.name() }}")
        return raw.decode('utf-8')
{%- else %}
        raw = _native.{{ prop.python_borrow_fn().unwrap() }}(self._handle_or_raise())
        if raw is None:
            raise RuntimeError("native opaque borrow failed for {{ prop.name() }}")
        return raw
{%- endif %}
{%- endfor %}

{%- else %}
@dataclass(frozen=True, slots=True)
class {{ record.class_name }}:
{%- for field in record.fields %}
    {{ field.name }}: {{ field.annotation }}{% if let Some(default) = field.default %} = {{ default }}{% endif %}
{%- endfor %}
{%- if let Some(wire) = record.wire %}

    def _boltffi_wire(self) -> bytes:
        return b"".join((
{%- for field in wire.fields %}
            {{ field.encode }},
{%- endfor %}
        ))

    @classmethod
    def _boltffi_from_wire(cls, data: bytes) -> "{{ record.class_name }}":
        reader = _BoltFfiWireReader(data)
        value = cls._boltffi_from_reader(reader)
        reader.finish()
        return value

    @classmethod
    def _boltffi_from_reader(cls, reader: "_BoltFfiWireReader") -> "{{ record.class_name }}":
        return cls(
{%- for field in wire.fields %}
            {{ field.name }}={{ field.decode }},
{%- endfor %}
        )
{%- endif %}
{%- for constructor in record.constructors %}

    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ record.class_name }}":
{%- for line in constructor.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in record.static_methods %}

    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in record.instance_methods %}

    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}


{%- endif %}
_native.{{ record.register_method }}({{ record.class_name }})
{% if let Some(exception_name) = record.exception_name %}

class {{ exception_name }}(RuntimeError):
    __slots__ = ("error",)

    def __init__(self, error: {{ record.class_name }}) -> None:
        self.error = error
        super().__init__(error)
{% endif %}

{% endfor %}
{% for enumeration in enums %}
{%- if let Some(wire) = enumeration.wire %}
class {{ enumeration.class_name }}:
    __slots__ = ()

    @classmethod
    def _boltffi_from_wire(cls, data: bytes) -> "{{ enumeration.class_name }}":
        reader = _BoltFfiWireReader(data)
        value = cls._boltffi_from_reader(reader)
        reader.finish()
        return value

    @classmethod
    def _boltffi_from_reader(cls, reader: "_BoltFfiWireReader") -> "{{ enumeration.class_name }}":
        tag = reader.u32()
{%- for variant in wire.variants %}
        if tag == {{ variant.tag }}:
            return {{ variant.class_name }}._boltffi_from_reader_payload(reader)
{%- endfor %}
        raise ValueError("invalid {{ enumeration.class_name }} tag")
{%- for constructor in enumeration.constructors %}

    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ enumeration.class_name }}":
{%- for line in constructor.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.static_methods %}

    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.instance_methods %}

    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}

{% for variant in wire.variants %}
@dataclass(frozen=True, slots=True)
class {{ variant.class_name }}({{ enumeration.class_name }}):
{%- if variant.has_fields() %}
{%- for field in variant.fields %}
    {{ field.name }}: {{ field.annotation }}{% if let Some(default) = field.default %} = {{ default }}{% endif %}
{%- endfor %}
{%- else %}
    pass
{%- endif %}

    def _boltffi_wire(self) -> bytes:
{%- if variant.has_fields() %}
        return _boltffi_wire_u32({{ variant.tag }}) + b"".join((
{%- for field in variant.wire_fields %}
            {{ field.encode }},
{%- endfor %}
        ))
{%- else %}
        return _boltffi_wire_u32({{ variant.tag }})
{%- endif %}

    @classmethod
    def _boltffi_from_reader_payload(cls, reader: "_BoltFfiWireReader") -> "{{ variant.class_name }}":
{%- if variant.has_fields() %}
        return cls(
{%- for field in variant.wire_fields %}
            {{ field.name }}={{ field.decode }},
{%- endfor %}
        )
{%- else %}
        return cls()
{%- endif %}

{% endfor %}
{%- else %}
class {{ enumeration.class_name }}(IntEnum):
{%- for variant in enumeration.variants %}
    {{ variant.name }} = {{ variant.value }}
{%- endfor %}
{%- for constructor in enumeration.constructors %}

    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ enumeration.class_name }}":
{%- for line in constructor.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.static_methods %}

    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.instance_methods %}

    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}

{%- endif %}

_native.{{ enumeration.register_method }}({{ enumeration.class_name }})
{% if let Some(exception_name) = enumeration.exception_name %}

class {{ exception_name }}(RuntimeError):
    __slots__ = ("error",)

    def __init__(self, error: {{ enumeration.class_name }}) -> None:
        self.error = error
        super().__init__(error)
{% endif %}

{% endfor %}
{% for class in classes %}
class {{ class.class_name }}:
    __slots__ = ("_handle",)

{% if !class.init.is_empty() %}
{% for init in class.init %}
    def __init__(self{% for parameter in init.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> None:
        self._handle = _native.{{ init.native_name }}({{ init.arguments }})
{% endfor %}
{% else %}
    def __init__(self) -> None:
        raise TypeError("{{ class.class_name }} cannot be constructed directly")
{% endif %}

    @classmethod
    def _from_handle(cls, handle: int) -> "{{ class.class_name }}":
        value = cls.__new__(cls)
        value._handle = handle
        return value

    def __del__(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            self._handle = None
            _native.{{ class.release_method }}(handle)
{%- for constructor in class.constructors %}

    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ class.class_name }}":
{%- for line in constructor.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in class.static_methods %}

    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in class.instance_methods %}

    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for stream in class.streams %}

    def {{ stream.python_name }}(self) -> "{{ stream.subscription_class }}":
        return {{ stream.subscription_class }}._from_handle(_native.{{ stream.subscribe_method }}(self._handle))
{%- endfor %}

{% for stream in class.streams %}
class {{ stream.subscription_class }}:
    __slots__ = ("_handle",)

    def __init__(self) -> None:
        raise TypeError("{{ stream.subscription_class }} cannot be constructed directly")

    @classmethod
    def _from_handle(cls, handle: int) -> "{{ stream.subscription_class }}":
        value = cls.__new__(cls)
        value._handle = handle
        return value

    def __del__(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            self._handle = None
            _native.{{ stream.free_method }}(handle)

    def pop_batch(self, max_count: int = 16) -> list[{{ stream.item_annotation }}]:
{%- for line in stream.pop_batch_body %}
        {{ line }}
{%- endfor %}

    def wait(self, timeout_milliseconds: int) -> int:
        return _native.{{ stream.wait_method }}(self._require_handle(), timeout_milliseconds)

    def unsubscribe(self) -> None:
        handle = self._require_handle()
        self._handle = None
        _native.{{ stream.unsubscribe_method }}(handle)
        _native.{{ stream.free_method }}(handle)

    def _require_handle(self) -> int:
        handle = self._handle
        if handle is None:
            raise RuntimeError("stream subscription is closed")
        return handle

{% endfor %}
{% endfor %}
{% for constant in constants %}
{{ constant.python_name }}: {{ constant.annotation }} = {{ constant.expression }}
{% endfor %}
{% for function in functions %}
{% if function.asynchronous %}async {% endif %}def {{ function.python_name }}({% for parameter in function.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ function.return_annotation }}:
{%- for line in function.body %}
    {{ line }}
{%- endfor %}

{%- endfor %}

MODULE_NAME = {{ module_name_literal }}
PACKAGE_NAME = {{ package_name_literal }}
PACKAGE_VERSION = {{ package_version }}

__all__ = [
    "MODULE_NAME",
    "PACKAGE_NAME",
    "PACKAGE_VERSION",
{%- for record in records %}
    "{{ record.class_name }}",
{%- if let Some(exception_name) = record.exception_name %}
    "{{ exception_name }}",
{%- endif %}
{%- endfor %}
{%- for enumeration in enums %}
    "{{ enumeration.class_name }}",
{%- if let Some(exception_name) = enumeration.exception_name %}
    "{{ exception_name }}",
{%- endif %}
{%- if let Some(wire) = enumeration.wire %}
{%- for variant in wire.variants %}
    "{{ variant.class_name }}",
{%- endfor %}
{%- endif %}
{%- endfor %}
{%- for class in classes %}
    "{{ class.class_name }}",
{%- for stream in class.streams %}
    "{{ stream.subscription_class }}",
{%- endfor %}
{%- endfor %}
{%- for constant in constants %}
    "{{ constant.python_name }}",
{%- endfor %}
{%- for function in functions %}
    "{{ function.python_name }}",
{%- endfor %}
]
