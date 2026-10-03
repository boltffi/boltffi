import sys

from setuptools import Extension, find_packages, setup

extension_compile_args = ["/std:c11", "/experimental:c11atomics"] if sys.platform == "win32" else []

setup(
    name={{ package_name_literal }},
    version={{ package_version_literal }},
    python_requires={{ python_requires_literal }},
    packages=find_packages(include=[{{ module_name_literal }}, {{ subpackages_literal }}]),
    package_data={
        "": ["py.typed", "*.pyi"],
        {{ module_name_literal }}: ["*.dll", "*.dylib", "*.so"],
    },
    ext_modules=[
        Extension(
            {{ extension_name_literal }},
            sources=[{{ extension_source_literal }}],
            extra_compile_args=extension_compile_args,
        ),
    ],
{%- if !console_scripts.is_empty() %}
    entry_points={
        "console_scripts": [
{%- for script in console_scripts %}
            {{ script }},
{%- endfor %}
        ],
    },
{%- endif %}
    zip_safe=False,
)
