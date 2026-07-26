    {{ method.c_return_type }} result = boltffi_jni_byte_array_to_buffer(env, __boltffi_return_array);
    (*env)->DeleteLocalRef(env, __boltffi_return_array);
