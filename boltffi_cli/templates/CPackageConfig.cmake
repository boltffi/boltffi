get_filename_component(_{{ name }}_prefix "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
set({{ name }}_VERSION {{ version }})

if(NOT TARGET {{ name }}::{{ name }})
    add_library({{ name }}::{{ name }} SHARED IMPORTED)
    set_target_properties({{ name }}::{{ name }} PROPERTIES
        IMPORTED_LOCATION "${_{{ name }}_prefix}/lib/{{ shared_library }}"
{% if let Some(import_library) = import_library %}
        IMPORTED_IMPLIB "${_{{ name }}_prefix}/lib/{{ import_library }}"
{% endif %}
{% if no_soname %}
        IMPORTED_NO_SONAME TRUE
{% endif %}
        INTERFACE_INCLUDE_DIRECTORIES "${_{{ name }}_prefix}/include")
endif()

if(NOT TARGET {{ name }}::{{ name }}_static)
    add_library({{ name }}::{{ name }}_static STATIC IMPORTED)
    set_target_properties({{ name }}::{{ name }}_static PROPERTIES
        IMPORTED_LOCATION "${_{{ name }}_prefix}/lib/static/{{ static_library }}"
        INTERFACE_INCLUDE_DIRECTORIES "${_{{ name }}_prefix}/include")
    set_property(TARGET {{ name }}::{{ name }}_static PROPERTY
        INTERFACE_LINK_LIBRARIES {{ static_dependencies }})
    set_property(TARGET {{ name }}::{{ name }}_static PROPERTY
        INTERFACE_LINK_OPTIONS {{ static_link_options }})
endif()

unset(_{{ name }}_prefix)
