static PyObject *{{ type_object }} = NULL;

static PyObject *{{ register.c_function() }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    (void)self;
    if (nargs != 1) {
        PyErr_SetString(PyExc_TypeError, "class registration requires one type");
        return NULL;
    }
    if (!boltffi_python_store_registered_type(&{{ type_object }}, args[0], "{{ class_name }}")) return NULL;
    Py_RETURN_NONE;
}

static PyObject *{{ boxer }}({{ handle_type }} handle) {
    PyObject *wrapper = NULL;
    PyObject *value = NULL;
    if (handle == 0) Py_RETURN_NONE;
    if (!boltffi_python_expect_registered_type({{ type_object }}, "{{ class_name }}")) goto fail;
    wrapper = PyType_GenericAlloc((PyTypeObject *){{ type_object }}, 0);
    if (wrapper == NULL) goto fail;
    value = {{ box_primitive }}(handle);
    if (value == NULL) goto fail;
    if (PyObject_SetAttrString(wrapper, "_handle", value) < 0) goto fail;
    Py_DECREF(value);
    return wrapper;
fail:
    Py_XDECREF(value);
    Py_XDECREF(wrapper);
    return NULL;
}
