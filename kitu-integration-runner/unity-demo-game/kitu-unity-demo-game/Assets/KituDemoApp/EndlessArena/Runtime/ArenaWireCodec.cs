using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Numerics;
using System.Text;
using Newtonsoft.Json.Linq;

namespace UnityOnlyArena
{
    public enum ArenaWireEncoding { Json, MessagePack }

    /// <summary>Bounded named-map application protocol. OSC type tags and bundle boundaries survive both encodings.</summary>
    public sealed class ArenaWireCodec
    {
        public const int InputBytes = 128 * 1024, OutputBytes = 8 * 1024 * 1024;
        private static readonly UTF8Encoding Utf8 = new UTF8Encoding(false, true);
        private readonly ArenaWireEncoding encoding;
        private readonly int maximumBytes, maximumNodes, maximumCollection;
        public ArenaWireCodec(ArenaWireEncoding encoding, bool output = false)
            : this(encoding, output ? OutputBytes : InputBytes, output ? 262144 : 16384, output ? 65536 : 8192) { }
        public ArenaWireCodec(ArenaWireEncoding encoding, int maximumBytes, int maximumNodes, int maximumCollection)
        {
            if (!Enum.IsDefined(typeof(ArenaWireEncoding), encoding) || maximumBytes < 1 || maximumNodes < 1 || maximumCollection < 1)
                throw new ArgumentOutOfRangeException(nameof(maximumBytes));
            this.encoding = encoding; this.maximumBytes = maximumBytes;
            this.maximumNodes = maximumNodes; this.maximumCollection = maximumCollection;
        }
        public JObject DecodeClient(byte[] bytes) { var value = Decode(bytes); Client(value); return value; }
        public JObject DecodeServer(byte[] bytes) { var value = Decode(bytes); Server(value); return value; }
        public JObject DecodeInput(byte[] bytes) { var value = Decode(bytes); Input(value); return value; }
        public byte[] EncodeClient(JObject value) { Client(value); return Encode(value); }
        public byte[] EncodeServer(JObject value) { Server(value); return Encode(value); }
        public byte[] EncodeInput(JObject value) { Input(value); return Encode(value); }

