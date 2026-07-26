static inline void boltffi_jni_exit(JNIEnv *env, int attached) {
#if defined(__ANDROID__)
    if (env != NULL) {
        (*env)->PopLocalFrame(env, NULL);
        boltffi_jni_clear_exception(env);
    }
#else
    (void)env;
#endif
    if (attached) {
        (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
    }
}
