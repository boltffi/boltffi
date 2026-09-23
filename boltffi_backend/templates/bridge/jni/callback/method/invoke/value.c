{% include "bridge/jni/callback/method/invoke/raw_return.c" %}
    if ((*env)->ExceptionCheck(env)) {
        goto __boltffi_fail;
    }
{% include "bridge/jni/callback/method/invoke/return.c" %}
{% include "bridge/jni/callback/method/cleanup.c" %}
    boltffi_jni_exit(env, attached);
    return result;
