@file:OptIn(kotlin.ExperimentalUnsignedTypes::class)

package {{ package }}
{%- if async_runtime || stream_runtime %}

import kotlinx.coroutines.launch
{%- endif %}
{%- if stream_runtime %}
import kotlinx.coroutines.channels.awaitClose
{%- endif %}

{{ runtime }}

@Suppress("FunctionName")
private object Native {
    fun ensureInitialized() {}
{{ native_library_loader }}

{%- for function in native_functions %}
    @JvmStatic external fun {{ function.name() }}({% for parameter in function.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}): {{ function.returns() }}
{%- endfor %}
{%- if async_runtime %}

    @JvmStatic fun boltffiFutureContinuationCallback(handle: Long, pollResult: Byte) {
        BoltFfiAsync.resume(handle, pollResult)
    }
{%- endif %}
}
{%- if !closures.is_empty() %}

{{ closures }}
{%- endif %}

{{ declarations }}
