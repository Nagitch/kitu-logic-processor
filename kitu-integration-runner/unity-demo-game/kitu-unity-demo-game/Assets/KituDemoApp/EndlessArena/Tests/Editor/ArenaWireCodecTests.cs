using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using Newtonsoft.Json.Linq;
using NUnit.Framework;
using UnityEngine;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaWireCodecTests
    {
        private static string Repository => Path.GetFullPath(Path.Combine(Application.dataPath, "../../../.."));
        private static string Fixtures => Path.Combine(Repository, "crates/kitu-transport/tests/fixtures/application-wire");
        private static IEnumerable<TestCaseData> Cases()
        {
            var manifest = JObject.Parse(File.ReadAllText(Path.Combine(Fixtures, "manifest.json")));
            foreach (var item in (JArray)manifest["cases"])
                foreach (string encoding in new[] { "json", "msgpack" })
                    if (item[encoding].Type != JTokenType.Null)
                        yield return new TestCaseData((string)item["kind"], (string)item[encoding], (bool)item["valid"], encoding)
                            .SetName("ArenaWireGolden_" + item["name"] + "_" + encoding);
        }
        [TestCaseSource(nameof(Cases))]
        public void RustGoldensAndUnityEncodingAgree(string kind, string file, bool valid, string encoding)
        {
            var codec = new ArenaWireCodec(encoding == "json" ? ArenaWireEncoding.Json : ArenaWireEncoding.MessagePack, kind == "server");
            byte[] bytes = File.ReadAllBytes(Path.Combine(Fixtures, file));
            if (!valid) { Assert.That(() => Decode(codec, kind, bytes), Throws.Exception); return; }
            var value = Decode(codec, kind, bytes);
            var mp = new ArenaWireCodec(ArenaWireEncoding.MessagePack, kind == "server");
            byte[] canonicalMp = Encode(mp, kind, value);
            string canonicalFile = Path.ChangeExtension(file, ".msgpack");
            Assert.That(canonicalMp, Is.EqualTo(File.ReadAllBytes(Path.Combine(Fixtures, canonicalFile))), "Exact Rust canonical MessagePack bytes, including float bits and every bundle boundary");
            byte[] encoded = Encode(codec, kind, value);
            Assert.That(Encode(mp, kind, Decode(codec, kind, encoded)), Is.EqualTo(canonicalMp));
            string output = Environment.GetEnvironmentVariable("KITU_ARENA_WIRE_EVIDENCE_DIR");
            if (!string.IsNullOrEmpty(output))
            {
                Directory.CreateDirectory(output);
                File.WriteAllBytes(Path.Combine(output, file), encoded);
            }
        }
        private static JObject Decode(ArenaWireCodec codec, string kind, byte[] bytes)
            => kind == "client" ? codec.DecodeClient(bytes) : kind == "server" ? codec.DecodeServer(bytes) : codec.DecodeInput(bytes);
        private static byte[] Encode(ArenaWireCodec codec, string kind, JObject value)
            => kind == "client" ? codec.EncodeClient(value) : kind == "server" ? codec.EncodeServer(value) : codec.EncodeInput(value);

        [TestCase(ArenaWireEncoding.Json), TestCase(ArenaWireEncoding.MessagePack)]
        public void ActualEncodedByteLimitIsInclusiveAndNoPartialFrameEscapes(ArenaWireEncoding encoding)
        {
            var value = ArenaWireCodec.Hello("limit", null); var codec = new ArenaWireCodec(encoding);
            byte[] bytes = codec.EncodeClient(value);
            Assert.That(new ArenaWireCodec(encoding, bytes.Length, 16384, 8192).EncodeClient(value), Is.EqualTo(bytes));
            Assert.That(() => new ArenaWireCodec(encoding, bytes.Length - 1, 16384, 8192).EncodeClient(value), Throws.TypeOf<InvalidDataException>());
            Assert.That(() => new ArenaWireCodec(encoding, bytes.Length - 1, 16384, 8192).DecodeClient(bytes), Throws.TypeOf<InvalidDataException>());
            var input = ArenaWireCodec.InputFrame("limit", 1, "/example", new JArray(new JObject { ["type"] = "str", ["value"] = new string('x', ArenaWireCodec.InputBytes) }));
            Assert.That(() => codec.EncodeInput(input), Throws.TypeOf<InvalidDataException>());
        }
        [TestCase(ArenaWireEncoding.Json), TestCase(ArenaWireEncoding.MessagePack)]
        public void StructuralBudgetsRejectBeforePublishingATree(ArenaWireEncoding encoding)
        {
            var value = ArenaWireCodec.Hello("limit", null); var codec = new ArenaWireCodec(encoding);
            byte[] bytes = codec.EncodeClient(value);
            Assert.That(() => new ArenaWireCodec(encoding, 4096, 4, 8192).DecodeClient(bytes), Throws.TypeOf<InvalidDataException>());
            Assert.That(() => new ArenaWireCodec(encoding, 4096, 16384, 2).DecodeClient(bytes), Throws.TypeOf<InvalidDataException>());
            byte[] deep = encoding == ArenaWireEncoding.Json ? Encoding.UTF8.GetBytes(new string('[', 34) + "0" + new string(']', 34)) : Enumerable.Repeat((byte)0x91, 34).Concat(new byte[] { 0 }).ToArray();
            Assert.That(() => codec.DecodeClient(deep), Throws.TypeOf<InvalidDataException>());
        }
        [TestCase("appId"), TestCase("wireVersion"), TestCase("schemaVersion"), TestCase("presentationVersion"), TestCase("tickRate"), TestCase("features")]
        public void EveryCompatibilityDimensionIsRequired(string field)
        {
            var value = ArenaWireCodec.Compatibility();
            if (field == "appId") value[field] = "other";
            else if (field == "features") value[field] = new JArray("typed-osc");
            else value[field] = 2;
            Assert.That(() => ArenaWireCodec.CheckCompatibility(value), Throws.TypeOf<InvalidDataException>());
            Assert.That(() => ArenaWireCodec.CheckCompatibility(ArenaWireCodec.Compatibility()), Throws.Nothing);
        }
        [TestCase("-0"), TestCase("-0.0"), TestCase("-0e0")]
        public void JsonNegativeZeroRetainsFloat32Sign(string number)
        {
            string json = "{\"bundle\":{\"messages\":[{\"address\":\"/zero\",\"args\":[{\"type\":\"float\",\"value\":" + number + "}]}]}}";
            var value = new ArenaWireCodec(ArenaWireEncoding.Json).DecodeInput(Encoding.UTF8.GetBytes(json));
            float scalar = (float)value["bundle"]["messages"][0]["args"][0]["value"];
            Assert.That(BitConverter.ToUInt32(BitConverter.GetBytes(scalar), 0), Is.EqualTo(0x80000000u));
        }
    }
}
