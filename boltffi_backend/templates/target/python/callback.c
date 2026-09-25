static void {{ free }}(uint64_t handle) {
    PyGILState_STATE gil = PyGILState_Ensure();
    Py_XDECREF((PyObject *)(uintptr_t)handle);
    PyGILState_Release(gil);
}

static uint64_t {{ clone }}(uint64_t handle) {
    PyGILState_STATE gil = PyGILState_Ensure();
    PyObject *value = (PyObject *)(uintptr_t)handle;
    Py_XINCREF(value);
    PyGILState_Release(gil);
    return handle;
}

{%- for method in methods %}
static {{ method.returns.c_type }} {{ method.function }}(uint64_t handle{% if let Some(fallible) = method.fallible_return %}{% for declaration in fallible.declarations %}, {{ declaration }}{% endfor %}{% endif %}{% for param in method.params %}{% for declaration in param.declarations %}, {{ declaration }}{% endfor %}{% endfor %}{% if let Some(completion) = method.completion %}, {{ completion.declaration }}, {{ completion.data_declaration }}{% endif %}) {
{%- for param in method.params %}
    PyObject *{{ param.object }} = NULL;
{%- endfor %}
    PyObject *callback = NULL;
    PyObject *arguments = NULL;
    PyObject *result = NULL;
{%- if method.wire_payload %}
    PyObject *return_wire = NULL;
    const uint8_t *return_ptr = NULL;
    uintptr_t return_len = 0;
{%- endif %}
{%- if let Some(fallible) = method.fallible_return %}
    int fallible_ok = 0;
    PyObject *fallible_payload = NULL;
{%- if fallible.success.direct %}
    {{ fallible.success.c_type() }} {{ fallible.success.value() }} = {{ fallible.success.default_value() }};
{%- endif %}
{%- endif %}
{%- if let Some(completion) = method.completion %}
{%- if completion.payload.fallible %}
    int completion_ok = 0;
    PyObject *completion_result_payload = NULL;
{%- endif %}
{%- if completion.payload.direct_bytes %}
    {{ completion.payload.direct_type() }} {{ completion.payload.direct_value() }} = ({{ completion.payload.direct_type() }}){0};
{%- endif %}
{%- if completion.payload.error_direct_bytes %}
    {{ completion.payload.error_direct_type() }} {{ completion.payload.error_direct_value() }} = ({{ completion.payload.error_direct_type() }}){0};
{%- endif %}
{%- if completion.payload.has_value() %}
    {{ completion.payload.c_type() }} {{ completion.payload.value() }} = {{ completion.payload.default_value() }};
{%- endif %}
    FfiStatus completion_status = FFI_STATUS_OK;
{%- endif %}
{%- if method.returns.has_value() %}
    {{ method.returns.c_type }} {{ method.returns.value() }} = {{ method.returns.default_value }};
{%- endif %}
    PyGILState_STATE gil = PyGILState_Ensure();
    PyObject *receiver = (PyObject *)(uintptr_t)handle;
    callback = PyObject_GetAttrString(receiver, "{{ method.python_name }}");
    if (callback == NULL) {
        goto done;
    }
    arguments = PyTuple_New({{ method.params.len() }});
    if (arguments == NULL) {
        goto done;
    }
{%- for param in method.params %}
    {{ param.object }} = {{ param.expression }};
    if ({{ param.object }} == NULL) {
        goto done;
    }
{%- if param.class_release.is_some() %}
    {{ param.name }} = 0;
{%- endif %}
    PyTuple_SET_ITEM(arguments, {{ loop.index0 }}, {{ param.object }});
    {{ param.object }} = NULL;
{%- endfor %}
    result = PyObject_CallObject(callback, arguments);
    if (result == NULL) {
        goto done;
    }
    if (PyObject_HasAttrString(result, "__await__")) {
        PyErr_SetString(PyExc_TypeError, "{{ method.python_name }}() returned an awaitable, Python callback methods must return the value directly");
        goto done;
    }
{%- if let Some(completion) = method.completion %}
{%- if completion.payload.fallible %}
    if (!PyTuple_Check(result) || PyTuple_GET_SIZE(result) != 2) {
        PyErr_SetString(PyExc_TypeError, "{{ method.python_name }}() must return a (success, payload) tuple");
        goto done;
    }
    completion_ok = PyObject_IsTrue(PyTuple_GET_ITEM(result, 0));
    if (completion_ok < 0) {
        goto done;
    }
    completion_result_payload = PyTuple_GET_ITEM(result, 1);
    if (completion_ok) {
{%- if completion.payload.wire %}
        if (!{{ completion.payload.parser() }}(completion_result_payload, &return_wire, &return_ptr, &return_len)) {
            goto done;
        }
        {{ completion.payload.value() }} = {{ copy_buffer_storage }}(return_ptr, return_len);
{%- elif completion.payload.direct_bytes %}
        if (!{{ completion.payload.parser() }}(completion_result_payload, &{{ completion.payload.direct_value() }})) {
            goto done;
        }
        {{ completion.payload.value() }} = {{ copy_buffer_storage }}((const uint8_t *)&{{ completion.payload.direct_value() }}, (uintptr_t)sizeof({{ completion.payload.direct_value() }}));
{%- else %}
        {{ completion.payload.value() }} = {{ completion.payload.default_value() }};
{%- endif %}
    } else {
        completion_status = FFI_STATUS_INTERNAL_ERROR;
{%- if completion.payload.error_wire %}
        if (!{{ completion.payload.error_parser() }}(completion_result_payload, &return_wire, &return_ptr, &return_len)) {
            goto done;
        }
        {{ completion.payload.value() }} = {{ copy_buffer_storage }}(return_ptr, return_len);
{%- elif completion.payload.error_direct_bytes %}
        if (!{{ completion.payload.error_parser() }}(completion_result_payload, &{{ completion.payload.error_direct_value() }})) {
            goto done;
        }
        {{ completion.payload.value() }} = {{ copy_buffer_storage }}((const uint8_t *)&{{ completion.payload.error_direct_value() }}, (uintptr_t)sizeof({{ completion.payload.error_direct_value() }}));
{%- else %}
        {{ completion.payload.value() }} = {{ completion.payload.default_value() }};
{%- endif %}
    }
{%- elif completion.payload.has_value() %}
{%- if completion.payload.wire %}
    if (!{{ completion.payload.parser() }}(result, &return_wire, &return_ptr, &return_len)) {
        goto done;
    }
    {{ completion.payload.value() }} = {{ copy_buffer_storage }}(return_ptr, return_len);
{%- elif completion.payload.direct_bytes %}
    if (!{{ completion.payload.parser() }}(result, &{{ completion.payload.direct_value() }})) {
        goto done;
    }
    {{ completion.payload.value() }} = {{ copy_buffer_storage }}((const uint8_t *)&{{ completion.payload.direct_value() }}, (uintptr_t)sizeof({{ completion.payload.direct_value() }}));
{%- else %}
    if (!{{ completion.payload.parser() }}(result, &{{ completion.payload.value() }})) {
        goto done;
    }
{%- endif %}
{%- endif %}
{%- else %}
{%- if let Some(fallible) = method.fallible_return %}
    if (!PyTuple_Check(result) || PyTuple_GET_SIZE(result) != 2) {
        PyErr_SetString(PyExc_TypeError, "{{ method.python_name }}() must return a (success, payload) tuple");
        goto done;
    }
    fallible_ok = PyObject_IsTrue(PyTuple_GET_ITEM(result, 0));
    if (fallible_ok < 0) {
        goto done;
    }
    fallible_payload = PyTuple_GET_ITEM(result, 1);
    if (fallible_ok) {
{%- if fallible.success.wire %}
        if (!{{ fallible.success.parser() }}(fallible_payload, &return_wire, &return_ptr, &return_len)) {
            goto done;
        }
        *{{ fallible.success.out() }} = {{ copy_buffer_storage }}(return_ptr, return_len);
{%- elif fallible.success.direct %}
        if (!{{ fallible.success.parser() }}(fallible_payload, &{{ fallible.success.value() }})) {
            goto done;
        }
        *{{ fallible.success.out() }} = {{ fallible.success.value() }};
{%- endif %}
    } else {
        if (!{{ fallible.error.parser }}(fallible_payload, &return_wire, &return_ptr, &return_len)) {
            goto done;
        }
        {{ fallible.error.value }} = {{ copy_buffer_storage }}(return_ptr, return_len);
    }
{%- else %}
{%- if method.returns.has_value() %}
{%- if method.returns.wire %}
    if (!{{ method.returns.parser() }}(result, &return_wire, &return_ptr, &return_len)) {
        goto done;
    }
    {{ method.returns.value() }} = {{ copy_buffer_storage }}(return_ptr, return_len);
{%- else %}
    if (!{{ method.returns.parser() }}(result, &{{ method.returns.value() }})) {
        goto done;
    }
{%- endif %}
{%- endif %}
{%- endif %}
{%- endif %}
done:
    if (PyErr_Occurred()) {
{%- if let Some(fallible) = method.fallible_return %}
        {{ fallible.error.value }} = boltffi_python_callback_error();
{%- elif let Some(completion) = method.completion %}
        completion_status = FFI_STATUS_INTERNAL_ERROR;
{%- if completion.payload.fallible %}
        {{ completion.payload.value() }} = boltffi_python_callback_error();
{%- else %}
        PyErr_Print();
{%- endif %}
{%- else %}
        PyErr_Print();
{%- endif %}
    }
{%- for param in method.params %}
{%- if let Some(release) = param.class_release %}
    if ({{ param.name }} != 0) {{ release }}({{ param.name }});
{%- endif %}
{%- endfor %}
{%- for param in method.params %}
    Py_XDECREF({{ param.object }});
{%- endfor %}
{%- if method.wire_payload %}
    Py_XDECREF(return_wire);
{%- endif %}
    Py_XDECREF(result);
    Py_XDECREF(arguments);
    Py_XDECREF(callback);
    PyGILState_Release(gil);
{%- if let Some(completion) = method.completion %}
{%- if completion.payload.has_value() %}
    {{ completion.callback }}({{ completion.data }}, completion_status, {{ completion.payload.value() }});
{%- else %}
    {{ completion.callback }}({{ completion.data }}, completion_status);
{%- endif %}
    return;
{%- endif %}
{%- if method.returns.has_value() %}
    return {{ method.returns.value() }};
{%- endif %}
}

{%- endfor %}
static {{ vtable_type }} {{ vtable }} = {
{%- for slot in slots %}
    .{{ slot.name }} = {{ slot.function }},
{%- endfor %}
};

static int {{ register }}(void) {
    if ({{ register_storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return 0;
    }
    {{ register_storage }}(&{{ vtable }});
    return 1;
}

static int {{ parser }}(PyObject *value, BoltFFICallbackHandle *out) {
    if (value == Py_None) {
        PyErr_SetString(PyExc_TypeError, "callback cannot be None");
        return 0;
    }
    Py_INCREF(value);
    *out = {{ create_handle_storage }}((uint64_t)(uintptr_t)value);
    if (out->vtable == NULL) {
        Py_DECREF(value);
        PyErr_SetString(PyExc_RuntimeError, "failed to create callback handle");
        return 0;
    }
    return 1;
}

static int {{ optional_parser }}(PyObject *value, BoltFFICallbackHandle *out) {
    if (value == Py_None) {
        out->handle = 0;
        out->vtable = NULL;
        return 1;
    }
    return {{ parser }}(value, out);
}
