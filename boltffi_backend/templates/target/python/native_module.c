
{% if support.uses_wire_arguments() || support.uses_owned_buffers() %}
static void boltffi_python_write_u16_le(uint8_t *buffer, uint16_t value) {
    buffer[0] = (uint8_t)(value & 0xffu);
    buffer[1] = (uint8_t)((value >> 8) & 0xffu);
}

static void boltffi_python_write_u32_le(uint8_t *buffer, uint32_t value) {
    buffer[0] = (uint8_t)(value & 0xffu);
    buffer[1] = (uint8_t)((value >> 8) & 0xffu);
    buffer[2] = (uint8_t)((value >> 16) & 0xffu);
    buffer[3] = (uint8_t)((value >> 24) & 0xffu);
}

static void boltffi_python_write_u64_le(uint8_t *buffer, uint64_t value) {
    buffer[0] = (uint8_t)(value & 0xffu);
    buffer[1] = (uint8_t)((value >> 8) & 0xffu);
    buffer[2] = (uint8_t)((value >> 16) & 0xffu);
    buffer[3] = (uint8_t)((value >> 24) & 0xffu);
    buffer[4] = (uint8_t)((value >> 32) & 0xffu);
    buffer[5] = (uint8_t)((value >> 40) & 0xffu);
    buffer[6] = (uint8_t)((value >> 48) & 0xffu);
    buffer[7] = (uint8_t)((value >> 56) & 0xffu);
}

static int boltffi_python_wire_fixed(const uint8_t *payload, Py_ssize_t payload_len, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    PyObject *wire = PyBytes_FromStringAndSize((const char *)payload, payload_len);
    if (wire == NULL) {
        return 0;
    }
    *out_wire = wire;
    *out_ptr = (const uint8_t *)PyBytes_AS_STRING(wire);
    *out_len = (uintptr_t)payload_len;
    return 1;
}

static int boltffi_python_wire_payload(const uint8_t *payload, Py_ssize_t payload_len, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    if (payload_len < 0 || (uint64_t)payload_len > UINT32_MAX || payload_len > PY_SSIZE_T_MAX - 4) {
        PyErr_SetString(PyExc_OverflowError, "payload is too large");
        return 0;
    }
    Py_ssize_t wire_len = payload_len + 4;
    PyObject *wire = PyBytes_FromStringAndSize(NULL, wire_len);
    if (wire == NULL) {
        return 0;
    }
    uint8_t *bytes = (uint8_t *)PyBytes_AS_STRING(wire);
    boltffi_python_write_u32_le(bytes, (uint32_t)payload_len);
    if (payload_len > 0) {
        memcpy(bytes + 4, payload, (size_t)payload_len);
    }
    *out_wire = wire;
    *out_ptr = bytes;
    *out_len = (uintptr_t)wire_len;
    return 1;
}
{% endif %}
{% if support.uses_wire_strings() %}
static int boltffi_python_wire_string(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    Py_ssize_t len = 0;
    const char *utf8 = PyUnicode_AsUTF8AndSize(value, &len);
    if (utf8 == NULL) {
        return 0;
    }
    return boltffi_python_wire_payload((const uint8_t *)utf8, len, out_wire, out_ptr, out_len);
}
{% endif %}
{% if support.uses_wire_bytes() %}
static int boltffi_python_wire_bytes(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    Py_buffer view;
    if (PyObject_GetBuffer(value, &view, PyBUF_CONTIG_RO) < 0) {
        return 0;
    }
    int ok = boltffi_python_wire_payload((const uint8_t *)view.buf, view.len, out_wire, out_ptr, out_len);
    PyBuffer_Release(&view);
    return ok;
}
{% endif %}
{% if support.uses_raw_wire_arguments() %}
static int boltffi_python_wire_raw(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    Py_buffer view;
    if (PyObject_GetBuffer(value, &view, PyBUF_CONTIG_RO) < 0) {
        return 0;
    }
    int ok = boltffi_python_wire_fixed((const uint8_t *)view.buf, view.len, out_wire, out_ptr, out_len);
    PyBuffer_Release(&view);
    return ok;
}
{% endif %}
{% if !codec_decoders.is_empty() || !codec_encoders.is_empty() %}
static PyObject *boltffi_python_wire_codecs = NULL;

static int boltffi_python_validate_codec_bytes(const uint8_t *ptr, uintptr_t len) {
    if (ptr == NULL && len != 0) {
        PyErr_SetString(PyExc_RuntimeError, "native callback argument contains an invalid buffer");
        return 0;
    }
    if (len > PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native callback argument is too large");
        return 0;
    }
    return 1;
}

static PyObject *boltffi_python_register_wire_codec(PyObject *self, PyObject *args) {
    const char *key = NULL;
    PyObject *callable = NULL;
    (void)self;
    if (!PyArg_ParseTuple(args, "sO", &key, &callable)) {
        return NULL;
    }
    if (!PyCallable_Check(callable)) {
        PyErr_SetString(PyExc_TypeError, "wire codec must be callable");
        return NULL;
    }
    if (boltffi_python_wire_codecs == NULL) {
        boltffi_python_wire_codecs = PyDict_New();
        if (boltffi_python_wire_codecs == NULL) {
            return NULL;
        }
    }
    if (PyDict_SetItemString(boltffi_python_wire_codecs, key, callable) < 0) {
        return NULL;
    }
    Py_RETURN_NONE;
}

static PyObject *boltffi_python_wire_codec(const char *key) {
    PyObject *callable = NULL;
    if (boltffi_python_wire_codecs == NULL) {
        PyErr_SetString(PyExc_RuntimeError, "wire codec registry is not initialized");
        return NULL;
    }
    callable = PyDict_GetItemString(boltffi_python_wire_codecs, key);
    if (callable == NULL) {
        PyErr_Format(PyExc_RuntimeError, "wire codec %s is not registered", key);
        return NULL;
    }
    return callable;
}

static PyObject *boltffi_python_decode_wire_codec(const char *key, const uint8_t *ptr, uintptr_t len) {
    PyObject *callable = NULL;
    PyObject *wire = NULL;
    PyObject *result = NULL;
    if (!boltffi_python_validate_codec_bytes(ptr, len)) {
        return NULL;
    }
    callable = boltffi_python_wire_codec(key);
    if (callable == NULL) {
        return NULL;
    }
    wire = PyBytes_FromStringAndSize((const char *)ptr, (Py_ssize_t)len);
    if (wire == NULL) {
        return NULL;
    }
    result = PyObject_CallOneArg(callable, wire);
    Py_DECREF(wire);
    return result;
}

