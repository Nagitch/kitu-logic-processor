using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Runtime.InteropServices;
using System.Text;
using Newtonsoft.Json.Linq;

namespace UnityOnlyArena
{
    // Checks the shipped bytes and visual mapping. Game data is evaluated only
    // by the native application's existing Tanu, Rhai and TSQ1 loaders.
    public sealed class ArenaContentPackage
    {
        private static readonly string[] Paths = { "unity-assets.json", "arena.tmd", "boss.rhai", "timelines/boss-telegraph.tsq", "timelines/floor-transition.tsq" };
        private static readonly int[] Limits = { 8192, 131072, 65536, 8192, 8192 };
        private static readonly string[] Roles = { "baseMaterial", "cube", "capsule", "sphere" };
        public string Directory { get; private set; }
        public string Identity { get; private set; }
        public JObject Report { get; private set; }
        public IReadOnlyDictionary<string, string> Keys => keys;
        private readonly Dictionary<string, string> keys = new Dictionary<string, string>();

        public static ArenaContentPackage Load(string directory)
        {
            if (!Path.IsPathRooted(directory)) throw new InvalidDataException("Arena package requires an absolute directory");
            var result = new ArenaContentPackage { Directory = Path.GetFullPath(directory) };
            RequireDirectory(result.Directory);
            RequireDirectory(Path.Combine(result.Directory, "timelines"));
            byte[] manifestBytes = ReadBounded(Path.Combine(result.Directory, "package.json"), 16384);
            var manifest = Parse(manifestBytes);
            Fields(manifest, "schemaVersion", "files");
            Version(manifest);
            var files = manifest["files"] as JArray;
            if (files == null || files.Count != Paths.Length) throw new InvalidDataException("Arena package needs exactly five source files");
            byte[] mapping = null;
            for (int index = 0; index < Paths.Length; index++)
            {
                var entry = files[index] as JObject;
                Fields(entry, "path", "bytes", "sha256");
                if (entry["path"].Type != JTokenType.String || (string)entry["path"] != Paths[index])
                    throw new InvalidDataException("Unexpected Arena package path or order");
                ulong count = ArenaWireCodec.Unsigned(entry["bytes"]);
                if (count > (ulong)Limits[index])
                    throw new InvalidDataException("Arena packaged source exceeds its byte limit");
                if (entry["sha256"].Type != JTokenType.String) throw new InvalidDataException("Arena package needs SHA-256 strings");
                string digest = (string)entry["sha256"];
                if (digest.Length != 64 || digest.Any(c => !(c >= '0' && c <= '9') && !(c >= 'a' && c <= 'f')))
                    throw new InvalidDataException("Invalid Arena package SHA-256");
                byte[] bytes = ReadBounded(Path.Combine(result.Directory, Paths[index]), Limits[index]);
                if ((ulong)bytes.LongLength != count || Hash(bytes) != digest)
                    throw new InvalidDataException("Arena package source digest mismatch: " + Paths[index]);
                if (index == 0) mapping = bytes;
            }
            var visual = Parse(mapping);
            Fields(visual, "schemaVersion", "assets");
            Version(visual);
            var assets = visual["assets"] as JArray;
            if (assets == null || assets.Count != Roles.Length) throw new InvalidDataException("Arena package needs four visual roles");
            var uniqueKeys = new HashSet<string>(StringComparer.Ordinal);
            for (int index = 0; index < Roles.Length; index++)
            {
                var asset = assets[index] as JObject;
                Fields(asset, "role", "key", "type");
                string type = index == 0 ? "Material" : "GameObject";
                if (asset["role"].Type != JTokenType.String || (string)asset["role"] != Roles[index] ||
                    asset["type"].Type != JTokenType.String || (string)asset["type"] != type || asset["key"].Type != JTokenType.String)
                    throw new InvalidDataException("Invalid Arena visual role or type");
                string key = (string)asset["key"];
                if (string.IsNullOrEmpty(key) || Encoding.UTF8.GetByteCount(key) > 128 || key.Contains('\0') || !uniqueKeys.Add(key))
                    throw new InvalidDataException("Invalid or duplicate Arena visual key");
                result.keys.Add(Roles[index], key);
            }
            result.Identity = Hash(manifestBytes);
            result.Report = new JObject { ["directory"] = result.Directory, ["identity"] = result.Identity,
                ["schemaVersion"] = 1, ["files"] = files.DeepClone(), ["assets"] = assets.DeepClone() };
            return result;
        }

