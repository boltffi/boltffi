    private static final class ExtractionRoot {
        private OwnedDirectory directory;
        private Throwable failure;

        private OwnedDirectory directory() throws IOException {
            if (directory != null) {
                return directory;
            }
            if (failure != null) {
                throwFailure();
            }
            try {
                directory = open();
                return directory;
            } catch (IOException | SecurityException openFailure) {
                failure = openFailure;
                throw openFailure;
            }
        }

        private void throwFailure() throws IOException {
            if (failure instanceof IOException) {
                throw (IOException) failure;
            }
            throw (SecurityException) failure;
        }

        private static OwnedDirectory open() throws IOException {
            String temporaryRoot = System.getProperty("java.io.tmpdir", "");
            if (temporaryRoot.isEmpty()) {
                throw new IOException("temporary directory property is unavailable");
            }
            Path temporaryDirectory = Paths.get(temporaryRoot).toRealPath();
            BasicFileAttributes attributes = Files.readAttributes(
                temporaryDirectory,
                BasicFileAttributes.class,
                LinkOption.NOFOLLOW_LINKS
            );
            if (attributes.isSymbolicLink() || !attributes.isDirectory()) {
                throw new IOException("temporary directory is unavailable");
            }
            DirectorySecurity security = DirectorySecurity.detect(temporaryDirectory);
            UserPrincipal owner = security.currentOwner(temporaryDirectory);
            return OwnedDirectory.temporary(
                temporaryDirectory,
                "boltffi-native-",
                security,
                owner
            );
        }

        private static void discard(Path path, Throwable failure) {
            if (path == null) {
                return;
            }
            try {
                Files.deleteIfExists(path);
            } catch (IOException | SecurityException cleanupFailure) {
                if (cleanupFailure != failure) {
                    failure.addSuppressed(cleanupFailure);
                }
            }
        }
    }