static int boltffi_python_encode_wire_codec(const char *key, PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    PyObject *callable = boltffi_python_wire_codec(key);
    PyObject *wire = NULL;
    if (callable == NULL) {
        return 0;
    }
    wire = PyObject_CallOneArg(callable, value);
    if (wire == NULL) {
        return 0;
    }
    if (!PyBytes_Check(wire)) {
        Py_DECREF(wire);
        PyErr_SetString(PyExc_TypeError, "wire codec must return bytes");
        return 0;
    }
    *out_wire = wire;
    *out_ptr = (const uint8_t *)PyBytes_AS_STRING(wire);
    *out_len = (uintptr_t)PyBytes_GET_SIZE(wire);
    return 1;
}

{% for decoder in codec_decoders %}
static PyObject *{{ decoder.function() }}(const uint8_t *ptr, uintptr_t len) {
    return boltffi_python_decode_wire_codec({{ decoder.key() }}, ptr, len);
}
{% endfor %}
{% for encoder in codec_encoders %}
static int {{ encoder.function() }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    return boltffi_python_encode_wire_codec({{ encoder.key() }}, value, out_wire, out_ptr, out_len);
}
{% endfor %}
{% endif %}
{% if support.uses_owned_buffers() %}
static uint16_t boltffi_python_read_u16_le(const uint8_t *buffer) {
    return ((uint16_t)buffer[0])
        | ((uint16_t)buffer[1] << 8);
}

static uint32_t boltffi_python_read_u32_le(const uint8_t *buffer) {
    return ((uint32_t)buffer[0])
        | ((uint32_t)buffer[1] << 8)
        | ((uint32_t)buffer[2] << 16)
        | ((uint32_t)buffer[3] << 24);
}

static uint64_t boltffi_python_read_u64_le(const uint8_t *buffer) {
    return ((uint64_t)buffer[0])
        | ((uint64_t)buffer[1] << 8)
        | ((uint64_t)buffer[2] << 16)
        | ((uint64_t)buffer[3] << 24)
        | ((uint64_t)buffer[4] << 32)
        | ((uint64_t)buffer[5] << 40)
        | ((uint64_t)buffer[6] << 48)
        | ((uint64_t)buffer[7] << 56);
}

typedef struct {
    const uint8_t *ptr;
    uintptr_t len;
    uintptr_t offset;
} boltffi_python_wire_reader;

typedef struct {
    uint8_t *ptr;
    uintptr_t len;
    uintptr_t offset;
} boltffi_python_wire_writer;

static int boltffi_python_wire_add(uintptr_t *size, uintptr_t amount) {
    if (amount > (uintptr_t)PY_SSIZE_T_MAX - *size) {
        PyErr_SetString(PyExc_OverflowError, "wire payload is too large");
        return 0;
    }
    *size += amount;
    return 1;
}

static int boltffi_python_wire_reader_read(boltffi_python_wire_reader *reader, uintptr_t count, const uint8_t **out) {
    if (count > reader->len - reader->offset) {
        PyErr_SetString(PyExc_ValueError, "truncated BoltFFI wire bytes");
        return 0;
    }
    *out = reader->ptr + reader->offset;
    reader->offset += count;
    return 1;
}

static int boltffi_python_wire_reader_u8(boltffi_python_wire_reader *reader, uint8_t *out) {
    const uint8_t *bytes = NULL;
    if (!boltffi_python_wire_reader_read(reader, 1, &bytes)) {
        return 0;
    }
    *out = bytes[0];
    return 1;
}

static int boltffi_python_wire_reader_u16(boltffi_python_wire_reader *reader, uint16_t *out) {
    const uint8_t *bytes = NULL;
    if (!boltffi_python_wire_reader_read(reader, 2, &bytes)) {
        return 0;
    }
    *out = boltffi_python_read_u16_le(bytes);
    return 1;
}

static int boltffi_python_wire_reader_u32(boltffi_python_wire_reader *reader, uint32_t *out) {
    const uint8_t *bytes = NULL;
    if (!boltffi_python_wire_reader_read(reader, 4, &bytes)) {
        return 0;
    }
    *out = boltffi_python_read_u32_le(bytes);
    return 1;
}

static int boltffi_python_wire_reader_u64(boltffi_python_wire_reader *reader, uint64_t *out) {
    const uint8_t *bytes = NULL;
    if (!boltffi_python_wire_reader_read(reader, 8, &bytes)) {
        return 0;
    }
    *out = boltffi_python_read_u64_le(bytes);
    return 1;
}

static int boltffi_python_wire_writer_write(boltffi_python_wire_writer *writer, const uint8_t *payload, uintptr_t len) {
    if (len > writer->len - writer->offset) {
        PyErr_SetString(PyExc_RuntimeError, "wire writer overflow");
        return 0;
    }
    if (len != 0) {
        memcpy(writer->ptr + writer->offset, payload, (size_t)len);
    }
    writer->offset += len;
    return 1;
}

static int boltffi_python_wire_writer_u8(boltffi_python_wire_writer *writer, uint8_t value) {
    return boltffi_python_wire_writer_write(writer, &value, 1);
}

static int boltffi_python_wire_writer_u16(boltffi_python_wire_writer *writer, uint16_t value) {
    uint8_t bytes[2];
    boltffi_python_write_u16_le(bytes, value);
    return boltffi_python_wire_writer_write(writer, bytes, 2);
}

static int boltffi_python_wire_writer_u32(boltffi_python_wire_writer *writer, uint32_t value) {
    uint8_t bytes[4];
    boltffi_python_write_u32_le(bytes, value);
    return boltffi_python_wire_writer_write(writer, bytes, 4);
}

static int boltffi_python_wire_writer_u64(boltffi_python_wire_writer *writer, uint64_t value) {
    uint8_t bytes[8];
    boltffi_python_write_u64_le(bytes, value);
    return boltffi_python_wire_writer_write(writer, bytes, 8);
}

static int boltffi_python_validate_owned_memory(FfiBuf_u8 buffer) {
    if (buffer.ptr == NULL && buffer.len != 0) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned invalid buffer");
        return 0;
    }
    if (buffer.len > PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native buffer is too large");
        return 0;
    }
    return 1;
}

