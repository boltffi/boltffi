    private static final class BundledResource {
        private final InputStream input;

        private BundledResource(InputStream input) {
            this.input = input;
        }

        private static BundledResource find(
            Class<?> owner,
            String mappedName
        ) {
            BundledResource resource = find(
                owner,
                desktopNativeDirectories().iterator(),
                mappedName
            );
            return resource == null
                ? openPath(owner, "/" + mappedName)
                : resource;
        }

        private static BundledResource find(
            Class<?> owner,
            Iterator<String> directories,
            String mappedName
        ) {
            if (!directories.hasNext()) {
                return null;
            }
            String directory = directories.next();
            BundledResource resource = openPath(
                owner,
                "/" + directory + "/" + mappedName
            );
            if (resource != null) {
                return resource;
            }
            resource = openPath(
                owner,
                "/native/" + directory + "/" + mappedName
            );
            return resource == null
                ? find(owner, directories, mappedName)
                : resource;
        }

        private static BundledResource openPath(
            Class<?> owner,
            String resourcePath
        ) {
            InputStream input = owner.getResourceAsStream(resourcePath);
            return input == null ? null : new BundledResource(input);
        }
    }

