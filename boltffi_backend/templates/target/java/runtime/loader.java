    static {
        String virtualMachineName = System.getProperty("java.vm.name", "")
            .toLowerCase(java.util.Locale.ROOT);
        boolean androidRuntime = virtualMachineName.contains("dalvik")
            || virtualMachineName.contains("art");
        if (androidRuntime) {
            System.loadLibrary({{ libraries.android_literal() }});
{%- if libraries.bundled_desktop_loader() %}
        } else {
            {{ runtime_owner }}.load(
                {{ owner }}.class,
                {{ libraries.desktop_jni_literal() }},
                {{ libraries.desktop_fallback_literal() }}
            );
{%- elif libraries.system_desktop_loader() %}
        } else {
            System.loadLibrary({{ libraries.desktop_fallback_literal() }});
{%- endif %}
        }
    }
