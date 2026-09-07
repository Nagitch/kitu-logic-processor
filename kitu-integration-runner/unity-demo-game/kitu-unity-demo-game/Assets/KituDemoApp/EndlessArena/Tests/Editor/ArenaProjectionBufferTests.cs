using NUnit.Framework;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaProjectionBufferTests
    {
        [Test]
        public void AbsentCueRemainsNullAfterWireDeserialization()
        {
            var state = ArenaPresentationState.FromJson(
                "{\"contractVersion\":1,\"run\":1,\"tick\":5526,\"simulationStep\":5300,\"bosses\":[],\"floor\":null}");
            Assert.That(state.tick, Is.EqualTo(5526));
            Assert.That(state.bosses, Is.Empty);
            Assert.That(state.floor, Is.Null);
        }

        [Test]
        public void SegmentedDeliveryPublishesOnlyTheCompletePair()
        {
            var buffer = new ArenaProjectionBuffer();
            var state = new ArenaReferenceState { tick = 20, simulationSteps = 12 };
            var presentation = new ArenaPresentationState { tick = 20, simulationStep = 12 };
            buffer.PushState(state);
            Assert.That(buffer.TryTake(out var waitingState, out var waitingPresentation), Is.False);
            Assert.That(waitingState, Is.Null);
            Assert.That(waitingPresentation, Is.Null);
            buffer.PushPresentation(presentation);
            Assert.That(buffer.TryTake(out var actualState, out var actualPresentation), Is.True);
            Assert.That(actualState, Is.SameAs(state));
            Assert.That(actualPresentation, Is.SameAs(presentation));
            Assert.That(buffer.TryTake(out _, out _), Is.False);
        }

        [Test]
        public void StaleTickAndDifferentSimulationStepCannotMix()
        {
            var buffer = new ArenaProjectionBuffer();
            buffer.PushState(new ArenaReferenceState { tick = 20, simulationSteps = 12 });
            buffer.PushPresentation(new ArenaPresentationState { tick = 19, simulationStep = 12 });
            Assert.That(buffer.TryTake(out _, out _), Is.False);
            buffer.PushPresentation(new ArenaPresentationState { tick = 20, simulationStep = 11 });
            Assert.That(buffer.TryTake(out _, out _), Is.False);
            buffer.PushPresentation(new ArenaPresentationState { tick = 20, simulationStep = 12 });
            Assert.That(buffer.TryTake(out _, out _), Is.True);
        }

        [Test]
        public void BackwardSeekAcceptsTheMatchingPairInEitherDeliveryOrder()
        {
            var buffer = new ArenaProjectionBuffer();
            buffer.PushState(new ArenaReferenceState { tick = 200, simulationSteps = 150 });
            buffer.PushPresentation(new ArenaPresentationState { tick = 200, simulationStep = 150 });
            Assert.That(buffer.TryTake(out _, out _), Is.True);
            buffer.PushPresentation(new ArenaPresentationState { tick = 10, simulationStep = 8 });
            Assert.That(buffer.TryTake(out _, out _), Is.False);
            buffer.PushState(new ArenaReferenceState { tick = 10, simulationSteps = 8 });
            Assert.That(buffer.TryTake(out var state, out var presentation), Is.True);
            Assert.That(state.tick, Is.EqualTo(10));
            Assert.That(presentation.tick, Is.EqualTo(10));
        }

        [Test]
        public void ReconnectDiscardsAnIncompletePriorSession()
        {
            var buffer = new ArenaProjectionBuffer();
            buffer.PushState(new ArenaReferenceState { tick = 20, simulationSteps = 12 });
            buffer.Reset();
            buffer.PushPresentation(new ArenaPresentationState { tick = 20, simulationStep = 12 });
            Assert.That(buffer.TryTake(out _, out _), Is.False);
            var replacement = new ArenaReferenceState { tick = 20, simulationSteps = 12 };
            buffer.PushState(replacement);
            Assert.That(buffer.TryTake(out var state, out _), Is.True);
            Assert.That(state, Is.SameAs(replacement));
        }
    }
}