        private static byte[] ReadBounded(string path, int limit)
        {
            var attributes = File.GetAttributes(path);
            if ((attributes & (FileAttributes.Directory | FileAttributes.ReparsePoint | FileAttributes.Device)) != 0)
                throw new InvalidDataException("Arena package source must be a regular file");
#if UNITY_EDITOR_OSX || UNITY_STANDALONE_OSX
            // macOS SDK: O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC. Check the
            // opened descriptor before FileStream, so a FIFO cannot hang the UI
            // and a path replaced by a symlink cannot escape the source checks.
            int descriptor = NativeFiles.Open(path, 0x01000104);
            if (descriptor < 0) throw new IOException("Cannot open Arena package source (errno " + Marshal.GetLastWin32Error() + "): " + path);
            try
            {
                if (NativeFiles.Stat(descriptor, out var stat) != 0 || (stat.Mode & 0xf000) != 0x8000)
                    throw new InvalidDataException("Arena package source must be a regular file");
                if (stat.Length < 0 || stat.Length > limit) throw new InvalidDataException("Arena package source exceeds byte limit");
                // Unity Mono's FileStream cannot adopt a POSIX descriptor. Read
                // the verified descriptor directly, with one extra byte to
                // detect growth past the limit and no path reopening.
                using (var output = new MemoryStream((int)stat.Length))
                {
                    var buffer = new byte[Math.Min(8192, limit + 1)];
                    while (true)
                    {
                        long count = NativeFiles.Read(descriptor, buffer, new UIntPtr((uint)Math.Min(buffer.Length, limit + 1 - (int)output.Length))).ToInt64();
                        if (count < 0)
                        {
                            int error = Marshal.GetLastWin32Error();
                            if (error == 4) continue; // EINTR
                            throw new IOException("Arena package read failed (errno " + error + ")");
                        }
                        if (count == 0) return output.ToArray();
                        if (count > buffer.Length || output.Length + count > limit)
                            throw new InvalidDataException("Arena package source exceeds byte limit");
                        output.Write(buffer, 0, (int)count);
                    }
                }
            }
            finally { NativeFiles.Close(descriptor); }
#elif UNITY_EDITOR_WIN || UNITY_STANDALONE_WIN
            using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read)) return ReadStream(file, limit);
#else
            throw new PlatformNotSupportedException("Arena packaged-source loading currently supports macOS and Windows filesystem APIs");
#endif
        }

        private static byte[] ReadStream(FileStream file, int limit)
        {
            if (file.Length > limit) throw new InvalidDataException("Arena package source exceeds byte limit");
            var bytes = new byte[(int)file.Length];
            int offset = 0;
            while (offset < bytes.Length)
            {
                int read = file.Read(bytes, offset, bytes.Length - offset);
                if (read == 0) throw new InvalidDataException("Arena package changed while reading");
                offset += read;
            }
            if (file.ReadByte() != -1) throw new InvalidDataException("Arena package grew while reading");
            return bytes;
        }

        private static JObject Parse(byte[] bytes) => ArenaWireCodec.ReadPackageObject(bytes);

        private static void RequireDirectory(string path)
        {
            var attributes = File.GetAttributes(path);
            if ((attributes & FileAttributes.Directory) == 0 || (attributes & FileAttributes.ReparsePoint) != 0)
                throw new InvalidDataException("Arena package requires real source directories");
        }

#if UNITY_EDITOR_OSX || UNITY_STANDALONE_OSX
        private static class NativeFiles
        {
            // Verified against sizeof/offsetof in the Apple Silicon SDK's
            // struct stat:144 bytes, st_mode at4, st_size at96.
            [StructLayout(LayoutKind.Explicit, Size = 144)]
            internal struct FileStat { [FieldOffset(4)] internal ushort Mode; [FieldOffset(96)] internal long Length; }
            [DllImport("/usr/lib/libSystem.B.dylib", EntryPoint = "open", SetLastError = true)]
            internal static extern int Open([MarshalAs(UnmanagedType.LPUTF8Str)] string path, int flags);
            [DllImport("/usr/lib/libSystem.B.dylib", EntryPoint = "fstat", SetLastError = true)]
            internal static extern int Stat(int descriptor, out FileStat stat);
            [DllImport("/usr/lib/libSystem.B.dylib", EntryPoint = "close")]
            internal static extern int Close(int descriptor);
            [DllImport("/usr/lib/libSystem.B.dylib", EntryPoint = "read", SetLastError = true)]
            internal static extern IntPtr Read(int descriptor, [Out] byte[] buffer, UIntPtr count);
        }
#endif
        private static void Fields(JObject value, params string[] names)
        {
            if (value == null || value.Count != names.Length || names.Any(name => value.Property(name) == null))
                throw new InvalidDataException("Unexpected Arena package fields");
        }

        private static void Version(JObject value)
        {
            if (ArenaWireCodec.Unsigned(value["schemaVersion"]) != 1)
                throw new InvalidDataException("Incompatible Arena package schema");
        }

        private static string Hash(byte[] value)
        {
            using (var sha = SHA256.Create()) return BitConverter.ToString(sha.ComputeHash(value)).Replace("-", "").ToLowerInvariant();
        }
    }
}
