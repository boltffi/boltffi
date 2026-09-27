{%- for param in params %}
{%- if param.is_closure() %}
{{ param.closure_declaration() }}
{%- endif %}
{%- endfor %}
static PyObject *{{ start_wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
{%- for param in params %}
{%- if param.is_direct() %}
    {{ param.c_type() }} {{ param.name() }}{% if param.class_handle().is_some() %} = {0}{% endif %};
{%- if let Some(class) = param.class_handle() %}
{%- if class.release().is_some() %}
    int __boltffi_{{ param.name() }}_owned = 0;
{%- endif %}
{%- endif %}
{%- endif %}
{%- if param.is_encoded() %}
    PyObject *{{ param.wire() }} = NULL;
    const uint8_t *{{ param.pointer() }} = NULL;
    uintptr_t {{ param.length() }} = 0;
{%- endif %}
{%- if param.is_closure() %}
    {{ param.closure_call() }} = NULL;
    {{ param.closure_context() }} = NULL;
    {{ param.closure_release() }} = NULL;
    int {{ param.closure_release_needed() }} = 0;
{%- endif %}
{%- endfor %}
    RustFutureHandle handle = NULL;
    PyObject *result = NULL;
    (void)self;
    if (nargs != {{ params.len() }}) {
        PyErr_Format(PyExc_TypeError, "{{ python_name }}() takes {{ params.len() }} positional arguments but %zd were given", nargs);
        goto done;
    }
    if ({{ start_storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        goto done;
    }
{%- for param in params %}
{%- if param.is_direct() && param.class_handle().is_none() %}
    if (!{{ param.parser() }}(args[{{ param.index() }}], &{{ param.name() }})) {
        goto done;
    }
{%- endif %}
{%- if param.is_encoded() %}
    if (!{{ param.parser() }}(args[{{ param.index() }}], &{{ param.wire() }}, &{{ param.pointer() }}, &{{ param.length() }})) {
        goto done;
    }
{%- endif %}
{%- if param.is_closure() %}
    if (!{{ param.parser() }}(args[{{ param.index() }}], &{{ param.closure_call() }}, &{{ param.closure_context() }}, &{{ param.closure_release() }})) {
        goto done;
    }
    {{ param.closure_release_needed() }} = {{ param.closure_context() }} != NULL && {{ param.closure_release() }} != NULL;
{%- endif %}
{%- endfor %}
{% include "target/python/class_arguments.c" %}
{%- for param in params %}
{%- if let Some(class) = param.class_handle() %}
{%- if class.release().is_some() %}
    __boltffi_{{ param.name() }}_owned = 0;
{%- endif %}
{%- endif %}
{%- if param.is_closure() %}
    {{ param.closure_release_needed() }} = 0;
{%- endif %}
{%- endfor %}
    handle = {{ start_storage }}({%- for arg in call_args %}{{ arg }}{% if !loop.last %}, {% endif %}{%- endfor %});
    result = boltffi_python_box_future_handle(handle);
done:
{%- for param in params %}
{%- if let Some(class) = param.class_handle() %}
{%- if let Some(release) = class.release() %}
    if (__boltffi_{{ param.name() }}_owned) {{ release }}({{ param.name() }});
{%- endif %}
{%- endif %}
{%- if param.is_encoded() %}
    Py_XDECREF({{ param.wire() }});
{%- endif %}
{%- if param.is_closure() %}
    if ({{ param.closure_release_needed() }}) {
        {{ param.closure_release() }}({{ param.closure_context() }});
    }
{%- endif %}
{%- endfor %}
    return result;
}

static PyObject *{{ poll_wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    RustFutureHandle handle = NULL;
    PyObject *callback_state = NULL;
    (void)self;
    if (nargs != 3) {
        PyErr_Format(PyExc_TypeError, "{{ poll_python_name }}() takes 3 positional arguments but %zd were given", nargs);
        return NULL;
    }
    if ({{ poll_storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return NULL;
    }
    if (!boltffi_python_parse_future_handle(args[0], &handle)) {
        return NULL;
    }
    callback_state = PyTuple_Pack(2, args[1], args[2]);
    if (callback_state == NULL) {
        return NULL;
    }
    {{ poll_storage }}(handle, (uint64_t)(uintptr_t)callback_state, boltffi_python_future_wake);
    Py_RETURN_NONE;
}

static PyObject *{{ complete_wrapper }}(PyObject *self, PyObject *handle_object) {
    RustFutureHandle handle = NULL;
    FfiStatus status = {0};
{%- if let Some(fallible) = fallible %}
{%- if let Some(success_declaration) = fallible.success_declaration %}
    {{ success_declaration }};
{%- endif %}
    {{ fallible.error_type }} {{ fallible.error_value }} = {0};
    PyObject *error = NULL;
{%- endif %}
    PyObject *result = NULL;
    (void)self;
    if ({{ complete_storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        goto done;
    }
    if (!boltffi_python_parse_future_handle(handle_object, &handle)) {
        goto done;
    }
{%- if let Some(fallible) = fallible %}
    {{ fallible.error_value }} = {{ complete_storage }}(handle, &status{% if !complete_call_args.is_empty() %}, {% endif %}{%- for arg in complete_call_args %}{{ arg }}{% if !loop.last %}, {% endif %}{%- endfor %});
    if (!boltffi_python_check_future_status(status, handle, {{ panic_storage }})) {
        goto done;
    }
    if ({{ fallible.error_value }}.len != 0) {
        error = {{ fallible.error.converter() }}({{ fallible.error_value }});
        if (error != NULL) {
            PyErr_SetObject(PyExc_RuntimeError, error);
        }
        goto done;
    }
{%- if returns.is_void() %}
    Py_INCREF(Py_None);
    result = Py_None;
{%- else %}
    result = {{ returns.converter() }}({{ fallible.success_value() }});
{%- endif %}
{%- else %}
{%- if returns.is_void() %}
    {{ complete_storage }}(handle, &status{% if !complete_call_args.is_empty() %}, {% endif %}{%- for arg in complete_call_args %}{{ arg }}{% if !loop.last %}, {% endif %}{%- endfor %});
    if (!boltffi_python_check_future_status(status, handle, {{ panic_storage }})) {
        goto done;
    }
    Py_INCREF(Py_None);
    result = Py_None;
{%- else %}
    result = {{ returns.converter() }}({{ complete_storage }}(handle, &status{% if !complete_call_args.is_empty() %}, {% endif %}{%- for arg in complete_call_args %}{{ arg }}{% if !loop.last %}, {% endif %}{%- endfor %}));
    if (!boltffi_python_check_future_status(status, handle, {{ panic_storage }})) {
        Py_CLEAR(result);
        goto done;
    }
{%- endif %}
{%- endif %}
done:
{%- if fallible.is_some() %}
    Py_XDECREF(error);
{%- endif %}
    return result;
}

static PyObject *{{ panic_message_wrapper }}(PyObject *self, PyObject *handle_object) {
    RustFutureHandle handle = NULL;
    (void)self;
    if ({{ panic_storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return NULL;
    }
    if (!boltffi_python_parse_future_handle(handle_object, &handle)) {
        return NULL;
    }
    return boltffi_python_decode_owned_utf8({{ panic_storage }}(handle));
}

static PyObject *{{ cancel_wrapper }}(PyObject *self, PyObject *handle_object) {
    RustFutureHandle handle = NULL;
    (void)self;
    if ({{ cancel_storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return NULL;
    }
    if (!boltffi_python_parse_future_handle(handle_object, &handle)) {
        return NULL;
    }
    {{ cancel_storage }}(handle);
    Py_RETURN_NONE;
}

static PyObject *{{ free_wrapper }}(PyObject *self, PyObject *handle_object) {
    RustFutureHandle handle = NULL;
    (void)self;
    if ({{ free_storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return NULL;
    }
    if (!boltffi_python_parse_future_handle(handle_object, &handle)) {
        return NULL;
    }
    {{ free_storage }}(handle);
    Py_RETURN_NONE;
}
