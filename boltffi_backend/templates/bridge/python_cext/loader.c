#define PY_SSIZE_T_CLEAN
#include <Python.h>
#include <float.h>
#include <limits.h>
#include <math.h>
#include <string.h>

#include {{ c_header }}

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

{%- for function in functions %}
{{ function.typedef_declaration }};
static {{ function.typedef_name }} {{ function.storage_name }} = NULL;
{%- endfor %}

#ifdef _WIN32
static HMODULE boltffi_python_library_handle = NULL;
#else
static void *boltffi_python_library_handle = NULL;
#endif

static void boltffi_python_release_host_state(void);
static int boltffi_python_bind_host_state(void);

static void boltffi_python_clear_symbols(void) {
{%- for function in functions %}
    {{ function.storage_name }} = NULL;
{%- endfor %}
}

static void boltffi_python_unload_library(void) {
    boltffi_python_clear_symbols();
    if (boltffi_python_library_handle == NULL) {
        return;
    }
#ifdef _WIN32
    FreeLibrary(boltffi_python_library_handle);
#else
    dlclose(boltffi_python_library_handle);
#endif
    boltffi_python_library_handle = NULL;
}

static int boltffi_python_load_library(PyObject *library_path) {
#ifdef _WIN32
    wchar_t *wide_library_path = NULL;
#else
    const char *utf8_library_path = NULL;
    const char *loader_error = NULL;
#endif
    if (!PyUnicode_Check(library_path)) {
        PyErr_SetString(PyExc_TypeError, "expected str library path");
        return 0;
    }
#ifdef _WIN32
    wide_library_path = PyUnicode_AsWideCharString(library_path, NULL);
    if (wide_library_path == NULL) {
        return 0;
    }
    boltffi_python_library_handle = LoadLibraryW(wide_library_path);
    PyMem_Free(wide_library_path);
    if (boltffi_python_library_handle == NULL) {
        PyErr_Format(PyExc_ImportError, "failed to load native library from %S", library_path);
        return 0;
    }
#else
    utf8_library_path = PyUnicode_AsUTF8(library_path);
    if (utf8_library_path == NULL) {
        return 0;
    }
    dlerror();
    boltffi_python_library_handle = dlopen(utf8_library_path, RTLD_NOW | RTLD_LOCAL);
    if (boltffi_python_library_handle == NULL) {
        loader_error = dlerror();
        if (loader_error == NULL) {
            PyErr_Format(PyExc_ImportError, "failed to load native library from %S", library_path);
        } else {
            PyErr_Format(PyExc_ImportError, "failed to load native library from %S: %s", library_path, loader_error);
        }
        return 0;
    }
#endif
    return 1;
}

static int boltffi_python_bind_symbols(void) {
{%- for function in functions %}
#ifdef _WIN32
    {{ function.storage_name }} = ({{ function.typedef_name }})GetProcAddress(boltffi_python_library_handle, {{ function.symbol }});
#else
    {{ function.storage_name }} = ({{ function.typedef_name }})dlsym(boltffi_python_library_handle, {{ function.symbol }});
#endif
    if ({{ function.storage_name }} == NULL) {
        boltffi_python_unload_library();
        PyErr_SetString(PyExc_ImportError, "failed to resolve native symbol " {{ function.symbol }});
        return 0;
    }
{%- endfor %}
    return 1;
}

static PyObject *{{ loader_function }}(PyObject *self, PyObject *library_path) {
    (void)self;
    if (boltffi_python_library_handle != NULL) {
        Py_RETURN_NONE;
    }
    if (!boltffi_python_load_library(library_path)) {
        return NULL;
    }
    if (!boltffi_python_bind_symbols()) {
        return NULL;
    }
    if (!boltffi_python_bind_host_state()) {
        return NULL;
    }
    Py_RETURN_NONE;
}

static void {{ free_function }}(void *module) {
    (void)module;
    boltffi_python_release_host_state();
    boltffi_python_unload_library();
}
