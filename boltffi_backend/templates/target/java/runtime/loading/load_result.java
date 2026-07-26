    private static final class LoadResult {
        private final boolean loaded;
        private final UnsatisfiedLinkError failure;

        private LoadResult(
            boolean loaded,
            UnsatisfiedLinkError failure
        ) {
            this.loaded = loaded;
            this.failure = failure;
        }

        private static LoadResult loaded() {
            return new LoadResult(true, null);
        }

        private static LoadResult unavailable() {
            return new LoadResult(false, null);
        }

        private static LoadResult failed(UnsatisfiedLinkError failure) {
            return new LoadResult(false, failure);
        }

        private static LoadResult system(String libraryName) {
            try {
                System.loadLibrary(libraryName);
                return loaded();
            } catch (UnsatisfiedLinkError failure) {
                return failed(failure);
            } catch (SecurityException failure) {
                return failed(nativeLibraryFailure(
                    "Could not load system native library '" + libraryName + "'",
                    failure
                ));
            }
        }

        private boolean isLoaded() {
            return loaded;
        }

        private UnsatisfiedLinkError failure() {
            return failure;
        }

        private LoadResult merge(LoadResult other) {
            if (loaded) {
                return this;
            }
            if (other.loaded) {
                return other;
            }
            if (failure == null) {
                return other;
            }
            if (other.failure != null) {
                failure.addSuppressed(other.failure);
            }
            return new LoadResult(false, failure);
        }
    }

