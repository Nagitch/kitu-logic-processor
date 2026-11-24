# Application spatial runtime

Applications choose their spatial mode when they initialize a session. A
build with `kitu-runtime/rapier-physics` supports both the existing `f32`
movement path and the integer spatial API with Rapier. The application owns
the selection and keeps it fixed for the session. The
[example](../../crates/kitu-runtime/examples/spatial_session.rs) accepts
`legacy` or `deterministic` as its startup setting.

~~~sh
cargo build --locked -p kitu-runtime --features rapier-physics --example spatial_session
target/debug/examples/spatial_session legacy
target/debug/examples/spatial_session deterministic
~~~

The example prints `x_m=2` for the legacy move and `x_m=1` for the integer
move stopped by an obstacle. Windows uses the same commands with `.exe` on
the executable path.

## Units and integer queries

One unit is one metre. Author placement and bounds in signed integer
millimetres. Convert placement to signed integer micrometres before storing
actor state in a typed ECS resource. Keep rendering projections in metres,
through `PositionUm::to_metres` and the rigid pose fields. At large coordinates,
the renderer and Rapier's `f32` positions have coarser precision than integer
micrometres.

`SpatialWorld::move_aabb` sweeps a point or an axis-aligned actor box along
one straight displacement. It stops at the first entry into an obstacle's
interior. Contact on a face, moving away from it, and grazing an edge are
allowed. Fraction comparisons use `i128` intermediates. The returned
position is rounded componentwise toward the start. A result that would
introduce an overlap leaves the actor at its starting position. Actors
starting inside a dynamic obstacle can move toward a nearest face if
the motion reduces each existing overlap. Starting inside a static
obstacle, overflowing the segment endpoint, and overflowing expanded bounds
are spatial errors.

`SpatialWorld::hit_segment` includes both endpoints, including a zero-length
point query. It returns the first hit as a reduced fraction of the segment.
On an exact tie obstacles precede targets, then IDs are compared
lexicographically. The caller can exclude one ID.

## Fixed-tick ordering

Use `RuntimeApplication::tick` with the existing committed-input boundary:

1. Process rigid inputs in their committed order.
2. Call `RigidBodyWorld::advance` once, using the `ApplicationTick` context.
   Rapier 0.36.1 steps with `enhanced-determinism` and the runtime's fixed
   timestep. Steps can be skipped during an application pause. The next step
   must have a greater source tick and the same fixed duration.
3. The physics API constructs the new integer obstacle snapshot from the
   configured colliders' post-step world AABBs. It rounds the minimum down
   and the maximum up, in micrometres. It publishes the whole snapshot
   after validating all bounds and IDs.
4. Process integer movement and hit queries in their committed order. Emit
   application projections for that tick using the completed rigid frame
   and the integer positions.

A failed post-step projection marks the physics world faulted and invalidates
spatial queries until the application rebuilds both worlds. This prevents
using stale obstacle data. The last completed physics frame remains available
for inspection. Applications handle state-dependent failures through their
own output and session state.

The API supports fixed and dynamic cuboids with identity initial rotation.
A rotating box blocks by its world AABB, which can extend beyond the visible
shape. Integer actors can move around rigid obstacles and cast queries against
them. Continuous motion of the obstacle between steps, capsule controllers,
slope and step handling, and two-way force exchange are future work.

## Verification

The [native replay CI](../../.github/workflows/replay-determinism.yml) drives
these APIs on macOS ARM64 and Windows x64 with one fixed application tick at
a time. It compares two runs per platform for each synthetic fixture. The
dynamic-obstacle fixture checks that an impulse can change both movement and
line of sight on the same tick, and that removing a body clears its obstacle
on the removal tick. Tick records include position, hits, movement blockers,
obstacle bounds, exact rigid-body float bits, and a hash of the engine state.
The snapshot encoding and equality evidence are specific to the tested
toolchain, dependency versions, and inputs.
