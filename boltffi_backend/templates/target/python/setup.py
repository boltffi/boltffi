import sys

from setuptools import Extension, setup

extension_compile_args = ["/std:c11", "/experimental:c11atomics"] if sys.platform == "win32" else []

setup(
    name={{ package_name_literal }},
    version={{ package_version_literal }},
    python_requires=">=3.10",
    packages=[{{ module_name_literal }}],
    package_data={ {{ module_name_literal }}: ["py.typed", "*.pyi", "*.dll", "*.dylib", "*.so"] },
    ext_modules=[
        Extension(
            {{ extension_name_literal }},
            sources=[{{ extension_source_literal }}],
            extra_compile_args=extension_compile_args,
        ),
    ],
    zip_safe=False,
)
