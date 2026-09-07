using System;
using Newtonsoft.Json.Linq;

namespace UnityOnlyArena
{
    /// <summary>Input/projection delivery shared by the socket and embedded clients.</summary>
    public interface IArenaConnection : IDisposable
    {
        bool Connected { get; }
        string Status { get; }
        bool Send(JObject input);
        bool TryReceive(out JObject message);
        void Pump(double elapsedSeconds);
        void Disconnect();
    }

    public enum ArenaBackend { Automatic, Embedded, Server }

    /// <summary>Schedules fixed native ticks without deriving game time from input count.</summary>
    public sealed class ArenaTickClock
    {
        public const double StepSeconds = 1d / 60d;
        public const int MaximumTicksPerPump = 8;
        public double PendingSeconds { get; private set; }

        public int Advance(double elapsedSeconds, Action tick)
        {
            if (tick == null) throw new ArgumentNullException(nameof(tick));
            if (double.IsNaN(elapsedSeconds) || double.IsInfinity(elapsedSeconds) || elapsedSeconds < 0)
                throw new ArgumentOutOfRangeException(nameof(elapsedSeconds));
            if (elapsedSeconds > double.MaxValue - PendingSeconds)
                throw new ArgumentOutOfRangeException(nameof(elapsedSeconds));
            PendingSeconds += elapsedSeconds;
            int count = 0;
            while (count < MaximumTicksPerPump && PendingSeconds + 1e-12 >= StepSeconds)
            {
                tick();
                PendingSeconds = Math.Max(0, PendingSeconds - StepSeconds);
                count++;
            }
            return count;
        }
    }

    internal static class ArenaLaunchArguments
    {
        internal static string Value(string name)
        {
            string[] args = Environment.GetCommandLineArgs();
            for (int i = 0; i < args.Length; i++)
                if (args[i] == name)
                {
                    if (i + 1 == args.Length || args[i + 1].StartsWith("--", StringComparison.Ordinal))
                        throw new ArgumentException("Missing value for " + name);
                    return args[i + 1];
                }
            return null;
        }
    }
}
