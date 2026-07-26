static PyObject *{{ type_object }} = NULL;

typedef struct {
    PyObject_HEAD
    {{ c_type }} value;
} {{ object_struct }};

static void {{ prefix }}_dealloc(PyObject *self) {
    PyTypeObject *type = Py_TYPE(self);
    type->tp_free(self);
    Py_DECREF(type);
}

static PyObject *{{ prefix }}_new(PyTypeObject *cls, PyObject *args, PyObject *kwargs) {
    static const char *field_names[{{ fields.len() }}] = {
{%- for field in fields %}
        "{{ field.python_name }}",
{%- endfor %}
    };
    PyObject *field_values[{{ fields.len() }}] = {0};
    {{ c_type }} value;
    memset(&value, 0, sizeof(value));
    Py_ssize_t positional = PyTuple_GET_SIZE(args);
    if (positional > {{ fields.len() }}) {
        PyErr_Format(PyExc_TypeError, "{{ class_name }}() takes {{ fields.len() }} positional arguments but %zd were given", positional);
        return NULL;
    }
    for (Py_ssize_t index = 0; index < positional; index++) {
        field_values[index] = PyTuple_GET_ITEM(args, index);
    }
    if (kwargs != NULL) {
        Py_ssize_t matched = 0;
        for (Py_ssize_t index = 0; index < {{ fields.len() }}; index++) {
            PyObject *keyword = PyDict_GetItemString(kwargs, field_names[index]);
            if (keyword == NULL) {
                continue;
            }
            if (field_values[index] != NULL) {
                PyErr_Format(PyExc_TypeError, "{{ class_name }}() got multiple values for argument '%s'", field_names[index]);
                return NULL;
            }
            field_values[index] = keyword;
            matched++;
        }
        if (matched != PyDict_GET_SIZE(kwargs)) {
            PyObject *key = NULL;
            PyObject *unused = NULL;
            Py_ssize_t position = 0;
            while (PyDict_Next(kwargs, &position, &key, &unused)) {
                int known = 0;
                for (Py_ssize_t index = 0; index < {{ fields.len() }}; index++) {
                    known |= PyUnicode_CompareWithASCIIString(key, field_names[index]) == 0;
                }
                if (!known) {
                    PyErr_Format(PyExc_TypeError, "{{ class_name }}() got an unexpected keyword argument %R", key);
                    return NULL;
                }
            }
        }
    }
{%- for field in fields %}
    if (field_values[{{ loop.index0 }}] != NULL) {
        if (!{{ field.parser }}(field_values[{{ loop.index0 }}], &value.{{ field.c_name }})) {
            return NULL;
        }
{%- match field.default %}
{%- when Some(default) %}
    } else {
        value.{{ field.c_name }} = {{ default }};
    }
{%- when None %}
    } else {
        PyErr_SetString(PyExc_TypeError, "{{ class_name }}() missing required argument '{{ field.python_name }}'");
        return NULL;
    }
{%- endmatch %}
{%- endfor %}
    PyObject *self = cls->tp_alloc(cls, 0);
    if (self == NULL) {
        return NULL;
    }
    (({{ object_struct }} *)self)->value = value;
    return self;
}
{% for field in fields %}
static PyObject *{{ prefix }}_get_{{ field.python_name }}(PyObject *self, void *closure) {
    (void)closure;
    return {{ field.boxer }}((({{ object_struct }} *)self)->value.{{ field.c_name }});
}
{% endfor %}
static PyGetSetDef {{ prefix }}_getset[] = {
{%- for field in fields %}
    {"{{ field.python_name }}", {{ prefix }}_get_{{ field.python_name }}, NULL, NULL, NULL},
{%- endfor %}
    {NULL, NULL, NULL, NULL, NULL}
};

static int {{ prefix }}_setattro(PyObject *self, PyObject *name, PyObject *set_value) {
    (void)self;
    (void)set_value;
    return boltffi_python_raise_frozen_field(name);
}

