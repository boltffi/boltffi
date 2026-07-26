@file:OptIn(kotlin.ExperimentalUnsignedTypes::class)

package {{ package }}
{%- if async_runtime || stream_runtime %}

import kotlinx.coroutines.launch
{%- endif %}
{%- if stream_runtime %}
import kotlinx.coroutines.channels.awaitClose
{%- endif %}

{{ runtime }}

@Suppress("FunctionName")
private object Native {
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

        if (tryLoadOptionalDesktopLibrary(fallbackLibrary)) {
            preferredFailure = tryLoadDesktopLibrary(preferredLibrary)
            if (preferredFailure == null) {
                return
            }
        }

        throw preferredFailure
    }

    private fun tryLoadDesktopLibrary(libraryName: String): UnsatisfiedLinkError? {
        try {
            if (loadBundledLibraryIfPresent(libraryName) || loadExternalLibraryIfPresent(libraryName)) {
                return null
            }
            return UnsatisfiedLinkError("Could not load native library '$libraryName'")
        } catch (error: UnsatisfiedLinkError) {
            return error
        }
    }

    private fun tryLoadOptionalDesktopLibrary(libraryName: String): Boolean {
        return try {
            loadBundledLibraryIfPresent(libraryName) || loadExternalLibraryIfPresent(libraryName)
        } catch (_: UnsatisfiedLinkError) {
            false
        }
    }

    private fun loadExternalLibraryIfPresent(libraryName: String): Boolean {
        return try {
            System.loadLibrary(libraryName)
            true
        } catch (_: UnsatisfiedLinkError) {
            false
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

{%- for function in native_functions %}
    @JvmStatic external fun {{ function.name() }}({% for parameter in function.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}): {{ function.returns() }}
{%- endfor %}
{%- if async_runtime %}

    @JvmStatic fun boltffiFutureContinuationCallback(handle: Long, pollResult: Byte) {
        BoltFfiAsync.resume(handle, pollResult)
    }
{%- endif %}
}
{%- if !closures.is_empty() %}

{{ closures }}
{%- endif %}

{{ declarations }}