static int boltffi_python_validate_owned_buffer(FfiBuf_u8 buffer) {
    if (!boltffi_python_validate_owned_memory(buffer)) {
        return 0;
    }
    if (buffer.len < 4) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned truncated wire buffer");
        return 0;
    }
    return 1;
}

static int boltffi_python_validate_owned_fixed_buffer(FfiBuf_u8 buffer, uintptr_t expected_len) {
    if (!boltffi_python_validate_owned_memory(buffer)) {
        return 0;
    }
    if (buffer.len != expected_len) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned wrong fixed wire size");
        return 0;
    }
    return 1;
}

static void boltffi_python_release_owned_buffer(FfiBuf_u8 buffer) {
    {{ support.free_buffer() }}(buffer);
}
{% endif %}
{% if support.uses_callback_handles() %}
typedef struct {
    void (*free)(uint64_t);
    uint64_t (*clone)(uint64_t);
} boltffi_python_callback_vtable_header;

static void boltffi_python_release_callback_handle_value(BoltFFICallbackHandle handle) {
    const boltffi_python_callback_vtable_header *vtable = (const boltffi_python_callback_vtable_header *)handle.vtable;
    if (vtable != NULL && vtable->free != NULL) {
        vtable->free(handle.handle);
    }
}

static void boltffi_python_release_callback_capsule(PyObject *capsule) {
    BoltFFICallbackHandle *handle = PyCapsule_GetPointer(capsule, "boltffi.callback");
    if (handle == NULL) {
        PyErr_Clear();
        return;
    }
    boltffi_python_release_callback_handle_value(*handle);
    PyMem_Free(handle);
}

static PyObject *boltffi_python_box_callback_handle(BoltFFICallbackHandle handle) {
    BoltFFICallbackHandle *stored = PyMem_Malloc(sizeof(BoltFFICallbackHandle));
    PyObject *capsule = NULL;
    if (stored == NULL) {
        boltffi_python_release_callback_handle_value(handle);
        return PyErr_NoMemory();
    }
    *stored = handle;
    capsule = PyCapsule_New(stored, "boltffi.callback", boltffi_python_release_callback_capsule);
    if (capsule == NULL) {
        boltffi_python_release_callback_handle_value(*stored);
        PyMem_Free(stored);
    }
    return capsule;
}
{% endif %}
{% if support.uses_owned_utf8() %}
static PyObject *boltffi_python_decode_owned_utf8(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_buffer(buffer)) {
        goto done;
    }
    uint32_t len = boltffi_python_read_u32_le(buffer.ptr);
    if ((uintptr_t)len > buffer.len - 4) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned truncated string buffer");
        goto done;
    }
    if (len > (uint32_t)PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native string is too large");
        goto done;
    }
    result = PyUnicode_FromStringAndSize((const char *)(buffer.ptr + 4), (Py_ssize_t)len);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if support.uses_owned_bytes() %}
static PyObject *boltffi_python_decode_owned_bytes(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_buffer(buffer)) {
        goto done;
    }
    uint32_t len = boltffi_python_read_u32_le(buffer.ptr);
    if ((uintptr_t)len > buffer.len - 4) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned truncated bytes buffer");
        goto done;
    }
    if (len > (uint32_t)PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native bytes are too large");
        goto done;
    }
    result = PyBytes_FromStringAndSize((const char *)(buffer.ptr + 4), (Py_ssize_t)len);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if support.uses_owned_raw_wire() %}
static PyObject *boltffi_python_decode_owned_raw_wire(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_memory(buffer)) {
        goto done;
    }
    result = PyBytes_FromStringAndSize((const char *)buffer.ptr, (Py_ssize_t)buffer.len);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if support.uses_async_functions() %}
static PyObject *boltffi_python_box_future_handle(RustFutureHandle handle) {
    if (handle == NULL) {
        PyErr_SetString(PyExc_RuntimeError, "native future handle is null");
        return NULL;
    }
    return PyLong_FromVoidPtr((void *)handle);
}

static int boltffi_python_parse_future_handle(PyObject *value, RustFutureHandle *out) {
    void *handle = PyLong_AsVoidPtr(value);
    if (handle == NULL && PyErr_Occurred()) {
        return 0;
    }
    if (handle == NULL) {
        PyErr_SetString(PyExc_RuntimeError, "native future handle is null");
        return 0;
    }
    *out = (RustFutureHandle)handle;
    return 1;
}

static void boltffi_python_future_wake(uint64_t callback_data, int8_t poll_result) {
    PyGILState_STATE gil = PyGILState_Ensure();
    PyObject *state = (PyObject *)(uintptr_t)callback_data;
    PyObject *loop = PyTuple_GET_ITEM(state, 0);
    PyObject *future = PyTuple_GET_ITEM(state, 1);
    PyObject *set_result = PyObject_GetAttrString(future, "set_result");
    PyObject *result = PyLong_FromLong((long)poll_result);
    PyObject *scheduled = NULL;
    if (set_result != NULL && result != NULL) {
        scheduled = PyObject_CallMethod(loop, "call_soon_threadsafe", "OO", set_result, result);
    }
    if (scheduled == NULL) {
        PyErr_WriteUnraisable(state);
    }
    Py_XDECREF(scheduled);
    Py_XDECREF(result);
    Py_XDECREF(set_result);
    Py_DECREF(state);
    PyGILState_Release(gil);
}

static int boltffi_python_check_future_status(
    FfiStatus status,
    RustFutureHandle handle,
    FfiBuf_u8 (*panic_message)(RustFutureHandle)
) {
    PyObject *message = NULL;
    if (status.code == 0) {
        return 1;
    }
    if (status.code == 100 && panic_message != NULL) {
        message = boltffi_python_decode_owned_utf8(panic_message(handle));
        if (message != NULL) {
            PyErr_SetObject(PyExc_RuntimeError, message);
            Py_DECREF(message);
            return 0;
        }
        PyErr_Clear();
    }
    PyErr_Format(PyExc_RuntimeError, "native future failed with status %d", status.code);
    return 0;
}
{% endif %}
{% if support.uses_registered_types() %}
static int boltffi_python_validate_registered_type_object(PyObject *type_object, const char *type_name) {
    if (!PyType_Check(type_object)) {
        PyErr_Format(PyExc_TypeError, "expected type for %s", type_name);
        return 0;
    }
    return 1;
}

