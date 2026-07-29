from __future__ import annotations

import uuid

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
MODULE_NAME: str
PACKAGE_NAME: str
PACKAGE_VERSION: str | None
{% for record in records %}
@dataclass(frozen=True, slots=True)
class {{ record.class_name }}:
{%- for constant in record.constants %}
    {{ constant.python_name }}: ClassVar[{{ constant.annotation }}]
{%- endfor %}
{%- for field in record.fields %}
    {{ field.name }}: {{ field.annotation }}{% if let Some(default) = field.default %} = {{ default }}{% endif %}
{%- endfor %}
{%- for constructor in record.constructors %}
    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ record.class_name }}": ...
{%- endfor %}
{%- for method in record.static_methods %}
    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}: ...
{%- endfor %}
{%- for method in record.instance_methods %}
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}: ...
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
{%- if enumeration.constructors.is_empty() && enumeration.static_methods.is_empty() && enumeration.instance_methods.is_empty() && enumeration.constants.is_empty() %}
    pass
{%- endif %}
{%- for constant in enumeration.constants %}
    {{ constant.python_name }}: ClassVar[{{ constant.annotation }}]
{%- endfor %}
{%- for constructor in enumeration.constructors %}
    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ enumeration.class_name }}": ...
{%- endfor %}
{%- for method in enumeration.static_methods %}
    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}: ...
{%- endfor %}
{%- for method in enumeration.instance_methods %}
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}: ...
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

{% endfor %}
{%- else %}
class {{ enumeration.class_name }}(IntEnum):
{%- for variant in enumeration.variants %}
    {{ variant.name }} = {{ variant.value }}
{%- endfor %}
{%- for constant in enumeration.constants %}
    {{ constant.python_name }}: ClassVar[{{ constant.annotation }}]
{%- endfor %}
{%- for constructor in enumeration.constructors %}
    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ enumeration.class_name }}": ...
{%- endfor %}
{%- for method in enumeration.static_methods %}
    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}: ...
{%- endfor %}
{%- for method in enumeration.instance_methods %}
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}: ...
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
    _handle: int
{%- for constant in class.constants %}
    {{ constant.python_name }}: ClassVar[{{ constant.annotation }}]
{%- endfor %}
{% if !class.init.is_empty() %}
{% for init in class.init %}
    def __init__(self{% for parameter in init.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> None: ...
{% endfor %}
{% else %}
    def __init__(self) -> None: ...
{% endif %}
    @classmethod
    def _from_handle(cls, handle: int) -> "{{ class.class_name }}": ...
    def __del__(self) -> None: ...
{%- for constructor in class.constructors %}
    @classmethod
    {% if constructor.asynchronous %}async {% endif %}def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ class.class_name }}": ...
{%- endfor %}
{%- for method in class.static_methods %}
    @staticmethod
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}: ...
{%- endfor %}
{%- for method in class.instance_methods %}
    {% if method.asynchronous %}async {% endif %}def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}: ...
{%- endfor %}
{%- for stream in class.streams %}
    def {{ stream.python_name }}(self) -> "{{ stream.subscription_class }}": ...
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
{% endfor %}
{% for function in functions %}
{% if function.asynchronous %}async {% endif %}def {{ function.python_name }}({% for parameter in function.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ function.return_annotation }}: ...
{%- endfor %}
