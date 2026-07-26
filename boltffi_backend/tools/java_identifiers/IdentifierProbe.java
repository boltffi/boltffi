public final class IdentifierProbe {
    private static final int MAX_CODE_POINT = 0x10ffff;
    private static final int SURROGATE_START = 0xd800;
    private static final int SURROGATE_END = 0xdfff;

    private IdentifierProbe() {}

    public static void main(String[] arguments) {
        System.out.println("release\t" + System.getProperty("java.specification.version"));
        emit(IdentifierProperty.START);
        emit(IdentifierProperty.PART);
        emit(IdentifierProperty.IGNORABLE);
    }

    private static void emit(IdentifierProperty property) {
        int rangeStart = -1;
        int previous = -1;
        int codePoint = 0;
        while (codePoint <= MAX_CODE_POINT) {
            boolean member = !surrogate(codePoint) && property.contains(codePoint);
            if (member && rangeStart < 0) {
                rangeStart = codePoint;
            }
            if (!member && rangeStart >= 0) {
                emitRange(property, rangeStart, previous);
                rangeStart = -1;
            }
            previous = codePoint;
            codePoint += 1;
        }
        if (rangeStart >= 0) {
            emitRange(property, rangeStart, MAX_CODE_POINT);
        }
    }

    private static boolean surrogate(int codePoint) {
        return codePoint >= SURROGATE_START && codePoint <= SURROGATE_END;
    }

    private static void emitRange(IdentifierProperty property, int start, int end) {
        System.out.println(
                property.spelling
                        + "\t"
                        + Integer.toHexString(start)
                        + "\t"
                        + Integer.toHexString(end));
    }

    private enum IdentifierProperty {
        START("start") {
            @Override
            boolean contains(int codePoint) {
                return Character.isJavaIdentifierStart(codePoint);
            }
        },
        PART("part") {
            @Override
            boolean contains(int codePoint) {
                return Character.isJavaIdentifierPart(codePoint);
            }
        },
        IGNORABLE("ignorable") {
            @Override
            boolean contains(int codePoint) {
                return Character.isIdentifierIgnorable(codePoint);
            }
        };

        private final String spelling;

        IdentifierProperty(String spelling) {
            this.spelling = spelling;
        }

        abstract boolean contains(int codePoint);
    }
}
