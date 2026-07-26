    private enum DirectorySecurity {
        POSIX,
        ACL;

        private static DirectorySecurity detect(
            Path path
        ) throws IOException {
            FileStore store = Files.getFileStore(path);
            if (store.supportsFileAttributeView(
                AclFileAttributeView.class
            )) {
                return ACL;
            }
            if (store.supportsFileAttributeView(
                PosixFileAttributeView.class
            )) {
                return POSIX;
            }
            throw new IOException(
                "owner-only directory attributes are unavailable"
            );
        }

        private FileAttribute<?> ownerAttribute(
            UserPrincipal owner
        ) {
            return this == POSIX
                ? PosixFilePermissions.asFileAttribute(
                    ownerPermissions()
                )
                : aclAttribute(owner);
        }

        private void verify(
            Path directory,
            UserPrincipal expectedOwner
        ) throws IOException {
            verifyDirectory(directory);
            if (this == POSIX) {
                PosixFileAttributes attributes =
                    posixAttributes(directory);
                if (!attributes.owner().equals(expectedOwner)
                    || !attributes.permissions().equals(ownerPermissions())) {
                    throw new IOException(
                        "native library extraction directory is not owner-only"
                    );
                }
                return;
            }

            AclFileAttributeView view = aclView(directory);
            List<AclEntry> acl = view.getAcl();
            boolean ownerOnly = !acl.isEmpty()
                && acl.stream().allMatch(entry ->
                    entry.type() == AclEntryType.ALLOW
                        && entry.principal().equals(expectedOwner)
                        && entry.flags().equals(ownerAclFlags())
                );
            Set<AclEntryPermission> permissions =
                acl.stream()
                    .flatMap(entry -> entry.permissions().stream())
                    .collect(Collectors.toCollection(() ->
                        EnumSet.noneOf(
                            AclEntryPermission.class
                        )
                    ));
            if (!view.getOwner().equals(expectedOwner)
                || !ownerOnly
                || !permissions.containsAll(ownerAclPermissions())) {
                throw new IOException(
                    "native library extraction ACL is not owner-only"
                );
            }
        }

        private UserPrincipal currentOwner(
            Path parent
        ) throws IOException {
            Path probe = null;
            try {
                probe = Files.createTempFile(
                    parent,
                    "boltffi-owner-",
                    ".probe"
                );
                BasicFileAttributes attributes =
                    Files.readAttributes(
                        probe,
                        BasicFileAttributes.class,
                        LinkOption.NOFOLLOW_LINKS
                    );
                if (attributes.isSymbolicLink() || !attributes.isRegularFile()) {
                    throw new IOException(
                        "native extraction owner probe is not a regular file"
                    );
                }
                UserPrincipal owner = this == POSIX
                    ? posixAttributes(probe).owner()
                    : aclView(probe).getOwner();
                Files.delete(probe);
                return owner;
            } catch (IOException failure) {
                ExtractionRoot.discard(probe, failure);
                throw failure;
            } catch (SecurityException failure) {
                ExtractionRoot.discard(probe, failure);
                throw failure;
            }
        }

        private static Set<
            PosixFilePermission
        > ownerPermissions() {
            return EnumSet.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE,
                PosixFilePermission.OWNER_EXECUTE
            );
        }

        private static PosixFileAttributes
        posixAttributes(
            Path directory
        ) throws IOException {
            return Files.readAttributes(
                directory,
                PosixFileAttributes.class,
                LinkOption.NOFOLLOW_LINKS
            );
        }

        private static AclFileAttributeView aclView(
            Path directory
        ) throws IOException {
            AclFileAttributeView view =
                Files.getFileAttributeView(
                    directory,
                    AclFileAttributeView.class,
                    LinkOption.NOFOLLOW_LINKS
                );
            if (view == null) {
                throw new IOException(
                    "native library extraction ACL is unavailable"
                );
            }
            return view;
        }

        private static List<AclEntry> ownerAcl(
            UserPrincipal owner
        ) {
            AclEntry entry =
                AclEntry.newBuilder()
                    .setType(AclEntryType.ALLOW)
                    .setPrincipal(owner)
                    .setPermissions(ownerAclPermissions())
                    .setFlags(ownerAclFlags())
                    .build();
            return Collections.singletonList(entry);
        }

        private static Set<
            AclEntryPermission
        > ownerAclPermissions() {
            return EnumSet.allOf(
                AclEntryPermission.class
            );
        }

        private static Set<
            AclEntryFlag
        > ownerAclFlags() {
            return EnumSet.of(
                AclEntryFlag.FILE_INHERIT,
                AclEntryFlag.DIRECTORY_INHERIT
            );
        }

        private static FileAttribute<
            List<AclEntry>
        > aclAttribute(
            final UserPrincipal owner
        ) {
            final List<AclEntry> acl = ownerAcl(owner);
            return new FileAttribute<
                List<AclEntry>
            >() {
                public String name() {
                    return "acl:acl";
                }

                public List<AclEntry> value() {
                    return acl;
                }
            };
        }

        private static void verifyDirectory(
            Path directory
        ) throws IOException {
            BasicFileAttributes attributes =
                Files.readAttributes(
                    directory,
                    BasicFileAttributes.class,
                    LinkOption.NOFOLLOW_LINKS
                );
            if (attributes.isSymbolicLink() || !attributes.isDirectory()) {
                throw new IOException(
                    "native library extraction path is not a directory"
                );
            }
        }
    }

