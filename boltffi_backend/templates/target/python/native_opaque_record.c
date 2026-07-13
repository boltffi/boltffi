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

/* Boxer: calls _from_handle; on any failure drops the owned handle once. */
static PyObject *{{ boxer }}(void *handle) {
    if (handle == NULL) {
        PyErr_SetString(PyExc_RuntimeError, "{{ class_name }}: null native handle");
        return NULL;
    }
    if (!boltffi_python_expect_registered_type({{ type_object }}, "{{ class_name }}")) {
        {{ drop_storage }}(handle);
        return NULL;
    }
    PyObject *obj = PyObject_CallMethod({{ type_object }}, "_from_handle", "K", (unsigned long long)(uintptr_t)handle);
    if (obj == NULL) {
        {{ drop_storage }}(handle);
        return NULL;
    }
    return obj;
}

/* Drop wrapper: Python-callable; accepts handle int, calls loaded drop storage. */
static PyObject *{{ drop_wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ drop_wrapper }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    void *handle = PyLong_AsVoidPtr(args[0]);
    if (handle == NULL && PyErr_Occurred()) {
        return NULL;
    }
    if (handle != NULL) {
        {{ drop_storage }}(handle);
    }
    Py_RETURN_NONE;
}
{%- for has in has_wrappers %}

/* Has wrapper for optional field. */
static PyObject *{{ has.c_wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ has.c_wrapper }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    void *handle = PyLong_AsVoidPtr(args[0]);
    if (handle == NULL && PyErr_Occurred()) {
        return NULL;
    }
    if (handle == NULL) {
        PyErr_SetString(PyExc_ValueError, "{{ has.c_wrapper }}(): handle must be non-zero");
        return NULL;
    }
    int32_t r = (int32_t){{ has.has_storage }}((const void *)handle);
    return PyBool_FromLong(r != 0);
}
{%- endfor %}
{%- for get in get_wrappers %}

/* Primitive get wrapper. */
static PyObject *{{ get.c_wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ get.c_wrapper }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    void *handle = PyLong_AsVoidPtr(args[0]);
    if (handle == NULL && PyErr_Occurred()) {
        return NULL;
    }
    if (handle == NULL) {
        PyErr_SetString(PyExc_ValueError, "{{ get.c_wrapper }}(): handle must be non-zero");
        return NULL;
    }
    {{ get.c_return_type }} r = {{ get.get_storage }}((const void *)handle);
    return {{ get.boxer }}(r);
}
{%- endfor %}
{%- for borrow in borrow_wrappers %}

/* Borrow wrapper: copies String/Bytes into PyBytes. */
static PyObject *{{ borrow.c_wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ borrow.c_wrapper }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    void *handle = PyLong_AsVoidPtr(args[0]);
    if (handle == NULL && PyErr_Occurred()) {
        return NULL;
    }
    if (handle == NULL) {
        PyErr_SetString(PyExc_ValueError, "{{ borrow.c_wrapper }}(): handle must be non-zero");
        return NULL;
    }
    const uint8_t *ptr = NULL;
    uintptr_t len = 0;
    int32_t ok = (int32_t){{ borrow.borrow_storage }}((const void *)handle, &ptr, &len);
    if (!ok) {
        PyErr_SetString(PyExc_RuntimeError, "native opaque borrow failed");
        return NULL;
    }
    /* Empty slice: Rust contract allows NULL ptr with len == 0 */
    if (len == 0) {
        return PyBytes_FromStringAndSize("", 0);
    }
    /* Guard against overflow before narrowing to Py_ssize_t */
    if (len > (uintptr_t)PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native opaque borrow slice too large");
        return NULL;
    }
    /* Reject NULL pointer for a non-empty slice: Rust contract violation */
    if (ptr == NULL) {
        PyErr_SetString(PyExc_RuntimeError, "native opaque borrow: non-empty slice with null pointer");
        return NULL;
    }
    return PyBytes_FromStringAndSize((const char *)ptr, (Py_ssize_t)len);
}
{%- endfor %}
