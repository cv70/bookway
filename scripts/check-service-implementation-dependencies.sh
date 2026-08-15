#!/usr/bin/env sh

# Service implementation packages are binaries; only their *-api packages are public contracts.
set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

service_names=""
for api_manifest in backend/bookway/*/api/Cargo.toml; do
    service_name=$(basename "$(dirname "$(dirname "$api_manifest")")")
    if [ ! -f "backend/bookway/$service_name/Cargo.toml" ]; then
        continue
    fi

    if [ -n "$service_names" ]; then
        service_names="$service_names|$service_name"
    else
        service_names="$service_name"
    fi
done

if [ -z "$service_names" ]; then
    printf '%s\n' 'No microservice API packages were found.' >&2
    exit 1
fi

implementation_name="bookway-($service_names)"
direct_dependency="^[[:space:]]*$implementation_name[[:space:]]*="
renamed_dependency="^[[:space:]]*package[[:space:]]*=[[:space:]]*[\"']$implementation_name[\"']"

violations=$(find backend -path backend/target -prune -o -name Cargo.toml -type f -exec \
    grep -nH -E "$direct_dependency|$renamed_dependency" {} + 2>/dev/null || true)

if [ -n "$violations" ]; then
    printf '%s\n' 'Microservice implementation crates must not be dependencies:' >&2
    printf '%s\n' "$violations" >&2
    printf '%s\n' 'Use the corresponding bookway-<service>-api contract package instead.' >&2
    exit 1
fi
