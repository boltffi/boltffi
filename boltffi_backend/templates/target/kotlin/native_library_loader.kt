    init {
        val androidLibrary = {{ native_libraries.android() }}
        val desktopPreferredLibrary = {{ native_libraries.desktop_jni() }}
        val desktopFallbackLibrary = {{ native_libraries.desktop_fallback() }}
        val vmName = System.getProperty("java.vm.name").orEmpty()
        val isAndroidRuntime =
            vmName.contains("dalvik", ignoreCase = true) ||
            vmName.contains("art", ignoreCase = true)
        if (isAndroidRuntime) {
            System.loadLibrary(androidLibrary)
{%- if native_libraries.bundled_desktop_loader() %}
        } else {
            loadDesktopLibraries(desktopPreferredLibrary, desktopFallbackLibrary)
        }
{%- elif native_libraries.system_desktop_loader() %}
        } else {
            System.loadLibrary(desktopFallbackLibrary)
        }
{%- else %}
        }
{%- endif %}
    }
{%- if native_libraries.bundled_desktop_loader() %}

    @Volatile
    private var bundledLibraryDirectory: java.io.File? = null

    private fun loadDesktopLibraries(preferredLibrary: String, fallbackLibrary: String) {
        var preferredFailure = tryLoadDesktopLibrary(preferredLibrary)
        if (preferredFailure == null) {
            return
        }

        if (tryLoadDesktopLibrary(fallbackLibrary) == null) {
            preferredFailure = tryLoadDesktopLibrary(preferredLibrary)
            if (preferredFailure == null) {
                return
            }
        }

        val resources = bundledLibraryResourceCandidates(System.mapLibraryName(preferredLibrary))
        throw UnsatisfiedLinkError(
            "Could not load native library '$preferredLibrary'\n" +
                "Searched JVM resources ${resources.joinToString()} and java.library.path\n" +
                "For Kotlin Multiplatform, run boltffi pack kmp --experimental and include src/jvmMain/resources\n" +
                "For Android Kotlin bindings used on desktop, enable targets.android.kotlin.desktop_pack.enabled, " +
                "run boltffi pack android and include desktopJniLibs in the JVM resource path " +
                "or set targets.android.kotlin.desktop_pack.output to a JVM resource directory"
        ).apply { initCause(preferredFailure) }
    }

    private fun tryLoadDesktopLibrary(libraryName: String): UnsatisfiedLinkError? {
        val bundledFailure = try {
            if (loadBundledLibraryIfPresent(libraryName)) {
                return null
            }
            null
        } catch (error: UnsatisfiedLinkError) {
            error
        }
        return try {
            System.loadLibrary(libraryName)
            null
        } catch (error: UnsatisfiedLinkError) {
            bundledFailure?.apply { addSuppressed(error) } ?: error
        }
    }

    private fun loadBundledLibraryIfPresent(libraryName: String): Boolean {
        val mappedName = System.mapLibraryName(libraryName)
        for (resourcePath in bundledLibraryResourceCandidates(mappedName)) {
            Native::class.java.getResourceAsStream(resourcePath)?.use { input ->
                val extracted = extractBundledLibrary(resourcePath, input)
                System.load(extracted.absolutePath)
                return true
            }
        }
        return false
    }

    private fun extractBundledLibrary(
        resourcePath: String,
        input: java.io.InputStream,
    ): java.io.File {
        val fileName = resourcePath.substringAfterLast('/')
        val extracted = java.io.File(bundledLibraryDirectory(), fileName)
        if (!extracted.isFile) {
            java.io.FileOutputStream(extracted).use { output ->
                input.copyTo(output)
            }
            extracted.deleteOnExit()
        }
        return extracted
    }

    private fun bundledLibraryDirectory(): java.io.File {
        bundledLibraryDirectory?.let { return it }
        synchronized(this) {
            bundledLibraryDirectory?.let { return it }
            val created = java.io.File.createTempFile("boltffi-native-", "")
            if (!created.delete() || !created.mkdir()) {
                throw java.io.IOException("failed to create temp directory for bundled native extraction")
            }
            created.deleteOnExit()
            bundledLibraryDirectory = created
            return created
        }
    }

    private fun bundledLibraryResourceCandidates(mappedName: String): List<String> {
        val candidates = mutableListOf<String>()
        for (directory in desktopNativeDirectories()) {
            candidates += "/$directory/$mappedName"
            candidates += "/native/$directory/$mappedName"
        }
        candidates += "/$mappedName"
        return candidates
    }

    private fun desktopNativeDirectories(): List<String> {
        val osName = System.getProperty("os.name").orEmpty().lowercase()
        val osArch = System.getProperty("os.arch").orEmpty().lowercase()
        return when {
{% for platform in resource_platforms %}            ({% for operating_system in platform.operating_systems() %}osName.contains("{{ operating_system }}"){% if !loop.last %} || {% endif %}{% endfor %}) &&
                ({% for architecture in platform.architectures() %}osArch == "{{ architecture }}"{% if !loop.last %} || {% endif %}{% endfor %}) ->
                listOf({% for directory in platform.directories() %}"{{ directory }}"{% if !loop.last %}, {% endif %}{% endfor %})
{% endfor %}            else -> emptyList()
        }
    }
{%- endif %}