static int boltffi_python_store_registered_type(PyObject **type_slot, PyObject *type_object, const char *type_name) {
    if (!boltffi_python_validate_registered_type_object(type_object, type_name)) {
        return 0;
    }
    Py_INCREF(type_object);
    Py_XDECREF(*type_slot);
    *type_slot = type_object;
    return 1;
}

static int boltffi_python_expect_registered_type(PyObject *type_object, const char *type_name) {
    if (type_object != NULL) {
        return 1;
    }
    PyErr_Format(PyExc_ImportError, "native type %s is not registered", type_name);
    return 0;
}

static int boltffi_python_expect_type_instance(PyObject *value, PyObject *type_object, const char *type_name) {
    int is_instance = 0;
    if (!boltffi_python_expect_registered_type(type_object, type_name)) {
        return 0;
    }
    is_instance = PyObject_IsInstance(value, type_object);
    if (is_instance < 0) {
        return 0;
    }
    if (is_instance == 0) {
        PyErr_Format(PyExc_TypeError, "expected %s", type_name);
        return 0;
    }
    return 1;
}

static PyObject *boltffi_python_get_record_field(PyObject *value, const char *record_name, const char *field_name) {
    PyObject *field_value = PyObject_GetAttrString(value, field_name);
    if (field_value == NULL && PyErr_ExceptionMatches(PyExc_AttributeError)) {
        PyErr_Clear();
        PyErr_Format(PyExc_TypeError, "%s is missing field %s", record_name, field_name);
    }
    return field_value;
}

static PyObject *boltffi_python_box_registered_record(PyObject *type_object, PyObject *constructor_args, const char *record_name) {
    PyObject *record_value = NULL;
    if (constructor_args == NULL) {
        return NULL;
    }
    if (!boltffi_python_expect_registered_type(type_object, record_name)) {
        Py_DECREF(constructor_args);
        return NULL;
    }
    record_value = PyObject_CallObject(type_object, constructor_args);
    Py_DECREF(constructor_args);
    return record_value;
}
{% endif %}
{% if support.uses_c_style_enums() %}
typedef PyObject *(*boltffi_python_load_c_style_enum_member_fn)(PyObject *, Py_ssize_t);

typedef struct boltffi_python_c_style_enum_registration {
    PyObject *type_object;
    Py_ssize_t member_count;
    PyObject **members_by_wire_tag;
} boltffi_python_c_style_enum_registration;

static void boltffi_python_release_registered_enum_members(PyObject **members_by_wire_tag, Py_ssize_t member_count) {
    Py_ssize_t member_index = 0;
    for (member_index = 0; member_index < member_count; member_index += 1) {
        Py_XDECREF(members_by_wire_tag[member_index]);
        members_by_wire_tag[member_index] = NULL;
    }
}

static void boltffi_python_clear_c_style_enum_registration(
    boltffi_python_c_style_enum_registration *registration
) {
    Py_XDECREF(registration->type_object);
    registration->type_object = NULL;
    boltffi_python_release_registered_enum_members(
        registration->members_by_wire_tag,
        registration->member_count
    );
}

static PyObject *boltffi_python_load_c_style_enum_member(
    PyObject *type_object,
    const char *enum_name,
    const char *member_name,
    PyObject *native_value
) {
    PyObject *named_member = NULL;
    PyObject *resolved_member = NULL;
    if (native_value == NULL) {
        return NULL;
    }
    named_member = PyObject_GetAttrString(type_object, member_name);
    if (named_member == NULL) {
        return NULL;
    }
    resolved_member = PyObject_CallOneArg(type_object, native_value);
    if (resolved_member == NULL) {
        Py_DECREF(named_member);
        return NULL;
    }
    if (named_member != resolved_member) {
        PyErr_Format(PyExc_ValueError, "native enum %s member %s has the wrong value", enum_name, member_name);
        Py_DECREF(named_member);
        Py_DECREF(resolved_member);
        return NULL;
    }
    Py_DECREF(resolved_member);
    return named_member;
}

static int boltffi_python_store_c_style_enum_registration(
    boltffi_python_c_style_enum_registration *registration,
    PyObject *type_object,
    const char *enum_name,
    boltffi_python_load_c_style_enum_member_fn load_member
) {
    PyObject **loaded_members = NULL;
    Py_ssize_t member_index = 0;
    if (!boltffi_python_validate_registered_type_object(type_object, enum_name)) {
        return 0;
    }
    loaded_members = PyMem_Calloc((size_t)registration->member_count, sizeof(PyObject *));
    if (loaded_members == NULL) {
        PyErr_NoMemory();
        return 0;
    }
    for (member_index = 0; member_index < registration->member_count; member_index += 1) {
        loaded_members[member_index] = load_member(type_object, member_index);
        if (loaded_members[member_index] == NULL) {
            boltffi_python_release_registered_enum_members(loaded_members, registration->member_count);
            PyMem_Free(loaded_members);
            return 0;
        }
    }
    boltffi_python_clear_c_style_enum_registration(registration);
    if (!boltffi_python_store_registered_type(&registration->type_object, type_object, enum_name)) {
        boltffi_python_release_registered_enum_members(loaded_members, registration->member_count);
        PyMem_Free(loaded_members);
        return 0;
    }
    for (member_index = 0; member_index < registration->member_count; member_index += 1) {
        registration->members_by_wire_tag[member_index] = loaded_members[member_index];
    }
    PyMem_Free(loaded_members);
    return 1;
}

static int boltffi_python_expect_enum_instance(
    PyObject *value,
    const boltffi_python_c_style_enum_registration *registration,
    const char *enum_name
) {
    return boltffi_python_expect_type_instance(value, registration->type_object, enum_name);
}

static PyObject *boltffi_python_box_registered_enum_member(
    const boltffi_python_c_style_enum_registration *registration,
    Py_ssize_t member_index,
    const char *enum_name
) {
    PyObject *member = NULL;
    if (!boltffi_python_expect_registered_type(registration->type_object, enum_name)) {
        return NULL;
    }
    if (member_index < 0 || member_index >= registration->member_count) {
        PyErr_SetString(PyExc_RuntimeError, "native enum member index is invalid");
        return NULL;
    }
    if (registration->members_by_wire_tag[member_index] == NULL) {
        PyErr_Format(PyExc_ImportError, "native enum %s member cache is not initialized", enum_name);
        return NULL;
    }
    member = registration->members_by_wire_tag[member_index];
    Py_INCREF(member);
    return member;
}
{% endif %}
{% for primitive in support.primitives() %}
{% if primitive.is_bool() %}
static int {{ primitive.parser }}(PyObject *value, bool *out) {
    if (!PyBool_Check(value)) {
        PyErr_SetString(PyExc_TypeError, "expected bool");
        return 0;
    }
    *out = value == Py_True;
    return 1;
}

