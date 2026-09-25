BoltFFICallbackHandle boltffi_tests_create_class_receiver(
    const ___ClassHandleReceiverVTable *vtable, uint64_t identity
) {
    boltffi_register_callback_boltffi_tests_callbacks_class_handles_class_handle_receiver(vtable);
    return boltffi_create_callback_boltffi_tests_callbacks_class_handles_class_handle_receiver(identity);
}

uint32_t boltffi_tests_consume_callback_class(uint64_t handle) {
    uint32_t value = boltffi_method_class_boltffi_tests_callbacks_class_handles_callback_only_handle_value(handle);
    boltffi_release_class_boltffi_tests_callbacks_class_handles_callback_only_handle(handle);
    return value;
}
