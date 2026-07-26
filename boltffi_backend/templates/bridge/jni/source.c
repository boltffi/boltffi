{% include "bridge/jni/source/includes.c" %}

{%- if uses_lifecycle %}
{% include "bridge/jni/runtime.c" %}
{%- endif %}

{%- if uses_continuations %}
{% include "bridge/jni/continuation.c" %}
{%- endif %}

{%- if uses_exceptions %}
{% include "bridge/jni/source/exceptions.c" %}
{%- endif %}

{%- if checks_status %}
{% include "bridge/jni/source/status.c" %}
{%- endif %}

{%- if checks_error_buffer %}
{% include "bridge/jni/source/error_buffer.c" %}
{%- endif %}

{%- if uses_byte_arrays %}
{% include "bridge/jni/source/byte_arrays.c" %}
{%- endif %}

{%- if uses_record_arrays %}
{% include "bridge/jni/source/records.c" %}
{%- endif %}

{%- if uses_direct_buffers %}
{% include "bridge/jni/source/direct_buffers.c" %}
{%- endif %}

{%- if uses_callback_handles %}
{% include "bridge/jni/callback.c" %}
{%- endif %}

{%- if closures.len() > 0 %}
{% include "bridge/jni/closure_registration/prototypes.c" %}
{%- endif %}

{%- if closure_handles.len() > 0 %}
{% include "bridge/jni/closure_callback_handle.c" %}
{%- endif %}

{%- if closures.len() > 0 %}
{% include "bridge/jni/closure_registration.c" %}
{%- endif %}

{%- if callbacks.len() > 0 %}
{% include "bridge/jni/callback_registration.c" %}
{%- endif %}

{%- for invoker in callback_completions %}
{% include "bridge/jni/callback_completion.c" %}
{%- endfor %}

{%- for writer in success_out_writers %}
{% include "bridge/jni/success_out.c" %}
{%- endfor %}

{%- if uses_lifecycle %}
{% include "bridge/jni/lifecycle.c" %}
{%- endif %}

{%- for method in methods %}
{% include "bridge/jni/method.c" %}
{%- endfor %}

{%- for batch in direct_stream_batches %}
{% include "bridge/jni/stream_direct_batch.c" %}
{%- endfor %}
