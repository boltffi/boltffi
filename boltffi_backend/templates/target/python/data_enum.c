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

static int {{ wire_encoder }}(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    PyObject *wire = NULL;
    if (!boltffi_python_expect_type_instance(value, {{ type_object }}, "{{ class_name }}")) {
        return 0;
    }
{%- if transparent %}
    wire = PyObject_CallMethod({{ type_object }}, "_boltffi_wire_value", "O", value);
{%- else %}
    wire = PyObject_CallMethod(value, "_boltffi_wire", NULL);
{%- endif %}
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
