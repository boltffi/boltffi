#include <jni.h>
#include <stdint.h>
#include <stdbool.h>
{%- if uses_lifecycle %}
#include <stdio.h>
{%- endif %}
{%- if uses_limits %}
#include <limits.h>
{%- endif %}
{%- if uses_direct_buffers %}
#include <string.h>
{%- endif %}
{%- if uses_callback_handles || uses_closure_handles || uses_byte_arrays %}
#include <stdlib.h>
{%- endif %}
#if defined(__ANDROID__)
#include <pthread.h>
#endif

#include {{ c_header }}
