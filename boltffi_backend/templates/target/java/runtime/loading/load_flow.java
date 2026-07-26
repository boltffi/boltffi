    private static void loadDesktopLibraries(
        Class<?> owner,
        String preferredLibrary,
        String fallbackLibrary
    ) {
        DesktopLibraries desktopLibraries = DesktopLibraries.prepare(
            owner,
            preferredLibrary
        );
        LoadResult preferredResult = desktopLibraries.loadPreferred(preferredLibrary);
        if (preferredResult.isLoaded()) {
            return;
        }

        if (!preferredLibrary.equals(fallbackLibrary)) {
            desktopLibraries = desktopLibraries.withFallback(fallbackLibrary);
        }
        LoadResult sharedResult = desktopLibraries.loadAfterPreferredFailure(
            preferredResult,
            preferredLibrary,
            fallbackLibrary
        );
        if (sharedResult.isLoaded()) {
            return;
        }
        throw sharedResult.failure();
    }

    private static UnsatisfiedLinkError nativeLibraryFailure(
        String message,
        Throwable cause
    ) {
        UnsatisfiedLinkError failure = new UnsatisfiedLinkError(message);
        failure.initCause(cause);
        return failure;
    }

