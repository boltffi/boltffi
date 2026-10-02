from __future__ import annotations

from uuid import UUID as _UUID

{% if !records.is_empty() || has_data_enums %}
from dataclasses import dataclass

{% endif %}
{% if !enums.is_empty() %}
from enum import IntEnum

{% endif %}
{% if uses_sequence_annotations || uses_callable_annotations %}
from collections.abc import {% if uses_callable_annotations %}Callable{% if uses_sequence_annotations %}, {% endif %}{% endif %}{% if uses_sequence_annotations %}Sequence{% endif %}

{% endif %}
{% if has_associated_constants %}
from typing import ClassVar

{% endif %}
{% if uses_overloads %}
from typing import overload as _overload

{% endif %}
MODULE_NAME: str
PACKAGE_NAME: str
PACKAGE_VERSION: str | None
{% for record in records %}
@dataclass(frozen=True, slots=True)
class {{ record.class_name }}:
{{- record.documentation.docstring("    ") }}
{%- for constant in record.constants %}
    {{ constant.python_name }}: ClassVar[{{ constant.annotation }}]
{{- constant.documentation.docstring("    ") }}
{%- endfor %}
{%- for field in record.fields %}
    {{ field.name }}: {{ field.annotation }}{% if let Some(default) = field.default %} = {{ default }}{% endif %}
{{- field.documentation.docstring("    ") }}
{%- endfor %}
{%- for constructor in record.constructors %}
{%- for signature in constructor.parameters.stub_declarations() %}
{%- if constructor.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% if !constructor.parameters.is_empty() %}, {{ signature }}{% endif %}) -> "{{ record.class_name }}":{% if constructor.documentation.is_empty() %} ...{% else %}{{ constructor.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for method in record.static_methods %}
{%- for signature in method.parameters.stub_declarations() %}
{%- if method.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({{ signature }}) -> {{ method.return_annotation }}:{% if method.documentation.is_empty() %} ...{% else %}{{ method.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for method in record.instance_methods %}
{%- for signature in method.parameters.stub_declarations() %}
{%- if method.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% if !method.parameters.is_empty() %}, {{ signature }}{% endif %}) -> {{ method.return_annotation }}:{% if method.documentation.is_empty() %} ...{% else %}{{ method.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}

{% if let Some(exception_name) = record.exception_name %}
class {{ exception_name }}(RuntimeError):
    error: {{ record.class_name }}
    def __init__(self, error: {{ record.class_name }}) -> None: ...

{% endif %}
{% endfor %}
{% for enumeration in enums %}
{%- if let Some(wire) = enumeration.wire %}
class {{ enumeration.class_name }}:
{{- enumeration.documentation.docstring("    ") }}
{%- if enumeration.constructors.is_empty() && enumeration.static_methods.is_empty() && enumeration.instance_methods.is_empty() && enumeration.constants.is_empty() %}
    pass
{%- endif %}
{%- for constant in enumeration.constants %}
    {{ constant.python_name }}: ClassVar[{{ constant.annotation }}]
{{- constant.documentation.docstring("    ") }}
{%- endfor %}
{%- for constructor in enumeration.constructors %}
{%- for signature in constructor.parameters.stub_declarations() %}
{%- if constructor.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% if !constructor.parameters.is_empty() %}, {{ signature }}{% endif %}) -> "{{ enumeration.class_name }}":{% if constructor.documentation.is_empty() %} ...{% else %}{{ constructor.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.static_methods %}
{%- for signature in method.parameters.stub_declarations() %}
{%- if method.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({{ signature }}) -> {{ method.return_annotation }}:{% if method.documentation.is_empty() %} ...{% else %}{{ method.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.instance_methods %}
{%- for signature in method.parameters.stub_declarations() %}
{%- if method.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% if !method.parameters.is_empty() %}, {{ signature }}{% endif %}) -> {{ method.return_annotation }}:{% if method.documentation.is_empty() %} ...{% else %}{{ method.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}

{% for variant in wire.variants %}
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

{% endfor %}
{%- else %}
class {{ enumeration.class_name }}(IntEnum):
{{- enumeration.documentation.docstring("    ") }}
{%- for variant in enumeration.variants %}
    {{ variant.name }} = {{ variant.value }}
{%- endfor %}
{%- for constant in enumeration.constants %}
    {{ constant.python_name }}: ClassVar[{{ constant.annotation }}]
{{- constant.documentation.docstring("    ") }}
{%- endfor %}
{%- for constructor in enumeration.constructors %}
{%- for signature in constructor.parameters.stub_declarations() %}
{%- if constructor.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% if !constructor.parameters.is_empty() %}, {{ signature }}{% endif %}) -> "{{ enumeration.class_name }}":{% if constructor.documentation.is_empty() %} ...{% else %}{{ constructor.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.static_methods %}
{%- for signature in method.parameters.stub_declarations() %}
{%- if method.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({{ signature }}) -> {{ method.return_annotation }}:{% if method.documentation.is_empty() %} ...{% else %}{{ method.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for method in enumeration.instance_methods %}
{%- for signature in method.parameters.stub_declarations() %}
{%- if method.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% if !method.parameters.is_empty() %}, {{ signature }}{% endif %}) -> {{ method.return_annotation }}:{% if method.documentation.is_empty() %} ...{% else %}{{ method.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}

{%- endif %}
{% if let Some(exception_name) = enumeration.exception_name %}
class {{ exception_name }}(RuntimeError):
    error: {{ enumeration.class_name }}
    def __init__(self, error: {{ enumeration.class_name }}) -> None: ...

{% endif %}
{% endfor %}
{% for class in classes %}
class {{ class.class_name }}:
{{- class.documentation.docstring("    ") }}
    _handle: int
{%- for constant in class.constants %}
    {{ constant.python_name }}: ClassVar[{{ constant.annotation }}]
{{- constant.documentation.docstring("    ") }}
{%- endfor %}
{% if !class.init.is_empty() %}
{% for init in class.init %}
{%- for signature in init.parameters.stub_declarations() %}
{%- if init.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    def __init__(self{% if !init.parameters.is_empty() %}, {{ signature }}{% endif %}) -> None:{% if init.documentation.is_empty() %} ...{% else %}{{ init.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{% endfor %}
{% else %}
    def __init__(self) -> None: ...
{% endif %}
    @classmethod
    def _from_handle(cls, handle: int) -> "{{ class.class_name }}": ...
    def __del__(self) -> None: ...
{%- for constructor in class.constructors %}
{%- for signature in constructor.parameters.stub_declarations() %}
{%- if constructor.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% if !constructor.parameters.is_empty() %}, {{ signature }}{% endif %}) -> "{{ class.class_name }}":{% if constructor.documentation.is_empty() %} ...{% else %}{{ constructor.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for method in class.static_methods %}
{%- for signature in method.parameters.stub_declarations() %}
{%- if method.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({{ signature }}) -> {{ method.return_annotation }}:{% if method.documentation.is_empty() %} ...{% else %}{{ method.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for method in class.instance_methods %}
{%- for signature in method.parameters.stub_declarations() %}
{%- if method.parameters.has_stub_overloads() %}
    @_overload
{%- endif %}
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% if !method.parameters.is_empty() %}, {{ signature }}{% endif %}) -> {{ method.return_annotation }}:{% if method.documentation.is_empty() %} ...{% else %}{{ method.documentation.docstring("        ") }}{% endif %}
{%- endfor %}
{%- endfor %}
{%- for stream in class.streams %}
    def {{ stream.python_name }}(self) -> "{{ stream.subscription_class }}":{% if stream.documentation.is_empty() %} ...{% else %}{{ stream.documentation.docstring("        ") }}{% endif %}
{%- endfor %}

{% for stream in class.streams %}
class {{ stream.subscription_class }}:
    _handle: int | None
    def __init__(self) -> None: ...
    @classmethod
    def _from_handle(cls, handle: int) -> "{{ stream.subscription_class }}": ...
    def __del__(self) -> None: ...
    def pop_batch(self, max_count: int = 16) -> list[{{ stream.item_annotation }}]: ...
    def wait(self, timeout_milliseconds: int) -> int: ...
    def unsubscribe(self) -> None: ...

{% endfor %}
{% endfor %}
{% for constant in constants %}
{{ constant.python_name }}: {{ constant.annotation }}
{{- constant.documentation.docstring("") }}
{% endfor %}
{% for function in functions %}
{%- for signature in function.parameters.stub_declarations() %}
{%- if function.parameters.has_stub_overloads() %}
@_overload
{%- endif %}
{% if function.asynchronous %}async {% endif %}def {{ function.python_name }}({{ signature }}) -> {{ function.return_annotation }}:{% if function.documentation.is_empty() %} ...{% else %}{{ function.documentation.docstring("    ") }}{% endif %}
{%- endfor %}
{%- endfor %}
