BoltFFICallbackHandle boltffi_tests_create_class_receiver(
    const ___ClassHandleReceiverVTable *vtable, uint64_t identity
) {
    boltffi_register_callback_13boltffi_tests19ClassHandleReceiver(vtable);
    return boltffi_create_callback_13boltffi_tests19ClassHandleReceiver(identity);
}

uint32_t boltffi_tests_consume_callback_class(uint64_t handle) {
    uint32_t value = boltffi_method_class_13boltffi_tests18CallbackOnlyHandle5value(handle);
    boltffi_release_class_13boltffi_tests18CallbackOnlyHandle(handle);
    return value;
}
