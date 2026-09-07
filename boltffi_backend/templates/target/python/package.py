from __future__ import annotations

{% if !records.is_empty() || has_data_enums %}
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
{% if uses_wire_helpers %}
import struct

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
_BOLTFFI_STRUCT_I8 = struct.Struct("<b")
_BOLTFFI_STRUCT_U8 = struct.Struct("<B")
_BOLTFFI_STRUCT_I16 = struct.Struct("<h")
_BOLTFFI_STRUCT_U16 = struct.Struct("<H")
_BOLTFFI_STRUCT_I32 = struct.Struct("<i")
_BOLTFFI_STRUCT_U32 = struct.Struct("<I")
_BOLTFFI_STRUCT_I64 = struct.Struct("<q")
_BOLTFFI_STRUCT_U64 = struct.Struct("<Q")
_BOLTFFI_STRUCT_F32 = struct.Struct("<f")
_BOLTFFI_STRUCT_F64 = struct.Struct("<d")

_BOLTFFI_UNPACK_I8 = _BOLTFFI_STRUCT_I8.unpack_from
_BOLTFFI_UNPACK_I16 = _BOLTFFI_STRUCT_I16.unpack_from
_BOLTFFI_UNPACK_U16 = _BOLTFFI_STRUCT_U16.unpack_from
_BOLTFFI_UNPACK_I32 = _BOLTFFI_STRUCT_I32.unpack_from
_BOLTFFI_UNPACK_U32 = _BOLTFFI_STRUCT_U32.unpack_from
_BOLTFFI_UNPACK_I64 = _BOLTFFI_STRUCT_I64.unpack_from
_BOLTFFI_UNPACK_U64 = _BOLTFFI_STRUCT_U64.unpack_from
_BOLTFFI_UNPACK_F32 = _BOLTFFI_STRUCT_F32.unpack_from
_BOLTFFI_UNPACK_F64 = _BOLTFFI_STRUCT_F64.unpack_from


