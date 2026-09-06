using System;
using NUnit.Framework;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaTickClockTests
    {
        [Test]
        public void ClockRetainsCatchupDebtAndFractionalTime()
        {
            var clock = new ArenaTickClock();
            int ticks = 0;
            Assert.That(clock.Advance(1, () => ticks++), Is.EqualTo(8));
            Assert.That(clock.PendingSeconds, Is.EqualTo(1 - 8d / 60).Within(1e-10));
            while (clock.PendingSeconds > ArenaTickClock.StepSeconds - 1e-10) clock.Advance(0, () => ticks++);
            Assert.That(ticks, Is.EqualTo(60));
            Assert.That(clock.Advance(1d / 120, () => ticks++), Is.EqualTo(0));
            Assert.That(clock.Advance(1d / 120, () => ticks++), Is.EqualTo(1));
            Assert.That(ticks, Is.EqualTo(61));
        }

        [Test]
        public void SchedulingDependsOnElapsedTimeAndSurvivesRenderRateChanges()
        {
            var slow = new ArenaTickClock();
            var fast = new ArenaTickClock();
            int slowTicks = 0, fastTicks = 0;
            for (int frame = 0; frame < 30; frame++) slow.Advance(1d / 30, () => slowTicks++);
            for (int frame = 0; frame < 240; frame++) fast.Advance(1d / 240, () => fastTicks++);
            Assert.That(slowTicks, Is.EqualTo(60));
            Assert.That(fastTicks, Is.EqualTo(slowTicks));
            Assert.That(fast.PendingSeconds, Is.EqualTo(slow.PendingSeconds).Within(1e-10));
        }

        [Test]
        public void InvalidElapsedTimeDoesNotConsumePendingTicks()
        {
            var clock = new ArenaTickClock();
            clock.Advance(1d / 120, () => Assert.Fail("not yet due"));
            double initial = clock.PendingSeconds;
            foreach (double invalid in new[] { double.NaN, double.PositiveInfinity, -1d })
                Assert.Throws<ArgumentOutOfRangeException>(() => clock.Advance(invalid, () => Assert.Fail("invalid time")));
            Assert.That(clock.PendingSeconds, Is.EqualTo(initial));
            Assert.Throws<ArgumentNullException>(() => clock.Advance(0, null));
        }
    }
}
