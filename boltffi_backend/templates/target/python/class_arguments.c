{%- for transfer in [true, false] %}
{%- for param in params %}
{%- if let Some(class) = param.class_handle() %}
{%- if class.release().is_some() == *transfer %}
    {
        PyObject *__boltffi_class_owner = args[{{ param.index() }}];
{%- if class.nullable() %}
        if (__boltffi_class_owner != Py_None) {
{%- endif %}
        PyObject *__boltffi_class_handle = PyObject_GetAttrString(__boltffi_class_owner, "_handle");
        if (__boltffi_class_handle == NULL) goto done;
        if (__boltffi_class_handle == Py_None) {
            Py_DECREF(__boltffi_class_handle);
            PyErr_SetString(PyExc_ValueError, "{{ param.name() }} has been moved or released");
            goto done;
        }
        int __boltffi_class_valid = {{ param.parser() }}(__boltffi_class_handle, &{{ param.name() }});
        Py_DECREF(__boltffi_class_handle);
        if (!__boltffi_class_valid) goto done;
{%- if class.release().is_some() %}
        if (PyObject_SetAttrString(__boltffi_class_owner, "_handle", Py_None) < 0) goto done;
        __boltffi_{{ param.name() }}_owned = {{ param.name() }} != 0;
{%- endif %}
{%- if class.nullable() %}
        }
{%- endif %}
    }
{%- endif %}
{%- endif %}
{%- endfor %}
{%- endfor %}
