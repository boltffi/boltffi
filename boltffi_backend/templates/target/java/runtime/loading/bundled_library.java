    private static final class BundledLibrary {
        private final File file;
        private final String failureMessage;
        private final Throwable failureCause;

        private BundledLibrary(
            File file,
            String failureMessage,
            Throwable failureCause
        ) {
            this.file = file;
            this.failureMessage = failureMessage;
            this.failureCause = failureCause;
        }

        private static BundledLibrary extract(
            Class<?> owner,
            String libraryName,
            ExtractionRoot extraction
        ) {
            String mappedName = System.mapLibraryName(libraryName);
            try {
                validateMappedName(mappedName);
                BundledResource resource = BundledResource.find(owner, mappedName);
                if (resource == null) {
                    return absent();
                }
                try (InputStream input = resource.input) {
                    return extracted(
                        extraction.directory().extract(mappedName, input)
                    );
                }
            } catch (IOException failure) {
                return failed(
                    "Could not extract bundled native library '" + mappedName + "'",
                    failure
                );
            } catch (SecurityException failure) {
                return failed(
                    "Could not access bundled native library '" + mappedName + "'",
                    failure
                );
            }
        }

        private static BundledLibrary absent() {
            return new BundledLibrary(null, null, null);
        }

        private static BundledLibrary extracted(File file) {
            return new BundledLibrary(file, null, null);
        }

        private static BundledLibrary failed(
            String message,
            Throwable cause
        ) {
            return new BundledLibrary(null, message, cause);
        }

        private LoadResult tryLoad(String libraryName) {
            if (file == null) {
                UnsatisfiedLinkError failure = extractionFailure();
                return failure == null
                    ? LoadResult.unavailable()
                    : LoadResult.failed(failure);
            }

            try {
                System.load(file.getAbsolutePath());
                return LoadResult.loaded();
            } catch (UnsatisfiedLinkError failure) {
                return LoadResult.failed(failure);
            } catch (SecurityException failure) {
                return LoadResult.failed(nativeLibraryFailure(
                    "Could not load bundled native library '" + libraryName + "'",
                    failure
                ));
            }
        }

        private UnsatisfiedLinkError extractionFailure() {
            return failureMessage == null
                ? null
                : nativeLibraryFailure(failureMessage, failureCause);
        }

        private static void validateMappedName(
            String mappedName
        ) throws IOException {
            if (mappedName.isEmpty()
                || mappedName.indexOf('/') >= 0
                || mappedName.indexOf('\\') >= 0) {
                throw new IOException(
                    "invalid mapped native library name '" + mappedName + "'"
                );
            }
        }
    }

