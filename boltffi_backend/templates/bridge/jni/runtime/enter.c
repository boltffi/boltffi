static inline bool boltffi_jni_enter(JNIEnv **env, int *attached) {
    if (boltffi_jni_vm == NULL) {
        return false;
    }
    *env = NULL;
    *attached = 0;
    jint env_status = (*boltffi_jni_vm)->GetEnv(boltffi_jni_vm, (void **)env, JNI_VERSION_1_6);
    if (env_status == JNI_EDETACHED) {
#if defined(__ANDROID__)
        if (boltffi_jni_android_attach_cached(boltffi_jni_vm, env, attached) != JNI_OK) {
            return false;
        }
#else
        if (boltffi_jni_attach_current_thread(boltffi_jni_vm, env) != JNI_OK) {
            return false;
        }
        *attached = 1;
#endif
    } else if (env_status != JNI_OK) {
        return false;
    }

#if defined(__ANDROID__)
    JNIEnv *callback_env = *env;
    if ((*callback_env)->PushLocalFrame(callback_env, BOLTFFI_JNI_LOCAL_FRAME_CAPACITY) != JNI_OK) {
        boltffi_jni_clear_exception(callback_env);
        if (*attached) {
            (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
            *attached = 0;
        }
        return false;
    }
#endif

    return true;
}
