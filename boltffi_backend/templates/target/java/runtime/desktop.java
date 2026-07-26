package {{ package }};

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.FileStore;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardOpenOption;
import java.nio.file.attribute.AclEntry;
import java.nio.file.attribute.AclEntryFlag;
import java.nio.file.attribute.AclEntryPermission;
import java.nio.file.attribute.AclEntryType;
import java.nio.file.attribute.AclFileAttributeView;
import java.nio.file.attribute.BasicFileAttributes;
import java.nio.file.attribute.FileAttribute;
import java.nio.file.attribute.PosixFileAttributeView;
import java.nio.file.attribute.PosixFileAttributes;
import java.nio.file.attribute.PosixFilePermission;
import java.nio.file.attribute.PosixFilePermissions;
import java.nio.file.attribute.UserPrincipal;
import java.util.Arrays;
import java.util.Collections;
import java.util.EnumSet;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.stream.Collectors;

final class {{ runtime_owner }} {
    static void load(
        Class<?> owner,
        String preferredLibrary,
        String fallbackLibrary
    ) {
        loadDesktopLibraries(owner, preferredLibrary, fallbackLibrary);
    }

    private {{ runtime_owner }}() {}

{% include "target/java/runtime/loading/load_flow.java" -%}
{% include "target/java/runtime/resource/platform_directories.java" -%}
{% include "target/java/runtime/loading/load_result.java" -%}
{% include "target/java/runtime/loading/desktop_libraries.java" -%}
{% include "target/java/runtime/loading/bundled_library.java" -%}
{% include "target/java/runtime/extraction/directory_security.java" -%}
{% include "target/java/runtime/extraction/owned_directory.java" -%}
{% include "target/java/runtime/resource/bundled_resource.java" -%}
{% include "target/java/runtime/extraction/root.java" -%}
}
