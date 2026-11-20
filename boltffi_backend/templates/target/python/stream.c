static PyObject *{{ subscribe.wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    {{ stream_handle_type }} subscription = 0;
{% match receiver %}
{% when Some with (receiver) %}
    {{ receiver.handle_type }} receiver = 0;
{% when None %}
{% endmatch %}
    PyObject *result = NULL;
    (void)self;
    if (nargs != {{ subscribe_arity }}) {
        PyErr_Format(PyExc_TypeError, "{{ subscribe.python_name }}() takes {{ subscribe_arity }} positional arguments but %zd were given", nargs);
        goto done;
    }
    if ({{ subscribe.storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        goto done;
    }
{% match receiver %}
{% when Some with (receiver) %}
    if (!{{ receiver.parser }}(args[0], &receiver)) {
        goto done;
    }
    subscription = {{ subscribe.storage }}(receiver);
{% when None %}
    subscription = {{ subscribe.storage }}();
{% endmatch %}
    result = {{ stream_handle_boxer }}(subscription);
done:
    return result;
}

static PyObject *{{ pop_batch.wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    {{ stream_handle_type }} subscription = 0;
    uintptr_t output_capacity = 0;
{% if item.is_direct() %}
    uintptr_t item_count = 0;
    Py_ssize_t item_index = 0;
    {{ item.c_type() }} *items = NULL;
{% endif %}
    PyObject *list = NULL;
    PyObject *result = NULL;
    (void)self;
    if (nargs != 2) {
        PyErr_Format(PyExc_TypeError, "{{ pop_batch.python_name }}() takes 2 positional arguments but %zd were given", nargs);
        goto done;
    }
    if ({{ pop_batch.storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        goto done;
    }
    if (!{{ stream_handle_parser }}(args[0], &subscription)) {
        goto done;
    }
    if (!boltffi_python_parse_usize(args[1], &output_capacity)) {
        goto done;
    }
    if (output_capacity > (uintptr_t)PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "stream batch size is too large");
        goto done;
    }
    if (output_capacity == 0) {
        result = PyList_New(0);
        goto done;
    }
{% if item.is_direct() %}
    items = PyMem_Calloc((size_t)output_capacity, sizeof({{ item.c_type() }}));
    if (items == NULL) {
        PyErr_NoMemory();
        goto done;
    }
    item_count = {{ pop_batch.storage }}(subscription, items, output_capacity);
    if (item_count > output_capacity) {
        PyErr_SetString(PyExc_RuntimeError, "native stream returned an invalid batch size");
        goto done;
    }
    list = PyList_New((Py_ssize_t)item_count);
    if (list == NULL) {
        goto done;
    }
    for (item_index = 0; item_index < (Py_ssize_t)item_count; item_index += 1) {
        PyObject *item = {{ item.boxer() }}(items[item_index]);
        if (item == NULL) {
            goto done;
        }
        PyList_SET_ITEM(list, item_index, item);
    }
    result = list;
    list = NULL;
{% else %}
    result = boltffi_python_decode_owned_raw_wire({{ pop_batch.storage }}(subscription, output_capacity));
{% endif %}
done:
    Py_XDECREF(list);
{% if item.is_direct() %}
    PyMem_Free(items);
{% endif %}
    return result;
}

static PyObject *{{ wait.wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    {{ stream_handle_type }} subscription = 0;
    uint32_t timeout_milliseconds = 0;
    (void)self;
    if (nargs != 2) {
        PyErr_Format(PyExc_TypeError, "{{ wait.python_name }}() takes 2 positional arguments but %zd were given", nargs);
        return NULL;
    }
    if ({{ wait.storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return NULL;
    }
    if (!{{ stream_handle_parser }}(args[0], &subscription)) {
        return NULL;
    }
    if (!boltffi_python_parse_u32(args[1], &timeout_milliseconds)) {
        return NULL;
    }
    return PyLong_FromLong((long){{ wait.storage }}(subscription, timeout_milliseconds));
}

static PyObject *{{ unsubscribe.wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    {{ stream_handle_type }} subscription = 0;
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ unsubscribe.python_name }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    if ({{ unsubscribe.storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return NULL;
    }
    if (!{{ stream_handle_parser }}(args[0], &subscription)) {
        return NULL;
    }
    {{ unsubscribe.storage }}(subscription);
    Py_RETURN_NONE;
}

static PyObject *{{ free.wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    {{ stream_handle_type }} subscription = 0;
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ free.python_name }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    if ({{ free.storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return NULL;
    }
    if (!{{ stream_handle_parser }}(args[0], &subscription)) {
        return NULL;
    }
    {{ free.storage }}(subscription);
    Py_RETURN_NONE;
}
{% if let Some(take_error) = take_error %}

static PyObject *{{ take_error.wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    {{ stream_handle_type }} subscription = 0;
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ take_error.python_name }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    if ({{ take_error.storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return NULL;
    }
    if (!{{ stream_handle_parser }}(args[0], &subscription)) {
        return NULL;
    }
    return boltffi_python_decode_owned_raw_wire({{ take_error.storage }}(subscription));
}
{% endif %}