def _boltffi_u32(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U32.pack(int(value))


def _boltffi_wire_bool(value: bool) -> bytes:
    return b"\x01" if value else b"\x00"


def _boltffi_wire_i8(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I8.pack(int(value))


def _boltffi_wire_u8(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U8.pack(int(value))


def _boltffi_wire_i16(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I16.pack(int(value))


def _boltffi_wire_u16(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U16.pack(int(value))


def _boltffi_wire_i32(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I32.pack(int(value))


def _boltffi_wire_u32(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U32.pack(int(value))


def _boltffi_wire_i64(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I64.pack(int(value))


def _boltffi_wire_u64(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U64.pack(int(value))


def _boltffi_wire_isize(value: int) -> bytes:
    return _BOLTFFI_STRUCT_I64.pack(int(value))


def _boltffi_wire_usize(value: int) -> bytes:
    return _BOLTFFI_STRUCT_U64.pack(int(value))


def _boltffi_wire_f32(value: float) -> bytes:
    return _BOLTFFI_STRUCT_F32.pack(float(value))


def _boltffi_wire_f64(value: float) -> bytes:
    return _BOLTFFI_STRUCT_F64.pack(float(value))


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
        self._data = data
        self._offset = 0

    def finish(self) -> None:
        if self._offset != len(self._data):
            raise ValueError("trailing BoltFFI wire bytes")

    def read(self, count: int) -> bytes:
        offset = self._offset
        end = offset + count
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = end
        return self._data[offset:end]

    def bool(self) -> bool:
        value = self.u8()
        if value > 1:
            raise ValueError("invalid BoltFFI bool")
        return value == 1

    def i8(self) -> int:
        offset = self._offset
        self._offset = offset + 1
        return _BOLTFFI_UNPACK_I8(self._data, offset)[0]

    def u8(self) -> int:
        offset = self._offset
        if offset >= len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = offset + 1
        return self._data[offset]

    def i16(self) -> int:
        offset = self._offset
        self._offset = offset + 2
        return _BOLTFFI_UNPACK_I16(self._data, offset)[0]

    def u16(self) -> int:
        offset = self._offset
        self._offset = offset + 2
        return _BOLTFFI_UNPACK_U16(self._data, offset)[0]

    def i32(self) -> int:
        offset = self._offset
        self._offset = offset + 4
        return _BOLTFFI_UNPACK_I32(self._data, offset)[0]

    def u32(self) -> int:
        offset = self._offset
        self._offset = offset + 4
        return _BOLTFFI_UNPACK_U32(self._data, offset)[0]

    def i64(self) -> int:
        offset = self._offset
        self._offset = offset + 8
        return _BOLTFFI_UNPACK_I64(self._data, offset)[0]

    def u64(self) -> int:
        offset = self._offset
        self._offset = offset + 8
        return _BOLTFFI_UNPACK_U64(self._data, offset)[0]

    def isize(self) -> int:
        return self.i64()

    def usize(self) -> int:
        return self.u64()

    def f32(self) -> float:
        offset = self._offset
        self._offset = offset + 4
        return _BOLTFFI_UNPACK_F32(self._data, offset)[0]

    def f64(self) -> float:
        offset = self._offset
        self._offset = offset + 8
        return _BOLTFFI_UNPACK_F64(self._data, offset)[0]

    def string(self) -> str:
        count = self.u32()
        offset = self._offset
        end = offset + count
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = end
        return str(memoryview(self._data)[offset:end], "utf-8")

    def bytes(self) -> bytes:
        return self.read(self.u32())

    def fixed(self, layout) -> tuple:
        offset = self._offset
        self._offset = offset + layout.size
        return layout.unpack_from(self._data, offset)

    def fixed_sequence(self, layout, factory) -> list:
        count = self.u32()
        offset = self._offset
        end = offset + count * layout.size
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = end
        window = memoryview(self._data)[offset:end]
        return [factory(*values) for values in layout.iter_unpack(window)]

    def enum_sequence(self, layout, enum_type) -> list:
        count = self.u32()
        offset = self._offset
        end = offset + count * layout.size
        if end > len(self._data):
            raise ValueError("truncated BoltFFI wire bytes")
        self._offset = end
        window = memoryview(self._data)[offset:end]
        members = enum_type._value2member_map_
        try:
            return [members[value] for (value,) in layout.iter_unpack(window)]
        except KeyError as error:
            raise ValueError(f"invalid {enum_type.__name__} value") from error

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
        tag = self.u8()
        if tag == 0:
            return None
        if tag == 1:
            return decode()
        raise ValueError("invalid BoltFFI option tag")

    def result(self, decode_ok, decode_err):
        tag = self.u8()
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
    try:
        value = decode(reader)
    except struct.error as error:
        raise ValueError("truncated BoltFFI wire bytes") from error
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
{% for enumeration in enums %}
{%- if let Some(wire) = enumeration.wire %}
class {{ enumeration.class_name }}:
{{- enumeration.documentation.docstring("    ") }}
    __slots__ = ()

    @classmethod
    def _boltffi_from_wire(cls, data: bytes) -> "{{ enumeration.class_name }}":
        reader = _BoltFfiWireReader(data)
        try:
            value = cls._boltffi_from_reader(reader)
        except struct.error as error:
            raise ValueError("truncated BoltFFI wire bytes") from error
        reader.finish()
        return value

    @classmethod
    def _boltffi_from_reader(cls, reader: "_BoltFfiWireReader") -> "{{ enumeration.class_name }}":
        tag = reader.u32()
{%- for variant in wire.variants %}
        if tag == {{ variant.tag }}:
{%- if let Some(payload) = variant.transparent_payload %}
            return {{ payload }}._boltffi_from_reader(reader)
{%- else %}
            return {{ variant.class_name }}._boltffi_from_reader_payload(reader)
{%- endif %}
{%- endfor %}
        raise ValueError("invalid {{ enumeration.class_name }} tag")
{%- if wire.transparent %}

    @classmethod
    def _boltffi_wire_value(cls, value) -> bytes:
{%- for variant in wire.variants %}
{%- if let Some(payload) = variant.transparent_payload %}
        if type(value) is {{ payload }}:
            return _boltffi_wire_u32({{ variant.tag }}) + value._boltffi_wire()
{%- endif %}
{%- endfor %}
        if isinstance(value, cls):
            return value._boltffi_wire()
        raise TypeError(f"expected {{ enumeration.class_name }}, got {type(value).__name__}")
{%- endif %}
{%- for constructor in enumeration.constructors %}

    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ enumeration.class_name }}":
{{- constructor.documentation.docstring("        ") }}
{%- for line in constructor.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.static_methods %}

    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("        ") }}
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.instance_methods %}

    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("        ") }}
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}

{% for variant in wire.variants %}
{%- if !variant.transparent() %}
@dataclass(frozen=True, slots=True)
class {{ variant.class_name }}({{ enumeration.class_name }}):
{{- variant.documentation.docstring("    ") }}
{%- if variant.has_fields() %}
{%- for field in variant.fields %}
    {{ field.name }}: {{ field.annotation }}{% if let Some(default) = field.default %} = {{ default }}{% endif %}
{{- field.documentation.docstring("    ") }}
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
{%- endif %}

{% endfor %}
{%- else %}
class {{ enumeration.class_name }}(IntEnum):
{{- enumeration.documentation.docstring("    ") }}
{%- for variant in enumeration.variants %}
    {{ variant.name }} = {{ variant.value }}
{%- endfor %}
{%- for constructor in enumeration.constructors %}

    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ enumeration.class_name }}":
{{- constructor.documentation.docstring("        ") }}
{%- for line in constructor.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.static_methods %}

    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("        ") }}
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.instance_methods %}

    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("        ") }}
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
{% for record in records %}
{%- match record.wire %}
{%- when RecordWire::Fixed(fixed) %}
{%- if record.bases.is_empty() %}
{{ record.class_name }} = _native.{{ record.class_name }}
{%- else %}
{{ record.class_name }} = _native.{{ record.type_factory }}(({% for base in record.bases %}{{ base }},{% if !loop.last %} {% endif %}{% endfor %}))
{%- endif %}
{{ record.class_name }}.__module__ = __name__
{%- if !record.documentation.is_empty() %}
{{ record.class_name }}.__doc__ = {{ record.documentation.literal() }}
{%- endif %}
{{ record.class_name }}.__match_args__ = ({% for field in record.fields %}"{{ field.name }}",{% endfor %})
{{ record.class_name }}.__annotations__ = {{ "{" }}{% for field in record.fields %}"{{ field.name }}": {{ field.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}{{ "}" }}
{{ fixed.struct_global() }} = struct.Struct("{{ fixed.format() }}")


def _boltffi_attach_{{ record.class_name }}_wire(self) -> bytes:
    return {{ fixed.struct_global() }}.pack({% for field in record.fields %}self.{{ field.name }}{% if !loop.last %}, {% endif %}{% endfor %})


def _boltffi_attach_{{ record.class_name }}_from_wire(cls, data: bytes) -> "{{ record.class_name }}":
    reader = _BoltFfiWireReader(data)
    try:
        value = cls._boltffi_from_reader(reader)
    except struct.error as error:
        raise ValueError("truncated BoltFFI wire bytes") from error
    reader.finish()
    return value


def _boltffi_attach_{{ record.class_name }}_from_reader(cls, reader: "_BoltFfiWireReader") -> "{{ record.class_name }}":
    return cls(*reader.fixed({{ fixed.struct_global() }}))


{{ record.class_name }}._boltffi_wire = _boltffi_attach_{{ record.class_name }}_wire
{{ record.class_name }}._boltffi_from_wire = classmethod(_boltffi_attach_{{ record.class_name }}_from_wire)
{{ record.class_name }}._boltffi_from_reader = classmethod(_boltffi_attach_{{ record.class_name }}_from_reader)
{%- for constructor in record.constructors %}


{% if constructor.asynchronous %}async {% endif %}def _boltffi_attach_{{ record.class_name }}_{{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ record.class_name }}":
{{- constructor.documentation.docstring("    ") }}
{%- for line in constructor.body %}
    {{ line }}
{%- endfor %}


{{ record.class_name }}.{{ constructor.python_name }} = classmethod(_boltffi_attach_{{ record.class_name }}_{{ constructor.python_name }})
{%- endfor %}
{%- for method in record.static_methods %}


{% if method.asynchronous %}async {% endif %}def _boltffi_attach_{{ record.class_name }}_{{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("    ") }}
{%- for line in method.body %}
    {{ line }}
{%- endfor %}


{{ record.class_name }}.{{ method.python_name }} = staticmethod(_boltffi_attach_{{ record.class_name }}_{{ method.python_name }})
{%- endfor %}
{%- for method in record.instance_methods %}


{% if method.asynchronous %}async {% endif %}def _boltffi_attach_{{ record.class_name }}_{{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("    ") }}
{%- for line in method.body %}
    {{ line }}
{%- endfor %}


{{ record.class_name }}.{{ method.python_name }} = _boltffi_attach_{{ record.class_name }}_{{ method.python_name }}
{%- endfor %}
{%- when RecordWire::Fields(wire_fields) %}
@dataclass(frozen=True, slots=True)
class {{ record.class_name }}{% if !record.bases.is_empty() %}({% for base in record.bases %}{{ base }}{% if !loop.last %}, {% endif %}{% endfor %}){% endif %}:
{{- record.documentation.docstring("    ") }}
{%- for field in record.fields %}
    {{ field.name }}: {{ field.annotation }}{% if let Some(default) = field.default %} = {{ default }}{% endif %}
{{- field.documentation.docstring("    ") }}
{%- endfor %}

    def _boltffi_wire(self) -> bytes:
        return b"".join((
{%- for field in wire_fields %}
            {{ field.encode }},
{%- endfor %}
        ))

    @classmethod
    def _boltffi_from_wire(cls, data: bytes) -> "{{ record.class_name }}":
        reader = _BoltFfiWireReader(data)
        try:
            value = cls._boltffi_from_reader(reader)
        except struct.error as error:
            raise ValueError("truncated BoltFFI wire bytes") from error
        reader.finish()
        return value

    @classmethod
    def _boltffi_from_reader(cls, reader: "_BoltFfiWireReader") -> "{{ record.class_name }}":
        return cls(
{%- for field in wire_fields %}
            {{ field.name }}={{ field.decode }},
{%- endfor %}
        )
{%- for constructor in record.constructors %}

    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ record.class_name }}":
{{- constructor.documentation.docstring("        ") }}
{%- for line in constructor.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in record.static_methods %}

    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("        ") }}
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in record.instance_methods %}

    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("        ") }}
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}


