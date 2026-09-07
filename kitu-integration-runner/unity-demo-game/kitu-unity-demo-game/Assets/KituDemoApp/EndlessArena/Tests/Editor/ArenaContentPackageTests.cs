using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using NUnit.Framework;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaContentPackageTests
    {
        private string directory;
        private static readonly string[] Paths = { "unity-assets.json", "arena.tmd", "boss.rhai", "timelines/boss-telegraph.tsq", "timelines/floor-transition.tsq" };
        private static readonly string[] Roles = { "baseMaterial", "cube", "capsule", "sphere" };

        [SetUp]
        public void Setup()
        {
            directory = Path.Combine(Path.GetTempPath(), "arena-package-test-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(Path.Combine(directory, "timelines"));
            var assets = new JArray();
            for (int i = 0; i < Roles.Length; i++)
                assets.Add(new JObject { ["role"] = Roles[i], ["key"] = "arena/test/" + Roles[i], ["type"] = i == 0 ? "Material" : "GameObject" });
            Write("unity-assets.json", new JObject { ["schemaVersion"] = 1, ["assets"] = assets }.ToString(Formatting.None));
            // This reader validates shipped bytes and mappings, not game syntax.
            foreach (string path in Paths.Skip(1)) File.WriteAllBytes(Path.Combine(directory, path), new byte[] { 0, 255, 13, 10 });
            Rehash();
        }

        [TearDown]
        public void Cleanup() { if (Directory.Exists(directory)) Directory.Delete(directory, true); }

        [Test]
        public void ExactManifestBytesIdentifyPackageAndSourceBytesRemainUntouched()
        {
            byte[] source = File.ReadAllBytes(Path.Combine(directory, "arena.tmd"));
            var first = ArenaContentPackage.Load(directory);
            Assert.That(first.Keys.Count, Is.EqualTo(4));
            Assert.That(first.Keys["capsule"], Is.EqualTo("arena/test/capsule"));
            Assert.That(first.Identity, Is.EqualTo(Hash(File.ReadAllBytes(Path.Combine(directory, "package.json")))));
            Assert.That((string)first.Report["directory"], Is.EqualTo(Path.GetFullPath(directory)));
            File.AppendAllText(Path.Combine(directory, "package.json"), "\n", new UTF8Encoding(false));
            var second = ArenaContentPackage.Load(directory);
            Assert.That(second.Identity, Is.Not.EqualTo(first.Identity), "identity describes exact manifest bytes");
            CollectionAssert.AreEqual(source, File.ReadAllBytes(Path.Combine(directory, "arena.tmd")));
        }

        [Test]
        public void TamperedOrMissingSourceFailsInsteadOfAcceptingManifestMetadata()
        {
            File.WriteAllBytes(Path.Combine(directory, "arena.tmd"), new byte[] { 1, 255, 13, 10 });
            StringAssert.Contains("digest mismatch", Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory)).Message);
            Rehash();
            File.Delete(Path.Combine(directory, "boss.rhai"));
            Assert.Throws<FileNotFoundException>(() => ArenaContentPackage.Load(directory));
        }

        [TestCase("schema")]
        [TestCase("unknown-field")]
        [TestCase("path")]
        [TestCase("order")]
        [TestCase("count-type")]
        [TestCase("uppercase-digest")]
        public void StrictManifestRejectsIncompatibleOrAmbiguousEntries(string invalid)
        {
            var manifest = Read("package.json");
            var files = (JArray)manifest["files"];
            switch (invalid)
            {
                case "schema": manifest["schemaVersion"] = 2; break;
                case "unknown-field": manifest["ignored"] = true; break;
                case "path": files[0]["path"] = "../unity-assets.json"; break;
                case "order": var first = files[0].DeepClone(); files[0] = files[1].DeepClone(); files[1] = first; break;
                case "count-type": files[0]["bytes"] = "1"; break;
                case "uppercase-digest": files[0]["sha256"] = ((string)files[0]["sha256"]).ToUpperInvariant(); break;
            }
            Write("package.json", manifest.ToString(Formatting.None));
            Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory));
        }

        [TestCase("schema")]
        [TestCase("missing-role")]
        [TestCase("duplicate-key")]
        [TestCase("wrong-type")]
        [TestCase("long-key")]
        [TestCase("nul-key")]
        public void RehashedMappingStillRequiresValidRolesTypesAndKeys(string invalid)
        {
            var visual = Read("unity-assets.json");
            var assets = (JArray)visual["assets"];
            switch (invalid)
            {
                case "schema": visual["schemaVersion"] = 2; break;
                case "missing-role": assets.RemoveAt(3); break;
                case "duplicate-key": assets[1]["key"] = assets[0]["key"].DeepClone(); break;
                case "wrong-type": assets[1]["type"] = "Material"; break;
                case "long-key": assets[1]["key"] = new string('x', 129); break;
                case "nul-key": assets[1]["key"] = "arena/\0cube"; break;
            }
            Write("unity-assets.json", visual.ToString(Formatting.None));
            Rehash();
            Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory));
        }

        [Test]
        public void OversizedSourceAndManifestAreRejectedBeforeAllocation()
        {
            File.WriteAllBytes(Path.Combine(directory, "arena.tmd"), new byte[131073]);
            Rehash();
            StringAssert.Contains("byte limit", Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory)).Message);
            File.WriteAllBytes(Path.Combine(directory, "package.json"), new byte[16385]);
            StringAssert.Contains("byte limit", Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory)).Message);
        }

        [Test]
        public void Utf8DuplicateFieldsTrailingDataAndRelativeDirectoryAreRejected()
        {
            Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load("relative-package"));
            File.WriteAllBytes(Path.Combine(directory, "package.json"), new byte[] { 123, 34, 120, 34, 58, 34, 255, 34, 125 });
            Assert.Throws<DecoderFallbackException>(() => ArenaContentPackage.Load(directory));
            Write("package.json", "{\"schemaVersion\":1,\"schemaVersion\":1,\"files\":[]}");
            Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory));
            Rehash();
            File.AppendAllText(Path.Combine(directory, "package.json"), " {}", new UTF8Encoding(false));
            Assert.That(() => ArenaContentPackage.Load(directory), Throws.Exception);
        }

        [TestCase("{/*comment*/\"schemaVersion\":1,\"files\":[]}")]
        [TestCase("{'schemaVersion':1,'files':[]}")]
        [TestCase("{\"schemaVersion\":1,\"files\":[],}")]
        public void NonJsonExtensionsAreRejected(string manifest)
        {
            Write("package.json", manifest);
            Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory));
        }

        [Test]
        public void NamedPipeSourceIsRejectedWithoutWaitingForAWriter()
        {
            RequireMac();
            string path = Path.Combine(directory, "boss.rhai");
            File.Delete(path);
            Assert.That(MakeFifo(path, 0x180), Is.EqualTo(0), "create isolated test FIFO");
            var clock = System.Diagnostics.Stopwatch.StartNew();
            StringAssert.Contains("regular file", Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory)).Message);
            Assert.That(clock.Elapsed.TotalSeconds, Is.LessThan(2), "a FIFO with no writer must never block the reader");
        }

        [TestCase("file")]
        [TestCase("timelines")]
        [TestCase("root")]
        public void SymlinkSourceComponentsAreRejected(string component)
        {
            RequireMac();
            string original = component == "root" ? directory : Path.Combine(directory, component == "file" ? "boss.rhai" : "timelines");
            string target = original + "-symlink-target";
            bool isFile = component == "file";
            if (isFile) File.Move(original, target); else Directory.Move(original, target);
            try
            {
                Assert.That(Symlink(target, original), Is.EqualTo(0), "create isolated test symlink");
                Assert.Throws<InvalidDataException>(() => ArenaContentPackage.Load(directory));
            }
            finally
            {
                File.Delete(original);
                if (isFile) File.Move(target, original); else Directory.Move(target, original);
            }
        }

        private static void RequireMac()
        {
            if (UnityEngine.Application.platform != UnityEngine.RuntimePlatform.OSXEditor)
                Assert.Ignore("The nonblocking filesystem boundary currently targets macOS.");
        }
        [DllImport("/usr/lib/libSystem.B.dylib", EntryPoint = "mkfifo", SetLastError = true)]
        private static extern int MakeFifo([MarshalAs(UnmanagedType.LPUTF8Str)] string path, uint mode);
        [DllImport("/usr/lib/libSystem.B.dylib", EntryPoint = "symlink", SetLastError = true)]
        private static extern int Symlink([MarshalAs(UnmanagedType.LPUTF8Str)] string target, [MarshalAs(UnmanagedType.LPUTF8Str)] string path);

        private JObject Read(string path) => JObject.Parse(File.ReadAllText(Path.Combine(directory, path)));
        private void Write(string path, string value) => File.WriteAllText(Path.Combine(directory, path), value, new UTF8Encoding(false));
        private void Rehash()
        {
            var files = new JArray();
            foreach (string path in Paths)
            {
                byte[] bytes = File.ReadAllBytes(Path.Combine(directory, path));
                files.Add(new JObject { ["path"] = path, ["bytes"] = bytes.Length, ["sha256"] = Hash(bytes) });
            }
            Write("package.json", new JObject { ["schemaVersion"] = 1, ["files"] = files }.ToString(Formatting.None));
        }
        private static string Hash(byte[] bytes)
        {
            using (var sha = SHA256.Create()) return BitConverter.ToString(sha.ComputeHash(bytes)).Replace("-", "").ToLowerInvariant();
        }
    }
}
