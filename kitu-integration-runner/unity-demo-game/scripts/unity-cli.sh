#!/bin/sh
# Run the installed Unity CLI against this checkout's demo project.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
project_dir=${KITU_UNITY_PROJECT:-"$script_dir/../kitu-unity-demo-game"}
if [ ! -f "$project_dir/ProjectSettings/ProjectVersion.txt" ]; then
    printf 'Unity project not found: %s\n' "$project_dir" >&2
    exit 1
fi
project_dir=$(CDPATH= cd -- "$project_dir" && pwd -P)

if [ -n "${KITU_UNITY_CLI:-}" ]; then
    cli=$KITU_UNITY_CLI
    case "$cli" in
        /*) ;;
        *) printf 'KITU_UNITY_CLI must be an absolute executable path.\n' >&2; exit 1 ;;
    esac
elif command -v unity >/dev/null 2>&1; then
    cli=$(command -v unity)
elif [ -n "${HOME:-}" ] && [ -x "$HOME/.unity/bin/unity" ]; then
    cli=$HOME/.unity/bin/unity
elif [ -n "${HOME:-}" ] && [ -x "$HOME/.local/bin/unity" ]; then
    cli=$HOME/.local/bin/unity
else
    printf 'Unity CLI is not installed. See unity-demo-game/README.md#unity-cli.\n' >&2
    exit 127
fi

if [ ! -x "$cli" ]; then
    printf 'Unity CLI is not executable: %s\n' "$cli" >&2
    exit 126
fi

# A relative PATH entry must keep resolving after we change directories.
case "$cli" in
    /*) ;;
    *) cli=$(CDPATH= cd -- "$(dirname -- "$cli")" && pwd -P)/$(basename -- "$cli") ;;
esac

# The CLI uses this for Pipeline commands; cwd also selects the project for
# commands such as open/run/build. No Hub registry or global preference changes.
export UNITY_PROJECT_PATH="$project_dir"
cd "$project_dir"
exec "$cli" "$@"
