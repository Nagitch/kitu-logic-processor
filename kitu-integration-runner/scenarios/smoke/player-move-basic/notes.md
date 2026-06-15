# Player Move Basic

This smoke fixture sends one runtime-boundary `/input/move` intent for `player:local`.

The expected output is the runtime-owned `/render/player/transform` message emitted after one tick.
The fixture does not patch ECS state directly.