static PyObject *{{ prefix }}_richcompare(PyObject *self, PyObject *other, int op) {
    if ((op != Py_EQ && op != Py_NE) || !Py_IS_TYPE(other, Py_TYPE(self))) {
        Py_RETURN_NOTIMPLEMENTED;
    }
    const {{ c_type }} *left = &(({{ object_struct }} *)self)->value;
    const {{ c_type }} *right = &(({{ object_struct }} *)other)->value;
    int equal = 1{% for field in fields %}
        && left->{{ field.c_name }} == right->{{ field.c_name }}{% endfor %};
    return PyBool_FromLong(op == Py_EQ ? equal : !equal);
}

static PyObject *{{ prefix }}_field_tuple(PyObject *self) {
    PyObject *values = PyTuple_New({{ fields.len() }});
    PyObject *item = NULL;
    if (values == NULL) {
        return NULL;
    }
{%- for field in fields %}
    item = {{ field.boxer }}((({{ object_struct }} *)self)->value.{{ field.c_name }});
    if (item == NULL) {
        Py_DECREF(values);
        return NULL;
    }
    PyTuple_SET_ITEM(values, {{ loop.index0 }}, item);
{%- endfor %}
    return values;
}

static PyObject *{{ prefix }}_repr(PyObject *self) {
    PyObject *values = {{ prefix }}_field_tuple(self);
    if (values == NULL) {
        return NULL;
    }
    PyObject *text = PyUnicode_FromFormat(
        "{{ class_name }}({% for field in fields %}{{ field.python_name }}=%R{% if !loop.last %}, {% endif %}{% endfor %})"{% for field in fields %},
        PyTuple_GET_ITEM(values, {{ loop.index0 }}){% endfor %}
    );
    Py_DECREF(values);
    return text;
}

static Py_hash_t {{ prefix }}_hash(PyObject *self) {
    PyObject *values = {{ prefix }}_field_tuple(self);
    if (values == NULL) {
        return -1;
    }
    Py_hash_t result = PyObject_Hash(values);
    Py_DECREF(values);
    return result;
}

static PyObject *{{ prefix }}_reduce(PyObject *self, PyObject *unused) {
    (void)unused;
    PyObject *values = {{ prefix }}_field_tuple(self);
    if (values == NULL) {
        return NULL;
    }
    PyObject *state = PyTuple_Pack(2, (PyObject *)Py_TYPE(self), values);
    Py_DECREF(values);
    return state;
}

static PyMethodDef {{ prefix }}_type_methods[] = {
    {"__reduce__", (PyCFunction){{ prefix }}_reduce, METH_NOARGS, NULL},
    {NULL, NULL, 0, NULL}
};

static PyType_Slot {{ prefix }}_type_slots[] = {
    {Py_tp_new, (void *){{ prefix }}_new},
    {Py_tp_dealloc, (void *){{ prefix }}_dealloc},
    {Py_tp_getset, (void *){{ prefix }}_getset},
    {Py_tp_methods, (void *){{ prefix }}_type_methods},
    {Py_tp_setattro, (void *){{ prefix }}_setattro},
    {Py_tp_richcompare, (void *){{ prefix }}_richcompare},
    {Py_tp_repr, (void *){{ prefix }}_repr},
    {Py_tp_hash, (void *){{ prefix }}_hash},
    {0, NULL}
};

static PyType_Spec {{ prefix }}_type_spec = {
    "{{ class_name }}",
    sizeof({{ object_struct }}),
    0,
    Py_TPFLAGS_DEFAULT | Py_TPFLAGS_BASETYPE,
    {{ prefix }}_type_slots
};

static int {{ type_setup }}(PyObject *module) {
    {{ type_object }} = PyType_FromSpec(&{{ prefix }}_type_spec);
    if ({{ type_object }} == NULL) {
        return 0;
    }
    if (PyModule_AddObjectRef(module, "{{ class_name }}", {{ type_object }}) < 0) {
        return 0;
    }
    return 1;
}

static int {{ parser }}(PyObject *value, {{ c_type }} *out) {
    if (!boltffi_python_expect_type_instance(value, {{ type_object }}, "{{ class_name }}")) {
        return 0;
    }
    *out = (({{ object_struct }} *)value)->value;
    return 1;
}

static PyObject *{{ boxer }}({{ c_type }} value) {
    PyTypeObject *type = (PyTypeObject *){{ type_object }};
    PyObject *self = type->tp_alloc(type, 0);
    if (self == NULL) {
        return NULL;
    }
    (({{ object_struct }} *)self)->value = value;
    return self;
}