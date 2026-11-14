    [StructLayout(LayoutKind.Sequential)]
    internal struct FfiBuf
    {
        internal nint ptr;
        internal nuint len;
        internal nuint cap;
        internal nuint align;

        internal static FfiBuf FromBytes(byte[] bytes) =>
            NativeMethods.BufFromBytes(bytes, (nuint)bytes.Length);

        internal static FfiBuf FromUnexpectedCallbackError(global::System.Exception error)
        {
            byte[] message = global::System.Text.Encoding.UTF8.GetBytes(error.ToString());
            return NativeMethods.CallbackError(message, (nuint)message.Length);
        }

{% if features.system_memory %}        internal static FfiBuf FromRawArray<T>(T[] values) where T : unmanaged =>
            FromBytes(global::System.Runtime.InteropServices.MemoryMarshal.AsBytes(global::System.MemoryExtensions.AsSpan(values)).ToArray());
{% else %}        internal static FfiBuf FromRawArray<T>(T[] values) where T : unmanaged
        {
            byte[] bytes = new byte[checked(values.Length * {{ features.element_size("T") }})];
            if (bytes.Length != 0)
            {
                global::System.Runtime.InteropServices.GCHandle pin = global::System.Runtime.InteropServices.GCHandle.Alloc(values, global::System.Runtime.InteropServices.GCHandleType.Pinned);
                try { Marshal.Copy(pin.AddrOfPinnedObject(), bytes, 0, bytes.Length); }
                finally { pin.Free(); }
            }
            return FromBytes(bytes);
        }
{% endif %}
        internal static FfiBuf FromRawBoolArray(bool[] values)
        {
            byte[] bytes = new byte[values.Length];
            for (int index = 0; index < values.Length; index++) bytes[index] = values[index] ? (byte)1 : (byte)0;
            return FromBytes(bytes);
        }
    }

{% if !features.array_empty %}    internal static class BoltFFIEmptyArray<T>
    {
        internal static readonly T[] Value = new T[0];
    }

{% endif %}{% if !features.bit_converter_single_bits %}    [StructLayout(LayoutKind.Explicit)]
    internal struct BoltFFISingleBits
    {
        [FieldOffset(0)] internal int bits;
        [FieldOffset(0)] internal float value;
    }

