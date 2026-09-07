namespace UnityOnlyArena
{
    // WebSocket messages may arrive in separate render frames. Publish the complete
    // authoritative projection together; a backward replay seek is a valid pair.
    public sealed class ArenaProjectionBuffer
    {
        private ArenaReferenceState pendingState;
        private ArenaPresentationState pendingPresentation;

        public void PushState(ArenaReferenceState state) => pendingState = state;
        public void PushPresentation(ArenaPresentationState presentation) => pendingPresentation = presentation;
        public void Reset() { pendingState = null; pendingPresentation = null; }

        public bool TryTake(out ArenaReferenceState state, out ArenaPresentationState presentation)
        {
            state = null;
            presentation = null;
            if (pendingState == null || pendingPresentation == null ||
                pendingState.tick != pendingPresentation.tick ||
                pendingState.simulationSteps != pendingPresentation.simulationStep) return false;
            state = pendingState;
            presentation = pendingPresentation;
            Reset();
            return true;
        }
    }
}
