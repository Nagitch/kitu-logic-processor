using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;
using UnityEngine;

namespace Kitu.Runtime
{
    public readonly struct KituRenderTransformEvent
    {
        public KituRenderTransformEvent(string entityId, long tick, float x, float y, float z)
        {
            EntityId = entityId;
            Tick = tick;
            X = x;
            Y = y;
            Z = z;
        }

        public string EntityId { get; }
        public long Tick { get; }
        public float X { get; }
        public float Y { get; }
        public float Z { get; }
    }

    public readonly struct KituWorldObjectState
    {
        public KituWorldObjectState(string id, string kind, float x, float y, float z, string color)
        {
            Id = id;
            Kind = kind;
            X = x;
            Y = y;
            Z = z;
            Color = color;
        }

        public string Id { get; }
        public string Kind { get; }
        public float X { get; }
        public float Y { get; }
        public float Z { get; }
        public string Color { get; }
    }

    public readonly struct KituWorldSnapshotEvent
    {
        public KituWorldSnapshotEvent(long tick, IReadOnlyList<KituWorldObjectState> objects)
        {
            Tick = tick;
            Objects = objects;
        }

        public long Tick { get; }
        public IReadOnlyList<KituWorldObjectState> Objects { get; }
    }

    public static class KituOscJson
    {
        private static readonly Regex ArgRegex = new Regex(
            "\\{\\\"type\\\":\\\"(?<type>[^\\\"]+)\\\",\\\"value\\\":(?<value>\\\"(?:\\\\.|[^\\\"])*\\\"|-?\\d+(?:\\.\\d+)?(?:[eE][+-]?\\d+)?|true|false)\\}");

        public static string BuildMoveInput(string entityId, float x, float y)
        {
            return "{\"address\":\"/input/move\",\"args\":["
                + BuildStringArg(entityId)
                + ","
                + BuildFloatArg(x)
                + ","
                + BuildFloatArg(y)
                + "]}";
        }

        public static bool TryParseRenderTransform(string json, out KituRenderTransformEvent renderEvent)
        {
            renderEvent = default;
            if (json.IndexOf("\"type\":\"osc\"", StringComparison.Ordinal) < 0
                || json.IndexOf("\"address\":\"/render/player/transform\"", StringComparison.Ordinal) < 0)
            {
                return false;
            }

            var args = ParseArgs(json);
            if (args.Count != 5)
            {
                return false;
            }

            if (!args[0].TryString(out var entityId)
                || !args[1].TryInt64(out var tick)
                || !args[2].TryFloat(out var x)
                || !args[3].TryFloat(out var y)
                || !args[4].TryFloat(out var z))
            {
                return false;
            }

            renderEvent = new KituRenderTransformEvent(entityId, tick, x, y, z);
            return true;
        }

        public static bool TryParseWorldSnapshot(string json, out KituWorldSnapshotEvent snapshotEvent)
        {
            snapshotEvent = default;
            if (json.IndexOf("\"type\":\"state\"", StringComparison.Ordinal) < 0)
            {
                return false;
            }

            var dto = JsonUtility.FromJson<StateEventDto>(json);
            if (dto == null || dto.snapshot == null)
            {
                return false;
            }

            var objects = new List<KituWorldObjectState>();
            if (dto.snapshot.objects != null)
            {
                foreach (var worldObject in dto.snapshot.objects)
                {
                    if (string.IsNullOrEmpty(worldObject.id))
                    {
                        continue;
                    }

                    objects.Add(new KituWorldObjectState(
                        worldObject.id,
                        worldObject.kind,
                        worldObject.x,
                        worldObject.y,
                        worldObject.z,
                        worldObject.color));
                }
            }

            snapshotEvent = new KituWorldSnapshotEvent(dto.snapshot.tick, objects);
            return true;
        }

        private static string BuildStringArg(string value)
        {
            return "{\"type\":\"str\",\"value\":\"" + Escape(value) + "\"}";
        }

        private static string BuildFloatArg(float value)
        {
            return "{\"type\":\"float\",\"value\":"
                + value.ToString("R", CultureInfo.InvariantCulture)
                + "}";
        }

        private static string Escape(string value)
        {
            var builder = new StringBuilder(value.Length);
            foreach (var ch in value)
            {
                switch (ch)
                {
                    case '\\':
                        builder.Append("\\\\");
                        break;
                    case '"':
                        builder.Append("\\\"");
                        break;
                    case '\n':
                        builder.Append("\\n");
                        break;
                    case '\r':
                        builder.Append("\\r");
                        break;
                    case '\t':
                        builder.Append("\\t");
                        break;
                    default:
                        builder.Append(ch);
                        break;
                }
            }

            return builder.ToString();
        }

        private static List<JsonArg> ParseArgs(string json)
        {
            var args = new List<JsonArg>();
            foreach (Match match in ArgRegex.Matches(json))
            {
                args.Add(new JsonArg(
                    match.Groups["type"].Value,
                    match.Groups["value"].Value));
            }

            return args;
        }

        private readonly struct JsonArg
        {
            private readonly string _type;
            private readonly string _rawValue;

            public JsonArg(string type, string rawValue)
            {
                _type = type;
                _rawValue = rawValue;
            }

            public bool TryString(out string value)
            {
                value = null;
                if (_type != "str" || _rawValue.Length < 2)
                {
                    return false;
                }

                value = Regex.Unescape(_rawValue.Substring(1, _rawValue.Length - 2));
                return true;
            }

            public bool TryInt64(out long value)
            {
                value = 0;
                if (_type != "int64" && _type != "int")
                {
                    return false;
                }

                return long.TryParse(_rawValue, NumberStyles.Integer, CultureInfo.InvariantCulture, out value);
            }

            public bool TryFloat(out float value)
            {
                value = 0f;
                if (_type != "float" && _type != "int" && _type != "int64")
                {
                    return false;
                }

                return float.TryParse(_rawValue, NumberStyles.Float, CultureInfo.InvariantCulture, out value);
            }
        }

        [Serializable]
        private sealed class StateEventDto
        {
            public string type;
            public WorldSnapshotDto snapshot;
        }

        [Serializable]
        private sealed class WorldSnapshotDto
        {
            public long tick;
            public WorldObjectDto[] objects;
        }

        [Serializable]
        private sealed class WorldObjectDto
        {
            public string id;
            public string kind;
            public float x;
            public float y;
            public float z;
            public string color;
        }
    }
}