        public static JObject Compatibility() => new JObject {
            ["appId"] = "endless-arena", ["wireVersion"] = 1, ["schemaVersion"] = 1,
            ["presentationVersion"] = 1, ["tickRate"] = 60,
            ["features"] = new JArray("output-batches", "presentation", "replay", "typed-osc")
        };
        public static void CheckCompatibility(JToken offered)
        {
            ValidateCompatibility(offered);
            var required = Compatibility();
            foreach (string field in new[] { "appId", "wireVersion", "schemaVersion", "presentationVersion", "tickRate" })
                if (!JToken.DeepEquals(required[field], offered[field])) throw Bad("Incompatible Arena " + field);
            foreach (string feature in (JArray)required["features"])
            {
                bool found = false;
                foreach (string actual in (JArray)offered["features"]) if (actual == feature) found = true;
                if (!found) throw Bad("Missing required Arena feature " + feature);
            }
        }
        public static JObject Hello(string clientId, string expectedSessionId, bool observer = false) => new JObject {
            ["type"] = "hello", ["payload"] = new JObject {
                ["compatibility"] = Compatibility(), ["clientId"] = clientId,
                ["role"] = observer ? "observer" : "controller", ["expectedSessionId"] = expectedSessionId == null ? JValue.CreateNull() : new JValue(expectedSessionId)
            }
        };
        public static JObject InputFrame(string source, ulong id, string address, JArray args) => new JObject {
            ["metadata"] = new JObject { ["source"] = source, ["messageId"] = id, ["schemaVersion"] = 1 },
            ["bundle"] = new JObject { ["messages"] = new JArray(new JObject { ["address"] = address, ["args"] = args }) }
        };
        public static ulong Unsigned(JToken value)
        {
            Require(value != null && value.Type == JTokenType.Integer && value.Annotation<NegativeZero>() == null, "Expected unsigned integer");
            var integer = Integer(value);
            Require(integer >= BigInteger.Zero && integer <= ulong.MaxValue, "Unsigned integer overflow");
            return (ulong)integer;
        }
        public static long Signed(JToken value)
        {
            Require(value != null && value.Type == JTokenType.Integer && value.Annotation<NegativeZero>() == null, "Expected signed integer");
            var integer = Integer(value);
            Require(integer >= long.MinValue && integer <= long.MaxValue, "Signed integer overflow");
            return (long)integer;
        }
        private static BigInteger Integer(JToken value)
        {
            object raw = ((JValue)value).Value;
            if (raw is BigInteger big) return big;
            if (raw is ulong unsigned) return new BigInteger(unsigned);
            return new BigInteger(Convert.ToInt64(raw, CultureInfo.InvariantCulture));
        }
        private JObject Decode(byte[] bytes)
        {
            Require(bytes != null && bytes.Length <= maximumBytes, "Application frame exceeds byte limit");
            var reader = new Reader(bytes, maximumNodes, maximumCollection);
            var value = encoding == ArenaWireEncoding.Json ? reader.JsonRoot() : reader.MessagePackRoot();
            Require(value is JObject, "Application frame must be a named map");
            return (JObject)value;
        }
        private byte[] Encode(JObject value)
        {
            using (var writer = new Writer(maximumBytes, maximumNodes, maximumCollection))
            {
                writer.Value(value, encoding == ArenaWireEncoding.MessagePack, 1);
                return writer.Bytes();
            }
        }
        private void Client(JToken value)
        {
            Shape(value, "type", "payload");
            switch (Text(value["type"]))
            {
                case "hello":
                    var hello = value["payload"];
                    Shape(hello, "compatibility", "clientId", "role", "expectedSessionId");
                    ValidateCompatibility(hello["compatibility"]); BoundedText(hello["clientId"], 128); Role(hello["role"]); NullableText(hello["expectedSessionId"]);
                    if (hello["expectedSessionId"].Type != JTokenType.Null) BoundedText(hello["expectedSessionId"], 128);
                    break;
                case "input": Input(value["payload"]); break;
                default: throw Bad("Unknown client frame type");
            }
        }
        private void Server(JToken value)
        {
            Shape(value, "deliverySequence", "frame"); Unsigned(value["deliverySequence"]);
            var frame = value["frame"]; Shape(frame, "type", "payload"); var payload = frame["payload"];
            switch (Text(frame["type"]))
            {
                case "hello":
                    Shape(payload, "compatibility", "execution", "sessionId", "role", "limits", "status");
                    ValidateCompatibility(payload["compatibility"]); ValidateExecution(payload["execution"]);
                    BoundedText(payload["sessionId"], 128); Role(payload["role"]);
                    Shape(payload["limits"], "maxInputBytes", "maxOutputBytes");
                    UInt32(payload["limits"]["maxInputBytes"], true); UInt32(payload["limits"]["maxOutputBytes"], true);
                    Status(payload["status"]); break;
                case "output": Shape(payload, "batch", "status"); Batch(payload); break;
                case "snapshot":
                    Shape(payload, "reason", "batch", "status");
                    Choice(payload["reason"], "initial", "seek"); Batch(payload); break;
                case "replay": Status(payload); break;
                case "error":
                    Shape(payload, "code", "message", "fatal", "inputId");
                    Choice(payload["code"], "incompatible", "protocol", "invalidInput", "readOnly", "controllerBusy", "queueFull", "sessionChanged", "internal");
                    BoundedText(payload["message"], 1024); Boolean(payload["fatal"]);
                    if (payload["inputId"].Type != JTokenType.Null) Unsigned(payload["inputId"]); break;
                default: throw Bad("Unknown server frame type");
            }
        }
        private void Input(JToken value)
        {
            Require(value is JObject, "Input must be a named map");
            if (value["metadata"] == null)
            {
                Shape(value, "bundle"); ((JObject)value).AddFirst(new JProperty("metadata", JValue.CreateNull()));
            }
            else Shape(value, "metadata", "bundle");
            if (value["metadata"] != null && value["metadata"].Type != JTokenType.Null)
            {
                var metadata = value["metadata"]; Shape(metadata, "source", "messageId", "schemaVersion");
                Text(metadata["source"]); Unsigned(metadata["messageId"]); UInt32(metadata["schemaVersion"]);
            }
            Bundle(value["bundle"]);
        }
        private void Batch(JToken payload)
        {
            var batch = payload["batch"]; Shape(batch, "tick", "bundles"); Signed(batch["tick"]);
            var bundles = Array(batch["bundles"]); foreach (var bundle in bundles) Bundle(bundle);
            Status(payload["status"]);
            Require(Signed(batch["tick"]) == Signed(payload["status"]["playbackMode"]["tick"]), "Batch and playback ticks disagree");
        }
        private void Bundle(JToken value)
        {
            Shape(value, "messages"); foreach (var message in Array(value["messages"]))
            {
                Shape(message, "address", "args"); string address = Text(message["address"]);
                Require(address.StartsWith("/", StringComparison.Ordinal) && address.IndexOf('\0') < 0, "Invalid OSC address");
                foreach (var argument in Array(message["args"]))
                {
                    Shape(argument, "type", "value"); var scalar = argument["value"];
                    switch (Text(argument["type"]))
                    {
                        case "int": long integer = Signed(scalar); Require(integer >= int.MinValue && integer <= int.MaxValue, "OSC Int overflow"); break;
                        case "int64": Signed(scalar); break;
                        case "float":
                            Require(scalar.Type == JTokenType.Float || (encoding == ArenaWireEncoding.Json && scalar.Type == JTokenType.Integer), "Expected OSC Float32");
                            float number = scalar.Type == JTokenType.Integer ? (float)Integer(scalar) : Convert.ToSingle(((JValue)scalar).Value, CultureInfo.InvariantCulture);
                            if (scalar.Annotation<NegativeZero>() != null) number = NegativeFloatZero();
                            Require(!float.IsNaN(number) && !float.IsInfinity(number), "OSC Float must be finite");
                            argument["value"] = new JValue(number); break;
                        case "str": Require(Text(scalar).IndexOf('\0') < 0, "OSC strings cannot contain NUL"); break;
                        case "bool": Boolean(scalar); break;
                        default: throw Bad("Unknown OSC scalar type");
                    }
                }
            }
        }
        public static void ValidateExecution(JToken value)
        {
            Shape(value, "package", "sourceHash", "target"); BoundedText(value["package"], 128); BoundedText(value["target"], 128);
            string hash = Text(value["sourceHash"]); Require(hash.Length == 64, "Invalid execution source hash");
            foreach (char character in hash) Require((character >= '0' && character <= '9') || (character >= 'a' && character <= 'f') || (character >= 'A' && character <= 'F'), "Invalid execution source hash");
        }
        private static void ValidateCompatibility(JToken value)
        {
            Shape(value, "appId", "wireVersion", "schemaVersion", "presentationVersion", "tickRate", "features");
            BoundedText(value["appId"], 128);
            foreach (string field in new[] { "wireVersion", "schemaVersion", "presentationVersion", "tickRate" }) UInt32(value[field], true);
            var features = Array(value["features"]); Require(features.Count <= 16, "Too many features"); string previous = null;
            foreach (var item in features)
            {
                string feature = Text(item); Require(feature.Length > 0 && feature.Length <= 64, "Invalid feature identifier");
                foreach (char character in feature) Require((character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') || (character >= '0' && character <= '9') || character == '-' || character == '_', "Invalid feature identifier");
                Require(previous == null || string.CompareOrdinal(previous, feature) < 0, "Features must be sorted and unique"); previous = feature;
            }
        }
        private static void Status(JToken value)
        {
            Shape(value, "playbackMode", "readOnly"); Boolean(value["readOnly"]);
            var mode = value["playbackMode"]; Shape(mode, "active", "recordingId", "tick", "totalTicks", "playing", "seeking", "error");
            Boolean(mode["active"]); NullableText(mode["recordingId"]); Signed(mode["tick"]); Unsigned(mode["totalTicks"]);
            Boolean(mode["playing"]); Boolean(mode["seeking"]); NullableText(mode["error"]);
            if (mode["recordingId"].Type != JTokenType.Null) BoundedText(mode["recordingId"], 128);
            if (mode["error"].Type != JTokenType.Null) BoundedText(mode["error"], 1024, true);
        }
        private static void Shape(JToken value, params string[] fields)
        {
            Require(value is JObject map && map.Count == fields.Length, "Missing or unknown map field");
            foreach (string field in fields) Require(value[field] != null, "Missing or unknown map field");
            // Decode accepts arbitrary map order; encode follows the shared DTO declaration order.
            foreach (string field in fields)
            {
                JToken child = value[field]; ((JObject)value).Remove(field); ((JObject)value).Add(field, child);
            }
        }
        private static JArray Array(JToken value) { Require(value is JArray, "Expected array"); return (JArray)value; }
        private static string Text(JToken value) { Require(value != null && value.Type == JTokenType.String && ((JValue)value).Value != null, "Expected string"); return (string)value; }
        private static void BoundedText(JToken value, int maximum, bool allowEmpty = false)
        {
            string text = Text(value); Require((allowEmpty || text.Length > 0) && Utf8.GetByteCount(text) <= maximum && text.IndexOf('\0') < 0, "Invalid bounded identity or diagnostic");
        }
        private static void NullableText(JToken value) { if (value.Type != JTokenType.Null) Text(value); }
        private static void Boolean(JToken value) { Require(value != null && value.Type == JTokenType.Boolean, "Expected boolean"); }
        private static void UInt32(JToken value, bool positive = false) { ulong number = Unsigned(value); Require(number <= uint.MaxValue && (!positive || number > 0), "Invalid u32 field"); }
        private static void Role(JToken value) => Choice(value, "controller", "observer");
        private static void Choice(JToken value, params string[] choices) { Require(System.Array.IndexOf(choices, Text(value)) >= 0, "Unknown protocol variant"); }
        private static void Require(bool condition, string message) { if (!condition) throw Bad(message); }
        private static InvalidDataException Bad(string message) => new InvalidDataException(message);
        private static float NegativeFloatZero() => BitConverter.ToSingle(BitConverter.GetBytes(0x80000000u), 0);
        private sealed class NegativeZero { }

        // The parser checks depth, node/collection budgets and declared lengths before allocating a collection.
        private sealed class Reader
        {
            private readonly byte[] bytes;
            private readonly int maximumNodes, maximumCollection;
            private int at, nodes;
            internal Reader(byte[] bytes, int maximumNodes, int maximumCollection) { this.bytes = bytes; this.maximumNodes = maximumNodes; this.maximumCollection = maximumCollection; }
            private byte Read() { Require(at < bytes.Length, "Truncated application frame"); return bytes[at++]; }
            private void Node(int depth) { Require(depth <= 32 && ++nodes <= maximumNodes, "Application structural limit exceeded"); }
            private void Count(uint count) { Require(count <= maximumCollection && count <= bytes.Length - at, "Application collection limit exceeded"); }
            internal JToken MessagePackRoot() { var value = MessagePack(1); Require(at == bytes.Length, "Trailing MessagePack data"); return value; }
            private JToken MessagePack(int depth)
            {
                Node(depth); byte marker = Read();
                if (marker <= 0x7f) return new JValue((long)marker);
                if (marker >= 0xe0) return new JValue((long)(sbyte)marker);
                if ((marker & 0xe0) == 0xa0) return new JValue(String((uint)(marker & 31)));
                if ((marker & 0xf0) == 0x90) return List((uint)(marker & 15), depth);
                if ((marker & 0xf0) == 0x80) return Map((uint)(marker & 15), depth);
                switch (marker)
                {
                    case 0xc0: return JValue.CreateNull();
                    case 0xc2: return new JValue(false);
                    case 0xc3: return new JValue(true);
                    case 0xca:
                        uint bits = (uint)Number(4); float value = BitConverter.ToSingle(BitConverter.GetBytes(bits), 0);
                        Require(!float.IsNaN(value) && !float.IsInfinity(value), "Nonfinite MessagePack Float32"); return new JValue(value);
                    case 0xcc: return new JValue(Number(1));
                    case 0xcd: return new JValue(Number(2));
                    case 0xce: return new JValue(Number(4));
                    case 0xcf: return new JValue(Number(8));
                    case 0xd0: return new JValue((long)unchecked((sbyte)Number(1)));
                    case 0xd1: return new JValue((long)unchecked((short)Number(2)));
                    case 0xd2: return new JValue((long)unchecked((int)Number(4)));
                    case 0xd3: return new JValue(unchecked((long)Number(8)));
                    case 0xd9: return new JValue(String((uint)Number(1)));
                    case 0xda: return new JValue(String((uint)Number(2)));
                    case 0xdb: return new JValue(String((uint)Number(4)));
                    case 0xdc: return List((uint)Number(2), depth);
                    case 0xdd: return List((uint)Number(4), depth);
                    case 0xde: return Map((uint)Number(2), depth);
                    case 0xdf: return Map((uint)Number(4), depth);
                    default: throw Bad("Unsupported MessagePack marker (Float64, binary and extensions are not in wire v1)");
                }
            }
            private ulong Number(int size) { ulong value = 0; for (int i = 0; i < size; i++) value = (value << 8) | Read(); return value; }
            private string String(uint length)
            {
                Require(length <= bytes.Length - at, "Truncated MessagePack string");
                string value = Utf8.GetString(bytes, at, (int)length); at += (int)length; return value;
            }
            private JArray List(uint count, int depth)
            {
                Count(count); Require(count <= maximumNodes - nodes, "Application node limit exceeded"); var array = new JArray();
                for (uint i = 0; i < count; i++) array.Add(MessagePack(depth + 1)); return array;
            }
            private JObject Map(uint count, int depth)
            {
                Count(count); Require((ulong)count * 2 <= (ulong)(maximumNodes - nodes), "Application node limit exceeded"); var map = new JObject();
                for (uint i = 0; i < count; i++)
                {
                    string key = Text(MessagePack(depth + 1)); Require(map.Property(key, StringComparison.Ordinal) == null, "Duplicate map field");
                    map.Add(key, MessagePack(depth + 1));
                }
                return map;
            }
            private void Space() { while (at < bytes.Length && (bytes[at] == 32 || bytes[at] == 9 || bytes[at] == 10 || bytes[at] == 13)) at++; }
            internal JToken JsonRoot() { var value = Json(1); Space(); Require(at == bytes.Length, "Trailing JSON data"); return value; }
            private JToken Json(int depth)
            {
                Node(depth); Space(); Require(at < bytes.Length, "Truncated JSON value"); byte marker = bytes[at];
                if (marker == '"') return new JValue(JsonString());
                if (marker == '{')
                {
                    at++; Space(); var map = new JObject(); if (Take('}')) return map;
                    while (true)
                    {
                        Require(map.Count < maximumCollection, "Application collection limit exceeded"); Node(depth + 1); Space(); string key = JsonString();
                        Require(map.Property(key, StringComparison.Ordinal) == null, "Duplicate map field"); Space(); Expect(':'); map.Add(key, Json(depth + 1)); Space();
                        if (Take('}')) return map; Expect(',');
                    }
                }
                if (marker == '[')
                {
                    at++; Space(); var array = new JArray(); if (Take(']')) return array;
                    while (true)
                    {
                        Require(array.Count < maximumCollection, "Application collection limit exceeded"); array.Add(Json(depth + 1)); Space();
                        if (Take(']')) return array; Expect(',');
                    }
                }
                if (marker == 't') { Literal("true"); return new JValue(true); }
                if (marker == 'f') { Literal("false"); return new JValue(false); }
                if (marker == 'n') { Literal("null"); return JValue.CreateNull(); }
                return JsonNumber();
            }
            private void Literal(string value) { foreach (char character in value) Expect(character); }
            private bool Take(char value) { if (at < bytes.Length && bytes[at] == value) { at++; return true; } return false; }
            private void Expect(char value) { Require(Read() == value, "Invalid JSON grammar"); }
            private string JsonString()
            {
                Expect('"'); var text = new StringBuilder(); int start = at;
                while (true)
                {
                    byte value = Read();
                    if (value == '"' || value == '\\')
                    {
                        text.Append(Utf8.GetString(bytes, start, at - start - 1));
                        if (value == '"') return text.ToString();
                        byte escape = Read();
                        switch (escape)
                        {
                            case (byte)'"': text.Append('"'); break;
                            case (byte)'\\': text.Append('\\'); break;
                            case (byte)'/': text.Append('/'); break;
                            case (byte)'b': text.Append('\b'); break;
                            case (byte)'f': text.Append('\f'); break;
                            case (byte)'n': text.Append('\n'); break;
                            case (byte)'r': text.Append('\r'); break;
                            case (byte)'t': text.Append('\t'); break;
                            case (byte)'u':
                                char character = HexChar();
                                if (char.IsHighSurrogate(character))
                                {
                                    Expect('\\'); Expect('u'); char low = HexChar(); Require(char.IsLowSurrogate(low), "Invalid JSON surrogate pair"); text.Append(character); text.Append(low);
                                }
                                else { Require(!char.IsLowSurrogate(character), "Invalid JSON surrogate"); text.Append(character); }
                                break;
                            default: throw Bad("Invalid JSON escape");
                        }
                        start = at;
                    }
                    else Require(value >= 32, "Unescaped JSON control character");
                }
            }
            private char HexChar()
            {
                int value = 0;
                for (int i = 0; i < 4; i++)
                {
                    byte digit = Read(); int decoded = digit >= '0' && digit <= '9' ? digit - '0' : digit >= 'a' && digit <= 'f' ? digit - 'a' + 10 : digit >= 'A' && digit <= 'F' ? digit - 'A' + 10 : -1;
                    Require(decoded >= 0, "Invalid JSON hexadecimal escape"); value = (value << 4) | decoded;
                }
                return (char)value;
            }
            private bool Digit() => at < bytes.Length && bytes[at] >= '0' && bytes[at] <= '9';
            private JToken JsonNumber()
            {
                int start = at; bool negative = Take('-'); Require(Digit(), "Invalid JSON number");
                if (!Take('0')) while (Digit()) at++;
                bool integral = true;
                if (Take('.')) { integral = false; Require(Digit(), "Invalid JSON fraction"); while (Digit()) at++; }
                if (Take('e') || Take('E')) { integral = false; if (!Take('+')) Take('-'); Require(Digit(), "Invalid JSON exponent"); while (Digit()) at++; }
                int length = at - start;
                string text = Encoding.ASCII.GetString(bytes, start, length);
                if (integral)
                {
                    Require(length <= 40, "JSON integer exceeds finite wire domain");
                    BigInteger number = BigInteger.Parse(text, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture);
                    var value = number >= long.MinValue && number <= long.MaxValue ? new JValue((long)number) : number >= 0 && number <= ulong.MaxValue ? new JValue((ulong)number) : new JValue(number);
                    if (negative && number.IsZero) value.AddAnnotation(new NegativeZero()); return value;
                }
                Require(float.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out float scalar) && !float.IsInfinity(scalar) && !float.IsNaN(scalar), "JSON Float exceeds finite f32 domain");
                return new JValue(negative && scalar == 0 ? NegativeFloatZero() : scalar);
            }
        }

        private sealed class Writer : IDisposable
        {
            private readonly MemoryStream stream = new MemoryStream();
            private readonly byte[] buffer = new byte[4096];
            private readonly int maximumBytes, maximumNodes, maximumCollection;
            private int nodes;
            internal Writer(int maximumBytes, int maximumNodes, int maximumCollection) { this.maximumBytes = maximumBytes; this.maximumNodes = maximumNodes; this.maximumCollection = maximumCollection; }
            internal byte[] Bytes() => stream.ToArray();
            public void Dispose() => stream.Dispose();
            private void Reserve(int count)
            {
                Require(count <= maximumBytes - stream.Length, "Encoded application frame exceeds byte limit");
                long required = stream.Length + count;
                if (required > stream.Capacity)
                    stream.Capacity = (int)Math.Min(maximumBytes, Math.Max(required, Math.Max(256L, stream.Capacity * 2L)));
            }
            private void Byte(byte value) { Reserve(1); stream.WriteByte(value); }
            private void Raw(string value, int start, int length)
            {
                int end = start + length;
                while (start < end)
                {
                    int size = Math.Min(1024, end - start); if (char.IsHighSurrogate(value[start + size - 1])) size--;
                    Require(size > 0, "Invalid encoded surrogate");
                    int written = Utf8.GetBytes(value, start, size, buffer, 0); Reserve(written); stream.Write(buffer, 0, written); start += size;
                }
            }
            private void Raw(string value) => Raw(value, 0, value.Length);
            internal void Value(JToken value, bool packed, int depth)
            {
                Require(depth <= 32 && ++nodes <= maximumNodes, "Encoded application structural limit exceeded");
                if (value is JObject map)
                {
                    Require(map.Count <= maximumCollection, "Encoded map exceeds collection limit");
                    if (packed) Header(map.Count, 0x80, 15, 0xde, 0xdf); else Byte((byte)'{'); bool first = true;
                    foreach (var property in map.Properties())
                    {
                        if (!packed && !first) Byte((byte)','); first = false;
                        Value(new JValue(property.Name), packed, depth + 1); if (!packed) Byte((byte)':'); Value(property.Value, packed, depth + 1);
                    }
                    if (!packed) Byte((byte)'}'); return;
                }
                if (value is JArray array)
                {
                    Require(array.Count <= maximumCollection, "Encoded array exceeds collection limit");
                    if (packed) Header(array.Count, 0x90, 15, 0xdc, 0xdd); else Byte((byte)'[');
                    for (int i = 0; i < array.Count; i++) { if (!packed && i > 0) Byte((byte)','); Value(array[i], packed, depth + 1); }
                    if (!packed) Byte((byte)']'); return;
                }
                switch (value.Type)
                {
                    case JTokenType.Null: if (packed) Byte(0xc0); else Raw("null"); break;
                    case JTokenType.Boolean: if (packed) Byte((bool)value ? (byte)0xc3 : (byte)0xc2); else Raw((bool)value ? "true" : "false"); break;
                    case JTokenType.String: String((string)value, packed); break;
                    case JTokenType.Integer:
                        BigInteger integer = Integer(value); Require(integer >= long.MinValue && integer <= ulong.MaxValue, "Encoded integer overflow");
                        if (!packed) Raw(integer.ToString(CultureInfo.InvariantCulture));
                        else if (integer >= 0) Unsigned((ulong)integer); else Signed((long)integer); break;
                    case JTokenType.Float:
                        float scalar = Convert.ToSingle(((JValue)value).Value, CultureInfo.InvariantCulture); Require(!float.IsNaN(scalar) && !float.IsInfinity(scalar), "Encoded float must be finite");
                        if (packed) { Byte(0xca); Number(BitConverter.ToUInt32(BitConverter.GetBytes(scalar), 0), 4); }
                        else
                        {
                            string text = scalar == 0 && BitConverter.ToUInt32(BitConverter.GetBytes(scalar), 0) == 0x80000000u ? "-0.0" : scalar.ToString("R", CultureInfo.InvariantCulture);
                            if (text.IndexOf('.') < 0 && text.IndexOf('E') < 0 && text.IndexOf('e') < 0) text += ".0"; Raw(text);
                        }
                        break;
                    default: throw Bad("Unsupported encoded scalar");
                }
            }
            private void Number(ulong value, int size) { for (int i = size - 1; i >= 0; i--) Byte((byte)(value >> (i * 8))); }
            private void Unsigned(ulong value)
            {
                if (value <= 127) Byte((byte)value);
                else if (value <= byte.MaxValue) { Byte(0xcc); Number(value, 1); }
                else if (value <= ushort.MaxValue) { Byte(0xcd); Number(value, 2); }
                else if (value <= uint.MaxValue) { Byte(0xce); Number(value, 4); }
                else { Byte(0xcf); Number(value, 8); }
            }
            private void Signed(long value)
            {
                if (value >= -32) Byte(unchecked((byte)value));
                else if (value >= sbyte.MinValue) { Byte(0xd0); Number(unchecked((ulong)value), 1); }
                else if (value >= short.MinValue) { Byte(0xd1); Number(unchecked((ulong)value), 2); }
                else if (value >= int.MinValue) { Byte(0xd2); Number(unchecked((ulong)value), 4); }
                else { Byte(0xd3); Number(unchecked((ulong)value), 8); }
            }
            private void Header(int count, int compact, int maximum, byte shortMarker, byte longMarker)
            {
                if (count <= maximum) Byte((byte)(compact | count));
                else if (count <= ushort.MaxValue) { Byte(shortMarker); Number((uint)count, 2); }
                else { Byte(longMarker); Number((uint)count, 4); }
            }
            private void String(string value, bool packed)
            {
                int count = Utf8.GetByteCount(value); Reserve(count);
                if (packed)
                {
                    if (count <= 31) Byte((byte)(0xa0 | count));
                    else if (count <= byte.MaxValue) { Byte(0xd9); Number((uint)count, 1); }
                    else if (count <= ushort.MaxValue) { Byte(0xda); Number((uint)count, 2); }
                    else { Byte(0xdb); Number((uint)count, 4); }
                    Raw(value); return;
                }
                Byte((byte)'"'); int start = 0;
                for (int i = 0; i < value.Length; i++)
                {
                    char character = value[i]; string escaped = character == '"' ? "\\\"" : character == '\\' ? "\\\\" : character < 32 ? "\\u" + ((int)character).ToString("x4", CultureInfo.InvariantCulture) : null;
                    if (escaped == null) continue;
                    Raw(value, start, i - start); Raw(escaped); start = i + 1;
                }
                Raw(value, start, value.Length - start); Byte((byte)'"');
            }
        }
    }
}