{% endif %}    internal sealed class WireReader
    {
{% if !features.unix_epoch %}        private static readonly global::System.DateTime UnixEpoch = new global::System.DateTime(1970, 1, 1, 0, 0, 0, global::System.DateTimeKind.Utc);

{% endif %}        private readonly nint pointer;
        private readonly int length;
        private int position;

        internal WireReader(FfiBuf buffer) : this(buffer.ptr, buffer.len) { }

        internal WireReader(nint pointer, nuint length)
        {
            this.pointer = pointer;
            this.length = pointer == 0 ? 0 : checked((int)length);
        }

        internal bool ReadBool() => ReadU8() != 0;

        internal sbyte ReadI8()
        {
            Require(1, "i8");
            sbyte value = (sbyte)Marshal.ReadByte(pointer, position);
            position += 1;
            return value;
        }

        internal WireReader Skip(int count)
        {
            Require(count, "padding");
            position += count;
            return this;
        }

        internal byte ReadU8()
        {
            Require(1, "u8");
            byte value = Marshal.ReadByte(pointer, position);
            position += 1;
            return value;
        }

        internal short ReadI16()
        {
            Require(2, "i16");
            short value = Marshal.ReadInt16(pointer, position);
            position += 2;
            return global::System.BitConverter.IsLittleEndian
                ? value
                : {% if features.system_memory %}global::System.Buffers.Binary.BinaryPrimitives.{% endif %}ReverseEndianness(value);
        }

        internal ushort ReadU16() => unchecked((ushort)ReadI16());

        internal int ReadI32()
        {
            Require(4, "i32");
            int value = Marshal.ReadInt32(pointer, position);
            position += 4;
            return global::System.BitConverter.IsLittleEndian
                ? value
                : {% if features.system_memory %}global::System.Buffers.Binary.BinaryPrimitives.{% endif %}ReverseEndianness(value);
        }

        internal uint ReadU32() => unchecked((uint)ReadI32());

        internal long ReadI64()
        {
            Require(8, "i64");
            long value = Marshal.ReadInt64(pointer, position);
            position += 8;
            return global::System.BitConverter.IsLittleEndian
                ? value
                : {% if features.system_memory %}global::System.Buffers.Binary.BinaryPrimitives.{% endif %}ReverseEndianness(value);
        }

        internal ulong ReadU64() => unchecked((ulong)ReadI64());

{% if features.bit_converter_single_bits %}        internal float ReadF32() => global::System.BitConverter.Int32BitsToSingle(ReadI32());
{% else %}        internal float ReadF32() => new BoltFFISingleBits { bits = ReadI32() }.value;
{% endif %}        internal double ReadF64() => global::System.BitConverter.Int64BitsToDouble(ReadI64());
        internal nint ReadNInt() => checked((nint)ReadI64());
        internal nuint ReadNUInt() => checked((nuint)ReadU64());

        internal string ReadString()
        {
            int count = ReadI32();
            if (count < 0) throw new global::System.InvalidOperationException("corrupt wire: negative string length");
            if (count == 0) return string.Empty;
            Require(count, "string payload");
{% if features.marshal_utf8_strings %}            string value = Marshal.PtrToStringUTF8(pointer + position, count)
                ?? throw new global::System.InvalidOperationException("corrupt wire: null string");
{% else %}            byte[] bytes = new byte[count];
            Marshal.Copy(pointer + position, bytes, 0, count);
            string value = global::System.Text.Encoding.UTF8.GetString(bytes);
{% endif %}            position += count;
            return value;
        }

        internal byte[] ReadBytes()
        {
            int count = ReadI32();
            if (count < 0) throw new global::System.InvalidOperationException("corrupt wire: negative byte length");
            if (count == 0) return {{ features.empty_array("byte") }};
            Require(count, "byte payload");
            byte[] value = new byte[count];
            Marshal.Copy(pointer + position, value, 0, count);
            position += count;
            return value;
        }

        internal T[] ReadRawArray<T>() where T : unmanaged
        {
            int byteCount = length - position;
            if (byteCount == 0) return {{ features.empty_array("T") }};
            int elementSize = {{ features.element_size("T") }};
            if (byteCount % elementSize != 0)
                throw new global::System.InvalidOperationException("corrupt direct vector: partial element");
            byte[] bytes = new byte[byteCount];
            Marshal.Copy(pointer + position, bytes, 0, byteCount);
            position += byteCount;
{% if features.system_memory %}            return global::System.Runtime.InteropServices.MemoryMarshal.Cast<byte, T>(bytes).ToArray();
{% else %}            T[] values = new T[byteCount / elementSize];
            global::System.Runtime.InteropServices.GCHandle pin = global::System.Runtime.InteropServices.GCHandle.Alloc(values, global::System.Runtime.InteropServices.GCHandleType.Pinned);
            try { Marshal.Copy(bytes, 0, pin.AddrOfPinnedObject(), byteCount); }
            finally { pin.Free(); }
            return values;
{% endif %}        }

        internal bool[] ReadRawBoolArray()
        {
            int count = length - position;
            if (count == 0) return {{ features.empty_array("bool") }};
            bool[] values = new bool[count];
            for (int index = 0; index < count; index++)
                values[index] = Marshal.ReadByte(pointer, position + index) != 0;
            position += count;
            return values;
        }

        internal global::System.TimeSpan ReadDuration()
        {
            long seconds = ReadI64();
            int nanos = ReadI32();
            if (seconds < 0 || nanos < 0 || nanos >= 1_000_000_000)
                throw new global::System.InvalidOperationException("corrupt wire: invalid duration");
            return new global::System.TimeSpan(checked(
                seconds * global::System.TimeSpan.TicksPerSecond + nanos / 100));
        }

        internal global::System.DateTime ReadDateTime()
        {
            long seconds = ReadI64();
            int nanos = ReadI32();
            if (nanos < 0 || nanos >= 1_000_000_000)
                throw new global::System.InvalidOperationException("corrupt wire: invalid date-time");
            return {% if features.unix_epoch %}global::System.DateTime.UnixEpoch{% else %}UnixEpoch{% endif %}.AddTicks(checked(
                seconds * global::System.TimeSpan.TicksPerSecond + nanos / 100));
        }

        internal global::System.Guid ReadGuid()
        {
            long mostSignificant = ReadI64();
            long leastSignificant = ReadI64();
{% if features.guid_big_endian %}            global::System.Span<byte> bytes = stackalloc byte[16];
            global::System.Buffers.Binary.BinaryPrimitives.WriteInt64BigEndian(bytes[..8], mostSignificant);
            global::System.Buffers.Binary.BinaryPrimitives.WriteInt64BigEndian(bytes[8..], leastSignificant);
            return new global::System.Guid(bytes, bigEndian: true);
{% else %}            return new global::System.Guid(
                unchecked((int)(mostSignificant >> 32)),
                unchecked((short)(mostSignificant >> 16)),
                unchecked((short)mostSignificant),
                unchecked((byte)(leastSignificant >> 56)),
                unchecked((byte)(leastSignificant >> 48)),
                unchecked((byte)(leastSignificant >> 40)),
                unchecked((byte)(leastSignificant >> 32)),
                unchecked((byte)(leastSignificant >> 24)),
                unchecked((byte)(leastSignificant >> 16)),
                unchecked((byte)(leastSignificant >> 8)),
                unchecked((byte)leastSignificant));
{% endif %}        }

        internal global::System.Uri ReadUri() => new global::System.Uri(ReadString());

        internal T[] ReadArray<T>(global::System.Func<WireReader, T> read)
        {
            int count = checked((int)ReadU32());
            T[] values = new T[count];
            for (int index = 0; index < count; index++) values[index] = read(this);
            return values;
        }

        internal BoltFFIResult<TOk, TErr> ReadResult<TOk, TErr>(
            global::System.Func<WireReader, TOk> readOk,
            global::System.Func<WireReader, TErr> readErr) =>
            ReadU8() switch
            {
                0 => BoltFFIResult<TOk, TErr>.Ok(readOk(this)),
                1 => BoltFFIResult<TOk, TErr>.Err(readErr(this)),
                _ => throw new global::System.InvalidOperationException("corrupt wire: invalid result tag"),
            };

        internal global::System.Collections.Generic.Dictionary<TKey, TValue> ReadMap<TKey, TValue>(
            global::System.Func<WireReader, TKey> readKey,
            global::System.Func<WireReader, TValue> readValue)
            where TKey : notnull
        {
            int count = checked((int)ReadU32());
            var values = new global::System.Collections.Generic.Dictionary<TKey, TValue>(count);
            for (int index = 0; index < count; index++)
                values.Add(readKey(this), readValue(this));
            return values;
        }

{% if !features.system_memory %}        private static short ReverseEndianness(short value) =>
            unchecked((short)(((value & 0xFF) << 8) | ((value >> 8) & 0xFF)));

        private static int ReverseEndianness(int value) =>
            unchecked((int)(((uint)value << 24) | (((uint)value & 0xFF00u) << 8) | (((uint)value >> 8) & 0xFF00u) | ((uint)value >> 24)));

        private static long ReverseEndianness(long value) =>
            unchecked((long)(((ulong)(uint)ReverseEndianness((int)value) << 32) | (uint)ReverseEndianness((int)(value >> 32))));

{% endif %}        private void Require(int count, string kind)
        {
            if (count < 0 || count > length - position)
                throw new global::System.InvalidOperationException("corrupt wire: truncated " + kind);
        }
    }

    internal sealed class WireWriter
    {
{% if !features.unix_epoch %}        private static readonly global::System.DateTime UnixEpoch = new global::System.DateTime(1970, 1, 1, 0, 0, 0, global::System.DateTimeKind.Utc);

{% endif %}        private byte[] buffer = new byte[64];
        private int position;

        internal byte[] ToArray()
        {
            if (position == 0) return {{ features.empty_array("byte") }};
            byte[] value = new byte[position];
            global::System.Buffer.BlockCopy(buffer, 0, value, 0, position);
            return value;
        }

        internal void WriteBool(bool value) => WriteU8(value ? (byte)1 : (byte)0);
        internal void WriteI8(sbyte value) => WriteU8(unchecked((byte)value));

        internal void Pad(int count)
        {
            EnsureCapacity(count);
            global::System.Array.Clear(buffer, position, count);
            position += count;
        }

        internal void WriteU8(byte value)
        {
            EnsureCapacity(1);
            buffer[position++] = value;
        }

{% if features.system_memory %}        internal void WriteI16(short value)
        {
            EnsureCapacity(2);
            global::System.Buffers.Binary.BinaryPrimitives.WriteInt16LittleEndian(global::System.MemoryExtensions.AsSpan(buffer, position), value);
            position += 2;
        }

        internal void WriteU16(ushort value)
        {
            EnsureCapacity(2);
            global::System.Buffers.Binary.BinaryPrimitives.WriteUInt16LittleEndian(global::System.MemoryExtensions.AsSpan(buffer, position), value);
            position += 2;
        }

        internal void WriteI32(int value)
        {
            EnsureCapacity(4);
            global::System.Buffers.Binary.BinaryPrimitives.WriteInt32LittleEndian(global::System.MemoryExtensions.AsSpan(buffer, position), value);
            position += 4;
        }

        internal void WriteU32(uint value)
        {
            EnsureCapacity(4);
            global::System.Buffers.Binary.BinaryPrimitives.WriteUInt32LittleEndian(global::System.MemoryExtensions.AsSpan(buffer, position), value);
            position += 4;
        }

        internal void WriteI64(long value)
        {
            EnsureCapacity(8);
            global::System.Buffers.Binary.BinaryPrimitives.WriteInt64LittleEndian(global::System.MemoryExtensions.AsSpan(buffer, position), value);
            position += 8;
        }

        internal void WriteU64(ulong value)
        {
            EnsureCapacity(8);
            global::System.Buffers.Binary.BinaryPrimitives.WriteUInt64LittleEndian(global::System.MemoryExtensions.AsSpan(buffer, position), value);
            position += 8;
        }
{% else %}        internal void WriteI16(short value) => WriteLittleEndian(unchecked((ushort)value), 2);
        internal void WriteU16(ushort value) => WriteLittleEndian(value, 2);
        internal void WriteI32(int value) => WriteLittleEndian(unchecked((uint)value), 4);
        internal void WriteU32(uint value) => WriteLittleEndian(value, 4);
        internal void WriteI64(long value) => WriteLittleEndian(unchecked((ulong)value), 8);
        internal void WriteU64(ulong value) => WriteLittleEndian(value, 8);

        private void WriteLittleEndian(ulong value, int count)
        {
            EnsureCapacity(count);
            for (int index = 0; index < count; index++)
                buffer[position + index] = unchecked((byte)(value >> (8 * index)));
            position += count;
        }
{% endif %}
{% if features.bit_converter_single_bits %}        internal void WriteF32(float value) => WriteI32(global::System.BitConverter.SingleToInt32Bits(value));
{% else %}        internal void WriteF32(float value) => WriteI32(new BoltFFISingleBits { value = value }.bits);
{% endif %}        internal void WriteF64(double value) => WriteI64(global::System.BitConverter.DoubleToInt64Bits(value));
        internal void WriteNInt(nint value) => WriteI64((long)value);
        internal void WriteNUInt(nuint value) => WriteU64((ulong)value);

        internal void WriteString(string value)
        {
            int count = global::System.Text.Encoding.UTF8.GetByteCount(value);
            WriteI32(count);
            if (count == 0) return;
            EnsureCapacity(count);
            global::System.Text.Encoding.UTF8.GetBytes(value, 0, value.Length, buffer, position);
            position += count;
        }

        internal void WriteBytes(byte[] value)
        {
            WriteI32(value.Length);
            if (value.Length == 0) return;
            EnsureCapacity(value.Length);
            global::System.Buffer.BlockCopy(value, 0, buffer, position, value.Length);
            position += value.Length;
        }

        internal void WriteDuration(global::System.TimeSpan value)
        {
            if (value.Ticks < 0) throw new global::System.ArgumentException("duration must be non-negative", nameof(value));
            WriteI64(value.Ticks / global::System.TimeSpan.TicksPerSecond);
            WriteI32(checked((int)((value.Ticks % global::System.TimeSpan.TicksPerSecond) * 100)));
        }

        internal void WriteDateTime(global::System.DateTime value)
        {
            if (value.Kind == global::System.DateTimeKind.Unspecified)
                throw new global::System.ArgumentException("DateTime kind must not be unspecified", nameof(value));
            long ticks = (value.ToUniversalTime() - {% if features.unix_epoch %}global::System.DateTime.UnixEpoch{% else %}UnixEpoch{% endif %}).Ticks;
            long seconds = ticks / global::System.TimeSpan.TicksPerSecond;
            long subsecond = ticks % global::System.TimeSpan.TicksPerSecond;
            if (subsecond < 0)
            {
                seconds -= 1;
                subsecond += global::System.TimeSpan.TicksPerSecond;
            }
            WriteI64(seconds);
            WriteI32(checked((int)(subsecond * 100)));
        }

        internal void WriteGuid(global::System.Guid value)
        {
{% if features.guid_big_endian %}            global::System.Span<byte> bytes = stackalloc byte[16];
            if (!value.TryWriteBytes(bytes, bigEndian: true, out _))
                throw new global::System.InvalidOperationException("Guid conversion failed");
            WriteI64(global::System.Buffers.Binary.BinaryPrimitives.ReadInt64BigEndian(bytes[..8]));
            WriteI64(global::System.Buffers.Binary.BinaryPrimitives.ReadInt64BigEndian(bytes[8..]));
{% else %}            byte[] bytes = value.ToByteArray();
            WriteI64(unchecked(
                ((long)bytes[3] << 56) | ((long)bytes[2] << 48) | ((long)bytes[1] << 40) | ((long)bytes[0] << 32) |
                ((long)bytes[5] << 24) | ((long)bytes[4] << 16) | ((long)bytes[7] << 8) | bytes[6]));
            WriteI64(unchecked(
                ((long)bytes[8] << 56) | ((long)bytes[9] << 48) | ((long)bytes[10] << 40) | ((long)bytes[11] << 32) |
                ((long)bytes[12] << 24) | ((long)bytes[13] << 16) | ((long)bytes[14] << 8) | bytes[15]));
{% endif %}        }

        internal void WriteUri(global::System.Uri value) => WriteString(value.ToString());

        private void EnsureCapacity(int additional)
        {
            if (position + additional <= buffer.Length) return;
            global::System.Array.Resize(ref buffer, global::System.Math.Max(buffer.Length * 2, position + additional));
        }
    }

    public readonly struct BoltFFIResult<TOk, TErr>
    {
        private readonly TOk okValue;
        private readonly TErr errValue;

        private BoltFFIResult(TOk okValue, TErr errValue, bool isOk)
        {
            this.okValue = okValue;
            this.errValue = errValue;
            IsOk = isOk;
        }

        public bool IsOk { get; }
        public bool IsErr => !IsOk;
        public TOk OkValue => IsOk ? okValue : throw new global::System.InvalidOperationException("result is Err");
        public TErr ErrValue => IsOk ? throw new global::System.InvalidOperationException("result is Ok") : errValue;

        public static BoltFFIResult<TOk, TErr> Ok(TOk value) => new BoltFFIResult<TOk, TErr>(value, default!, true);
        public static BoltFFIResult<TOk, TErr> Err(TErr value) => new BoltFFIResult<TOk, TErr>(default!, value, false);
    }

    public sealed class BoltException : global::System.Exception
    {
        public BoltException(string message) : base(message) { }
    }
