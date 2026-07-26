    private static final class OwnedDirectory {
        private final Path path;

        private OwnedDirectory(Path path) {
            this.path = path;
        }

        private static OwnedDirectory temporary(
            Path parent,
            String prefix,
            DirectorySecurity security,
            UserPrincipal owner
        ) throws IOException {
            Path path = null;
            try {
                path = Files.createTempDirectory(
                    parent,
                    prefix,
                    security.ownerAttribute(owner)
                );
                security.verify(path, owner);
                path.toFile().deleteOnExit();
                return new OwnedDirectory(path);
            } catch (IOException failure) {
                ExtractionRoot.discard(path, failure);
                throw failure;
            } catch (SecurityException failure) {
                ExtractionRoot.discard(path, failure);
                throw failure;
            } catch (UnsupportedOperationException unsupported) {
                IOException failure = new IOException(
                    "owner-only directory attributes are unavailable",
                    unsupported
                );
                ExtractionRoot.discard(path, failure);
                throw failure;
            }
        }

        private File extract(
            String mappedName,
            InputStream input
        ) throws IOException {
            Path destination = child(mappedName);
            try {
                try (
                    OutputStream output = Files.newOutputStream(
                        destination,
                        StandardOpenOption.CREATE_NEW,
                        StandardOpenOption.WRITE
                    )
                ) {
                    byte[] buffer = new byte[8192];
                    int bytesRead;
                    while ((bytesRead = input.read(buffer)) != -1) {
                        output.write(buffer, 0, bytesRead);
                    }
                }
                BasicFileAttributes attributes = Files.readAttributes(
                    destination,
                    BasicFileAttributes.class,
                    LinkOption.NOFOLLOW_LINKS
                );
                if (attributes.isSymbolicLink() || !attributes.isRegularFile()) {
                    throw new IOException(
                        "extracted native library is not a regular file"
                    );
                }
                File extracted = destination.toFile();
                extracted.deleteOnExit();
                return extracted;
            } catch (IOException failure) {
                ExtractionRoot.discard(destination, failure);
                throw failure;
            } catch (SecurityException failure) {
                ExtractionRoot.discard(destination, failure);
                throw failure;
            }
        }

        private Path child(String name) throws IOException {
            Path child = path.resolve(name).normalize();
            if (!path.equals(child.getParent())
                || child.getFileName() == null
                || !child.getFileName().toString().equals(name)) {
                throw new IOException("invalid native library extraction path");
            }
            return child;
        }
    }
