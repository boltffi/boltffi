    private static final class DesktopLibraries {
        private final Class<?> owner;
        private final ExtractionRoot extraction;
        private final BundledLibrary preferred;
        private final BundledLibrary fallback;

        private DesktopLibraries(
            Class<?> owner,
            ExtractionRoot extraction,
            BundledLibrary preferred,
            BundledLibrary fallback
        ) {
            this.owner = owner;
            this.extraction = extraction;
            this.preferred = preferred;
            this.fallback = fallback;
        }

        private static DesktopLibraries prepare(
            Class<?> owner,
            String preferredName
        ) {
            ExtractionRoot extraction = new ExtractionRoot();
            BundledLibrary library = BundledLibrary.extract(
                owner,
                preferredName,
                extraction
            );
            return new DesktopLibraries(owner, extraction, library, library);
        }

        private DesktopLibraries withFallback(String fallbackName) {
            BundledLibrary fallbackLibrary = BundledLibrary.extract(
                owner,
                fallbackName,
                extraction
            );
            return new DesktopLibraries(
                owner,
                extraction,
                preferred,
                fallbackLibrary
            );
        }

        private LoadResult loadAfterPreferredFailure(
            LoadResult preferredResult,
            String preferredName,
            String fallbackName
        ) {
            if (preferredName.equals(fallbackName)) {
                return preferredResult;
            }

            LoadResult fallbackResult = loadFallback(fallbackName);
            if (!fallbackResult.isLoaded()) {
                return preferredResult.merge(fallbackResult);
            }

            return preferredResult.merge(loadPreferred(preferredName));
        }

        private LoadResult loadPreferred(String libraryName) {
            return load(preferred, libraryName);
        }

        private LoadResult loadFallback(String libraryName) {
            return load(fallback, libraryName);
        }

        private LoadResult load(
            BundledLibrary bundled,
            String libraryName
        ) {
            LoadResult bundledResult = bundled.tryLoad(libraryName);
            return bundledResult.isLoaded()
                ? bundledResult
                : bundledResult.merge(LoadResult.system(libraryName));
        }
    }

