static {{ method.c_return_type }} {{ method.function }}({% for parameter in method.c_parameters %}{{ parameter.declaration() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
    JNIEnv *env = NULL;
    int attached = 0;
{%- if method.transfers_classes %}
    uint8_t __boltffi_classes_delivered = 0;
    jobject __boltffi_class_delivery = NULL;
{%- endif %}
{% include "bridge/jni/callback/method/locals.c" %}
    if (!boltffi_jni_enter(&env, &attached)) {
{%- for parameter in method.c_parameters %}
{%- if let Some(release) = parameter.class_release() %}
        {{ release }}({{ parameter.name() }});
{%- endif %}
{%- endfor %}
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
{%- if method.returns_void %}
        return;
{%- else if method.returns_error %}
        return {{ callback_error }}(NULL, 0);
{%- else %}
        return {{ method.failure_value }};
{%- endif %}
    }
{%- if method.transfers_classes %}
    __boltffi_class_delivery = (*env)->NewDirectByteBuffer(env, &__boltffi_classes_delivered, 1);
    if (__boltffi_class_delivery == NULL) goto __boltffi_fail;
{%- endif %}
{% include "bridge/jni/callback/method/byte_arrays.c" %}
{% include "bridge/jni/callback/method/direct_vectors.c" %}
{% include "bridge/jni/callback/method/record_arrays.c" %}
{% include "bridge/jni/callback/method/callback_handles.c" %}
{% include "bridge/jni/callback/method/closure_handles.c" %}
{% include "bridge/jni/callback/method/invoke.c" %}
__boltffi_fail:;
{%- if method.returns_error %}
    FfiBuf_u8 callback_error = boltffi_jni_callback_error(env);
{%- endif %}
{% include "bridge/jni/callback/method/cleanup.c" %}
    boltffi_jni_clear_exception(env);
    boltffi_jni_exit(env, attached);
{% include "bridge/jni/callback/method/fail.c" %}
}
