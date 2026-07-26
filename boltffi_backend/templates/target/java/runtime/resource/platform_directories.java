    private static List<String> desktopNativeDirectories() {
        String operatingSystem = System.getProperty("os.name", "")
            .toLowerCase(Locale.ROOT);
        String architecture = System.getProperty("os.arch", "")
            .toLowerCase(Locale.ROOT);
{% for platform in resource_platforms %}
        if (({% for operating_system in platform.operating_systems() %}operatingSystem.contains("{{ operating_system }}"){% if !loop.last %} || {% endif %}{% endfor %})
            && ({% for architecture_name in platform.architectures() %}architecture.equals("{{ architecture_name }}"){% if !loop.last %} || {% endif %}{% endfor %})) {
            return Arrays.asList(
{% for directory in platform.directories() %}                "{{ directory }}"{% if !loop.last %},{% endif %}
{% endfor %}            );
        }
{% endfor %}

        return Collections.emptyList();
    }

