# Kitu applications

This directory contains framework-local application scaffolding and historical
layout guidance. The playable Endless Arena application, Unity project,
application Admin, content and application verification now live in the
independent [Nagitch/kitu-unity-demo-game](https://github.com/Nagitch/kitu-unity-demo-game)
repository.

Framework crates under `crates/` must remain independent of application rules,
content and fixtures. New applications should consume the framework through
explicit contracts and keep their own hosts, manifests, scenarios and clients
in their application repository.
