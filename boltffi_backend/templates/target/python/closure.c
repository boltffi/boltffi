static {{ returns.c_type }} {{ invoke }}(void *context{% for param in params %}{% for declaration in param.declarations %}, {{ declaration }}{% endfor %}{% endfor %}{% if let Some(fallible) = fallible_return %}{% for declaration in fallible.declarations %}, {{ declaration }}{% endfor %}{% endif %}) {
{%- for param in params %}
    PyObject *{{ param.object }} = NULL;
{%- endfor %}
    PyObject *arguments = NULL;
    PyObject *result = NULL;
{%- if wire_payload %}
    PyObject *return_wire = NULL;
    const uint8_t *return_ptr = NULL;
    uintptr_t return_len = 0;
{%- endif %}
{%- if let Some(fallible) = fallible_return %}
    int fallible_ok = 0;
    PyObject *fallible_payload = NULL;
{%- if fallible.success.direct %}
    {{ fallible.success.c_type() }} {{ fallible.success.value() }} = {{ fallible.success.default_value() }};
{%- endif %}
{%- endif %}
{%- if returns.has_value() %}
    {{ returns.c_type }} {{ returns.value() }} = {{ returns.default_value }};
{%- endif %}
    PyGILState_STATE gil = PyGILState_Ensure();
    PyObject *callable = (PyObject *)context;
    arguments = PyTuple_New({{ params.len() }});
    if (arguments == NULL) {
        goto done;
    }
{%- for param in params %}
    {{ param.object }} = {{ param.expression }};
    if ({{ param.object }} == NULL) {
        goto done;
    }
    PyTuple_SET_ITEM(arguments, {{ loop.index0 }}, {{ param.object }});
    {{ param.object }} = NULL;
{%- endfor %}
    result = PyObject_CallObject(callable, arguments);
    if (result == NULL) {
        goto done;
    }
{%- if let Some(fallible) = fallible_return %}
    if (!PyTuple_Check(result) || PyTuple_GET_SIZE(result) != 2) {
        PyErr_SetString(PyExc_TypeError, "closure must return a (success, payload) tuple");
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
{%- if returns.has_value() %}
{%- if returns.wire %}
    if (!{{ returns.parser() }}(result, &return_wire, &return_ptr, &return_len)) {
        goto done;
    }
    {{ returns.value() }} = {{ copy_buffer_storage }}(return_ptr, return_len);
{%- else %}
    if (!{{ returns.parser() }}(result, &{{ returns.value() }})) {
        goto done;
    }
{%- endif %}
{%- endif %}
{%- endif %}
done:
    if (PyErr_Occurred()) {
{%- if let Some(fallible) = fallible_return %}
        {{ fallible.error.value }} = boltffi_python_callback_error();
{%- else %}
        PyErr_Print();
{%- endif %}
    }
{%- for param in params %}
    Py_XDECREF({{ param.object }});
{%- endfor %}
{%- if wire_payload %}
    Py_XDECREF(return_wire);
{%- endif %}
    Py_XDECREF(result);
    Py_XDECREF(arguments);
    PyGILState_Release(gil);
{%- if returns.has_value() %}
    return {{ returns.value() }};
{%- endif %}
}

static void {{ release }}(void *context) {
    PyGILState_STATE gil = PyGILState_Ensure();
    Py_XDECREF((PyObject *)context);
    PyGILState_Release(gil);
}

static int {{ parser }}(PyObject *value, {{ call_output_declaration }}, {{ context_output_declaration }}, {{ release_output_declaration }}) {
    if (!PyCallable_Check(value)) {
        PyErr_SetString(PyExc_TypeError, "expected callable");
        return 0;
    }
    Py_INCREF(value);
    *out_call = {{ invoke }};
    *out_context = value;
    *out_release = {{ release }};
    return 1;
}
