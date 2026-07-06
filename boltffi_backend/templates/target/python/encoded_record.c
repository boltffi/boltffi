static PyObject *{{ type_object }} = NULL;

static PyObject *{{ register_wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ register_method }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    if (!boltffi_python_store_registered_type(&{{ type_object }}, args[0], "{{ class_name }}")) {
        return NULL;
    }
    Py_RETURN_NONE;
}

{% match codec %}
{% when EncodedCodec::Registered %}
static int {{ wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    PyObject *wire = NULL;
    if (!boltffi_python_expect_type_instance(value, {{ type_object }}, "{{ class_name }}")) {
        return 0;
    }
    wire = PyObject_CallMethod(value, "_boltffi_wire", NULL);
    if (wire == NULL) {
        return 0;
    }
    if (!PyBytes_Check(wire)) {
        Py_DECREF(wire);
        PyErr_SetString(PyExc_TypeError, "{{ class_name }}._boltffi_wire() must return bytes");
        return 0;
    }
    if (PyBytes_GET_SIZE(wire) > PY_SSIZE_T_MAX) {
        Py_DECREF(wire);
        PyErr_SetString(PyExc_OverflowError, "{{ class_name }} wire payload is too large");
        return 0;
    }
    *out_wire = wire;
    *out_ptr = (const uint8_t *)PyBytes_AS_STRING(wire);
    *out_len = (uintptr_t)PyBytes_GET_SIZE(wire);
    return 1;
}

static PyObject *{{ owned_decoder }}(FfiBuf_u8 buffer) {
    PyObject *wire = NULL;
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_memory(buffer)) {
        goto done;
    }
    wire = PyBytes_FromStringAndSize((const char *)buffer.ptr, (Py_ssize_t)buffer.len);
    if (wire == NULL) {
        goto done;
    }
    if (!boltffi_python_expect_registered_type({{ type_object }}, "{{ class_name }}")) {
        goto done;
    }
    result = PyObject_CallMethod({{ type_object }}, "_boltffi_from_wire", "O", wire);
done:
    Py_XDECREF(wire);
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% when EncodedCodec::Native { fields } %}
static int {{ wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    PyObject *wire = NULL;
    uint8_t *bytes = NULL;
    uintptr_t wire_len = 0;
    boltffi_python_wire_writer writer = {0};
    int ok = 0;
{%- for field in fields %}
    PyObject *{{ field.value_name() }} = NULL;
{%- endfor %}
    if (!boltffi_python_expect_type_instance(value, {{ type_object }}, "{{ class_name }}")) {
        goto done;
    }
{%- for field in fields %}
    {{ field.value_name() }} = boltffi_python_get_record_field(value, "{{ class_name }}", "{{ field.python_name() }}");
    if ({{ field.value_name() }} == NULL) {
        goto done;
    }
{%- endfor %}
{%- for field in fields %}
    {
        PyObject *field_value = {{ field.value_name() }};
{%- match field.codec() %}
{%- when EncodedCodecNode::Primitive(primitive) %}
        if (!boltffi_python_wire_add(&wire_len, {{ primitive.wire_size() }})) {
            goto done;
        }
{%- when EncodedCodecNode::String %}
        Py_ssize_t utf8_len = 0;
        if (PyUnicode_AsUTF8AndSize(field_value, &utf8_len) == NULL) {
            goto done;
        }
        if (utf8_len > UINT32_MAX) {
            PyErr_SetString(PyExc_OverflowError, "string field is too large");
            goto done;
        }
        if (!boltffi_python_wire_add(&wire_len, 4 + (uintptr_t)utf8_len)) {
            goto done;
        }
{%- when EncodedCodecNode::Sequence(sequence) %}
        PyObject *sequence = PySequence_Fast(field_value, "expected sequence");
        Py_ssize_t item_count = 0;
        Py_ssize_t index = 0;
        if (sequence == NULL) {
            goto done;
        }
        item_count = PySequence_Fast_GET_SIZE(sequence);
        if (item_count > UINT32_MAX) {
            Py_DECREF(sequence);
            PyErr_SetString(PyExc_OverflowError, "sequence field is too large");
            goto done;
        }
        if (!boltffi_python_wire_add(&wire_len, 4)) {
            Py_DECREF(sequence);
            goto done;
        }
        for (index = 0; index < item_count; index += 1) {
            PyObject *{{ sequence.item_name() }} = PySequence_Fast_GET_ITEM(sequence, index);
{%- match sequence.item() %}
{%- when EncodedCodecNode::Primitive(primitive) %}
            if (!boltffi_python_wire_add(&wire_len, {{ primitive.wire_size() }})) {
                Py_DECREF(sequence);
                goto done;
            }
{%- when EncodedCodecNode::String %}
            Py_ssize_t utf8_len = 0;
            if (PyUnicode_AsUTF8AndSize({{ sequence.item_name() }}, &utf8_len) == NULL) {
                Py_DECREF(sequence);
                goto done;
            }
            if (utf8_len > UINT32_MAX) {
                Py_DECREF(sequence);
                PyErr_SetString(PyExc_OverflowError, "string item is too large");
                goto done;
            }
            if (!boltffi_python_wire_add(&wire_len, 4 + (uintptr_t)utf8_len)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- when EncodedCodecNode::Sequence(_) %}
            Py_DECREF(sequence);
            PyErr_SetString(PyExc_TypeError, "nested sequence field is unsupported");
            goto done;
{%- endmatch %}
        }
        Py_DECREF(sequence);
{%- endmatch %}
    }
{%- endfor %}
    wire = PyBytes_FromStringAndSize(NULL, (Py_ssize_t)wire_len);
    if (wire == NULL) {
        goto done;
    }
    bytes = (uint8_t *)PyBytes_AS_STRING(wire);
    writer.ptr = bytes;
    writer.len = wire_len;
    writer.offset = 0;
{%- for field in fields %}
    {
        PyObject *field_value = {{ field.value_name() }};
{%- match field.codec() %}
{%- when EncodedCodecNode::Primitive(primitive) %}
        {{ primitive.c_type() }} parsed = 0;
        if (!{{ primitive.parser() }}(field_value, &parsed)) {
            goto done;
        }
{%- if primitive.is_bool() %}
        if (!boltffi_python_wire_writer_u8(&writer, parsed ? 1 : 0)) {
            goto done;
        }
{%- elif primitive.is_i8() || primitive.is_u8() %}
        if (!boltffi_python_wire_writer_u8(&writer, (uint8_t)parsed)) {
            goto done;
        }
{%- elif primitive.is_i16() || primitive.is_u16() %}
        if (!boltffi_python_wire_writer_u16(&writer, (uint16_t)parsed)) {
            goto done;
        }
{%- elif primitive.is_i32() || primitive.is_u32() %}
        if (!boltffi_python_wire_writer_u32(&writer, (uint32_t)parsed)) {
            goto done;
        }
{%- elif primitive.is_i64() || primitive.is_u64() || primitive.is_isize() || primitive.is_usize() %}
        if (!boltffi_python_wire_writer_u64(&writer, (uint64_t)parsed)) {
            goto done;
        }
{%- elif primitive.is_f32() %}
        uint32_t bits = 0;
        memcpy(&bits, &parsed, sizeof(bits));
        if (!boltffi_python_wire_writer_u32(&writer, bits)) {
            goto done;
        }
{%- elif primitive.is_f64() %}
        uint64_t bits = 0;
        memcpy(&bits, &parsed, sizeof(bits));
        if (!boltffi_python_wire_writer_u64(&writer, bits)) {
            goto done;
        }
{%- endif %}
{%- when EncodedCodecNode::String %}
        Py_ssize_t utf8_len = 0;
        const char *utf8 = PyUnicode_AsUTF8AndSize(field_value, &utf8_len);
        if (utf8 == NULL) {
            goto done;
        }
        if (!boltffi_python_wire_writer_u32(&writer, (uint32_t)utf8_len)) {
            goto done;
        }
        if (!boltffi_python_wire_writer_write(&writer, (const uint8_t *)utf8, (uintptr_t)utf8_len)) {
            goto done;
        }
{%- when EncodedCodecNode::Sequence(sequence) %}
        PyObject *sequence = PySequence_Fast(field_value, "expected sequence");
        Py_ssize_t item_count = 0;
        Py_ssize_t index = 0;
        if (sequence == NULL) {
            goto done;
        }
        item_count = PySequence_Fast_GET_SIZE(sequence);
        if (item_count > UINT32_MAX) {
            Py_DECREF(sequence);
            PyErr_SetString(PyExc_OverflowError, "sequence too large to encode");
            goto done;
        }
        if (!boltffi_python_wire_writer_u32(&writer, (uint32_t)item_count)) {
            Py_DECREF(sequence);
            goto done;
        }
        for (index = 0; index < item_count; index += 1) {
            PyObject *{{ sequence.item_name() }} = PySequence_Fast_GET_ITEM(sequence, index);
{%- match sequence.item() %}
{%- when EncodedCodecNode::Primitive(primitive) %}
            {{ primitive.c_type() }} parsed = 0;
            if (!{{ primitive.parser() }}({{ sequence.item_name() }}, &parsed)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- if primitive.is_bool() %}
            if (!boltffi_python_wire_writer_u8(&writer, parsed ? 1 : 0)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- elif primitive.is_i8() || primitive.is_u8() %}
            if (!boltffi_python_wire_writer_u8(&writer, (uint8_t)parsed)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- elif primitive.is_i16() || primitive.is_u16() %}
            if (!boltffi_python_wire_writer_u16(&writer, (uint16_t)parsed)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- elif primitive.is_i32() || primitive.is_u32() %}
            if (!boltffi_python_wire_writer_u32(&writer, (uint32_t)parsed)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- elif primitive.is_i64() || primitive.is_u64() || primitive.is_isize() || primitive.is_usize() %}
            if (!boltffi_python_wire_writer_u64(&writer, (uint64_t)parsed)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- elif primitive.is_f32() %}
            uint32_t bits = 0;
            memcpy(&bits, &parsed, sizeof(bits));
            if (!boltffi_python_wire_writer_u32(&writer, bits)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- elif primitive.is_f64() %}
            uint64_t bits = 0;
            memcpy(&bits, &parsed, sizeof(bits));
            if (!boltffi_python_wire_writer_u64(&writer, bits)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- endif %}
{%- when EncodedCodecNode::String %}
            Py_ssize_t utf8_len = 0;
            const char *utf8 = PyUnicode_AsUTF8AndSize({{ sequence.item_name() }}, &utf8_len);
            if (utf8 == NULL) {
                Py_DECREF(sequence);
                goto done;
            }
            if (!boltffi_python_wire_writer_u32(&writer, (uint32_t)utf8_len)) {
                Py_DECREF(sequence);
                goto done;
            }
            if (!boltffi_python_wire_writer_write(&writer, (const uint8_t *)utf8, (uintptr_t)utf8_len)) {
                Py_DECREF(sequence);
                goto done;
            }
{%- when EncodedCodecNode::Sequence(_) %}
            Py_DECREF(sequence);
            PyErr_SetString(PyExc_TypeError, "nested sequence field is unsupported");
            goto done;
{%- endmatch %}
        }
        Py_DECREF(sequence);
{%- endmatch %}
    }
{%- endfor %}
    if (writer.offset != writer.len) {
        PyErr_SetString(PyExc_RuntimeError, "wire writer produced wrong byte count");
        goto done;
    }
    *out_wire = wire;
    *out_ptr = bytes;
    *out_len = wire_len;
    wire = NULL;
    ok = 1;
done:
    Py_XDECREF(wire);
{%- for field in fields %}
    Py_XDECREF({{ field.value_name() }});
{%- endfor %}
    return ok;
}

static PyObject *{{ owned_decoder }}_read(boltffi_python_wire_reader *reader) {
    PyObject *result = NULL;
    PyObject *values[{{ fields.len() }}] = {0};
{%- for field in fields %}
    {
{%- match field.codec() %}
{%- when EncodedCodecNode::Primitive(primitive) %}
        {{ primitive.c_type() }} decoded = 0;
{%- if primitive.is_bool() %}
        uint8_t byte = 0;
        if (!boltffi_python_wire_reader_u8(reader, &byte)) {
            goto done;
        }
        if (byte > 1) {
            PyErr_SetString(PyExc_ValueError, "invalid BoltFFI bool");
            goto done;
        }
        decoded = byte == 1;
{%- elif primitive.is_i8() %}
        uint8_t byte = 0;
        if (!boltffi_python_wire_reader_u8(reader, &byte)) {
            goto done;
        }
        decoded = (int8_t)byte;
{%- elif primitive.is_u8() %}
        uint8_t byte = 0;
        if (!boltffi_python_wire_reader_u8(reader, &byte)) {
            goto done;
        }
        decoded = byte;
{%- elif primitive.is_i16() %}
        uint16_t bytes = 0;
        if (!boltffi_python_wire_reader_u16(reader, &bytes)) {
            goto done;
        }
        decoded = (int16_t)bytes;
{%- elif primitive.is_u16() %}
        uint16_t bytes = 0;
        if (!boltffi_python_wire_reader_u16(reader, &bytes)) {
            goto done;
        }
        decoded = bytes;
{%- elif primitive.is_i32() %}
        uint32_t bytes = 0;
        if (!boltffi_python_wire_reader_u32(reader, &bytes)) {
            goto done;
        }
        decoded = (int32_t)bytes;
{%- elif primitive.is_u32() %}
        uint32_t bytes = 0;
        if (!boltffi_python_wire_reader_u32(reader, &bytes)) {
            goto done;
        }
        decoded = bytes;
{%- elif primitive.is_i64() %}
        uint64_t bytes = 0;
        if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
            goto done;
        }
        decoded = (int64_t)bytes;
{%- elif primitive.is_u64() %}
        uint64_t bytes = 0;
        if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
            goto done;
        }
        decoded = bytes;
{%- elif primitive.is_isize() %}
        uint64_t bytes = 0;
        if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
            goto done;
        }
        decoded = (intptr_t)((int64_t)bytes);
{%- elif primitive.is_usize() %}
        uint64_t bytes = 0;
        if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
            goto done;
        }
        decoded = (uintptr_t)bytes;
{%- elif primitive.is_f32() %}
        uint32_t bytes = 0;
        if (!boltffi_python_wire_reader_u32(reader, &bytes)) {
            goto done;
        }
        memcpy(&decoded, &bytes, sizeof(decoded));
{%- elif primitive.is_f64() %}
        uint64_t bytes = 0;
        if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
            goto done;
        }
        memcpy(&decoded, &bytes, sizeof(decoded));
{%- endif %}
        values[{{ loop.index0 }}] = {{ primitive.boxer() }}(decoded);
        if (values[{{ loop.index0 }}] == NULL) {
            goto done;
        }
{%- when EncodedCodecNode::String %}
        uint32_t len = 0;
        const uint8_t *bytes = NULL;
        if (!boltffi_python_wire_reader_u32(reader, &len)) {
            goto done;
        }
        if (!boltffi_python_wire_reader_read(reader, len, &bytes)) {
            goto done;
        }
        values[{{ loop.index0 }}] = PyUnicode_FromStringAndSize((const char *)bytes, (Py_ssize_t)len);
        if (values[{{ loop.index0 }}] == NULL) {
            goto done;
        }
{%- when EncodedCodecNode::Sequence(sequence) %}
        uint32_t count = 0;
        Py_ssize_t index = 0;
        PyObject *item = NULL;
        if (!boltffi_python_wire_reader_u32(reader, &count)) {
            goto done;
        }
        if (count > (uint32_t)PY_SSIZE_T_MAX) {
            PyErr_SetString(PyExc_OverflowError, "native sequence is too large");
            goto done;
        }
        values[{{ loop.index0 }}] = PyList_New((Py_ssize_t)count);
        if (values[{{ loop.index0 }}] == NULL) {
            goto done;
        }
        for (index = 0; index < (Py_ssize_t)count; index += 1) {
{%- match sequence.item() %}
{%- when EncodedCodecNode::Primitive(primitive) %}
            {{ primitive.c_type() }} decoded = 0;
{%- if primitive.is_bool() %}
            uint8_t byte = 0;
            if (!boltffi_python_wire_reader_u8(reader, &byte)) {
                goto done;
            }
            if (byte > 1) {
                PyErr_SetString(PyExc_ValueError, "invalid BoltFFI bool");
                goto done;
            }
            decoded = byte == 1;
{%- elif primitive.is_i8() %}
            uint8_t byte = 0;
            if (!boltffi_python_wire_reader_u8(reader, &byte)) {
                goto done;
            }
            decoded = (int8_t)byte;
{%- elif primitive.is_u8() %}
            uint8_t byte = 0;
            if (!boltffi_python_wire_reader_u8(reader, &byte)) {
                goto done;
            }
            decoded = byte;
{%- elif primitive.is_i16() %}
            uint16_t bytes = 0;
            if (!boltffi_python_wire_reader_u16(reader, &bytes)) {
                goto done;
            }
            decoded = (int16_t)bytes;
{%- elif primitive.is_u16() %}
            uint16_t bytes = 0;
            if (!boltffi_python_wire_reader_u16(reader, &bytes)) {
                goto done;
            }
            decoded = bytes;
{%- elif primitive.is_i32() %}
            uint32_t bytes = 0;
            if (!boltffi_python_wire_reader_u32(reader, &bytes)) {
                goto done;
            }
            decoded = (int32_t)bytes;
{%- elif primitive.is_u32() %}
            uint32_t bytes = 0;
            if (!boltffi_python_wire_reader_u32(reader, &bytes)) {
                goto done;
            }
            decoded = bytes;
{%- elif primitive.is_i64() %}
            uint64_t bytes = 0;
            if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
                goto done;
            }
            decoded = (int64_t)bytes;
{%- elif primitive.is_u64() %}
            uint64_t bytes = 0;
            if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
                goto done;
            }
            decoded = bytes;
{%- elif primitive.is_isize() %}
            uint64_t bytes = 0;
            if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
                goto done;
            }
            decoded = (intptr_t)((int64_t)bytes);
{%- elif primitive.is_usize() %}
            uint64_t bytes = 0;
            if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
                goto done;
            }
            decoded = (uintptr_t)bytes;
{%- elif primitive.is_f32() %}
            uint32_t bytes = 0;
            if (!boltffi_python_wire_reader_u32(reader, &bytes)) {
                goto done;
            }
            memcpy(&decoded, &bytes, sizeof(decoded));
{%- elif primitive.is_f64() %}
            uint64_t bytes = 0;
            if (!boltffi_python_wire_reader_u64(reader, &bytes)) {
                goto done;
            }
            memcpy(&decoded, &bytes, sizeof(decoded));
{%- endif %}
            item = {{ primitive.boxer() }}(decoded);
{%- when EncodedCodecNode::String %}
            uint32_t len = 0;
            const uint8_t *bytes = NULL;
            if (!boltffi_python_wire_reader_u32(reader, &len)) {
                goto done;
            }
            if (!boltffi_python_wire_reader_read(reader, len, &bytes)) {
                goto done;
            }
            item = PyUnicode_FromStringAndSize((const char *)bytes, (Py_ssize_t)len);
{%- when EncodedCodecNode::Sequence(_) %}
            PyErr_SetString(PyExc_TypeError, "nested sequence field is unsupported");
            goto done;
{%- endmatch %}
            if (item == NULL) {
                goto done;
            }
            PyList_SET_ITEM(values[{{ loop.index0 }}], index, item);
            item = NULL;
        }
{%- endmatch %}
    }
{%- endfor %}
    if (!boltffi_python_expect_registered_type({{ type_object }}, "{{ class_name }}")) {
        goto done;
    }
    result = PyObject_Vectorcall({{ type_object }}, values, {{ fields.len() }}, NULL);
done:
{%- for field in fields %}
    Py_XDECREF(values[{{ loop.index0 }}]);
{%- endfor %}
    return result;
}

static PyObject *{{ owned_decoder }}(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    boltffi_python_wire_reader reader = {0};
    if (!boltffi_python_validate_owned_memory(buffer)) {
        goto done;
    }
    reader.ptr = buffer.ptr;
    reader.len = buffer.len;
    reader.offset = 0;
    result = {{ owned_decoder }}_read(&reader);
    if (result != NULL && reader.offset != reader.len) {
        Py_CLEAR(result);
        PyErr_SetString(PyExc_ValueError, "trailing BoltFFI wire bytes");
    }
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endmatch %}