_native.{{ record.register_method }}({{ record.class_name }})
{%- endmatch %}
{% if let Some(exception_name) = record.exception_name %}

class {{ exception_name }}(RuntimeError):
    __slots__ = ("error",)

    def __init__(self, error: {{ record.class_name }}) -> None:
        self.error = error
        super().__init__(error)
{% endif %}

{% endfor %}
{% for class in classes %}
class {{ class.class_name }}:
{{- class.documentation.docstring("    ") }}
    __slots__ = ("_handle",)

{% if !class.init.is_empty() %}
{% for init in class.init %}
    def __init__(self{% for parameter in init.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> None:
{{- init.documentation.docstring("        ") }}
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
{{- constructor.documentation.docstring("        ") }}
{%- for line in constructor.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in class.static_methods %}

    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("        ") }}
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for method in class.instance_methods %}

    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{{- method.documentation.docstring("        ") }}
{%- for line in method.body %}
        {{ line }}
{%- endfor %}
{%- endfor %}
{%- for stream in class.streams %}

    def {{ stream.python_name }}(self) -> "{{ stream.subscription_class }}":
{{- stream.documentation.docstring("        ") }}
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
{% for constant in associated_constants %}
{% if let Some(owner) = constant.owner %}
{{ owner }}.{{ constant.python_name }} = {{ constant.expression }}
{% endif %}
{% endfor %}
{% for constant in constants %}
{{ constant.python_name }}: {{ constant.annotation }} = {{ constant.expression }}
{{- constant.documentation.docstring("") }}
{% endfor %}
{% for function in functions %}
{% if function.asynchronous %}async {% endif %}def {{ function.python_name }}({% for parameter in function.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ function.return_annotation }}:
{{- function.documentation.docstring("    ") }}
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
{%- if !variant.transparent() %}
    "{{ variant.class_name }}",
{%- endif %}
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