static PyObject *{{ primitive.boxer }}(bool value) {
    return PyBool_FromLong(value ? 1 : 0);
}
{% endif %}
{% if primitive.is_i8() %}
static int {{ primitive.parser }}(PyObject *value, int8_t *out) {
    long long parsed = PyLong_AsLongLong(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed < INT8_MIN || parsed > INT8_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected i8");
        return 0;
    }
    *out = (int8_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(int8_t value) {
    return PyLong_FromLong((long)value);
}
{% endif %}
{% if primitive.is_u8() %}
static int {{ primitive.parser }}(PyObject *value, uint8_t *out) {
    unsigned long long parsed = PyLong_AsUnsignedLongLong(value);
    if (parsed == (unsigned long long)-1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed > UINT8_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected u8");
        return 0;
    }
    *out = (uint8_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uint8_t value) {
    return PyLong_FromUnsignedLong((unsigned long)value);
}
{% endif %}
{% if primitive.is_i16() %}
static int {{ primitive.parser }}(PyObject *value, int16_t *out) {
    long long parsed = PyLong_AsLongLong(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed < INT16_MIN || parsed > INT16_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected i16");
        return 0;
    }
    *out = (int16_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(int16_t value) {
    return PyLong_FromLong((long)value);
}
{% endif %}
{% if primitive.is_u16() %}
static int {{ primitive.parser }}(PyObject *value, uint16_t *out) {
    unsigned long long parsed = PyLong_AsUnsignedLongLong(value);
    if (parsed == (unsigned long long)-1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed > UINT16_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected u16");
        return 0;
    }
    *out = (uint16_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uint16_t value) {
    return PyLong_FromUnsignedLong((unsigned long)value);
}
{% endif %}
{% if primitive.is_i32() %}
static int {{ primitive.parser }}(PyObject *value, int32_t *out) {
    long long parsed = PyLong_AsLongLong(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed < INT32_MIN || parsed > INT32_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected i32");
        return 0;
    }
    *out = (int32_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(int32_t value) {
    return PyLong_FromLong((long)value);
}
{% endif %}
{% if primitive.is_u32() %}
static int {{ primitive.parser }}(PyObject *value, uint32_t *out) {
    unsigned long long parsed = PyLong_AsUnsignedLongLong(value);
    if (parsed == (unsigned long long)-1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed > UINT32_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected u32");
        return 0;
    }
    *out = (uint32_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uint32_t value) {
    return PyLong_FromUnsignedLong((unsigned long)value);
}
{% endif %}
{% if primitive.is_i64() %}
static int {{ primitive.parser }}(PyObject *value, int64_t *out) {
    long long parsed = PyLong_AsLongLong(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    *out = (int64_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(int64_t value) {
    return PyLong_FromLongLong((long long)value);
}
{% endif %}
{% if primitive.is_u64() %}
static int {{ primitive.parser }}(PyObject *value, uint64_t *out) {
    unsigned long long parsed = PyLong_AsUnsignedLongLong(value);
    if (parsed == (unsigned long long)-1 && PyErr_Occurred()) {
        return 0;
    }
    *out = (uint64_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uint64_t value) {
    return PyLong_FromUnsignedLongLong((unsigned long long)value);
}
{% endif %}
{% if primitive.is_isize() %}
static int {{ primitive.parser }}(PyObject *value, intptr_t *out) {
    Py_ssize_t parsed = PyLong_AsSsize_t(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    *out = (intptr_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(intptr_t value) {
    return PyLong_FromSsize_t((Py_ssize_t)value);
}
{% endif %}
{% if primitive.is_usize() %}
static int {{ primitive.parser }}(PyObject *value, uintptr_t *out) {
    size_t parsed = PyLong_AsSize_t(value);
    if (parsed == (size_t)-1 && PyErr_Occurred()) {
        return 0;
    }
    *out = (uintptr_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uintptr_t value) {
    return PyLong_FromSize_t((size_t)value);
}
{% endif %}
{% if primitive.is_f32() %}
static int {{ primitive.parser }}(PyObject *value, float *out) {
    double parsed = PyFloat_AsDouble(value);
    if (parsed == -1.0 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed < -FLT_MAX || parsed > FLT_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected f32");
        return 0;
    }
    *out = (float)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(float value) {
    return PyFloat_FromDouble((double)value);
}
{% endif %}
{% if primitive.is_f64() %}
static int {{ primitive.parser }}(PyObject *value, double *out) {
    double parsed = PyFloat_AsDouble(value);
    if (parsed == -1.0 && PyErr_Occurred()) {
        return 0;
    }
    *out = parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(double value) {
    return PyFloat_FromDouble(value);
}
{% endif %}
{% endfor %}
{% for primitive in support.wire_primitives() %}
{% if primitive.is_bool() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    bool parsed = false;
    uint8_t bytes[1];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    bytes[0] = parsed ? 1 : 0;
    return boltffi_python_wire_fixed(bytes, 1, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_i8() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    int8_t parsed = 0;
    uint8_t bytes[1];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    bytes[0] = (uint8_t)parsed;
    return boltffi_python_wire_fixed(bytes, 1, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_u8() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    uint8_t parsed = 0;
    uint8_t bytes[1];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    bytes[0] = parsed;
    return boltffi_python_wire_fixed(bytes, 1, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_i16() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    int16_t parsed = 0;
    uint8_t bytes[2];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    boltffi_python_write_u16_le(bytes, (uint16_t)parsed);
    return boltffi_python_wire_fixed(bytes, 2, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_u16() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    uint16_t parsed = 0;
    uint8_t bytes[2];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    boltffi_python_write_u16_le(bytes, parsed);
    return boltffi_python_wire_fixed(bytes, 2, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_i32() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    int32_t parsed = 0;
    uint8_t bytes[4];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    boltffi_python_write_u32_le(bytes, (uint32_t)parsed);
    return boltffi_python_wire_fixed(bytes, 4, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_u32() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    uint32_t parsed = 0;
    uint8_t bytes[4];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    boltffi_python_write_u32_le(bytes, parsed);
    return boltffi_python_wire_fixed(bytes, 4, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_i64() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    int64_t parsed = 0;
    uint8_t bytes[8];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    boltffi_python_write_u64_le(bytes, (uint64_t)parsed);
    return boltffi_python_wire_fixed(bytes, 8, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_u64() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    uint64_t parsed = 0;
    uint8_t bytes[8];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    boltffi_python_write_u64_le(bytes, parsed);
    return boltffi_python_wire_fixed(bytes, 8, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_isize() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    intptr_t parsed = 0;
    uint8_t bytes[8];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    boltffi_python_write_u64_le(bytes, (uint64_t)((int64_t)parsed));
    return boltffi_python_wire_fixed(bytes, 8, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_usize() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    uintptr_t parsed = 0;
    uint8_t bytes[8];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    boltffi_python_write_u64_le(bytes, (uint64_t)parsed);
    return boltffi_python_wire_fixed(bytes, 8, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_f32() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    float parsed = 0.0f;
    uint32_t bits = 0;
    uint8_t bytes[4];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    memcpy(&bits, &parsed, sizeof(bits));
    boltffi_python_write_u32_le(bytes, bits);
    return boltffi_python_wire_fixed(bytes, 4, out_wire, out_ptr, out_len);
}
{% endif %}
{% if primitive.is_f64() %}
static int {{ primitive.wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    double parsed = 0.0;
    uint64_t bits = 0;
    uint8_t bytes[8];
    if (!{{ primitive.parser }}(value, &parsed)) {
        return 0;
    }
    memcpy(&bits, &parsed, sizeof(bits));
    boltffi_python_write_u64_le(bytes, bits);
    return boltffi_python_wire_fixed(bytes, 8, out_wire, out_ptr, out_len);
}
{% endif %}
static int {{ primitive.optional_wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    PyObject *payload = NULL;
    const uint8_t *payload_ptr = NULL;
    uintptr_t payload_len = 0;
    PyObject *wire = NULL;
    if (value == Py_None) {
        uint8_t tag[1] = {0};
        return boltffi_python_wire_fixed(tag, 1, out_wire, out_ptr, out_len);
    }
    if (!{{ primitive.wire_encoder }}(value, &payload, &payload_ptr, &payload_len)) {
        return 0;
    }
    if (payload_len > (uintptr_t)PY_SSIZE_T_MAX - 1) {
        Py_DECREF(payload);
        PyErr_SetString(PyExc_OverflowError, "optional scalar payload is too large");
        return 0;
    }
    wire = PyBytes_FromStringAndSize(NULL, (Py_ssize_t)(payload_len + 1));
    if (wire == NULL) {
        Py_DECREF(payload);
        return 0;
    }
    uint8_t *bytes = (uint8_t *)PyBytes_AS_STRING(wire);
    bytes[0] = 1;
    if (payload_len > 0) {
        memcpy(bytes + 1, payload_ptr, (size_t)payload_len);
    }
    Py_DECREF(payload);
    *out_wire = wire;
    *out_ptr = bytes;
    *out_len = payload_len + 1;
    return 1;
}
{% endfor %}
{% for primitive in support.owned_primitives() %}
{% if primitive.is_bool() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 1)) {
        goto done;
    }
    if (buffer.ptr[0] > 1) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned invalid bool wire value");
        goto done;
    }
    result = {{ primitive.boxer }}(buffer.ptr[0] == 1);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_i8() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 1)) {
        goto done;
    }
    result = {{ primitive.boxer }}((int8_t)buffer.ptr[0]);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_u8() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 1)) {
        goto done;
    }
    result = {{ primitive.boxer }}(buffer.ptr[0]);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_i16() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 2)) {
        goto done;
    }
    result = {{ primitive.boxer }}((int16_t)boltffi_python_read_u16_le(buffer.ptr));
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_u16() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 2)) {
        goto done;
    }
    result = {{ primitive.boxer }}(boltffi_python_read_u16_le(buffer.ptr));
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_i32() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 4)) {
        goto done;
    }
    result = {{ primitive.boxer }}((int32_t)boltffi_python_read_u32_le(buffer.ptr));
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_u32() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 4)) {
        goto done;
    }
    result = {{ primitive.boxer }}(boltffi_python_read_u32_le(buffer.ptr));
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_i64() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 8)) {
        goto done;
    }
    result = {{ primitive.boxer }}((int64_t)boltffi_python_read_u64_le(buffer.ptr));
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_u64() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 8)) {
        goto done;
    }
    result = {{ primitive.boxer }}(boltffi_python_read_u64_le(buffer.ptr));
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_isize() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 8)) {
        goto done;
    }
    result = {{ primitive.boxer }}((intptr_t)((int64_t)boltffi_python_read_u64_le(buffer.ptr)));
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_usize() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 8)) {
        goto done;
    }
    result = {{ primitive.boxer }}((uintptr_t)boltffi_python_read_u64_le(buffer.ptr));
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_f32() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    uint32_t bits = 0;
    float value = 0.0f;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 4)) {
        goto done;
    }
    bits = boltffi_python_read_u32_le(buffer.ptr);
    memcpy(&value, &bits, sizeof(value));
    result = {{ primitive.boxer }}(value);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if primitive.is_f64() %}
static PyObject *{{ primitive.owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    uint64_t bits = 0;
    double value = 0.0;
    if (!boltffi_python_validate_owned_fixed_buffer(buffer, 8)) {
        goto done;
    }
    bits = boltffi_python_read_u64_le(buffer.ptr);
    memcpy(&value, &bits, sizeof(value));
    result = {{ primitive.boxer }}(value);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
static PyObject *{{ primitive.optional_owned_wire_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
{% if primitive.is_f32() %}
    uint32_t bits = 0;
    float value = 0.0f;
{% endif %}
{% if primitive.is_f64() %}
    uint64_t bits = 0;
    double value = 0.0;
{% endif %}
    if (!boltffi_python_validate_owned_memory(buffer)) {
        goto done;
    }
    if (buffer.len < 1) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned truncated optional scalar");
        goto done;
    }
    if (buffer.ptr[0] == 0) {
        if (buffer.len != 1) {
            PyErr_SetString(PyExc_RuntimeError, "native function returned invalid none scalar payload");
            goto done;
        }
        Py_INCREF(Py_None);
        result = Py_None;
        goto done;
    }
    if (buffer.ptr[0] != 1) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned invalid optional scalar tag");
        goto done;
    }
    if (buffer.len != 1 + {{ primitive.wire_size }}) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned invalid optional scalar payload");
        goto done;
    }
{% if primitive.is_bool() %}
    if (buffer.ptr[1] > 1) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned invalid bool wire value");
        goto done;
    }
    result = {{ primitive.boxer }}(buffer.ptr[1] == 1);
{% endif %}
{% if primitive.is_i8() %}
    result = {{ primitive.boxer }}((int8_t)buffer.ptr[1]);
{% endif %}
{% if primitive.is_u8() %}
    result = {{ primitive.boxer }}(buffer.ptr[1]);
{% endif %}
{% if primitive.is_i16() %}
    result = {{ primitive.boxer }}((int16_t)boltffi_python_read_u16_le(buffer.ptr + 1));
{% endif %}
{% if primitive.is_u16() %}
    result = {{ primitive.boxer }}(boltffi_python_read_u16_le(buffer.ptr + 1));
{% endif %}
{% if primitive.is_i32() %}
    result = {{ primitive.boxer }}((int32_t)boltffi_python_read_u32_le(buffer.ptr + 1));
{% endif %}
{% if primitive.is_u32() %}
    result = {{ primitive.boxer }}(boltffi_python_read_u32_le(buffer.ptr + 1));
{% endif %}
{% if primitive.is_i64() %}
    result = {{ primitive.boxer }}((int64_t)boltffi_python_read_u64_le(buffer.ptr + 1));
{% endif %}
{% if primitive.is_u64() %}
    result = {{ primitive.boxer }}(boltffi_python_read_u64_le(buffer.ptr + 1));
{% endif %}
{% if primitive.is_isize() %}
    result = {{ primitive.boxer }}((intptr_t)((int64_t)boltffi_python_read_u64_le(buffer.ptr + 1)));
{% endif %}
{% if primitive.is_usize() %}
    result = {{ primitive.boxer }}((uintptr_t)boltffi_python_read_u64_le(buffer.ptr + 1));
{% endif %}
{% if primitive.is_f32() %}
    bits = boltffi_python_read_u32_le(buffer.ptr + 1);
    memcpy(&value, &bits, sizeof(value));
    result = {{ primitive.boxer }}(value);
{% endif %}
{% if primitive.is_f64() %}
    bits = boltffi_python_read_u64_le(buffer.ptr + 1);
    memcpy(&value, &bits, sizeof(value));
    result = {{ primitive.boxer }}(value);
{% endif %}
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endfor %}
{% for element in support.direct_vector_elements() %}
static int {{ element.parser() }}(PyObject *value, {{ element.c_type() }} *out);
static PyObject *{{ element.boxer() }}({{ element.c_type() }} value);
static int {{ element.vector_parser() }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    PyObject *sequence = NULL;
    Py_ssize_t item_count = 0;
    Py_ssize_t index = 0;
    {{ element.c_type() }} *values = NULL;
    sequence = PySequence_Fast(value, "expected sequence");
    if (sequence == NULL) {
        return 0;
    }
    item_count = PySequence_Fast_GET_SIZE(sequence);
    if (item_count > PY_SSIZE_T_MAX / (Py_ssize_t)sizeof({{ element.c_type() }})) {
        Py_DECREF(sequence);
        PyErr_SetString(PyExc_OverflowError, "sequence is too large");
        return 0;
    }
    *out_wire = PyBytes_FromStringAndSize(NULL, item_count * (Py_ssize_t)sizeof({{ element.c_type() }}));
    if (*out_wire == NULL) {
        Py_DECREF(sequence);
        return 0;
    }
    values = ({{ element.c_type() }} *)PyBytes_AS_STRING(*out_wire);
    for (index = 0; index < item_count; index += 1) {
        if (!{{ element.parser() }}(PySequence_Fast_GET_ITEM(sequence, index), &values[index])) {
            Py_DECREF(sequence);
            Py_CLEAR(*out_wire);
            return 0;
        }
    }
    Py_DECREF(sequence);
    *out_ptr = (const uint8_t *)values;
    *out_len = (uintptr_t)item_count;
    return 1;
}

static int {{ element.vector_encoder() }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    uintptr_t item_count = 0;
    if (!{{ element.vector_parser() }}(value, out_wire, out_ptr, &item_count)) {
        return 0;
    }
    if (item_count > UINTPTR_MAX / sizeof({{ element.c_type() }})) {
        Py_CLEAR(*out_wire);
        PyErr_SetString(PyExc_OverflowError, "sequence byte length is too large");
        return 0;
    }
    *out_len = item_count * sizeof({{ element.c_type() }});
    return 1;
}

static PyObject *{{ element.vector_boxer() }}(const {{ element.c_type() }} *values, uintptr_t len) {
    PyObject *result = NULL;
    PyObject *item = NULL;
    Py_ssize_t item_count = 0;
    Py_ssize_t index = 0;
    if (values == NULL && len != 0) {
        PyErr_SetString(PyExc_RuntimeError, "native vector pointer is invalid");
        return NULL;
    }
    if (len > (uintptr_t)PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native vector is too large");
        return NULL;
    }
    item_count = (Py_ssize_t)len;
    result = PyList_New(item_count);
    if (result == NULL) {
        return NULL;
    }
    for (index = 0; index < item_count; index += 1) {
        item = {{ element.boxer() }}(values[index]);
        if (item == NULL) {
            Py_CLEAR(result);
            return NULL;
        }
        PyList_SET_ITEM(result, index, item);
        item = NULL;
    }
    return result;
}

static PyObject *{{ element.vector_decoder() }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_memory(buffer)) {
        goto done;
    }
    if (buffer.len % sizeof({{ element.c_type() }}) != 0) {
        PyErr_SetString(PyExc_RuntimeError, "native vector buffer byte length is invalid");
        goto done;
    }
    result = {{ element.vector_boxer() }}(
        (const {{ element.c_type() }} *)buffer.ptr,
        buffer.len / sizeof({{ element.c_type() }})
    );
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endfor %}
{% if support.uses_native_record_types() %}
static int boltffi_python_raise_frozen_field(PyObject *name) {
    PyObject *dataclasses = PyImport_ImportModule("dataclasses");
    PyObject *frozen_error = dataclasses == NULL
        ? NULL
        : PyObject_GetAttrString(dataclasses, "FrozenInstanceError");
    Py_XDECREF(dataclasses);
    if (frozen_error == NULL) {
        PyErr_Clear();
    }
    PyErr_Format(frozen_error == NULL ? PyExc_AttributeError : frozen_error, "cannot assign to field %R", name);
    Py_XDECREF(frozen_error);
    return -1;
}
{% endif %}
{% for record in records %}
{{ record }}
{% endfor %}
{% for sequence in support.native_sequences() %}
static int {{ sequence.encoder() }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    typedef struct {
        PyObject *wire;
        const uint8_t *ptr;
        uintptr_t len;
    } item_wire;
    PyObject *sequence = NULL;
    item_wire *items = NULL;
    PyObject *wire = NULL;
    boltffi_python_wire_writer writer;
    uintptr_t wire_len = 4;
    Py_ssize_t item_count = 0;
    Py_ssize_t index = 0;
    int ok = 0;
    sequence = PySequence_Fast(value, "expected sequence");
    if (sequence == NULL) {
        return 0;
    }
    item_count = PySequence_Fast_GET_SIZE(sequence);
    if (item_count > UINT32_MAX) {
        PyErr_SetString(PyExc_OverflowError, "sequence too large to encode");
        goto done;
    }
    if (item_count > 0) {
        items = PyMem_Calloc((size_t)item_count, sizeof(item_wire));
        if (items == NULL) {
            PyErr_NoMemory();
            goto done;
        }
    }
    for (index = 0; index < item_count; index += 1) {
        if (!{{ sequence.item_encoder() }}(PySequence_Fast_GET_ITEM(sequence, index), &items[index].wire, &items[index].ptr, &items[index].len)) {
            goto done;
        }
        if (!boltffi_python_wire_add(&wire_len, items[index].len)) {
            goto done;
        }
    }
    wire = PyBytes_FromStringAndSize(NULL, (Py_ssize_t)wire_len);
    if (wire == NULL) {
        goto done;
    }
    writer.ptr = (uint8_t *)PyBytes_AS_STRING(wire);
    writer.len = wire_len;
    writer.offset = 0;
    if (!boltffi_python_wire_writer_u32(&writer, (uint32_t)item_count)) {
        goto done;
    }
    for (index = 0; index < item_count; index += 1) {
        if (!boltffi_python_wire_writer_write(&writer, items[index].ptr, items[index].len)) {
            goto done;
        }
    }
    *out_wire = wire;
    *out_ptr = (const uint8_t *)PyBytes_AS_STRING(wire);
    *out_len = wire_len;
    wire = NULL;
    ok = 1;
done:
    Py_XDECREF(wire);
    if (items != NULL) {
        for (index = 0; index < item_count; index += 1) {
            Py_XDECREF(items[index].wire);
        }
        PyMem_Free(items);
    }
    Py_DECREF(sequence);
    return ok;
}

static PyObject *{{ sequence.decoder() }}(FfiBuf_u8 buffer) {
    boltffi_python_wire_reader reader;
    PyObject *result = NULL;
    PyObject *item = NULL;
    uint32_t item_count = 0;
    Py_ssize_t count = 0;
    Py_ssize_t index = 0;
    if (!boltffi_python_validate_owned_memory(buffer)) {
        goto done;
    }
    reader.ptr = buffer.ptr;
    reader.len = buffer.len;
    reader.offset = 0;
    if (!boltffi_python_wire_reader_u32(&reader, &item_count)) {
        goto done;
    }
    if (item_count > (uint32_t)PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native sequence is too large");
        goto done;
    }
    count = (Py_ssize_t)item_count;
    result = PyList_New(count);
    if (result == NULL) {
        goto done;
    }
    for (index = 0; index < count; index += 1) {
        item = {{ sequence.item_reader() }}(&reader);
        if (item == NULL) {
            Py_CLEAR(result);
            goto done;
        }
        PyList_SET_ITEM(result, index, item);
        item = NULL;
    }
    if (reader.offset != reader.len) {
        PyErr_SetString(PyExc_ValueError, "trailing BoltFFI wire bytes");
        Py_CLEAR(result);
    }
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endfor %}
{% for enumeration in enums %}
{{ enumeration }}
{% endfor %}
{% for declaration in callback_declarations %}
{{ declaration }}
{% endfor %}
{% for class in classes %}
{{ class }}
{% endfor %}
{% for callback in callbacks %}
{{ callback }}
{% endfor %}
static int boltffi_python_bind_host_state(void) {
{%- for binding in host_bindings %}
    if (!{{ binding }}()) {
        return 0;
    }
{%- endfor %}
    return 1;
}

static void boltffi_python_release_host_state(void) {
{%- for cleanup in cleanup %}
    {{ cleanup }};
{%- endfor %}
{% if !codec_decoders.is_empty() || !codec_encoders.is_empty() %}
    Py_CLEAR(boltffi_python_wire_codecs);
{% endif %}
}
{% for function in functions %}
{{ function }}
{% endfor %}
{% for constant in constants %}
{{ constant }}
{% endfor %}
{% for stream in streams %}
{{ stream }}
{% endfor %}
static PyMethodDef {{ method_table }}[] = {
{% if !codec_decoders.is_empty() || !codec_encoders.is_empty() %}
    {"_register_wire_codec", (PyCFunction)boltffi_python_register_wire_codec, METH_VARARGS, NULL},
{% endif %}
{%- for method in methods %}
    {"{{ method.python_name }}", (PyCFunction){{ method.c_function }}, {{ method.flags }}, NULL},
{%- endfor %}
    {NULL, NULL, 0, NULL}
};

static struct PyModuleDef {{ module_definition }} = {
    PyModuleDef_HEAD_INIT,
    "{{ module_name }}",
    NULL,
    -1,
    {{ method_table }},
    NULL,
    NULL,
    NULL,
    {{ free_function }}
};

PyMODINIT_FUNC {{ init_function }}(void) {
    PyObject *module = PyModule_Create(&{{ module_definition }});
    if (module == NULL) {
        return NULL;
    }
{%- for setup in type_setups %}
    if (!{{ setup }}(module)) {
        Py_DECREF(module);
        return NULL;
    }
{%- endfor %}
    return module;
}
