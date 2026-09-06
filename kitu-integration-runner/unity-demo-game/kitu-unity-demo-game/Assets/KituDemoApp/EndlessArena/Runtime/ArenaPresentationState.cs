using System;
using Newtonsoft.Json;

namespace UnityOnlyArena
{
    // Detached presentation from the authoritative Runtime. Unity owns no cue clock.
    [Serializable]
    public sealed class ArenaPresentationState
    {
        public int contractVersion;
        public long run, tick, simulationStep;
        public ArenaBossCue[] bosses;
        public ArenaFloorCue floor;

        // JsonUtility constructs a default class for a JSON null field. Cue absence
        // is authoritative, so retain null with the transport's JSON serializer.
        public static ArenaPresentationState FromJson(string json) =>
            JsonConvert.DeserializeObject<ArenaPresentationState>(json);

        public ArenaBossCue Boss(int entityId)
        {
            if (bosses != null)
                foreach (var cue in bosses)
                    if (cue.entityId == entityId) return cue;
            return null;
        }
    }

    [Serializable]
    public sealed class ArenaBossCue
    {
        public string id, clipId;
        public int entityId, floor;
        public long startedTick, offsetTick;
        public int nextEventIndex, eventCount;
        public float radius, intensity;
    }

    [Serializable]
    public sealed class ArenaFloorCue
    {
        public string id, clipId;
        public int fromFloor, toFloor;
        public long startedTick, offsetTick;
        public int nextEventIndex, eventCount;
        public float opacity;
    }
}
